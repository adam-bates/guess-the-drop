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

use axum::{
    error_handling::HandleErrorLayer,
    extract::DefaultBodyLimit,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    Router, Server,
};
use headers::HeaderValue;
use lazy_static::lazy_static;
use nanoid::nanoid;
use pubsub::{HostAction, PlayerAction, PubSubClients};
use reqwest::{header::LOCATION, Method};
use s3::Bucket;
use sqlx::MySqlPool;
use tokio::sync::broadcast;
use tower::ServiceBuilder;
use tower_http::{
    compression,
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    timeout::TimeoutLayer,
};
use tower_sessions::{cookie::SameSite, Expiry, SessionManagerLayer};

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
    pub db: MySqlPool,
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
        // .with_max_level(tracing::Level::DEBUG)
        .init();

    let cfg = Arc::new(config::load()?);

    let bucket = init::s3::init_s3_bucket(&cfg)?;
    let db = init::db::init_mysql_pool(&cfg).await?;
    let session_store = init::session::init_session_store(&cfg, db.clone()).await?;
    let pubsub = init::pubsub::init_pubsub(&cfg).await?;
    let game_broadcasts: Arc<RwLock<HashMap<String, GameBroadcast>>> =
        Arc::new(RwLock::new(HashMap::new()));

    {
        let pubsub = pubsub.clone();
        let game_broadcasts = game_broadcasts.clone();

        tokio::spawn(async move {
            let mut stream = pubsub.host_actions.subscribe().await.unwrap();

            while let Some(value) = stream.next().await {
                if let Ok(game_broadcasts) = game_broadcasts.read() {
                    if let Some(broadcast) = game_broadcasts.get(&value.game_code) {
                        let _ = broadcast.to_players.send(value);
                    }
                }
            }
        });
    }

    {
        let pubsub = pubsub.clone();
        let game_broadcasts = game_broadcasts.clone();

        tokio::spawn(async move {
            let mut stream = pubsub.player_actions.subscribe().await.unwrap();

            while let Some(value) = stream.next().await {
                if let Ok(game_broadcasts) = game_broadcasts.read() {
                    if let Some(broadcast) = game_broadcasts.get(&value.game_code) {
                        let _ = broadcast.to_host.send(value);
                    }
                }
            }
        });
    }

    let addr = format!("[::]:{}", cfg.server_port.unwrap()).parse()?;

    let state = AppState {
        cfg,
        bucket,
        db,
        pubsub,
        game_broadcasts,
    };

    // TODO: Background job to clean up expired db & session records

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
