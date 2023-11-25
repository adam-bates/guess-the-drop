use serde::{Deserialize, Serialize};
use sqlx;

pub const GAME_STATUS_ACTIVE: &str = "ACTIVE";
pub const GAME_STATUS_INACTIVE: &str = "INACTIVE";
pub const GAME_STATUS_FINISHED: &str = "FINISHED";

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct Game {
    pub game_code: String,
    pub user_id: String,

    pub status: String,
    pub created_at: u64,
    pub active_at: u64,

    pub name: String,
    pub reward_message: Option<String>,
    pub total_reward_message: Option<String>,
}

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct GameWithJoinedSummary {
    pub game_code: String,
    pub user_id: String,

    pub status: String,
    pub created_at: u64,
    pub active_at: u64,

    pub name: String,
    pub reward_message: Option<String>,
    pub total_reward_message: Option<String>,

    pub players_count: Option<i64>,
    pub winners_count: Option<i64>,
    pub winning_points: Option<i32>,

    pub is_winner: Option<bool>,
}

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct GameWithHostedSummary {
    pub game_code: String,
    pub user_id: String,

    pub status: String,
    pub created_at: u64,
    pub active_at: u64,

    pub name: String,
    pub reward_message: Option<String>,
    pub total_reward_message: Option<String>,

    pub players_count: Option<i64>,
    pub winners_count: Option<i64>,
    pub winning_points: Option<i32>,
}
