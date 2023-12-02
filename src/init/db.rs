use crate::prelude::*;

use sqlx::MySqlPool;

pub async fn init_mysql_pool(cfg: &Config) -> Result<MySqlPool> {
    let pool = MySqlPool::connect(&cfg.db_connection_url).await?;

    sqlx::migrate!().run(&pool).await?;

    return Ok(pool);
}
