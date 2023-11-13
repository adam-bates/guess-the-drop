use std::time::SystemTime;

use crate::{models::SessionAuth, AppState, Result};

use askama::Template;
use askama_axum::Response;
use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

pub fn add_routes(router: Router<AppState>) -> Router<AppState> {
    return router
        .route("/join", get(join))
        .route("/games/:game_code", get(game));
}

#[derive(Debug, Serialize, Deserialize)]
struct JoinParams {
    code: Option<String>,
}

#[derive(Template)]
#[template(path = "join.html")]
struct JoinTemplate {
    is_connected: bool,
    username: String,
    game_code: String,
}

async fn join(
    Query(params): Query<JoinParams>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let Some(game_code) = params.code else {
        return Ok(Redirect::to("/").into_response());
    };

    if game_code.trim().is_empty() {
        return Ok(Redirect::to("/").into_response());
    }

    let session_id = session.id().0.to_string();

    let user: Option<SessionAuth> =
        sqlx::query_as("SELECT * FROM session_auths WHERE sid = $1 LIMIT 1")
            .bind(&session_id)
            .fetch_optional(&state.db)
            .await?;

    if user.is_some() {
        return Ok(Redirect::to(&format!("/games/{}", game_code)).into_response());
    }

    return Ok(JoinTemplate {
        is_connected: false,
        username: String::new(),
        game_code,
    }
    .into_response());
}

#[derive(Template)]
#[template(path = "game.html")]
struct GameTemplate {
    is_connected: bool,
    username: String,
    game_code: String,
}

async fn game(
    Path(game_code): Path<String>,
    session: Session,
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
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
                return Ok(GameTemplate {
                    is_connected: true,
                    username: user.username,
                    game_code,
                });
            }
        }
    }

    return Ok(GameTemplate {
        is_connected: false,
        username: "todo".to_string(),
        game_code,
    });
}
