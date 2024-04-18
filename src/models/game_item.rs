use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct GameItem {
    pub game_item_id: i64,
    pub game_code: String,

    pub name: String,
    pub image: Option<String>,

    pub enabled: bool,
}

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct GameItemWithGuessCount {
    pub game_item_id: i64,
    pub game_code: String,

    pub name: String,
    pub image: Option<String>,

    pub enabled: bool,

    pub guess_count: Option<i32>,
}
