use crate::{validator::message::IncomingMessage, AppState, ROOM_CONFIG};
use actix::prelude::*;
use serde::Serialize;
use std::time::Duration;

static PING_INTERVAL: u64 = 15;

/// Сообщение, которое идёт через актор BroadcastServer.
#[derive(Message, Clone, Debug, Serialize)]
#[rtype(result = "()")]
pub struct ClientMessage {
    pub msg: IncomingMessage,
    /// Кто отправил (sender.id)
    pub origin_sender_id: String,
}

/// Регистрация WebSocket‑подписчика.
#[derive(Message)]
#[rtype(result = "()")]
pub struct RegisterWs {
    pub sender_id: String,
    pub rec: Recipient<ClientMessage>,
}

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
                // SSE: heartbeat
                {
                    let mut sse = state.sse_senders.lock().await;
                    sse.retain(|(_, tx)| tx.send(String::new()).is_ok());
                }

                // WebSocket: ping‑сообщение для чистки мёртвых
                {
                    // Собираем шаблон валидного IncomingMessage:
                    let ping_msg = IncomingMessage {
                        // Берём любую непустую строку, допустим, из конфига:
                        room_id: ROOM_CONFIG.room.clone(),
                        sender: crate::validator::message::Sender {
                            id: String::new(),
                            sender_type: "heartbeat".to_string(),
                        },
                        target: None,
                        msg_command: Some("PING".to_string()),
                        payload: None,
                    };
                    let ping_client = ClientMessage {
                        msg: ping_msg.clone(),
                        origin_sender_id: String::new(),
                    };

                    let mut subs = state.ws_subs.lock().await;
                    subs.retain(|(_, rec)| rec.try_send(ping_client.clone()).is_ok());
                }
            });
        });
    }
}

impl Handler<RegisterWs> for BroadcastServer {
    type Result = ();
    fn handle(&mut self, msg: RegisterWs, _: &mut Self::Context) {
        let state = self.state.clone();
        actix::spawn(async move {
            let mut subs = state.ws_subs.lock().await;
            subs.push((msg.sender_id, msg.rec));
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
        let text = serde_json::to_string(&msg.msg).unwrap();
        let state = self.state.clone();

        println!("> {}", text);
        actix::spawn(async move {
            // WS
            {
                let mut subs = state.ws_subs.lock().await;
                let mut keep = Vec::new();
                for (sid, rec) in subs.drain(..) {
                    //todo убрать msg.origin_sender_id.clone()
                    if sid != msg.origin_sender_id {
                        let _ = rec.do_send(ClientMessage {
                            msg: msg.msg.clone(),
                            origin_sender_id: msg.origin_sender_id.clone(),
                        });
                    } else {
                        // свой не получает
                    }
                    // в любом случае WS‑клиент остаётся подписанным
                    keep.push((sid, rec));
                }
                *subs = keep;
            }

            // SSE
            {
                let mut sse = state.sse_senders.lock().await;
                let mut keep = Vec::new();
                for (sid, tx) in sse.drain(..) {
                    if sid != msg.origin_sender_id {
                        let _ = tx.send(text.clone());
                    }
                    keep.push((sid, tx));
                }
                // for (sid, tx) in sse.drain(..) {
                //     if sid.clone() != msg.origin_sender_id.clone() {
                //         let _ = tx.send(text.clone());
                //     }
                //     keep.push((sid, tx));
                // }
                *sse = keep;
            }

            // Long‑Polling — рассылка чужим LP и удаление их
            {
                let mut lps = state.lp_senders.lock().await;
                let mut keep = Vec::new();
                for (sid, tx) in lps.drain(..) {
                    if sid != msg.origin_sender_id.clone() {
                        // чужим — отправляем
                        let _ = tx.send(text.clone());
                        // и не сохраняем, т.к. одноразовый канал
                    } else {
                        // своему — не шлём, но сохраняем, чтобы он мог ждать дальше
                        keep.push((sid, tx));
                    }
                }
                *lps = keep;
            }
        });
    }
}
