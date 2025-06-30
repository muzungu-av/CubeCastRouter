use crate::AppState;
use actix_web::{web, Error, HttpResponse, Responder};
use actix_web_lab::sse::{Data, Event, Sse};
use futures_util::stream::StreamExt;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;
use tokio_stream::wrappers::UnboundedReceiverStream;

// --- SSE обработчик ---
pub async fn sse_handler(
    state: web::Data<AppState>,
) -> Sse<impl futures_util::stream::Stream<Item = Result<Event, Error>>> {
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    let mut senders = state.sse_senders.lock().await;
    senders.push(tx.clone());

    let event_stream =
        UnboundedReceiverStream::new(rx).map(|msg| Ok::<Event, Error>(Event::Data(Data::new(msg))));

    Sse::from_stream(event_stream).with_keep_alive(std::time::Duration::from_secs(15))
}

// --- Long Polling обработчик ---
pub async fn long_polling_handler(state: web::Data<AppState>) -> impl Responder {
    let id = state
        .lp_next_id
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let (tx, rx) = oneshot::channel::<String>();

    {
        let mut lp_senders = state.lp_senders.lock().await;
        lp_senders.push((id, tx));
    }

    let result = match timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(msg)) => msg,
        _ => {
            let mut lp_senders = state.lp_senders.lock().await;
            lp_senders.retain(|(sid, _)| *sid != id);
            "timeout".to_string()
        }
    };

    HttpResponse::Ok().json(result)
}
