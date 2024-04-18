mod config;
mod controllers;
mod init;
mod models;
mod pubsub;
mod result;

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use async_trait::async_trait;
use axum::{
    error_handling::HandleErrorLayer,
    extract::DefaultBodyLimit,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    Router, Server,
};
use chrono::DateTime;
use lazy_static::lazy_static;
use models::{ChatMessage, SessionAuth, User};
use nanoid::nanoid;
use pubsub::{HostAction, PlayerAction, PubSubClients};
use reqwest::{header::LOCATION, Method};
use result::AppError;
use s3::Bucket;
use sqlx::PgPool;
use tokio::sync::broadcast;
use tower::ServiceBuilder;
use tower_http::{
    compression,
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    timeout::TimeoutLayer,
};
use tower_sessions::{cookie::SameSite, Expiry, SessionManagerLayer};
use twitch_irc::{
    login::{RefreshingLoginCredentials, UserAccessToken},
    TwitchIRCClient,
};

pub mod prelude {
    pub use crate::config::Config;
    pub use crate::result::Result;

    pub use crate::AppState;

    pub use crate::controllers::Html;

    pub use crate::EXECUTION_ID;
}
use prelude::*;

lazy_static! {
    pub static ref EXECUTION_ID: String = {
        if cfg!(debug_assertions) {
            "local".to_string()
        } else {
            const ALPHA_NUM: [char; 62] = [
                'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p',
                'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F',
                'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V',
                'W', 'X', 'Y', 'Z', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
            ];
            nanoid!(16, &ALPHA_NUM)
        }
    };
}

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub bucket: Bucket,
    pub db: PgPool,
    pub pubsub: Arc<PubSubClients>,
    pub game_broadcasts: Arc<RwLock<HashMap<String, GameBroadcast>>>,
}

#[derive(Clone)]
pub struct GameBroadcast {
    pub to_host: broadcast::Sender<PlayerAction>,
    pub to_players: broadcast::Sender<HostAction>,
}

#[tokio::main]
async fn main() -> Result {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let cfg = Arc::new(config::load()?);

    let bucket = init::s3::init_s3_bucket(&cfg)?;
    let db = init::db::init_pg_pool(&cfg).await?;
    let session_store = init::session::init_session_store(&cfg, db.clone()).await?;
    let pubsub = init::pubsub::init_pubsub(&cfg).await?;
    let game_broadcasts: Arc<RwLock<HashMap<String, GameBroadcast>>> =
        Arc::new(RwLock::new(HashMap::new()));

    // {
    //     let pubsub = pubsub.clone();
    //     let game_broadcasts = game_broadcasts.clone();

    //     tokio::spawn(async move {
    //         let mut stream = pubsub.host_actions.subscribe().await.unwrap();

    //         while let Some(value) = stream.next().await {
    //             if let Ok(game_broadcasts) = game_broadcasts.read() {
    //                 if let Some(broadcast) = game_broadcasts.get(&value.game_code) {
    //                     let _ = broadcast.to_players.send(value);
    //                 }
    //             }
    //         }
    //     });
    // }

    // {
    //     let pubsub = pubsub.clone();
    //     let game_broadcasts = game_broadcasts.clone();

    //     tokio::spawn(async move {
    //         let mut stream = pubsub.player_actions.subscribe().await.unwrap();

    //         while let Some(value) = stream.next().await {
    //             if let Ok(game_broadcasts) = game_broadcasts.read() {
    //                 if let Some(broadcast) = game_broadcasts.get(&value.game_code) {
    //                     let _ = broadcast.to_host.send(value);
    //                 }
    //             }
    //         }
    //     });
    // }

    // {
    //     let cfg = cfg.clone();
    //     let db = db.clone();

    //     tokio::spawn(async move {
    //         loop {
    //             // println!("Sending messages ...");

    //             match send_chat_messages(cfg.clone(), db.clone()).await {
    //                 Ok(()) => {}

    //                 Err(e) => {
    //                     dbg!(&e);
    //                 }
    //             }
    //         }
    //     });
    // }

    let addr = format!("0.0.0.0:{}", cfg.server_port.unwrap()).parse()?;

    let state = AppState {
        cfg,
        bucket,
        db,
        pubsub,
        game_broadcasts,
    };

    let session_service = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|_| async {
            return StatusCode::BAD_REQUEST;
        }))
        .layer(
            SessionManagerLayer::new(session_store)
                .with_name("snowy.sid")
                .with_domain(state.cfg.server_domain.to_string())
                .with_expiry(Expiry::OnSessionEnd)
                .with_secure(false)
                .with_same_site(SameSite::Lax),
        );

    let router = Router::new();

    // dynamic paths
    let router = controllers::add_routes(router);

    // static assets
    let router = router
        .route_service("/favicon.ico", ServeFile::new("assets/favicon.ico"))
        .nest_service("/assets", ServeDir::new("assets"));

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_origin([state.cfg.server_host_uri.parse().unwrap()]);

    const KB: usize = 1024;
    const MB: usize = 1024 * KB;

    let router = router
        .with_state(state)
        .layer(DefaultBodyLimit::max(64 * MB))
        .layer(session_service)
        .layer(cors)
        .layer(
            compression::CompressionLayer::new().compress_when(CustomCompression {
                fallback: compression::DefaultPredicate::new(),
            }),
        )
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        .layer(middleware::from_fn(add_redirect_header));

    Server::bind(&addr)
        .serve(router.into_make_service())
        .await?;

    return Ok(());
}

#[derive(Clone)]
struct CustomCompression {
    fallback: compression::DefaultPredicate,
}

impl compression::Predicate for CustomCompression {
    fn should_compress<B>(&self, response: &axum::http::Response<B>) -> bool
    where
        B: axum::body::HttpBody,
    {
        if let Some(value) = response.headers().get("content-type") {
            if let Ok(value) = value.to_str() {
                if value == "text/event-stream" {
                    return false;
                }
            }
        }

        return self.fallback.should_compress(response);
    }
}

async fn add_redirect_header<B>(req: Request<B>, next: Next<B>) -> Response {
    let is_hx = if let Some(val) = req.headers().get("HX-Request") {
        matches!(val.to_str(), Ok("true"))
    } else {
        false
    };

    // if is_hx && cfg!(debug_assertions) {
    //     tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    // }

    let mut res = next.run(req).await;

    if is_hx && res.status().is_redirection() {
        let headers = res.headers_mut();

        let Some(dest) = headers.remove(LOCATION) else {
            return res;
        };

        headers.insert("HX-Redirect", dest.clone());
        headers.insert("HX-Replace-Url", dest);

        let status = res.status_mut();
        *status = StatusCode::OK;
    }

    return res;
}

async fn send_chat_messages(cfg: Arc<Config>, db: PgPool) -> Result {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM chat_messages WHERE lock_id IS NULL AND sent = false",
    )
    .fetch_one(&db)
    .await?;

    if count == 0 {
        return Ok(());
    }

    let lock_id = nanoid!(64);

    sqlx::query("UPDATE chat_messages SET lock_id = ? WHERE lock_id IS NULL AND sent = false")
        .bind(&lock_id)
        .execute(&db)
        .await?;

    let chat_messages: Vec<ChatMessage> =
        sqlx::query_as("SELECT * FROM chat_messages WHERE lock_id = ?")
            .bind(&lock_id)
            .fetch_all(&db)
            .await?;

    if chat_messages.is_empty() {
        return Ok(());
    }

    let mut game_code_messages: HashMap<String, Vec<ChatMessage>> = chat_messages
        .iter()
        .map(|m| (m.game_code.clone(), vec![]))
        .collect();

    for chat_message in chat_messages {
        if let Some(messages) = game_code_messages.get_mut(&chat_message.game_code) {
            messages.push(chat_message);
        }
    }

    let q = format!(
        "SELECT game_code, user_id FROM games WHERE game_code IN ({})",
        game_code_messages
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut q = sqlx::query_as(&q);
    for (game_code, _) in &game_code_messages {
        q = q.bind(game_code);
    }
    let game_users: Vec<(String, String)> = q.fetch_all(&db).await?;

    let mut user_messages: HashMap<String, Vec<ChatMessage>> = HashMap::new();
    for (game_code, user_id) in game_users {
        let messages = user_messages.entry(user_id).or_insert(vec![]);
        if let Some(to_add) = game_code_messages.remove(&game_code) {
            messages.extend(to_add);
        }
    }

    for (user_id, chat_messages) in user_messages {
        let db = db.clone();
        let cfg = cfg.clone();

        tokio::spawn(async move {
            let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE user_id = ?")
                .bind(&user_id)
                .fetch_optional(&db)
                .await
                .unwrap();

            let Some(user) = user else {
                eprintln!("Couldn't find user with id: {user_id}");
                return;
            };

            let client_config = twitch_irc::ClientConfig::new_simple(
                RefreshingLoginCredentials::init_with_username(
                    Some(user.twitch_login.clone()),
                    cfg.twitch_client_id.to_string(),
                    cfg.twitch_client_secret.secret().to_string(),
                    DbTokenStorage {
                        user_id: user.user_id.clone(),
                        db: db.clone(),
                        cfg: cfg.clone(),
                    },
                ),
            );

            let (_rx, client) =
                TwitchIRCClient::<twitch_irc::SecureTCPTransport, _>::new(client_config);

            client.join(user.twitch_login.to_string()).unwrap();

            let mut sent = vec![];

            for chat_message in &chat_messages {
                match client
                    .say(user.twitch_login.to_string(), chat_message.message.clone())
                    .await
                {
                    Ok(_) => sent.push(chat_message),

                    Err(e) => {
                        dbg!(e);
                    }
                }

                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }

            let q = format!(
                "UPDATE chat_messages SET lock_id = NULL, sent = true WHERE id IN ({})",
                chat_messages
                    .iter()
                    .map(|_| "?")
                    .collect::<Vec<_>>()
                    .join(", ")
            );

            let mut q = sqlx::query(&q);

            for chat_message in &chat_messages {
                q = q.bind(chat_message.id);
            }

            q.execute(&db).await.unwrap();
        });
    }

    return Ok(());
}

#[derive(Debug)]
struct DbTokenStorage {
    user_id: String,
    db: PgPool,
    cfg: Arc<Config>,
}

#[async_trait]
impl twitch_irc::login::TokenStorage for DbTokenStorage {
    type LoadError = AppError;
    type UpdateError = AppError;

    // Load the currently stored token from the storage.
    async fn load_token(&mut self) -> Result<UserAccessToken> {
        let session: Option<SessionAuth> = sqlx::query_as(
            "SELECT * FROM session_auths WHERE user_id = ? AND client_id = ? AND can_chat = true",
        )
        .bind(&self.user_id)
        .bind(self.cfg.twitch_client_id.as_str())
        .fetch_optional(&self.db)
        .await?;

        let Some(session) = session else {
            return Err(AppError(anyhow::anyhow!(
                "Couldn't find valid session for user id: {}",
                self.user_id
            )))?;
        };

        let created_at = DateTime::from_timestamp(session.created_at as i64, 0).unwrap();
        let expires_at = DateTime::from_timestamp(session.expiry as i64, 0);

        return Ok(UserAccessToken {
            access_token: session.access_token,
            refresh_token: session.refresh_token,
            created_at,
            expires_at,
        });
    }

    // Called after the token was updated successfully, to save the new token.
    // After `update_token()` completes, the `load_token()` method should then return
    // that token for future invocations
    async fn update_token(&mut self, token: &UserAccessToken) -> Result {
        sqlx::query("UPDATE session_auths SET access_token = ?, refresh_token = ?, created_at = ?, expiry = ? WHERE user_id = ? AND client_id = ? AND can_chat = true")
            .bind(&token.access_token)
            .bind(&token.refresh_token)
            .bind(token.created_at.timestamp() as i64)
            .bind(token.expires_at.map(|e| e.timestamp() as i64))
            .bind(&self.user_id)
            .bind(self.cfg.twitch_client_id.as_str())
            .execute(&self.db)
            .await?;

        return Ok(());
    }
}
