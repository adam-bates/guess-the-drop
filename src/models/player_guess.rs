use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct PlayerGuess {
    pub player_guess_id: i64,
    pub game_code: String,
    pub player_id: i64,
    pub item_id: i64,
    pub outcome_id: Option<i64>,
}
