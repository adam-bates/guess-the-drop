use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct GameWinner {
    pub game_winner_id: u64,
    pub game_player_id: u64,
}
