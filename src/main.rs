use std::{sync::Arc, time::SystemTime};

use askama::Template;
use axum::{
    error_handling::HandleErrorLayer,
    extract::{Query, State},
    http::StatusCode,
    response::{self, IntoResponse},
    routing::get,
    Router,
};
use nanoid::nanoid;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use shuttle_secrets::SecretStore;
use sqlx::PgPool;
use tower::ServiceBuilder;
use tower_http::services::{ServeDir, ServeFile};
use tower_sessions::{Expiry, PostgresStore, Session, SessionManagerLayer};
use twitch_irc::{
    login::StaticLoginCredentials, ClientConfig, SecureTCPTransport, TwitchIRCClient,
};
use twitch_oauth2::{ClientId, ClientSecret, Scope, UserTokenBuilder};

#[shuttle_runtime::main]
async fn main(
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
    #[shuttle_shared_db::Postgres] db_pool: PgPool,
) -> shuttle_axum::ShuttleAxum {
    let (db_pool, session_store) = init_db(db_pool).await.unwrap();

    let session_service = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|_| async {
            return StatusCode::BAD_REQUEST;
        }))
        .layer(
            SessionManagerLayer::new(session_store)
                .with_secure(false)
                .with_expiry(Expiry::OnSessionEnd),
        );

    let state = build_app_state(secret_store, db_pool);

    let router = Router::new();

    // dynamic paths
    let router = router
        .route("/", get(index))
        .route("/twitch/connect", get(twitch_connect))
        .route("/twitch/callback", get(twitch_callback));

    // static assets
    let router = router
        .route_service("/favicon.ico", ServeFile::new("assets/favicon.ico"))
        .nest_service("/assets", ServeDir::new("assets"));

    let router = router.with_state(state).layer(session_service);

    return Ok(router.into());
}

async fn init_db(db_pool: PgPool) -> anyhow::Result<(Arc<PgPool>, PostgresStore)> {
    // TODO: figure out why this stopped working ...
    // sqlx::migrate!().run(&db_pool).await?;

    let session_store = PostgresStore::new(db_pool.clone());
    session_store.migrate().await?;

    return Ok((Arc::new(db_pool), session_store));
}

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    db_pool: Arc<PgPool>,
}

struct Config {
    server_host: String,

    twitch_client_id: ClientId,
    twitch_client_secret: ClientSecret,
    twitch_callback_url: Url,
}

fn build_app_state(secret_store: SecretStore, db_pool: Arc<PgPool>) -> AppState {
    let server_host = secret_store.get("SERVER_HOST").unwrap();

    let twitch_callback_url = format!("{server_host}/twitch/callback").parse().unwrap();

    let config = Arc::new(Config {
        server_host,

        twitch_client_id: secret_store.get("TWITCH_CLIENT_ID").unwrap().into(),
        twitch_client_secret: secret_store.get("TWITCH_CLIENT_SECRET").unwrap().into(),

        twitch_callback_url,
    });

    return AppState { config, db_pool };
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {}

async fn index() -> impl IntoResponse {
    return IndexTemplate {};
}

async fn twitch_connect(session: Session, State(state): State<AppState>) -> impl IntoResponse {
    let mut twitch_client = UserTokenBuilder::new(
        state.config.twitch_client_id.clone(),
        state.config.twitch_client_secret.clone(),
        state.config.twitch_callback_url.clone(),
    )
    .set_scopes(vec![Scope::ChatRead, Scope::ChatEdit]);

    let (url, csrf_token) = twitch_client.generate_url();

    let session_id = format!("sid{}", nanoid!(16, &nanoid::alphabet::SAFE));
    session.insert("sid", session_id.clone()).unwrap();

    let now_s = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let ttl_s = now_s + (10 * 60); // 10 mins

    dbg!(&session_id, &csrf_token.secret(), &ttl_s);

    sqlx::query("INSERT INTO csrf_tokens (sid, token, ttl) VALUES ($1, $2, $3)")
        .bind(&session_id)
        .bind(csrf_token.secret())
        .bind(ttl_s as i64)
        .execute(&*state.db_pool)
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

    let session_id = session.get::<String>("sid").unwrap();

    let Some(session_id) = session_id else {
        return (StatusCode::BAD_REQUEST, format!("Error: No session ID"));
    };

    let now_s = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let csrf_token: CsrfToken =
        sqlx::query_as("SELECT * FROM csrf_tokens WHERE sid = $1 AND ttl > $2")
            .bind(&session_id)
            .bind(now_s as i64)
            .fetch_one(&*state.db_pool)
            .await
            .unwrap();

    dbg!(&session_id, &csrf_token.token, &now_s);

    todo!();

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

    dbg!(&token, token.access_token.to_string());

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
