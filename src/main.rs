mod config;
mod controllers;
mod models;
mod result;
mod sessions;

pub use crate::result::Result;

use std::sync::Arc;

use axum::{error_handling::HandleErrorLayer, http::StatusCode, Router};
use shuttle_axum::AxumService;
use shuttle_secrets::SecretStore;
use sqlx::PgPool;
use tower::ServiceBuilder;
use tower_http::services::{ServeDir, ServeFile};
use tower_sessions::{cookie::SameSite, Expiry, SessionManagerLayer};

#[derive(Clone)]
pub struct AppState {
    cfg: Arc<config::Config>,
    db: PgPool,
}

#[shuttle_runtime::main]
async fn main(
    #[shuttle_secrets::Secrets] secrets: SecretStore,
    #[shuttle_shared_db::Postgres] db: PgPool,
) -> shuttle_axum::ShuttleAxum {
    return run(secrets, db)
        .await
        .map_err(|e| shuttle_runtime::Error::Custom(e.0));
}

async fn run(secrets: SecretStore, db: PgPool) -> Result<AxumService> {
    let cfg = config::build(secrets);

    sqlx::migrate!().run(&db).await?;

    let session_store = sessions::store::build(db.clone()).await?;

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

    let router = router.with_state(state).layer(session_service);

    return Ok(router.into());
}
