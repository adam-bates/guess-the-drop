use super::GameItemTemplate;

use crate::Result;

use serde::{Deserialize, Serialize};
use sqlx::{self, MySqlPool};

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct GameTemplate {
    pub game_template_id: u32,
    pub user_id: String,

    pub name: String,
    pub reward_message: Option<String>,
}
