use mbl_core::Result;

use crate::Db;

/// Server-wide key/value settings.
#[derive(Debug, Clone)]
pub struct SettingsRepo {
    db: Db,
}

impl SettingsRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Persist a related group of settings atomically.
    pub async fn set_many(&self, entries: &[(&str, &str)]) -> Result<()> {
        let mut tx = self.db.write.begin().await?;
        for (key, value) in entries {
            sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
                .bind(key).bind(value).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        let value = sqlx::query_scalar!("SELECT value FROM settings WHERE key = ?", key)
            .fetch_optional(&self.db.read)
            .await?;
        Ok(value)
    }

    pub async fn set(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query!(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            key,
            value
        )
        .execute(&self.db.write)
        .await?;
        Ok(())
    }

    /// Returns `true` if a value was removed.
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let res =
            sqlx::query!("DELETE FROM settings WHERE key = ?", key).execute(&self.db.write).await?;
        Ok(res.rows_affected() > 0)
    }
}
