use actix::{dev::OneshotSender, prelude::*};

use actix_web::{
    web::{self, Bytes},
    App, Error, HttpRequest, HttpResponse, HttpServer, Responder,
};
use actix_web_actors::ws;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio_stream::wrappers::UnboundedReceiverStream;

// --- Определение структуры для сообщений ---
#[derive(Message, Clone, serde::Deserialize, serde::Serialize, Debug)]
#[rtype(result = "()")]
struct ClientMessage(String);

// --- Глобальная структура для подписчиков ---
struct AppState {
    sse_subscribers: Arc<Mutex<Vec<tokio::sync::mpsc::UnboundedSender<String>>>>,
    long_polling_subscribers: Arc<Mutex<Vec<OneshotSender<String>>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            sse_subscribers: Arc::new(Mutex::new(Vec::new())),
            long_polling_subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

// --- WebSocket актор ---
struct MyWs {
    addr: Addr<BroadcastServer>,
}

impl actix::Actor for MyWs {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let addr = ctx.address().recipient();
        self.addr
            .do_send(ClientMessage(format!("Новый клиент подключился")));
        self.addr.do_send(RegisterSubscriber(addr));
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct RegisterSubscriber(pub Recipient<ClientMessage>);

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MyWs {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        if let Ok(ws::Message::Text(text)) = msg {
            println!("[WebSocket] Получено: {}", text);
            self.addr.do_send(ClientMessage(text.to_string()));
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
async fn sse_handler(state: web::Data<AppState>, req: HttpRequest) -> impl Responder {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    state.sse_subscribers.lock().unwrap().push(tx.clone()); // Добавляем подписчика
    println!("[SSE] Клиент подключился");
    HttpResponse::Ok()
        .content_type("text/event-stream")
        .no_chunking(1024)
        .streaming(UnboundedReceiverStream::new(rx).map(|msg| {
            println!("[SSE] Отправлено: {}", msg);
            Ok::<_, actix_web::Error>(Bytes::from(format!("data: {}\n\n", msg)))
        }))
}

// --- Long Polling обработчик ---
async fn long_polling_handler(state: web::Data<AppState>) -> impl Responder {
    let (tx, rx) = tokio::sync::oneshot::channel();
    // Добавляем в список подписчиков для Long Polling
    state.long_polling_subscribers.lock().unwrap().push(tx);
    println!("[Long Polling] Клиент ожидает сообщение");
    let response = rx.await.unwrap_or_else(|_| "timeout".to_string());
    println!("[Long Polling] Отправлено: {}", response);
    HttpResponse::Ok().json(response)
}

// --- Сервер рассылки сообщений ---
#[derive(Message)]
#[rtype(result = "()")]

struct BroadcastServer {
    subscribers: Arc<Mutex<Vec<Recipient<ClientMessage>>>>,
    app_state: web::Data<AppState>,
}

impl BroadcastServer {
    // Теперь BroadcastServer хранит ссылку на AppState,
    // чтобы можно было отправлять сообщения SSE и Long Polling подписчикам.
    fn new(app_state: web::Data<AppState>) -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
            app_state,
        }
    }
}

impl Actor for BroadcastServer {
    type Context = Context<Self>;
}

impl Handler<RegisterSubscriber> for BroadcastServer {
    type Result = ();

    fn handle(&mut self, msg: RegisterSubscriber, _: &mut Self::Context) {
        self.subscribers.lock().unwrap().push(msg.0);
        println!("[Broadcast] Клиент зарегистрирован");
    }
}

impl Handler<ClientMessage> for BroadcastServer {
    type Result = ();

    fn handle(&mut self, msg: ClientMessage, _: &mut Self::Context) {
        println!("[Broadcast] Рассылка сообщения: {}", msg.0);
        // Отправляем сообщение подписчикам WebSocket.
        {
            let mut ws_subs = self.subscribers.lock().unwrap();
            ws_subs.retain(|sub| sub.try_send(msg.clone()).is_ok());
        }
        // Отправляем сообщение подписчикам SSE.
        {
            let mut sse_subs = self.app_state.sse_subscribers.lock().unwrap();
            sse_subs.retain(|sub| sub.send(msg.0.clone()).is_ok());
        }
        // Отправляем сообщение подписчикам Long Polling (одноразовые каналы, поэтому удаляем их).
        {
            let mut lp_subs = self.app_state.long_polling_subscribers.lock().unwrap();
            while let Some(sub) = lp_subs.pop() {
                let _ = sub.send(msg.0.clone());
            }
        }
    }
}

// --- Обработчик WebSocket ---
async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    srv: web::Data<Addr<BroadcastServer>>,
) -> Result<HttpResponse, Error> {
    let resp = ws::start(
        MyWs {
            addr: srv.get_ref().clone(),
        },
        &req,
        stream,
    );
    println!("[WebSocket] Клиент подключился");
    resp
}

async fn send_message(
    broadcast_srv: web::Data<Addr<BroadcastServer>>,
    msg: web::Json<ClientMessage>,
) -> impl Responder {
    println!("[API] Получен POST /send:  {:#?}", msg.0);
    // Отправляем сообщение через BroadcastServer, который разошлёт его всем типам подписчиков.
    broadcast_srv.do_send(msg.0.clone());
    HttpResponse::Ok().json("Message sent")
}

// --- Запуск сервера ---
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let state = web::Data::new(AppState::new());
    let broadcast_srv = BroadcastServer::new(state.clone()).start();
    println!("[Server] Запуск на 127.0.0.1:7070");
    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .app_data(web::Data::new(broadcast_srv.clone()))
            .route("/ws", web::get().to(ws_handler))
            .route("/sse", web::get().to(sse_handler))
            .route("/long-polling", web::get().to(long_polling_handler))
            .route("/send", web::post().to(send_message))
    })
    .bind("127.0.0.1:7070")?
    .run()
    .await
}

/*
Можно отправлять события с разными типами

fn get_stream() -> impl Stream<Item = Result<Bytes, actix_web::Error>> {
    interval(Duration::from_secs(2))
        .map(|_| Ok(Bytes::from("event: update\ndata: {\"message\": \"обновлено\"}\n\n")))
}

В React:
eventSource.addEventListener("update", (event) => {
  console.log("Получено обновление:", event.data);
});
*/
