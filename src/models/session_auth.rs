use crate::Result;

use serde::{Deserialize, Serialize};
use sqlx::{self, PgPool};

#[derive(sqlx::FromRow, Serialize, Deserialize)]
pub struct SessionAuth {
    pub id: i32,
    pub sid: String,
    pub username: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expiry: i64,
}

impl SessionAuth {
    pub async fn find_by_id(db: &PgPool, sid: &str) -> Result<Option<Self>> {
        let found: Option<Self> =
            sqlx::query_as("SELECT * FROM session_auths WHERE sid = $1 LIMIT 1")
                .bind(sid)
                .fetch_optional(db)
                .await?;

        return Ok(found);
    }
}
