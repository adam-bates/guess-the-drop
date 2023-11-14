use crate::Result;

use serde::{Deserialize, Serialize};
use sqlx::{self, MySqlPool};

#[derive(sqlx::FromRow, Serialize, Deserialize)]
pub struct SessionAuth {
    pub id: u32,
    pub sid: String,
    pub username: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expiry: i64,
}

impl SessionAuth {
    pub async fn find_by_id(db: &MySqlPool, sid: &str) -> Result<Option<Self>> {
        let found: Option<Self> =
            sqlx::query_as("SELECT * FROM session_auths WHERE sid = ? LIMIT 1")
                .bind(sid)
                .fetch_optional(db)
                .await?;

        return Ok(found);
    }
}
