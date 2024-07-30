use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize, Clone)]
pub struct CsrfToken {
    pub id: i32,
    pub sid: String,
    pub token: String,
    pub expiry: Option<i64>,
    pub redirect: Option<String>,
    pub with_chat: bool,
}
