use serde::{Deserialize, Serialize};
use sqlx;

pub const GAME_STATUS_ACTIVE: &str = "ACTIVE";
pub const GAME_STATUS_FINISHED: &str = "FINISHED";

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct Game {
    pub game_code: String,
    pub user_id: String,

    pub status: String,
    pub created_at: i64,
    pub active_at: i64,

    pub name: String,
    pub auto_lock: bool,
    pub reward_message: Option<String>,
    pub total_reward_message: Option<String>,

    pub is_locked: bool,
}

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct GameWithJoinedSummary {
    pub game_code: String,
    pub user_id: String,

    pub status: String,
    pub created_at: i64,
    pub active_at: i64,

    pub name: String,
    pub auto_lock: bool,
    pub reward_message: Option<String>,
    pub total_reward_message: Option<String>,

    pub is_locked: bool,

    pub players_count: Option<i64>,
    pub winners_count: Option<i64>,

    pub winning_points: Option<i32>,
    pub total_drops: Option<i64>,

    pub is_winner: Option<bool>,
    pub points: i32,
    pub host: String,
}

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct GameWithHostedSummary {
    pub game_code: String,
    pub user_id: String,

    pub status: String,
    pub created_at: i64,
    pub active_at: i64,

    pub name: String,
    pub auto_lock: bool,
    pub reward_message: Option<String>,
    pub total_reward_message: Option<String>,

    pub is_locked: bool,

    pub players_count: Option<i64>,
    pub winners_count: Option<i64>,

    pub winning_points: Option<i32>,
    pub total_drops: Option<i64>,
}
