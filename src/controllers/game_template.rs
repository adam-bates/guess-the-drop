use super::utils;

use crate::{
    models::{GameTemplate, User},
    prelude::*,
};

use askama::Template;
use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use nanoid::nanoid;
use serde::Deserialize;
use tower_sessions::Session;

const MAX_TEMPLATES_PER_USER: usize = 100;

pub fn add_routes(router: Router<AppState>) -> Router<AppState> {
    return router
        .route("/game-templates", get(templates))
        .route("/game-templates/new", get(new_template))
        .route(
            "/game-templates/new/x/add-item",
            get(new_template_x_add_item),
        );
}

#[derive(Template)]
#[template(path = "game-templates.html")]
struct GameTemplatesTemplate {
    user: User,
    templates: Vec<GameTemplate>,
    can_create: bool,
}

async fn templates(session: Session, State(state): State<AppState>) -> Result<impl IntoResponse> {
    let sid = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &sid).await?.split();

    let mut game_templates: Vec<GameTemplate> =
        sqlx::query_as("SELECT * FROM game_templates WHERE user_id = ?")
            .bind(&user.user_id)
            .fetch_all(&state.db)
            .await?;

    let mut id = 1;

    for _ in 0..20 {
        game_templates.push(GameTemplate {
            game_template_id: id,
            user_id: user.user_id.clone(),

            name: nanoid!(),
            reward_message: None,
        });

        id += 1;
    }

    return Ok(GameTemplatesTemplate {
        can_create: game_templates.len() < MAX_TEMPLATES_PER_USER,

        user,
        templates: game_templates,
    });
}

#[derive(Template)]
#[template(path = "new-game-template.html")]
struct NewGameTemplateTemplate {
    user: User,
}

async fn new_template(
    session: Session,
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    let sid = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &sid).await?.split();

    return Ok(NewGameTemplateTemplate { user });
}

#[derive(Deserialize)]
struct NewGameTempateAddItemParams {
    idx: u32,
}

#[derive(Template)]
#[template(path = "new-game-template-add-item.html")]
struct NewGameTemplateAddItemTemplate {
    idx: u32,
}

async fn new_template_x_add_item(params: Query<NewGameTempateAddItemParams>) -> impl IntoResponse {
    return NewGameTemplateAddItemTemplate { idx: params.idx };
}
