use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct GameTemplate {
    pub game_template_id: i64,
    pub user_id: String,

    pub name: String,
    pub auto_lock: bool,

    pub reward_message: Option<String>,
    pub total_reward_message: Option<String>,
}
