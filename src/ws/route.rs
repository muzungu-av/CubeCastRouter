use super::MyWs;
use crate::ws::broadcast::BroadcastServer;
use actix::Addr;
use actix_web::web::Payload;
use actix_web::{Error, HttpRequest, HttpResponse};
use actix_web_actors::ws as actix_ws;
use std::collections::HashMap;
use std::time::Instant;

/// WebSocket маршрут
pub async fn ws_route(
    req: HttpRequest,
    stream: Payload,
    srv: actix_web::web::Data<Addr<BroadcastServer>>,
) -> Result<HttpResponse, Error> {
    // 1) Парсим query string в HashMap
    let query: HashMap<String, String> = form_urlencoded::parse(req.query_string().as_bytes())
        .into_owned()
        .collect();

    // 2) Достаём id, по умолчанию пустая строка
    let sender_id = query.get("id").cloned().unwrap_or_default();

    let ws = MyWs {
        addr: srv.get_ref().clone(),
        hb: Instant::now(),
        sender_id,
    };
    actix_ws::start(ws, &req, stream)
}
