mod config;
mod http;
mod users_list;
mod ws;
mod validator {
    pub mod extractor;
    pub mod message;
}

use actix::prelude::*;
use actix_cors::Cors;
use actix_web::http::header;
use actix_web::{web, App, HttpServer};
use config::RoomConfig;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::{mpsc, oneshot};
use ws::broadcast::{BroadcastServer, ClientMessage};
use ws::route::ws_route;

/// файл с конфигом
static ROOM_CONFIG: Lazy<RoomConfig> =
    Lazy::new(|| RoomConfig::load_from_file("files/example_room.room"));

// // --- Сообщение от клиента ---
// --- Состояние приложения ---
struct AppState {
    pub ws_subs: Arc<Mutex<Vec<(String, Recipient<ClientMessage>)>>>,
    sse_senders: Arc<Mutex<Vec<(String, mpsc::UnboundedSender<String>)>>>,
    /// Для каждого LP‑запроса: (sender_id, oneshot::Sender<String>)
    pub lp_senders: Arc<Mutex<Vec<(String, oneshot::Sender<String>)>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            ws_subs: Arc::new(Mutex::new(Vec::new())),
            sse_senders: Arc::new(Mutex::new(Vec::new())),
            lp_senders: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

// --- main ---
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // **В этом месте CONFIG ещё не считывается, он будет лениво инициализирован при первом обращении.**
    // Чтобы заставить прочитать и распарсить конфиг сразу, нужно логгировать:
    println!(
        "Loaded config: room={}, teacher={}, sign_key={}, authorised_students={:?}",
        ROOM_CONFIG.room,
        ROOM_CONFIG.teacher,
        ROOM_CONFIG.sign_key,
        &ROOM_CONFIG.authorised_students
    );

    // 1) Создаём AppState и оборачиваем в web::Data
    let state = web::Data::new(AppState::new());

    // 2) Запускаем актор BroadcastServer, передавая ему AppState
    let srv = BroadcastServer::new(state.clone()).start();

    // 3) Упаковываем Addr<BroadcastServer> в web::Data
    let srv_data = web::Data::new(srv.clone());

    // 4) Создание и запуск HttpServer
    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .app_data(srv_data.clone())
            .wrap(
                Cors::default()
                    // Разрешить любые источники (в dev‑режиме; в проде лучше ужесточить)
                    .allow_any_origin()
                    // Разрешаем нужные HTTP‑методы
                    .allowed_methods(vec!["GET", "POST", "OPTIONS"])
                    // Разрешаем заголовок Content-Type
                    .allowed_header(header::CONTENT_TYPE)
                    // Если нужны другие кастомные заголовки, добавь их здесь:
                    // .allowed_header(header::AUTHORIZATION)
                    // Не забудь ответить на preflight-запросы
                    .supports_credentials() // если используешь куки/авторизацию
                    // TTL для preflight (в секундах)
                    .max_age(3600),
            )
            .route("/ws", web::get().to(ws_route))
            .route("/sse", web::get().to(http::sse_handler))
            .route("/lp", web::post().to(http::long_polling_handler))
            .route("/send", web::post().to(http::send_handler))
            .route("/wathing_users", web::post().to(users_list::get_users_list))
    })
    .bind(("127.0.0.1", 7070))?
    .run()
    .await
}

// и чтобы запрос GetWsClients возвращал Vec<String> с ID активных WS-юзеров.

//todo проверить что сообщения не рассылаются самому себе по SSE  (LP WS  вроде сделано)
//todo для sse_handler нет POST body данных для аутентификации
//todo аутентификация (может быть и подпись signed сообщений)
//todo  отправляемые команды нужно по всем каналам валидировать тоже (какие бывают) и исполнять (какой-то паттерн)
//todo накопление сообщений
