use super::GameItem;

use crate::Result;

use serde::{Deserialize, Serialize};
use sqlx::{self, MySqlPool};

#[derive(sqlx::FromRow, Serialize, Deserialize)]
pub struct Game {
    pub game_id: u32,
    pub user_id: u32,

    pub name: String,
    pub reward_message: Option<String>,
}
