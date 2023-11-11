mod game;
mod twitch;

use crate::AppState;

use askama::Template;
use axum::{response::IntoResponse, routing::get, Router};

pub fn add_routes(router: Router<AppState>) -> Router<AppState> {
    let router = game::add_routes(router);
    let router = twitch::add_routes(router);

    return router.route("/", get(index));
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {}

async fn index() -> impl IntoResponse {
    return IndexTemplate {};
}
