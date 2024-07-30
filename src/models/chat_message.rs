use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    pub id: i32,
    pub game_code: String,
    pub message: String,
    pub lock_id: Option<String>,
    pub sent: bool,
}
