use async_trait::async_trait;
use sqlx::{types::time::OffsetDateTime, MySqlPool};

use tower_sessions::{session::Id, ExpiredDeletion, Session, SessionStore, SqlxStoreError};

/// A MySQL session store.
#[derive(Clone, Debug)]
pub struct MySqlStore {
    pub pool: MySqlPool,
    pub schema_name: String,
    pub table_name: String,
}

impl MySqlStore {
    /// Create a new MySqlStore store with the provided connection pool.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use tower_sessions::{sqlx::MySqlPool, MySqlStore};
    ///
    /// # tokio_test::block_on(async {
    /// let database_url = std::option_env!("DATABASE_URL").unwrap();
    /// let pool = MySqlPool::connect(database_url).await.unwrap();
    /// let session_store = MySqlStore::new(pool);
    /// # })
    /// ```
    pub fn new(pool: MySqlPool) -> Self {
        Self {
            pool,
            schema_name: "tower_sessions".to_string(),
            table_name: "session".to_string(),
        }
    }
}

#[async_trait]
impl ExpiredDeletion for MySqlStore {
    async fn delete_expired(&self) -> Result<(), Self::Error> {
        let query = format!(
            r#"
            delete from `{schema_name}`.`{table_name}`
            where expiry_date < utc_timestamp()
            "#,
            schema_name = self.schema_name,
            table_name = self.table_name
        );
        sqlx::query(&query).execute(&self.pool).await?;
        Ok(())
    }
}

#[async_trait]
impl SessionStore for MySqlStore {
    type Error = SqlxStoreError;

    async fn save(&self, session: &Session) -> Result<(), Self::Error> {
        let query = format!(
            r#"
            insert into `{schema_name}`.`{table_name}`
              (id, data, expiry_date) values (?, ?, ?)
            on duplicate key update
              data = values(data),
              expiry_date = values(expiry_date)
            "#,
            schema_name = self.schema_name,
            table_name = self.table_name
        );
        sqlx::query(&query)
            .bind(&session.id().to_string())
            .bind(rmp_serde::to_vec(&session)?)
            .bind(session.expiry_date())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn load(&self, session_id: &Id) -> Result<Option<Session>, Self::Error> {
        let query = format!(
            r#"
            select data from `{schema_name}`.`{table_name}`
            where id = ? and expiry_date > ?
            "#,
            schema_name = self.schema_name,
            table_name = self.table_name
        );
        let data: Option<(Vec<u8>,)> = sqlx::query_as(&query)
            .bind(session_id.to_string())
            .bind(OffsetDateTime::now_utc())
            .fetch_optional(&self.pool)
            .await?;

        if let Some((data,)) = data {
            Ok(Some(rmp_serde::from_slice(&data)?))
        } else {
            Ok(None)
        }
    }

    async fn delete(&self, session_id: &Id) -> Result<(), Self::Error> {
        let query = format!(
            r#"delete from `{schema_name}`.`{table_name}` where id = ?"#,
            schema_name = self.schema_name,
            table_name = self.table_name
        );
        sqlx::query(&query)
            .bind(&session_id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
