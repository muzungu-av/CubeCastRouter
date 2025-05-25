use actix::prelude::*;
use actix_cors::Cors;
use actix_web::{web, App, Error, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_actors::ws;
use actix_web_lab::sse::{Data, Event, Sse};
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::UnboundedReceiverStream;
mod http_api;

// --- Сообщение от клиента ---
#[derive(Message, Clone, Deserialize, Serialize, Debug)]
#[rtype(result = "()")]
struct ClientMessage(String);

// --- Состояние приложения ---
struct AppState {
    ws_subs: Arc<Mutex<Vec<Recipient<ClientMessage>>>>,
    sse_senders: Arc<Mutex<Vec<mpsc::UnboundedSender<String>>>>,
    lp_senders: Arc<Mutex<Vec<oneshot::Sender<String>>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            ws_subs: Arc::new(Mutex::new(Vec::new())),
            sse_senders: Arc::new(Mutex::new(Vec::new())),
            lp_senders: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

// --- WebSocket актор ---
struct MyWs {
    addr: Addr<BroadcastServer>,
    hb: Instant, // отслеживаем последнее "pong"
}

impl MyWs {
    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(Duration::from_secs(10), |act, ctx| {
            // если последний pong был более 30 секунд назад, закрываем соединение
            if Instant::now().duration_since(act.hb) > Duration::from_secs(30) {
                println!("WebSocket соединение прервано из-за таймаута");
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }
}

impl Actor for MyWs {
    type Context = ws::WebsocketContext<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);
        let rec = ctx.address().recipient();
        self.addr.do_send(RegisterWs(rec));
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct RegisterWs(Recipient<ClientMessage>);

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MyWs {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now(); // обновляем время последнего ответа
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now(); // обновляем pong
            }
            Ok(ws::Message::Text(text)) => {
                println!("[WebSocket] Получено: {}", text);
                self.addr.do_send(ClientMessage(text.to_string()));
            }
            Ok(ws::Message::Close(reason)) => {
                println!("WebSocket закрыт: {:?}", reason);
                ctx.stop();
            }
            _ => {}
        }
    }
}

impl Handler<ClientMessage> for MyWs {
    type Result = ();

    fn handle(&mut self, msg: ClientMessage, ctx: &mut Self::Context) {
        println!("[WebSocket] Отправлено: {}", msg.0);
        ctx.text(msg.0);
    }
}

// --- SSE обработчик ---
async fn sse_handler(state: web::Data<AppState>) -> Sse<impl Stream<Item = Result<Event, Error>>> {
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    let mut senders = state.sse_senders.lock().await;
    senders.push(tx.clone());

    let event_stream =
        UnboundedReceiverStream::new(rx).map(|msg| Ok::<Event, Error>(Event::Data(Data::new(msg))));

    Sse::from_stream(event_stream).with_keep_alive(std::time::Duration::from_secs(15))
}

// --- Long Polling ---
async fn long_polling_handler(state: web::Data<AppState>) -> impl Responder {
    let (tx, rx) = oneshot::channel();
    {
        let mut lp_senders = state.lp_senders.lock().await;
        lp_senders.push(tx);
    }

    let res = rx.await.unwrap_or_else(|_| "timeout".into());
    HttpResponse::Ok().json(res)
}

// --- BroadcastServer: рассылка по всем трём типам ---
struct BroadcastServer {
    state: web::Data<AppState>,
}

impl BroadcastServer {
    fn new(state: web::Data<AppState>) -> Self {
        Self { state }
    }
}

#[derive(Message)]
#[rtype(result = "usize")]
struct GetCount;
impl Handler<GetCount> for BroadcastServer {
    type Result = ResponseFuture<usize>;
    fn handle(&mut self, _: GetCount, _: &mut Context<Self>) -> Self::Result {
        let state = self.state.clone();
        Box::pin(async move {
            let ws = state.ws_subs.lock().await;
            ws.len()
        })
    }
}

impl Actor for BroadcastServer {
    type Context = Context<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        let state = self.state.clone();
        // Heartbeat: без реальных данных, чистим мёртвые SSE-соединения
        ctx.run_interval(Duration::from_secs(2), move |_act, _ctx| {
            let state = state.clone();
            actix::spawn(async move {
                // SSE: отправляем ping (пустую строку), удаляем отвалившиеся соединения
                {
                    let mut senders = state.sse_senders.lock().await;
                    senders.retain(|tx| tx.send(String::new()).is_ok());
                }

                // WebSocket: отправляем ping
                {
                    let mut subs = state.ws_subs.lock().await;
                    subs.retain(|rec| rec.try_send(ClientMessage("ping".into())).is_ok());
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

impl Handler<ClientMessage> for BroadcastServer {
    type Result = ();
    fn handle(&mut self, msg: ClientMessage, _: &mut Self::Context) {
        let txt = msg.0.clone();
        let state = self.state.clone();
        actix::spawn(async move {
            // 1) WebSocket
            {
                let mut subs = state.ws_subs.lock().await;
                subs.retain(|rec| rec.try_send(msg.clone()).is_ok());
            }

            // 2) SSE
            {
                let mut senders = state.sse_senders.lock().await;
                senders.retain(|tx| tx.send(txt.clone()).is_ok());
            }

            // 3) Long Polling
            {
                let mut lps = state.lp_senders.lock().await;
                while let Some(tx) = lps.pop() {
                    let _ = tx.send(txt.clone());
                }
            }
        });
    }
}

// --- WebSocket маршрут ---
async fn ws_route(
    req: HttpRequest,
    stream: web::Payload,
    srv: web::Data<Addr<BroadcastServer>>,
) -> Result<HttpResponse, Error> {
    ws::start(
        MyWs {
            addr: srv.get_ref().clone(),
            hb: Instant::now(),
        },
        &req,
        stream,
    )
}

// --- POST /send ---
async fn send_msg(
    srv: web::Data<Addr<BroadcastServer>>,
    msg: web::Json<ClientMessage>,
) -> impl Responder {
    srv.do_send(msg.0.clone());
    HttpResponse::Ok().body("sent")
}

// --- main ---
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let state = web::Data::new(AppState::new());
    let srv = BroadcastServer::new(state.clone()).start();

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .app_data(web::Data::new(srv.clone()))
            .wrap(Cors::default().allow_any_origin())
            .route("/ws", web::get().to(ws_route))
            .route("/sse", web::get().to(sse_handler))
            .route("/long-polling", web::get().to(long_polling_handler))
            .route("/send", web::post().to(send_msg))
            .route("/stats", web::get().to(http_api::stats))
    })
    .bind(("127.0.0.1", 7070))?
    .run()
    .await
}
