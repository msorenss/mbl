use std::collections::HashMap;

use mbl_core::Result;

use crate::Db;

/// Settings already read, including keys that are not set. Every write goes through
/// [`SettingsRepo`], which updates the cache after it commits, so the database is read once
/// per key. Background loops read some settings every few seconds, and a read that waits for a
/// busy disk (an SD card Frigate records to) held up requests for seconds.
#[derive(Debug, Default)]
pub struct SettingsCache {
    values: HashMap<String, Option<String>>,
    /// Bumped by every write, so a read that started before a write does not store its older
    /// value afterwards.
    generation: u64,
}

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
        self.remember(entries.iter().map(|(k, v)| ((*k).to_owned(), Some((*v).to_owned()))));
        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        let generation = {
            let cache = self.cache();
            if let Some(value) = cache.values.get(key) {
                return Ok(value.clone());
            }
            cache.generation
        };
        let value = sqlx::query_scalar!("SELECT value FROM settings WHERE key = ?", key)
            .fetch_optional(&self.db.read)
            .await?;
        let mut cache = self.cache();
        if cache.generation == generation {
            cache.values.insert(key.to_owned(), value.clone());
        }
        Ok(value)
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, SettingsCache> {
        self.db.settings.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn remember(&self, entries: impl IntoIterator<Item = (String, Option<String>)>) {
        let mut cache = self.cache();
        cache.generation += 1;
        cache.values.extend(entries);
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
        self.remember([(key.to_owned(), Some(value.to_owned()))]);
        Ok(())
    }

    /// Returns `true` if a value was removed.
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let res =
            sqlx::query!("DELETE FROM settings WHERE key = ?", key).execute(&self.db.write).await?;
        self.remember([(key.to_owned(), None)]);
        Ok(res.rows_affected() > 0)
    }
}
