use crate::{AppState, BroadcastServer, GetCount};
use actix::prelude::*;
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
    let ws_count = srv.send(GetCount).await.unwrap_or(0);

    // получаем количество SSE-подписчиков
    let sse = state.sse_senders.lock().await;
    let sse_count = sse.len();

    // получаем количество Long Polling-подписчиков
    let lp = state.lp_senders.lock().await;
    let lp_count = lp.len();

    let result = Stats {
        ws: ws_count,
        sse: sse_count,
        long_polling: lp_count,
    };

    HttpResponse::Ok().json(result)
}
