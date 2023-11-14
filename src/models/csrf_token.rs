use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize)]
pub struct CsrfToken {
    pub id: u32,
    pub sid: String,
    pub token: String,
    pub expiry: i64,
    pub redirect: Option<String>,
}
