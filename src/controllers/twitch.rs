use crate::{AppState, Result};

use std::time::SystemTime;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{self, IntoResponse},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use twitch_irc::{
    login::StaticLoginCredentials, ClientConfig, SecureTCPTransport, TwitchIRCClient,
};
use twitch_oauth2::{Scope, UserTokenBuilder};

pub fn add_routes(router: Router<AppState>) -> Router<AppState> {
    return router
        .route("/twitch/connect", get(twitch_connect))
        .route("/twitch/callback", get(twitch_callback));
}

async fn twitch_connect(
    session: Session,
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    let mut twitch_client = UserTokenBuilder::new(
        state.cfg.twitch_client_id.clone(),
        state.cfg.twitch_client_secret.clone(),
        state.cfg.twitch_callback_url.clone(),
    )
    .set_scopes(vec![Scope::ChatRead, Scope::ChatEdit]);

    let (url, csrf_token) = twitch_client.generate_url();

    let session_id = session.id().0.to_string();

    let now_s = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let ttl_s = now_s + (10 * 60); // 10 mins

    sqlx::query("INSERT INTO csrf_tokens (sid, token, expiry) VALUES ($1, $2, $3)")
        .bind(&session_id)
        .bind(csrf_token.secret())
        .bind(ttl_s as i64)
        .execute(&state.db)
        .await?;

    return Ok(response::Redirect::to(url.as_str()));
}

#[derive(sqlx::FromRow, Serialize, Deserialize)]
struct CsrfToken {
    id: i32,
    sid: String,
    token: String,
    expiry: i64,
}

#[derive(sqlx::FromRow, Serialize, Deserialize)]
struct SessionAuth {
    // For all
    id: i32,
    sid: String,
    username: String,

    // Only if authed with Twitch
    access_token: Option<String>,
    refresh_token: Option<String>,
    expiry: Option<i64>,
}

#[derive(Deserialize)]
struct AuthCallbackParams {
    code: Option<String>,
    state: Option<String>,

    error: Option<String>,
    error_description: Option<String>,
}

async fn twitch_callback(
    params: Query<AuthCallbackParams>,
    session: Session,
    State(state): State<AppState>,
) -> Result<impl IntoResponse> {
    if let Some(err) = &params.error {
        return Ok((
            StatusCode::BAD_REQUEST,
            format!(
                "Error: {err}: {}",
                params
                    .error_description
                    .as_ref()
                    .unwrap_or(&"unknown".to_string()),
            ),
        ));
    }

    let (code, twitch_state) = match (&params.code, &params.state) {
        (Some(code), Some(state)) => (code, state),
        _ => {
            return Ok((
                StatusCode::BAD_REQUEST,
                "Invalid request: missing required params".to_string(),
            ));
        }
    };

    let session_id = session.id().0.to_string();

    let now_s = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    let csrf_token: Option<CsrfToken> =
        sqlx::query_as("DELETE FROM csrf_tokens WHERE sid = $1 AND expiry > $2 RETURNING *")
            .bind(&session_id)
            .bind(now_s as i64)
            .fetch_optional(&state.db)
            .await?;

    let Some(csrf_token) = csrf_token else {
        return Ok((
            StatusCode::BAD_REQUEST,
            "No valid tokens for session".to_string(),
        ));
    };

    let http_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let mut twitch_client = UserTokenBuilder::new(
        state.cfg.twitch_client_id.clone(),
        state.cfg.twitch_client_secret.clone(),
        state.cfg.twitch_callback_url.clone(),
    );
    twitch_client.set_csrf(csrf_token.token.into());

    if !twitch_client.csrf_is_valid(twitch_state) {
        todo!();
    }

    let token = twitch_client
        .get_user_token(&http_client, twitch_state, code)
        .await?;

    let client_config = ClientConfig::new_simple(StaticLoginCredentials::new(
        token.login.to_string(),
        Some(token.access_token.into()),
    ));

    let (_rx, client) = TwitchIRCClient::<SecureTCPTransport, _>::new(client_config);

    client.join(token.login.to_string())?;

    client
        .say(token.login.to_string(), "hello world".to_string())
        .await?;

    return Ok((StatusCode::OK, format!("OK")));
}
