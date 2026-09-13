//! SQLite implementation of [`assistant_core::ConversationStore`].

use assistant_core::{store::StoreError, Conversation, ConversationId, ConversationStore};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::Database;

fn to_store<E: ToString>(e: E) -> StoreError {
    StoreError::Backend(e.to_string())
}

#[derive(Debug, Clone)]
pub struct SqliteConversationStore {
    db: Database,
}

impl SqliteConversationStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

fn row_to_conversation(
    id: String,
    title: String,
    messages: String,
    created_at: String,
    updated_at: String,
) -> Result<Conversation, StoreError> {
    let id = ConversationId(id.parse().map_err(to_store)?);
    let messages = serde_json::from_str(&messages).map_err(to_store)?;
    let created_at: DateTime<Utc> = created_at.parse().map_err(to_store)?;
    let updated_at: DateTime<Utc> = updated_at.parse().map_err(to_store)?;
    Ok(Conversation {
        id,
        title,
        messages,
        created_at,
        updated_at,
    })
}

#[async_trait]
impl ConversationStore for SqliteConversationStore {
    async fn create(&self, conversation: Conversation) -> Result<(), StoreError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let messages = serde_json::to_string(&conversation.messages).map_err(to_store)?;
            db.with_conn(|c| {
                c.execute(
                    "INSERT INTO conversations (id, title, messages, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        conversation.id.0.to_string(),
                        conversation.title,
                        messages,
                        conversation.created_at.to_rfc3339(),
                        conversation.updated_at.to_rfc3339(),
                    ],
                )
                .map(|_| ())
            })
            .map_err(to_store)
        })
        .await
        .map_err(to_store)?
    }

    async fn get(&self, id: ConversationId) -> Result<Conversation, StoreError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db.with_conn(|c| {
                c.query_row(
                    "SELECT id, title, messages, created_at, updated_at FROM conversations WHERE id = ?1",
                    rusqlite::params![id.0.to_string()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
            })
            .map_err(|e| match e {
                crate::database::DbError::Sqlite(msg)
                    if msg.contains("no rows") || msg.contains("no row") =>
                {
                    StoreError::NotFound(id)
                }
                other => to_store(other),
            })
            .and_then(|(a, b, c, d, e)| row_to_conversation(a, b, c, d, e))
        })
        .await
        .map_err(to_store)?
    }

    async fn list(&self, limit: usize) -> Result<Vec<Conversation>, StoreError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db.with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT id, title, messages, created_at, updated_at FROM conversations ORDER BY updated_at DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })?;
                let mut out = vec![];
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(to_store)
            .and_then(|rows: Vec<(String, String, String, String, String)>| {
                rows.into_iter()
                    .map(|(a, b, c, d, e)| row_to_conversation(a, b, c, d, e))
                    .collect()
            })
        })
        .await
        .map_err(to_store)?
    }

    async fn save(&self, conversation: &Conversation) -> Result<(), StoreError> {
        let db = self.db.clone();
        let conversation = conversation.clone();
        tokio::task::spawn_blocking(move || {
            let messages = serde_json::to_string(&conversation.messages).map_err(to_store)?;
            db.with_conn(|c| {
                c.execute(
                    "INSERT INTO conversations (id, title, messages, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(id) DO UPDATE SET title=excluded.title, messages=excluded.messages, updated_at=excluded.updated_at",
                    rusqlite::params![
                        conversation.id.0.to_string(),
                        conversation.title,
                        messages,
                        conversation.created_at.to_rfc3339(),
                        conversation.updated_at.to_rfc3339(),
                    ],
                )
                .map(|_| ())
            })
            .map_err(to_store)
        })
        .await
        .map_err(to_store)?
    }

    async fn delete(&self, id: ConversationId) -> Result<bool, StoreError> {
        let db = self.db.clone();
        let n = tokio::task::spawn_blocking(move || {
            db.with_conn(|c| {
                c.execute(
                    "DELETE FROM conversations WHERE id = ?1",
                    rusqlite::params![id.0.to_string()],
                )
            })
            .map_err(to_store)
        })
        .await
        .map_err(to_store)??;
        Ok(n > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assistant_core::ChatMessage;

    #[tokio::test]
    async fn conversations_round_trip() {
        let db = Database::in_memory().unwrap();
        let store = SqliteConversationStore::new(db);
        let mut conv = Conversation::new("Test");
        conv.push(ChatMessage::user("hello"));
        let id = conv.id;
        store.create(conv).await.unwrap();
        let loaded = store.get(id).await.unwrap();
        assert_eq!(loaded.messages.len(), 1);
        let list = store.list(10).await.unwrap();
        assert_eq!(list.len(), 1);
        assert!(store.delete(id).await.unwrap());
        assert!(matches!(store.get(id).await, Err(StoreError::NotFound(_))));
    }
}
