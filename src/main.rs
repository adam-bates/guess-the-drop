use std::{sync::Arc, time::SystemTime};

use askama::Template;
use axum::{
    error_handling::HandleErrorLayer,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{self, IntoResponse},
    routing::{get, post},
    Form, Router,
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use shuttle_secrets::SecretStore;
use sqlx::PgPool;
use tower::ServiceBuilder;
use tower_http::services::{ServeDir, ServeFile};
use tower_sessions::{
    cookie::SameSite, CachingSessionStore, Expiry, MokaStore, PostgresStore, Session,
    SessionManagerLayer,
};
use twitch_irc::{
    login::StaticLoginCredentials, ClientConfig, SecureTCPTransport, TwitchIRCClient,
};
use twitch_oauth2::{ClientId, ClientSecret, Scope, UserTokenBuilder};

#[shuttle_runtime::main]
async fn main(
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
    #[shuttle_shared_db::Postgres] db_pool: PgPool,
) -> shuttle_axum::ShuttleAxum {
    let config = build_config(secret_store);

    sqlx::migrate!().run(&db_pool).await.unwrap();

    let db_session_store = PostgresStore::new(db_pool.clone());
    db_session_store.migrate().await.unwrap();

    let mem_session_store = MokaStore::new(Some(2000));
    let session_store = CachingSessionStore::new(mem_session_store, db_session_store);

    let state = AppState {
        config: Arc::new(config),
        db_pool,
    };

    let session_service = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|_| async {
            return StatusCode::BAD_REQUEST;
        }))
        .layer(
            SessionManagerLayer::new(session_store)
                .with_domain(state.config.server_domain.to_string())
                .with_expiry(Expiry::OnSessionEnd)
                .with_secure(false)
                .with_same_site(SameSite::Lax),
        );

    let router = Router::new();

    // dynamic paths
    let router = router
        .route("/", get(index))
        .route("/join", get(join))
        .route("/games/:game_code", get(game))
        .route("/twitch/connect", get(twitch_connect))
        .route("/twitch/callback", get(twitch_callback));

    // static assets
    let router = router
        .route_service("/favicon.ico", ServeFile::new("assets/favicon.ico"))
        .nest_service("/assets", ServeDir::new("assets"));

    let router = router.with_state(state).layer(session_service);

    return Ok(router.into());
}

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    db_pool: PgPool,
}

struct Config {
    server_protocol: String,
    server_domain: String,
    server_port: String,
    server_host_uri: String,

    twitch_client_id: ClientId,
    twitch_client_secret: ClientSecret,
    twitch_callback_url: Url,
}

fn build_config(secret_store: SecretStore) -> Config {
    let server_protocol = secret_store.get("SERVER_PROTOCOL").unwrap();
    let server_domain = secret_store.get("SERVER_DOMAIN").unwrap();
    let server_port = secret_store.get("SERVER_PORT").unwrap();

    let server_port_postfix = if &server_port == "" {
        "".to_string()
    } else {
        format!(":{server_port}")
    };

    let server_host_uri = format!("{server_protocol}://{server_domain}{server_port_postfix}");

    let twitch_callback_url = format!("{server_host_uri}/twitch/callback")
        .parse()
        .unwrap();

    return Config {
        server_protocol,
        server_domain,
        server_port,
        server_host_uri,

        twitch_client_id: secret_store.get("TWITCH_CLIENT_ID").unwrap().into(),
        twitch_client_secret: secret_store.get("TWITCH_CLIENT_SECRET").unwrap().into(),

        twitch_callback_url,
    };
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {}

async fn index() -> impl IntoResponse {
    return IndexTemplate {};
}

#[derive(Debug, Serialize, Deserialize)]
struct JoinParams {
    code: String,
}

#[derive(Template)]
#[template(path = "join.html")]
struct JoinTemplate {
    game_code: String,
}

async fn join(Query(params): Query<JoinParams>) -> impl IntoResponse {
    return JoinTemplate {
        game_code: params.code,
    };
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

async fn twitch_connect(session: Session, State(state): State<AppState>) -> impl IntoResponse {
    let mut twitch_client = UserTokenBuilder::new(
        state.config.twitch_client_id.clone(),
        state.config.twitch_client_secret.clone(),
        state.config.twitch_callback_url.clone(),
    )
    .set_scopes(vec![Scope::ChatRead, Scope::ChatEdit]);

    let (url, csrf_token) = twitch_client.generate_url();

    let session_id = session.id().0.to_string();

    let now_s = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let ttl_s = now_s + (10 * 60); // 10 mins

    sqlx::query("INSERT INTO csrf_tokens (sid, token, ttl) VALUES ($1, $2, $3)")
        .bind(&session_id)
        .bind(csrf_token.secret())
        .bind(ttl_s as i64)
        .execute(&state.db_pool)
        .await
        .unwrap();

    return response::Redirect::to(url.as_str());
}

#[derive(sqlx::FromRow, Serialize, Deserialize)]
struct CsrfToken {
    id: i32,
    sid: String,
    token: String,
    ttl: i64,
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
) -> impl IntoResponse {
    if let Some(err) = &params.error {
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "Error: {err}: {}",
                params
                    .error_description
                    .as_ref()
                    .unwrap_or(&"unknown".to_string()),
            ),
        );
    }

    let (code, twitch_state) = match (&params.code, &params.state) {
        (Some(code), Some(state)) => (code, state),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                "Invalid request: missing required params".to_string(),
            );
        }
    };

    let session_id = session.id().0.to_string();

    let now_s = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let csrf_token: Option<CsrfToken> =
        sqlx::query_as("DELETE FROM csrf_tokens WHERE sid = $1 AND ttl > $2 RETURNING *")
            .bind(&session_id)
            .bind(now_s as i64)
            .fetch_optional(&state.db_pool)
            .await
            .unwrap();

    let Some(csrf_token) = csrf_token else {
        return (
            StatusCode::BAD_REQUEST,
            "No valid tokens for session".to_string(),
        );
    };

    let http_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let mut twitch_client = UserTokenBuilder::new(
        state.config.twitch_client_id.clone(),
        state.config.twitch_client_secret.clone(),
        state.config.twitch_callback_url.clone(),
    );
    twitch_client.set_csrf(csrf_token.token.into());

    if !twitch_client.csrf_is_valid(twitch_state) {
        panic!();
    }

    let token = twitch_client
        .get_user_token(&http_client, twitch_state, code)
        .await
        .unwrap();

    let client_config = ClientConfig::new_simple(StaticLoginCredentials::new(
        token.login.to_string(),
        Some(token.access_token.into()),
    ));

    let (_rx, client) = TwitchIRCClient::<SecureTCPTransport, _>::new(client_config);

    client.join(token.login.to_string()).unwrap();

    client
        .say(token.login.to_string(), "hello world".to_string())
        .await
        .unwrap();

    return (StatusCode::OK, format!("OK"));
}
