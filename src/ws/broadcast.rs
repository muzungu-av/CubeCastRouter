use crate::{
    validator::message::{IncomingMessage, Sender},
    AppState, ClientMessage, ROOM_CONFIG,
};
use actix::prelude::*;
use std::time::Duration;

static PING_INTERVAL: u64 = 15;

#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterWs(pub Recipient<ClientMessage>);

pub struct BroadcastServer {
    pub state: actix_web::web::Data<AppState>,
}

impl BroadcastServer {
    pub fn new(state: actix_web::web::Data<AppState>) -> Self {
        Self { state }
    }
}

#[derive(Message)]
#[rtype(result = "usize")]
pub struct GetCount;

impl Actor for BroadcastServer {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        let state = self.state.clone();
        ctx.run_interval(Duration::from_secs(PING_INTERVAL), move |_act, _ctx| {
            let state = state.clone();
            actix::spawn(async move {
                // 1) SSE: отправляем пустую строку, чтобы выявить отвалившиеся соединения
                {
                    let mut senders = state.sse_senders.lock().await;
                    senders.retain(|tx| tx.send(String::new()).is_ok());
                }

                // 2) WebSocket: шлём «ping»-IncomingMessage, чтобы оставить только живые подписки
                {
                    // Собираем шаблон валидного IncomingMessage:
                    let ping_msg = IncomingMessage {
                        // Берём любую непустую строку, допустим, из конфига:
                        room_id: ROOM_CONFIG.room.clone(),
                        sender: Sender {
                            id: String::new(),
                            sender_type: "наблюдатель".to_string(), // один из разрешённых типов
                        },
                        target: None,
                        msg_command: "PING".to_string(),
                        payload: None,
                    };

                    // Оборачиваем в ClientMessage и клонируем для каждого получателя
                    let ping_client_msg = ClientMessage(ping_msg.clone());

                    let mut subs = state.ws_subs.lock().await;
                    subs.retain(|rec| rec.try_send(ping_client_msg.clone()).is_ok());
                }
            });
        });
    }
}

impl Handler<RegisterWs> for BroadcastServer {
    type Result = ();
    fn handle(&mut self, msg: RegisterWs, _: &mut Self::Context) {
        let state = self.state.clone();
        let recipient = msg.0;

        actix::spawn(async move {
            let mut subs = state.ws_subs.lock().await;
            subs.push(recipient);
        });
    }
}

impl Handler<GetCount> for BroadcastServer {
    type Result = ResponseFuture<usize>;
    fn handle(&mut self, _: GetCount, _: &mut Context<Self>) -> Self::Result {
        let state = self.state.clone();
        Box::pin(async move { state.ws_subs.lock().await.len() })
    }
}

impl Handler<ClientMessage> for BroadcastServer {
    type Result = ();
    fn handle(&mut self, msg: ClientMessage, _: &mut Self::Context) {
        // msg.0 – это IncomingMessage
        let serialized = serde_json::to_string(&msg.0).unwrap();
        let state = self.state.clone();
        actix::spawn(async move {
            // 1) WebSocket
            {
                let mut subs = state.ws_subs.lock().await;
                subs.retain(|rec| rec.try_send(ClientMessage(msg.0.clone())).is_ok());
            }

            // 2) SSE
            {
                let mut senders = state.sse_senders.lock().await;
                senders.retain(|tx| tx.send(serialized.clone()).is_ok());
            }

            // 3) Long-Polling: раздаём всем (id, tx) и сразу очищаем вектор
            {
                let mut lps = state.lp_senders.lock().await;
                for (_sid, one_tx) in lps.drain(..) {
                    let _ = one_tx.send(serialized.clone());
                }
            }
        });
    }
}
