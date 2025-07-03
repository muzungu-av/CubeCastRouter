use crate::validator::extractor::ValidatedJson;
use crate::validator::message::IncomingMessage;
use crate::ws::broadcast::BroadcastServer;
use crate::{AppState, ClientMessage};
use actix::Addr;
use actix_web::{web, Error, HttpRequest, HttpResponse, Responder};
use actix_web_lab::sse::{Data, Event, Sse};
use futures_util::stream::StreamExt;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;
use tokio_stream::wrappers::UnboundedReceiverStream;

// --- SSE обработчик ---
// pub async fn sse_handler(
//     state: web::Data<AppState>,
// ) -> Sse<impl futures_util::stream::Stream<Item = Result<Event, Error>>> {
//     let sender_id = "sse_id".to_string();
//     let (tx, rx) = mpsc::unbounded_channel::<String>();
//     let mut senders = state.sse_senders.lock().await;
//     senders.push((sender_id.clone(), tx));

//     let event_stream =
//         UnboundedReceiverStream::new(rx).map(|msg| Ok::<Event, Error>(Event::Data(Data::new(msg))));

//     Sse::from_stream(event_stream).with_keep_alive(std::time::Duration::from_secs(60))
// }

pub async fn sse_handler(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> Sse<impl futures_util::stream::Stream<Item = Result<Event, Error>>> {
    // 1) Извлекаем sender_id из query: /sse?id=123
    let query: HashMap<String, String> = form_urlencoded::parse(req.query_string().as_bytes())
        .into_owned()
        .collect();
    let sender_id = query.get("id").cloned().unwrap_or_default();

    // 2) Создаём канал и регистрируем в AppState
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    {
        let mut senders = state.sse_senders.lock().await;
        senders.push((sender_id.clone(), tx));
    }

    // 3) Превращаем rx в SSE‑стрим
    let event_stream =
        UnboundedReceiverStream::new(rx).map(|msg| Ok::<Event, Error>(Event::Data(Data::new(msg))));

    // 4) Возвращаем Sse с периодическим keep-alive
    Sse::from_stream(event_stream).with_keep_alive(std::time::Duration::from_secs(60))
}

// --- Long Polling обработчик ---
pub async fn long_polling_handler(
    state: web::Data<AppState>,
    msg: ValidatedJson<IncomingMessage>,
) -> impl Responder {
    // Сохраняем sender_id единожды
    let sender_id = msg.0.sender.id.clone();

    // Регистрируемся как подписчик и создаём oneshot‑канал
    let (tx, rx) = oneshot::channel::<String>();
    {
        let mut lps = state.lp_senders.lock().await;
        lps.push((sender_id.clone(), tx));
    }

    // Ждём чужого сообщения или таймаута
    let result = match timeout(Duration::from_secs(30), rx).await {
        // Успех — получаем payload
        Ok(Ok(payload)) => {
            // и сразу удаляем себя из подписчиков,
            // чтобы не остаться двжды и не получить своё следующее
            let mut lps = state.lp_senders.lock().await;
            lps.retain(|(sid, _)| sid != &sender_id);
            payload
        }
        // Таймаут — тоже чистим
        _ => {
            let mut lps = state.lp_senders.lock().await;
            lps.retain(|(sid, _)| sid != &sender_id);
            "heartbeat timeout".to_string()
        }
    };

    HttpResponse::Ok().json(result)
}

/// Приём собственных сообщений и их мгновенная рассылка
pub async fn send_handler(
    srv: web::Data<Addr<BroadcastServer>>,
    msg: ValidatedJson<IncomingMessage>,
) -> impl Responder {
    let origin = msg.0.sender.id.clone();
    srv.do_send(ClientMessage {
        msg: msg.0.clone(),
        origin_sender_id: origin,
    });

    HttpResponse::Ok().finish()
}
