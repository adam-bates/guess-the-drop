use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize, Debug, Clone)]
pub struct User {
    pub user_id: String,
    pub username: String,
    pub twitch_login: String,
}
