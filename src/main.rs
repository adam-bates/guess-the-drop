mod config;
mod controllers;
mod models;
mod result;
mod sessions;

use std::{sync::Arc, time::Duration};

use axum::{
    error_handling::HandleErrorLayer, extract::DefaultBodyLimit, http::StatusCode, Router, Server,
};
use reqwest::Method;
use s3::{creds::Credentials, Bucket, Region};
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

    pub use crate::controllers::Html;
}
use prelude::*;

#[derive(Clone)]
pub struct AppState {
    cfg: Arc<Config>,
    bucket: Bucket,
    db: MySqlPool,
}

#[tokio::main]
async fn main() -> Result {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let cfg = Arc::new(config::load()?);

    let bucket = s3_bucket(&cfg)?;

    let db = MySqlPool::connect(&cfg.db_connection_url).await?;
    sqlx::migrate!().run(&db).await?;

    let session_store = sessions::store::build(&cfg, db.clone()).await?;

    let addr = format!("0.0.0.0:{}", cfg.server_port.unwrap()).parse()?;

    let state = AppState { cfg, bucket, db };

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
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::new(Duration::from_secs(30)));

    Server::bind(&addr)
        .serve(router.into_make_service())
        .await?;

    return Ok(());
}

fn s3_bucket(cfg: &Config) -> Result<Bucket> {
    return Ok(Bucket::new(
        &cfg.r2_bucket,
        Region::R2 {
            account_id: cfg.r2_account_id.clone(),
        },
        Credentials::new(
            Some(&cfg.r2_s3_access_key_id),
            Some(&cfg.r2_s3_secret_access_key),
            None,
            None,
            None,
        )?,
    )?
    .with_path_style());
}
