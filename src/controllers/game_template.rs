use std::{collections::HashMap, io::BufWriter};

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
        )
        .route(
            "/game-templates/new/x/post-total-msg",
            get(new_template_x_post_total_msg),
        )
        .route(
            "/game-templates/new/x/no-post-total-msg",
            get(new_template_x_no_post_total_msg),
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

    // let mut id = 1;

    // for _ in 0..20 {
    //     game_templates.push(GameTemplate {
    //         game_template_id: id,
    //         user_id: user.user_id.clone(),

    //         name: nanoid!(),
    //         reward_message: None,
    //         total_reward_message: None,
    //     });

    //     id += 1;
    // }

    return Ok(Html(GameTemplatesTemplate {
        can_create: game_templates.len() < MAX_TEMPLATES_PER_USER,

        user,
        templates: game_templates,
    }));
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

    return Ok(Html(NewGameTemplateTemplate { user }));
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
    return Html(NewGameTemplateAddItemTemplate { idx: params.idx });
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

    return Ok(Html(NewGameTemplatePostMsgTemplate {
        session: session_auth,
    }));
}

#[derive(Template)]
#[template(path = "new-game-template-no-post-msg.html")]
struct NewGameTemplateNoPostMsgTemplate {}

async fn new_template_x_no_post_msg() -> impl IntoResponse {
    return NewGameTemplateNoPostMsgTemplate {};
}

#[derive(Template)]
#[template(path = "new-game-template-post-total-msg.html")]
struct NewGameTemplatePostTotalMsgTemplate {
    session: SessionAuth,
}

async fn new_template_x_post_total_msg(
    session: Session,
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    let sid = utils::session_id(&session)?;
    let (_, session_auth) = utils::require_user(&state, &sid).await?.split();

    return Ok(Html(NewGameTemplatePostTotalMsgTemplate {
        session: session_auth,
    }));
}

#[derive(Template)]
#[template(path = "new-game-template-no-post-msg.html")]
struct NewGameTemplateNoPostTotalMsgTemplate {}

async fn new_template_x_no_post_total_msg() -> impl IntoResponse {
    return NewGameTemplateNoPostTotalMsgTemplate {};
}

async fn post_template(
    session: Session,
    State(state): State<AppState>,
    mut form: Multipart,
) -> Result<impl IntoResponse> {
    let sid = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &sid).await?.split();

    let mut name = None;

    let mut should_post = None;
    let mut post_msg = None;

    let mut should_post_total = None;
    let mut post_total_msg = None;

    let mut items = HashMap::new();

    while let Some(field) = form.next_field().await? {
        match field.name() {
            Some("name") => {
                let txt = field.text().await?;
                let txt = txt.trim();

                if txt.is_empty() {
                    return Err(anyhow::anyhow!("Template name cannot be blank"))?;
                }

                name = Some(txt.to_string());
            }

            Some("should-post") => match field.bytes().await?.as_ref() {
                b"on" => should_post = Some(true),
                _ => should_post = Some(false),
            },
            Some("post-msg") => {
                let txt = field.text().await?;
                let txt = txt.trim();

                if txt.is_empty() {
                    return Err(anyhow::anyhow!("Chat message cannot be blank"))?;
                }

                post_msg = Some(txt.to_string());
            }

            Some("should-post-total") => match field.bytes().await?.as_ref() {
                b"on" => should_post_total = Some(true),
                _ => should_post_total = Some(false),
            },
            Some("post-total-msg") => {
                let txt = field.text().await?;
                let txt = txt.trim();

                if txt.is_empty() {
                    return Err(anyhow::anyhow!("Chat message cannot be blank"))?;
                }

                post_total_msg = Some(txt.to_string());
            }

            Some(item_field_name) if item_field_name.starts_with("items[") => {
                let Some(close_idx) = item_field_name.find(']') else {
                    continue;
                };

                let idx: usize = item_field_name[6..close_idx].parse()?;

                let (item_name, item_image, start_enabled) =
                    items.entry(idx).or_insert((None, None, None));

                match &item_field_name[(close_idx + 2)..] {
                    "name" => {
                        *item_name = Some(field.text().await?.trim().to_string());
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
                    "start_enabled" => {
                        *start_enabled = match field.bytes().await?.as_ref() {
                            b"on" => Some(true),
                            _ => None,
                        };
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

    let reward_message = should_post.map(|_| post_msg).flatten();
    let total_reward_message = should_post_total.map(|_| post_total_msg).flatten();

    let items = {
        let mut list = vec![];

        let mut keys: Vec<usize> = items.keys().cloned().collect();
        keys.sort();

        for key in keys {
            let (name, img, start_enabled) = items.remove(&key).unwrap();

            let Some(name) = name else {
                return Err(anyhow::anyhow!("Item {} has no name", key + 1))?;
            };

            list.push((name, img, start_enabled == Some(true)));
        }

        list
    };

    let items = {
        let mut list = vec![];

        for (name, img, start_enabled) in items {
            let img = if let Some((filename, bytes)) = img {
                let img = image::load_from_memory(&bytes)?;

                const MAX_IMG_SIZE: u32 = 256;
                if img.width() > MAX_IMG_SIZE || img.height() > MAX_IMG_SIZE {
                    img.resize(
                        MAX_IMG_SIZE,
                        MAX_IMG_SIZE,
                        image::imageops::FilterType::Nearest,
                    );
                }

                let mut data = vec![0u8; 0];
                img.write_with_encoder(image::codecs::png::PngEncoder::new(BufWriter::new(
                    &mut data,
                )))?;

                let key = format!("item_{}_{filename}", nanoid!());

                state.bucket.put_object(&key, &data).await?;

                Some(format!("{}/{key}", state.cfg.r2_bucket_public_url))
            } else {
                None
            };

            list.push((name, img, start_enabled));
        }

        list
    };

    sqlx::query("INSERT INTO game_templates (user_id, name, reward_message, total_reward_message) VALUES (?, ?, ?, ?)")
        .bind(&user.user_id)
        .bind(&name)
        .bind(&reward_message)
        .bind(&total_reward_message)
        .execute(&state.db)
        .await?;

    let record: GameTemplate =
        sqlx::query_as("SELECT * FROM game_templates WHERE user_id = ? AND name = ? LIMIT 1")
            .bind(&user.user_id)
            .bind(&name)
            .fetch_one(&state.db)
            .await?;

    if !items.is_empty() {
        let query = format!(
            "INSERT INTO game_item_templates (game_template_id, name, image, start_enabled) VALUES {}",
            items.iter().map(|_| "(?, ?, ?, ?)").collect::<Vec<&'static str>>().join(",")
        );

        let mut q = sqlx::query(&query);

        for (name, img, start_enabled) in items {
            q = q
                .bind(&record.game_template_id)
                .bind(name)
                .bind(img)
                .bind(start_enabled);
        }

        q.execute(&state.db).await?;
    }

    return Ok("ok");
}
