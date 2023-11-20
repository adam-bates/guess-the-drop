mod game;
mod game_template;
mod twitch;
mod utils;

use crate::{models::User, prelude::*};

use askama::Template;
use axum::{
    extract::{Multipart, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use nanoid::nanoid;
use reqwest::StatusCode;
use tower_sessions::Session;

pub fn add_routes(router: Router<AppState>) -> Router<AppState> {
    let router = game::add_routes(router);
    let router = game_template::add_routes(router);
    let router = twitch::add_routes(router);

    return router.route("/", get(index)).route(
        "/health",
        get(|| async { (StatusCode::OK, "Service is healthy") }),
    );
}

pub struct Html<T: Template>(pub T);

impl<T: Template> IntoResponse for Html<T> {
    fn into_response(self) -> axum::response::Response {
        match self.0.render() {
            Ok(body) => {
                let body = minify_html::minify(body.as_bytes(), &minify_html::Cfg::default());

                let headers = [(
                    axum::http::header::CONTENT_TYPE,
                    axum::http::HeaderValue::from_static(T::MIME_TYPE),
                )];

                return (headers, body).into_response();
            }
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    user: Option<User>,
}

async fn index(session: Session, State(state): State<AppState>) -> Result<impl IntoResponse> {
    let sid = utils::session_id(&session)?;
    let user_auth = utils::find_user(&state, &sid).await?;

    let user = user_auth.map(|ua| {
        let (user, _) = ua.split();
        return user;
    });

    return Ok(IndexTemplate { user });
}

async fn _upload(State(state): State<AppState>, mut files: Multipart) -> Result<impl IntoResponse> {
    let Some(file) = files.next_field().await? else {
        return Ok(format!("Error fetching file"));
    };

    let category = file.name().unwrap().to_string();

    let name = match file.file_name() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => return Ok(format!("File name is required")),
    };

    let data = file.bytes().await?;
    if data.is_empty() {
        return Ok(format!("Empty file not allowed"));
    }

    let key = format!("{category}_{name}_{}", nanoid!());

    state.bucket.put_object(&key, &data).await?;

    return Ok(format!("{}/{}", state.cfg.r2_bucket_public_url, key));
}
