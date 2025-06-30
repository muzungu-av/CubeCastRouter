use crate::validator::extractor::ValidatedJson;
use crate::validator::message::IncomingMessage;
use crate::{ws::broadcast::GetCount, AppState, BroadcastServer, ROOM_CONFIG};
use actix::prelude::*;
use actix_web::{web, HttpResponse, Responder};
use serde::Serialize;

/// Информация о том, кто запрашивает или отдаёт сообщение.
/// Для ответа (USER_LIST) мы могли бы, например,
/// указать, что это сервер (id="server", type="server").
#[derive(Serialize)]
struct SenderInfo {
    id: String,
    #[serde(rename = "type")]
    role: String, // Чтобы в JSON получилось "type": "server"
}

/// Описание одного пользователя в списке.
#[derive(Serialize)]
struct UserInfo {
    id: String,
    #[serde(rename = "type")]
    role: String, // "учитель" | "ученик" | "наблюдатель"
    connection: String, // "ws" | "sse" | "long_polling"
}

#[derive(Serialize)]
struct ConnectionsCount {
    ws: usize,
    sse: usize,
    lp: usize,
}

/// Поле payload для ответа «список пользователей».
#[derive(Serialize)]
pub struct UserListPayload {
    users: Vec<UserInfo>,
    counts: ConnectionsCount,
}

/// Общая обёртка для сообщения по протоколу
#[derive(Serialize)]
struct UserListMessage {
    room_id: String,
    sender: SenderInfo,
    #[serde(rename = "command")]
    msg_command: String, // "GET_USER_LIST"
    payload: UserListPayload,
}

// Возвращает количество подписчиков по типам: WebSocket, SSE и Long Polling
/// Строит структуру UserListMessage (без HTTP-обёртки).
async fn build_user_list(state: &AppState, srv: &Addr<BroadcastServer>) -> UserListMessage {
    // получаем количество WebSocket-подписчиков
    let ws_count = srv.send(GetCount).await.unwrap_or(0);
    // получаем количество SSE-подписчиков
    let sse_count = state.sse_senders.lock().await.len();
    // получаем количество Long Polling-подписчиков
    let lp_count = state.lp_senders.lock().await.len();

    let counts = ConnectionsCount {
        ws: ws_count,
        sse: sse_count,
        lp: lp_count,
    };
    // TODO: собрать список пользователей
    let users: Vec<UserInfo> = Vec::new();

    UserListMessage {
        room_id: ROOM_CONFIG.room.clone(),
        sender: SenderInfo {
            id: "server".into(),
            role: "server".into(),
        },
        msg_command: "GET_USER_LIST".into(),
        payload: UserListPayload { users, counts },
    }
}

/// POST /wathing_users: строит UserListMessage, отправляет подписчикам и возвращает его.
pub async fn get_users_list(
    state: web::Data<AppState>,
    srv: web::Data<Addr<BroadcastServer>>,
    _msg: ValidatedJson<IncomingMessage>,
) -> impl Responder {
    // _msg.0 — уже валидный IncomingMessage
    //  srv.get_ref().do_send(ClientMessage(_msg.0.clone()));
    let message = build_user_list(&state, &srv).await;
    HttpResponse::Ok().json(message)
}
