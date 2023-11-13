mod game;
mod twitch;

use std::time::SystemTime;

use crate::{models::SessionAuth, AppState, Result};

use askama::Template;
use axum::{extract::State, response::IntoResponse, routing::get, Router};
use tower_sessions::Session;

pub fn add_routes(router: Router<AppState>) -> Router<AppState> {
    let router = game::add_routes(router);
    let router = twitch::add_routes(router);

    return router.route("/", get(index));
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    is_connected: bool,
    username: String,
}

async fn index(session: Session, State(state): State<AppState>) -> Result<impl IntoResponse> {
    let session_id = session.id().0.to_string();

    let user: Option<SessionAuth> =
        sqlx::query_as("SELECT * FROM session_auths WHERE sid = $1 LIMIT 1")
            .bind(&session_id)
            .fetch_optional(&state.db)
            .await?;

    if let Some(user) = user {
        if let Some(expiry_s) = user.expiry {
            let now_s = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if expiry_s as u64 > now_s {
                return Ok(IndexTemplate {
                    is_connected: true,
                    username: user.username,
                });
            }
        }
    }

    return Ok(IndexTemplate {
        is_connected: false,
        username: String::new(),
    });
}
