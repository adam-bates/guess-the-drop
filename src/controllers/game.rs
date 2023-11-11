use crate::AppState;

use askama::Template;
use askama_axum::Response;
use axum::{
    extract::{Path, Query},
    response::{IntoResponse, Redirect},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};

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
    game_code: String,
}

async fn join(Query(params): Query<JoinParams>) -> Response {
    let Some(game_code) = params.code else {
        return Redirect::to("/").into_response();
    };

    if game_code.trim().is_empty() {
        return Redirect::to("/").into_response();
    }

    return JoinTemplate { game_code }.into_response();
}

#[derive(Template)]
#[template(path = "game.html")]
struct GameTemplate {
    game_code: String,
    twitch_name: String,
}

async fn game(Path(game_code): Path<String>) -> impl IntoResponse {
    return GameTemplate {
        game_code,
        twitch_name: "todo".to_string(),
    };
}
