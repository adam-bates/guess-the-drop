use crate::{models::SessionAuthWithUser, prelude::*};

use sqlx;
use tower_sessions::Session;

pub fn session_id(session: &Session) -> Result<String> {
    return Ok(session
        .get("sid")?
        .unwrap_or_else(|| session.id().0.to_string()));
}

pub async fn find_user(state: &AppState, session_id: &str) -> Result<Option<SessionAuthWithUser>> {
    // TODO: In-mem cache

    let found: Option<SessionAuthWithUser> = sqlx::query_as("SELECT * FROM session_auths INNER JOIN users ON session_auths.user_id = users.user_id WHERE sid = $1 LIMIT 1")
        .bind(session_id)
        .fetch_optional(&state.db)
        .await?;

    return Ok(found);
}

pub async fn require_user(state: &AppState, session_id: &str) -> Result<SessionAuthWithUser> {
    let found = find_user(state, session_id).await?;
    let found = found.ok_or_else(|| anyhow::anyhow!("Unauthenticated"))?;
    return Ok(found);
}
