use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize)]
pub struct SessionAuth {
    // For all
    pub id: i32,
    pub sid: String,
    pub username: String,

    // Only if authed with Twitch
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expiry: Option<i64>,
}
