use crate::prelude::*;

use sqlx::PgPool;

pub async fn init_pg_pool(cfg: &Config) -> Result<PgPool> {
    let pool = PgPool::connect(&cfg.db_connection_url).await?;

    sqlx::migrate!().run(&pool).await?;

    return Ok(pool);
}
