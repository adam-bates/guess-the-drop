mod config;
mod controllers;
mod models;
mod result;
mod sessions;

use prelude::*;

use std::{sync::Arc, time::Duration};

use axum::{error_handling::HandleErrorLayer, http::StatusCode, Router, Server};
use reqwest::Method;
use sqlx::MySqlPool;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    timeout::TimeoutLayer,
};
use tower_sessions::{cookie::SameSite, Expiry, SessionManagerLayer};

pub mod prelude {
    pub use crate::config::Config;
    pub use crate::result::Result;

    pub use crate::AppState;
}

#[derive(Clone)]
pub struct AppState {
    cfg: Arc<Config>,
    db: MySqlPool,
}

#[tokio::main]
async fn main() -> Result {
    let cfg = config::load()?;

    let db = MySqlPool::connect(&cfg.db_connection_url).await?;
    sqlx::migrate!().run(&db).await?;

    let session_store = sessions::store::build(&cfg, db.clone()).await?;

    let addr = format!("0.0.0.0:{}", cfg.server_port.unwrap()).parse()?;

    let state = AppState {
        cfg: Arc::new(cfg),
        db,
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

    let router = router
        .with_state(state)
        .layer(session_service)
        .layer(cors)
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::new(Duration::from_secs(30)));

    Server::bind(&addr)
        .serve(router.into_make_service())
        .await?;

    return Ok(());
}
