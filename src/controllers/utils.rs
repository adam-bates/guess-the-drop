use crate::Result;

use tower_sessions::Session;

pub fn session_id(session: &Session) -> Result<String> {
    return Ok(session
        .get("sid")?
        .unwrap_or_else(|| session.id().0.to_string()));
}
