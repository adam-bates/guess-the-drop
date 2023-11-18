use super::utils;

use crate::{
    models::{GameTemplate, User},
    prelude::*,
};

use askama::Template;
use axum::{extract::State, response::IntoResponse, routing::get, Router};
use tower_sessions::Session;

pub fn add_routes(router: Router<AppState>) -> Router<AppState> {
    return router.route("/game-templates", get(templates));
}

#[derive(Template)]
#[template(path = "game-templates.html")]
struct GameTemplatesTemplate {
    user: User,
    templates: Vec<GameTemplate>,
}

async fn templates(session: Session, State(state): State<AppState>) -> Result<impl IntoResponse> {
    let sid = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &sid).await?.split();

    return Ok(GameTemplatesTemplate {
        user,
        templates: vec![GameTemplate {
            game_template_id: 0,
            user_id: 0,

            name: "Zelda OOT Rando".to_string(),
            reward_message: None,
        }],
    });
}
