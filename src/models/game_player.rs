use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct GamePlayer {
    pub game_player_id: i64,

    pub game_code: String,
    pub user_id: String,

    pub points: i32,
}
