use super::User;

use crate::Result;

use serde::{Deserialize, Serialize};
use sqlx::{self, MySqlPool};

#[derive(sqlx::FromRow, Serialize, Deserialize, Clone)]
pub struct SessionAuth {
    pub id: u64,
    pub sid: String,
    pub user_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expiry: u64,
    pub can_chat: bool,
}

#[derive(sqlx::FromRow, Serialize, Deserialize, Clone)]
pub struct SessionAuthWithUser {
    pub id: u64,
    pub sid: String,
    pub user_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expiry: u64,
    pub can_chat: bool,

    pub username: String,
}

impl SessionAuthWithUser {
    pub fn split(self) -> (User, SessionAuth) {
        return (
            User {
                user_id: self.user_id.clone(),
                username: self.username,
            },
            SessionAuth {
                id: self.id,
                sid: self.sid,
                user_id: self.user_id,
                access_token: self.access_token,
                refresh_token: self.refresh_token,
                expiry: self.expiry,
                can_chat: self.can_chat,
            },
        );
    }
}
