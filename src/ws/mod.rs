pub mod broadcast;
pub mod route;

use crate::{
    validator::message::{IncomingMessage, Validate},
    ClientMessage,
};
use actix::prelude::*;
use actix::Addr;
use actix_web_actors::ws as actix_ws;
use broadcast::{BroadcastServer, RegisterWs};
use std::time::Duration;
use std::time::Instant;

// --- WebSocket актор ---
struct MyWs {
    addr: Addr<BroadcastServer>,
    hb: Instant, // отслеживаем последнее "pong"
}

impl MyWs {
    fn hb(&self, ctx: &mut actix_ws::WebsocketContext<Self>) {
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
    type Context = actix_ws::WebsocketContext<Self>;
    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);
        let rec = ctx.address().recipient();
        self.addr.do_send(RegisterWs(rec));
    }
}

impl StreamHandler<Result<actix_ws::Message, actix_ws::ProtocolError>> for MyWs {
    fn handle(
        &mut self,
        msg: Result<actix_ws::Message, actix_ws::ProtocolError>,
        ctx: &mut Self::Context,
    ) {
        match msg {
            Ok(actix_ws::Message::Text(text)) => {
                // 1) Пытаемся распарсить в IncomingMessage
                match serde_json::from_str::<IncomingMessage>(&text) {
                    Ok(parsed) => {
                        // 2) Валидируем
                        if let Err(err) = parsed.validate() {
                            // Можно отправить клиенту ошибку или просто пропустить
                            let _ = ctx.text(
                                serde_json::to_string(&serde_json::json!({
                                    "error": format!("Invalid message: {}", err)
                                }))
                                .unwrap(),
                            );
                            return;
                        }
                        // 3) Всё ок – рассылаем дальше:
                        self.addr.do_send(ClientMessage(parsed));
                    }
                    Err(e) => {
                        // JSON некорректен (не тот формат)
                        let _ = ctx.text(
                            serde_json::to_string(&serde_json::json!({
                                "error": format!("JSON parse error {e}")
                            }))
                            .unwrap(),
                        );
                    }
                }
            }
            Ok(actix_ws::Message::Ping(msg)) => {
                self.hb = Instant::now(); // обновляем время последнего ответа
                ctx.pong(&msg);
            }
            Ok(actix_ws::Message::Pong(_)) => {
                self.hb = Instant::now(); // обновляем pong
            }
            Ok(actix_ws::Message::Close(reason)) => {
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
        // 1) Сериализуем структуру в JSON-строку
        let text = match serde_json::to_string(&msg.0) {
            Ok(json) => json,
            Err(e) => {
                eprintln!("Ошибка сериализации IncomingMessage: {}", e);
                return;
            }
        };

        // 2) Печатаем в лог уже готовую JSON-строку
        println!("[WebSocket] Отправлено: {}", text);

        // 3) Отправляем клиенту эту строку
        ctx.text(text);
    }
}
