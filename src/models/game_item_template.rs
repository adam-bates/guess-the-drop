use crate::Result;

use serde::{Deserialize, Serialize};
use sqlx::{self, MySqlPool};

#[derive(sqlx::FromRow, Serialize, Deserialize)]
pub struct GameItemTemplate {
    pub game_item_template_id: u32,
    pub game_template_id: u32,

    pub name: String,
    pub image: Option<String>,

    pub start_enabled: bool,
}
