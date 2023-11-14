use super::mysql_store::MySqlStore;

use crate::prelude::*;

use sqlx::MySqlPool;
use tower_sessions::{CachingSessionStore, MokaStore};

const SESSION_CACHE_CAPACITY: u64 = 2000;

pub async fn build(
    cfg: &Config,
    db: MySqlPool,
) -> Result<CachingSessionStore<MokaStore, MySqlStore>> {
    let mut db_session_store = MySqlStore::new(db.clone());
    db_session_store.schema_name = cfg.db_database.clone();

    let create_table_query = format!(
        r#"
            create table if not exists `{schema_name}`.`{table_name}`
            (
                id char(36) primary key not null,
                data blob not null,
                expiry_date timestamp(6) not null
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
