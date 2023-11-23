use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize, Clone)]
pub struct CsrfToken {
    pub id: u64,
    pub sid: String,
    pub token: String,
    pub expiry: Option<u64>,
    pub redirect: Option<String>,
    pub with_chat: bool,
}
