mod postgres_store;
use postgres_store::PostgresStore;

use crate::prelude::*;

use sqlx::PgPool;
use tower_sessions::{CachingSessionStore, MokaStore};

const SESSION_CACHE_CAPACITY: u64 = 2000;

pub async fn init_session_store(
    cfg: &Config,
    db: PgPool,
) -> Result<CachingSessionStore<MokaStore, PostgresStore>> {
    let mut db_session_store = PostgresStore::new(db.clone());
    db_session_store.schema_name = cfg.db_database.clone();

    let create_table_query = format!(
        r#"
            create table if not exists "{schema_name}"."{table_name}"
            (
                id text primary key not null,
                data bytea not null,
                expiry_date timestamptz not null
            )
            "#,
        schema_name = db_session_store.schema_name,
        table_name = db_session_store.table_name,
    );
    sqlx::query(&create_table_query).execute(&db).await.unwrap();

    let mem_session_store = MokaStore::new(Some(SESSION_CACHE_CAPACITY));

    return Ok(CachingSessionStore::new(
        mem_session_store,
        db_session_store,
    ));
}
