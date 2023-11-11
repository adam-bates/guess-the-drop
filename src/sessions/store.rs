use sqlx::PgPool;
use tower_sessions::{CachingSessionStore, MokaStore, PostgresStore};

const SESSION_CACHE_CAPACITY: u64 = 2000;

pub async fn build(db: PgPool) -> anyhow::Result<CachingSessionStore<MokaStore, PostgresStore>> {
    let db_session_store = PostgresStore::new(db.clone());
    db_session_store.migrate().await?;

    let mem_session_store = MokaStore::new(Some(SESSION_CACHE_CAPACITY));

    return Ok(CachingSessionStore::new(
        mem_session_store,
        db_session_store,
    ));
}
