use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct GameItemTemplate {
    pub game_item_template_id: i64,
    pub game_template_id: i64,

    pub name: String,
    pub image: Option<String>,

    pub start_enabled: bool,
}
