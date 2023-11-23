use crate::Result;

use serde::{Deserialize, Serialize};
use sqlx::{self, MySqlPool};

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct GameItemTemplate {
    pub game_item_template_id: u64,
    pub game_template_id: u64,

    pub name: String,
    pub image: Option<String>,

    pub start_enabled: bool,
}
