//! Key-value settings (non-secret preferences; API keys stay in env).

use crate::Database;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("storage failure: {0}")]
    Storage(String),
}

fn to_err<E: ToString>(e: E) -> SettingsError {
    SettingsError::Storage(e.to_string())
}

/// Non-secret settings such as selected provider/model. API keys are never
/// stored here — they come from environment variables.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    db: Database,
}

impl SettingsStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>, SettingsError> {
        let db = self.db.clone();
        let key = key.to_string();
        tokio::task::spawn_blocking(move || {
            db.with_conn(|c| {
                let mut stmt = c.prepare("SELECT value FROM settings WHERE key = ?1")?;
                let mut rows = stmt.query(rusqlite::params![key])?;
                if let Some(row) = rows.next()? {
                    Ok(Some(row.get::<_, String>(0)?))
                } else {
                    Ok(None)
                }
            })
            .map_err(to_err)
        })
        .await
        .map_err(to_err)?
    }

    pub async fn set(&self, key: &str, value: &str) -> Result<(), SettingsError> {
        let db = self.db.clone();
        let (key, value) = (key.to_string(), value.to_string());
        tokio::task::spawn_blocking(move || {
            db.with_conn(|c| {
                c.execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                    rusqlite::params![key, value],
                )
                .map(|_| ())
            })
            .map_err(to_err)
        })
        .await
        .map_err(to_err)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn settings_round_trip() {
        let db = Database::in_memory().unwrap();
        let s = SettingsStore::new(db);
        assert_eq!(s.get("provider").await.unwrap(), None);
        s.set("provider", "mistral").await.unwrap();
        assert_eq!(s.get("provider").await.unwrap().as_deref(), Some("mistral"));
    }
}
