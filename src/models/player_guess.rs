use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct PlayerGuess {
    pub player_guess_id: u64,
    pub game_id: u64,
    pub player_id: u64,
    pub item_id: u64,
    pub outcome_id: Option<u64>,
}
