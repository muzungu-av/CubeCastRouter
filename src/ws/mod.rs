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
pub struct MyWs {
    addr: Addr<BroadcastServer>,
    hb: Instant, // отслеживаем последнее "pong"
    sender_id: String,
}

impl MyWs {
    fn start_heartbeat(&self, ctx: &mut actix_ws::WebsocketContext<Self>) {
        ctx.run_interval(Duration::from_secs(300), |act, ctx| {
            //таймаут
            if Instant::now().duration_since(act.hb) > Duration::from_secs(30) {
                println!("WebSocket {} таймаут, закрытие", act.sender_id);
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
        self.start_heartbeat(ctx);
        let rec = ctx.address().recipient();
        self.addr.do_send(RegisterWs {
            sender_id: self.sender_id.clone(),
            rec,
        });
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
                // Пытаемся распарсить в IncomingMessage
                match serde_json::from_str::<IncomingMessage>(&text) {
                    Ok(parsed) => {
                        // Валидируем
                        if let Err(err) = parsed.validate() {
                            // Можно отправить клиенту ошибку или просто пропустить
                            let _e = ctx.text(
                                serde_json::to_string(&serde_json::json!({
                                    "error": format!("Invalid message: {}", err)
                                }))
                                .unwrap(),
                            );
                            return;
                        }
                        // Всё ок – рассылаем дальше:
                        self.addr.do_send(ClientMessage {
                            msg: parsed.clone(),
                            origin_sender_id: parsed.sender.id.clone(),
                        });
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
        // Сериализуем структуру в JSON-строку
        let text = match serde_json::to_string(&msg.msg) {
            Ok(json) => json,
            Err(e) => {
                eprintln!("Ошибка сериализации IncomingMessage: {}", e);
                return;
            }
        };

        // Отправляем клиенту эту строку
        ctx.text(text);
    }
}
