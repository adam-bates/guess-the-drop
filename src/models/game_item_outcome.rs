use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct GameItemOutcome {
    pub outcome_id: i64,

    pub game_code: String,
    pub item_id: i64,
}
