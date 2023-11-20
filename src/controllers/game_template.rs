use std::collections::HashMap;

use super::utils;

use crate::{
    models::{GameItemTemplate, GameTemplate, SessionAuth, User},
    prelude::*,
};

use askama::Template;
use axum::{
    extract::{Multipart, Query, State},
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
        .route("/game-templates", get(templates).post(post_template))
        .route("/game-templates/new", get(new_template))
        .route(
            "/game-templates/new/x/add-item",
            get(new_template_x_add_item),
        )
        .route(
            "/game-templates/new/x/post-msg",
            get(new_template_x_post_msg),
        )
        .route(
            "/game-templates/new/x/no-post-msg",
            get(new_template_x_no_post_msg),
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

#[derive(Template)]
#[template(path = "new-game-template-post-msg.html")]
struct NewGameTemplatePostMsgTemplate {
    session: SessionAuth,
}

async fn new_template_x_post_msg(
    session: Session,
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    let sid = utils::session_id(&session)?;
    let (_, session_auth) = utils::require_user(&state, &sid).await?.split();

    return Ok(NewGameTemplatePostMsgTemplate {
        session: session_auth,
    });
}

#[derive(Template)]
#[template(path = "new-game-template-no-post-msg.html")]
struct NewGameTemplateNoPostMsgTemplate {}

async fn new_template_x_no_post_msg() -> impl IntoResponse {
    return NewGameTemplateNoPostMsgTemplate {};
}

async fn post_template(mut form: Multipart) -> Result<impl IntoResponse> {
    let mut name = None;
    let mut items = HashMap::new();

    while let Some(field) = form.next_field().await? {
        match field.name() {
            Some("name") => {
                let txt = field.text().await?;
                if txt.trim().is_empty() {
                    return Err(anyhow::anyhow!("Template name cannot be blank"))?;
                }

                name = Some(txt);
            }

            Some(item_field_name) if item_field_name.starts_with("items[") => {
                let Some(close_idx) = item_field_name.find(']') else {
                    continue;
                };

                let idx: usize = item_field_name[6..close_idx].parse()?;

                let (item_name, item_image) = items.entry(idx).or_insert((None, None));

                match &item_field_name[(close_idx + 2)..] {
                    "name" => {
                        *item_name = Some(field.text().await?.to_string());
                    }
                    "image" => {
                        match field.content_type() {
                            Some(ct) if ct.starts_with("image/") => {}

                            // ignore
                            _ => continue,
                        }

                        let file_name = match field.file_name() {
                            Some(file_name) if !file_name.trim().is_empty() => {
                                file_name.trim().to_string()
                            }

                            // ignore
                            _ => continue,
                        };

                        let bytes = match field.bytes().await {
                            Ok(bytes) if !bytes.is_empty() => bytes,

                            // ignore
                            _ => continue,
                        };

                        *item_image = Some((file_name, bytes));
                    }

                    // ignore
                    _ => {}
                }
            }

            // ignore
            _ => {}
        }
    }

    let Some(name) = name else {
        return Err(anyhow::anyhow!("Template name is required"))?;
    };

    let template = GameTemplate {
        game_template_id: 0,
        user_id: nanoid!(),

        name,
        reward_message: None,
    };

    let items = {
        let mut list = vec![];

        let mut keys: Vec<usize> = items.keys().cloned().collect();
        keys.sort();

        for key in keys {
            let (name, image) = items.remove(&key).unwrap();

            let Some(name) = name else {
                return Err(anyhow::anyhow!("Item {} has no name", key + 1))?;
            };

            list.push(GameItemTemplate {
                game_item_template_id: 0,
                game_template_id: 0,

                name,
                image: image.map(|x| x.0), // TODO: upload
                start_enabled: true,
            });
        }

        list
    };

    // TODO: upload images, save to DB
    dbg!(&template, &items);

    return Ok("ok");
}
