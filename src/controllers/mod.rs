mod game;
mod twitch;
mod utils;

use std::collections::HashMap;

use crate::{models::SessionAuth, prelude::*};

use askama::Template;
use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use nanoid::nanoid;
use reqwest::StatusCode;
use tower_sessions::Session;

const KB: usize = 1024;
const MB: usize = 1024 * KB;

pub fn add_routes(router: Router<AppState>) -> Router<AppState> {
    let router = router
        .route("/upload", post(upload))
        .route_layer(DefaultBodyLimit::max(10 * MB));

    let router = game::add_routes(router);
    let router = twitch::add_routes(router);

    return router
        .route("/", get(index))
        .route("/health", get(|| async { StatusCode::NO_CONTENT }));
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    user: Option<SessionAuth>,
}

async fn index(session: Session, State(state): State<AppState>) -> Result<impl IntoResponse> {
    let session_id = utils::session_id(&session)?;

    let user: Option<SessionAuth> =
        sqlx::query_as("SELECT * FROM session_auths WHERE sid = ? LIMIT 1")
            .bind(&session_id)
            .fetch_optional(&state.db)
            .await?;

    return Ok(IndexTemplate { user });
}

async fn upload(State(state): State<AppState>, mut files: Multipart) -> Result<impl IntoResponse> {
    let Some(file) = files.next_field().await? else {
        return Ok(format!("Error fetching file"));
    };

    let category = file.name().unwrap().to_string();

    let name = match file.file_name() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => return Ok(format!("File name is required")),
    };

    let data = file.bytes().await?;
    if data.is_empty() {
        return Ok(format!("Empty file not allowed"));
    }

    let key = format!("{category}_{name}_{}", nanoid!());

    state.bucket.put_object(&key, &data).await?;

    return Ok(format!("{}/{}", state.cfg.r2_bucket_public_url, key));
}
