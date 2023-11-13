use crate::{models::CsrfToken, AppState, Result};

use std::time::SystemTime;

use askama_axum::Response;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{self, IntoResponse},
    routing::get,
    Router,
};
use serde::Deserialize;
use tower_sessions::Session;
use twitch_oauth2::{Scope, TwitchToken, UserTokenBuilder};

pub fn add_routes(router: Router<AppState>) -> Router<AppState> {
    return router
        .route("/twitch/connect", get(twitch_connect))
        .route("/twitch/callback", get(twitch_callback))
        .route("/logout", get(logout));
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
    // .force_verify(true)
    .set_scopes(vec![Scope::ChatRead, Scope::ChatEdit]);

    let (url, csrf_token) = twitch_client.generate_url();

    let session_id = session.id().0.to_string();
    session.insert("sid", session_id.clone())?;

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
) -> Result<Response> {
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
        )
            .into_response());
    }

    let (code, twitch_state) = match (&params.code, &params.state) {
        (Some(code), Some(state)) => (code, state),
        _ => {
            return Ok((
                StatusCode::BAD_REQUEST,
                "Invalid request: missing required params".to_string(),
            )
                .into_response());
        }
    };

    let session_id = session
        .get("sid")?
        .unwrap_or_else(|| session.id().0.to_string());

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
        )
            .into_response());
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

    let now_s = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    let expiry_s = now_s + token.expires_in().as_secs();

    // TODO: Try to remove old session

    sqlx::query("DELETE FROM session_auths WHERE sid = $1")
        .bind(&session_id)
        .execute(&state.db)
        .await?;

    sqlx::query("INSERT INTO session_auths (sid, username, access_token, refresh_token, expiry) VALUES ($1, $2, $3, $4, $5)")
        .bind(&session_id)
        .bind(token.login.as_str())
        .bind(&token.access_token.secret())
        .bind(&token.refresh_token.as_ref().map(|t| t.secret()).unwrap_or_default())
        .bind(expiry_s as i64)
        .execute(&state.db)
        .await?;

    return Ok(response::Redirect::to("/").into_response());

    // let client_config = ClientConfig::new_simple(StaticLoginCredentials::new(
    //     token.login.to_string(),
    //     Some(token.access_token.into()),
    // ));

    // let (_rx, client) = TwitchIRCClient::<SecureTCPTransport, _>::new(client_config);

    // client.join(token.login.to_string())?;

    // client
    //     .say(token.login.to_string(), "hello world".to_string())
    //     .await?;

    // return Ok((StatusCode::OK, format!("OK")));
}

async fn logout(session: Session, State(state): State<AppState>) -> Result<impl IntoResponse> {
    let session_id = session.id().0.to_string();

    sqlx::query("DELETE FROM session_auths WHERE sid = $1")
        .bind(&session_id)
        .execute(&state.db)
        .await?;

    return Ok(response::Redirect::to("/"));
}
