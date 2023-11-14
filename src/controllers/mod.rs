mod game;
mod twitch;
mod utils;

use crate::{models::SessionAuth, prelude::*};

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
    user: Option<SessionAuth>,
}

async fn index(session: Session, State(state): State<AppState>) -> Result<impl IntoResponse> {
    let session_id = utils::session_id(&session)?;

    let user: Option<SessionAuth> =
        sqlx::query_as("SELECT * FROM session_auths WHERE sid = ? LIMIT 1")
            .bind(&session_id)
            .fetch_optional(&state.db)
            .await?;

    // if let Some(user) = user {
    //     let now_s = SystemTime::now()
    //         .duration_since(SystemTime::UNIX_EPOCH)
    //         .unwrap()
    //         .as_secs();

    //     if user.expiry as u64 > now_s {
    //         return Ok(IndexTemplate { user: Some(user) });
    //     }
    // }

    return Ok(IndexTemplate { user });
}
