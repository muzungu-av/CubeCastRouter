use super::MyWs;
use crate::ws::broadcast::BroadcastServer;
use actix::Addr;
use actix_web::web::Payload;
use actix_web::{Error, HttpRequest, HttpResponse};
use actix_web_actors::ws as actix_ws;
use std::time::Instant;

/// WebSocket маршрут
pub async fn ws_route(
    req: HttpRequest,
    stream: Payload,
    srv: actix_web::web::Data<Addr<BroadcastServer>>,
) -> Result<HttpResponse, Error> {
    actix_ws::start(
        MyWs {
            addr: srv.get_ref().clone(),
            hb: Instant::now(),
        },
        &req,
        stream,
    )
}
