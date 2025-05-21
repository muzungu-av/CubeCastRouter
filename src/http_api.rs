use crate::{AppState, BroadcastServer, GetWsCount};
use actix::Addr;
use actix_web::{web, HttpResponse, Responder};
use serde::Serialize;

#[derive(Serialize)]
struct Stats {
    ws: usize,
    sse: usize,
    long_polling: usize,
}

/// Возвращает количество подписчиков по типам: WebSocket, SSE и Long Polling
pub async fn stats(
    state: web::Data<AppState>,
    srv: web::Data<Addr<BroadcastServer>>,
) -> impl Responder {
    // получаем количество WebSocket-подписчиков
    let ws_count = srv.send(GetWsCount).await.unwrap_or(0);

    // получаем количество SSE-подписчиков
    let sse_count = state.sse_subscribers.lock().unwrap().len();

    // получаем количество Long Polling-подписчиков
    let lp_count = state.long_polling_subscribers.lock().unwrap().len();

    let result = Stats {
        ws: ws_count,
        sse: sse_count,
        long_polling: lp_count,
    };

    HttpResponse::Ok().json(result)
}
