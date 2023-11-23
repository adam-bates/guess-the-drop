use std::{collections::HashMap, io::BufWriter};

use super::utils;

use crate::{
    models::{GameItemTemplate, GameTemplate, SessionAuth, User},
    prelude::*,
};

use askama::Template;
use askama_axum::Response;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Redirect},
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
        )
        .route(
            "/game-templates/:id",
            get(edit_template).put(put_template).delete(delete_template),
        )
        .route(
            "/game-templates/:id/x/add-item",
            get(edit_template_x_add_item),
        )
        .route(
            "/game-templates/:id/x/post-msg",
            get(edit_template_x_post_msg),
        )
        .route(
            "/game-templates/:id/x/no-post-msg",
            get(edit_template_x_no_post_msg),
        )
        .route(
            "/game-templates/:id/x/post-total-msg",
            get(edit_template_x_post_total_msg),
        )
        .route(
            "/game-templates/:id/x/no-post-total-msg",
            get(edit_template_x_no_post_total_msg),
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

    let game_templates: Vec<GameTemplate> =
        sqlx::query_as("SELECT * FROM game_templates WHERE user_id = ?")
            .bind(&user.user_id)
            .fetch_all(&state.db)
            .await?;

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

#[derive(Template)]
#[template(path = "edit-game-template.html")]
struct EditGameTemplateTemplate {
    session: SessionAuth,
    user: User,
    template: GameTemplate,
    items: Vec<(usize, GameItemTemplate)>,
    img_base_uri: String,
}

async fn edit_template(
    Path(id): Path<u64>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let sid = utils::session_id(&session)?;
    let (user, session) = utils::require_user(&state, &sid).await?.split();

    let game_template: Option<GameTemplate> = sqlx::query_as(
        "SELECT * FROM game_templates WHERE game_template_id = ? AND user_id = ? LIMIT 1",
    )
    .bind(&id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;
    let Some(game_template) = game_template else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };

    let game_item_templates: Vec<GameItemTemplate> =
        sqlx::query_as("SELECT * FROM game_item_templates WHERE game_template_id = ?")
            .bind(&game_template.game_template_id)
            .fetch_all(&state.db)
            .await?;

    return Ok(Html(EditGameTemplateTemplate {
        session,
        user,
        template: game_template,
        items: game_item_templates.into_iter().enumerate().collect(),
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
    })
    .into_response());
}

#[derive(Deserialize)]
struct EditGameTempateAddItemParams {
    idx: u32,
}

#[derive(Template)]
#[template(path = "edit-game-template-add-item.html")]
struct EditGameTemplateAddItemTemplate {
    idx: u32,
    template: GameTemplate,
}

async fn edit_template_x_add_item(
    Path(id): Path<u64>,
    params: Query<EditGameTempateAddItemParams>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let sid = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &sid).await?.split();

    let game_template: Option<GameTemplate> = sqlx::query_as(
        "SELECT * FROM game_templates WHERE game_template_id = ? AND user_id = ? LIMIT 1",
    )
    .bind(&id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(game_template) = game_template else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };

    return Ok(Html(EditGameTemplateAddItemTemplate {
        idx: params.idx,
        template: game_template,
    })
    .into_response());
}

#[derive(Template)]
#[template(path = "edit-game-template-post-msg.html")]
struct EditGameTemplatePostMsgTemplate {
    session: SessionAuth,
    template: GameTemplate,
}

async fn edit_template_x_post_msg(
    Path(id): Path<u64>,
    session: Session,
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    let sid = utils::session_id(&session)?;
    let (user, session_auth) = utils::require_user(&state, &sid).await?.split();

    let game_template: Option<GameTemplate> = sqlx::query_as(
        "SELECT * FROM game_templates WHERE game_template_id = ? AND user_id = ? LIMIT 1",
    )
    .bind(&id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(game_template) = game_template else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };

    return Ok(Html(EditGameTemplatePostMsgTemplate {
        session: session_auth,
        template: game_template,
    })
    .into_response());
}

#[derive(Template)]
#[template(path = "edit-game-template-no-post-msg.html")]
struct EditGameTemplateNoPostMsgTemplate {
    template: GameTemplate,
}

async fn edit_template_x_no_post_msg(
    Path(id): Path<u64>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let sid = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &sid).await?.split();

    let game_template: Option<GameTemplate> = sqlx::query_as(
        "SELECT * FROM game_templates WHERE game_template_id = ? AND user_id = ? LIMIT 1",
    )
    .bind(&id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(game_template) = game_template else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };

    return Ok(Html(EditGameTemplateNoPostMsgTemplate {
        template: game_template,
    })
    .into_response());
}

#[derive(Template)]
#[template(path = "edit-game-template-post-total-msg.html")]
struct EditGameTemplatePostTotalMsgTemplate {
    session: SessionAuth,
    template: GameTemplate,
}

async fn edit_template_x_post_total_msg(
    Path(id): Path<u64>,
    session: Session,
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    let sid = utils::session_id(&session)?;
    let (user, session_auth) = utils::require_user(&state, &sid).await?.split();

    let game_template: Option<GameTemplate> = sqlx::query_as(
        "SELECT * FROM game_templates WHERE game_template_id = ? AND user_id = ? LIMIT 1",
    )
    .bind(&id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(game_template) = game_template else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };

    return Ok(Html(EditGameTemplatePostTotalMsgTemplate {
        session: session_auth,
        template: game_template,
    })
    .into_response());
}

#[derive(Template)]
#[template(path = "edit-game-template-no-post-msg.html")]
struct EditGameTemplateNoPostTotalMsgTemplate {
    template: GameTemplate,
}

async fn edit_template_x_no_post_total_msg(
    Path(id): Path<u64>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let sid = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &sid).await?.split();

    let game_template: Option<GameTemplate> = sqlx::query_as(
        "SELECT * FROM game_templates WHERE game_template_id = ? AND user_id = ? LIMIT 1",
    )
    .bind(&id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(game_template) = game_template else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };

    return Ok(Html(EditGameTemplateNoPostTotalMsgTemplate {
        template: game_template,
    })
    .into_response());
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
            let (name, mut img, start_enabled) = items.remove(&key).unwrap();

            let Some(name) = name else {
                return Err(anyhow::anyhow!("Item {} has no name", key + 1))?;
            };

            if let Some((_filename, bytes)) = &mut img {
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

                *bytes = data.into();
            }

            list.push((name, img, start_enabled == Some(true)));
        }

        list
    };

    let items = {
        let mut list = vec![];

        for (name, img, start_enabled) in items {
            let img = if let Some((filename, bytes)) = img {
                let key = format!("item_{}_{filename}", nanoid!());

                state.bucket.put_object(&key, &bytes).await?;

                Some(key)
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

    return Ok(Redirect::to("/game-templates").into_response());
}

async fn put_template(
    Path(id): Path<u64>,
    session: Session,
    State(state): State<AppState>,
    mut form: Multipart,
) -> Result<Response> {
    let sid = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &sid).await?.split();

    let prev_game_template: Option<GameTemplate> = sqlx::query_as(
        "SELECT * FROM game_templates WHERE game_template_id = ? AND user_id = ? LIMIT 1",
    )
    .bind(&id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(prev_game_template) = prev_game_template else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };

    let prev_game_items: Vec<GameItemTemplate> =
        sqlx::query_as("SELECT * FROM game_item_templates WHERE game_template_id = ?")
            .bind(&id)
            .fetch_all(&state.db)
            .await?;

    let prev_game_items: HashMap<u64, GameItemTemplate> = prev_game_items
        .into_iter()
        .map(|t| (t.game_item_template_id, t))
        .collect();

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

                let (item_id, item_name, item_image, start_enabled) =
                    items.entry(idx).or_insert((None, None, None, None));

                match &item_field_name[(close_idx + 2)..] {
                    "id" => {
                        *item_id = Some(field.text().await?.parse::<u64>()?);
                    }
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
            let (id, name, mut img, start_enabled) = items.remove(&key).unwrap();

            let Some(name) = name else {
                return Err(anyhow::anyhow!("Item {} has no name", key + 1))?;
            };

            if let Some((_filename, bytes)) = &mut img {
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

                *bytes = data.into();
            }

            if let Some(id) = id {
                if !prev_game_items.contains_key(&id) {
                    return Err(anyhow::anyhow!(
                        "Invalid item id: {id}\nMap: {prev_game_items:#?}"
                    ))?;
                }
            }

            list.push((id, name, img, start_enabled == Some(true)));
        }

        list
    };

    let (items_to_create, items_to_update) = {
        let mut to_create = vec![];
        let mut to_update = vec![];

        for (id, name, img, start_enabled) in items {
            let img = if let Some((filename, bytes)) = img {
                let key = format!("item_{}_{filename}", nanoid!());

                if let Some(id) = id {
                    let prev_item = &prev_game_items[&id];

                    if let Some(img_key) = &prev_item.image {
                        dbg!("Replacing [{img_key}] with {key}");
                        let _ = state.bucket.delete_object(img_key).await;
                    }
                }

                state.bucket.put_object(&key, &bytes).await?;

                Some(key)
            } else if let Some(id) = id {
                prev_game_items[&id].image.clone()
            } else {
                None
            };

            if let Some(id) = id {
                to_update.push((id, name, img, start_enabled));
            } else {
                to_create.push((name, img, start_enabled));
            }
        }

        (to_create, to_update)
    };

    sqlx::query("UPDATE game_templates SET name = ?, reward_message = ?, total_reward_message = ? WHERE game_template_id = ? AND user_id = ?")
        .bind(&name)
        .bind(&reward_message)
        .bind(&total_reward_message)
        .bind(&id)
        .bind(&user.user_id)
        .execute(&state.db)
        .await?;

    if !items_to_create.is_empty() {
        let query = format!(
            "INSERT INTO game_item_templates (game_template_id, name, image, start_enabled) VALUES {}",
            items_to_create.iter().map(|_| "(?, ?, ?, ?)").collect::<Vec<&'static str>>().join(",")
        );

        let mut q = sqlx::query(&query);

        for (name, img, start_enabled) in items_to_create {
            q = q.bind(&id).bind(name).bind(img).bind(start_enabled);
        }

        q.execute(&state.db).await?;
    }

    let mut items_to_delete = prev_game_items;

    for (id, name, img, start_enabled) in items_to_update {
        sqlx::query("UPDATE game_item_templates SET name = ?, image = ?, start_enabled = ? WHERE game_item_template_id = ?")
                .bind(&name)
                .bind(&img)
                .bind(&start_enabled)
                .bind(&id)
                .execute(&state.db)
                .await?;

        items_to_delete.remove(&id);
    }

    for id in items_to_delete.keys() {
        sqlx::query("DELETE FROM game_item_templates WHERE game_item_template_id = ?")
            .bind(&id)
            .execute(&state.db)
            .await?;
    }

    return Ok(Redirect::to("/game-templates").into_response());
}

async fn delete_template(
    Path(id): Path<u64>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let sid = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &sid).await?.split();

    let prev_game_template = sqlx::query(
        "SELECT * FROM game_templates WHERE game_template_id = ? AND user_id = ? LIMIT 1",
    )
    .bind(&id)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    if prev_game_template.is_none() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }

    let prev_game_items: Vec<GameItemTemplate> =
        sqlx::query_as("SELECT * FROM game_item_templates WHERE game_template_id = ?")
            .bind(&id)
            .fetch_all(&state.db)
            .await?;

    for item in prev_game_items {
        if let Some(image) = item.image {
            let _ = state.bucket.delete_object(image).await;
        }
    }

    sqlx::query("DELETE FROM game_item_templates WHERE game_template_id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    sqlx::query("DELETE FROM game_templates WHERE game_template_id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    return Ok("".into_response());
}
