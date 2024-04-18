use super::User;

use serde::{Deserialize, Serialize};
use sqlx;

#[derive(sqlx::FromRow, Serialize, Deserialize, Clone, Debug)]
pub struct SessionAuth {
    pub id: i64,
    pub sid: String,
    pub user_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub created_at: i64,
    pub expiry: i64,
    pub can_chat: bool,
}

#[derive(sqlx::FromRow, Serialize, Deserialize, Clone, Debug)]
pub struct SessionAuthWithUser {
    pub id: i64,
    pub sid: String,
    pub user_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub created_at: i64,
    pub expiry: i64,
    pub can_chat: bool,

    pub username: String,
    pub twitch_login: String,
}

impl SessionAuthWithUser {
    pub fn split(self) -> (User, SessionAuth) {
        return (
            User {
                user_id: self.user_id.clone(),
                username: self.username,
                twitch_login: self.twitch_login,
            },
            SessionAuth {
                id: self.id,
                sid: self.sid,
                user_id: self.user_id,
                access_token: self.access_token,
                refresh_token: self.refresh_token,
                created_at: self.created_at,
                expiry: self.expiry,
                can_chat: self.can_chat,
            },
        );
    }
}
