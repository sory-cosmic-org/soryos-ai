//! SQLite implementation of [`assistant_core::MemoryStore`].

use assistant_core::{memory::MemoryError, Memory, MemoryCategory, MemoryStore};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::Database;

fn to_err<E: ToString>(e: E) -> MemoryError {
    MemoryError::Storage(e.to_string())
}

#[derive(Debug, Clone)]
pub struct SqliteMemoryStore {
    db: Database,
}

impl SqliteMemoryStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

fn parse_category(s: &str) -> MemoryCategory {
    match s {
        "preference" => MemoryCategory::Preference,
        "fact" => MemoryCategory::Fact,
        "project" => MemoryCategory::Project,
        other => MemoryCategory::Other(other.to_string()),
    }
}

#[async_trait]
impl MemoryStore for SqliteMemoryStore {
    async fn save(&self, memory: Memory) -> Result<Memory, MemoryError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            db.with_conn(|c| {
                c.execute(
                    "INSERT INTO memories (id, content, category, importance, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(id) DO UPDATE SET content=excluded.content, category=excluded.category,
                        importance=excluded.importance, updated_at=excluded.updated_at",
                    rusqlite::params![
                        memory.id.to_string(),
                        memory.content,
                        memory.category.label(),
                        memory.importance,
                        memory.created_at.to_rfc3339(),
                        memory.updated_at.to_rfc3339(),
                    ],
                )
                .map(|_| ())
            })
            .map_err(to_err)
            .map(|_| memory)
        })
        .await
        .map_err(to_err)?
    }

    async fn list(&self, limit: usize) -> Result<Vec<Memory>, MemoryError> {
        self.search("", limit).await
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Memory>, MemoryError> {
        let db = self.db.clone();
        let query = query.to_lowercase();
        tokio::task::spawn_blocking(move || {
            db.with_conn(|c| {
                let mut stmt = c.prepare(
                    "SELECT id, content, category, importance, created_at, updated_at
                     FROM memories ORDER BY importance DESC, updated_at DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map(rusqlite::params![1000_i64], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                })?;
                let mut out = vec![];
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            })
            .map_err(to_err)
            .map(|rows: Vec<(String, String, String, f64, String, String)>| {
                let mut memories: Vec<Memory> = rows
                    .into_iter()
                    .filter_map(
                        |(id, content, category, importance, created_at, updated_at)| {
                            Some(Memory {
                                id: id.parse().ok()?,
                                content,
                                category: parse_category(&category),
                                importance: importance as f32,
                                created_at: created_at.parse::<DateTime<Utc>>().ok()?,
                                updated_at: updated_at.parse::<DateTime<Utc>>().ok()?,
                            })
                        },
                    )
                    // Simple keyword scoring; a vector index can replace
                    // this later without changing the trait.
                    .filter(|m| {
                        query.is_empty()
                            || m.content.to_lowercase().contains(&query)
                            || m.category.label().contains(&query)
                    })
                    .collect();
                memories.truncate(limit);
                memories
            })
        })
        .await
        .map_err(to_err)?
    }

    async fn delete(&self, id: Uuid) -> Result<bool, MemoryError> {
        let db = self.db.clone();
        let n = tokio::task::spawn_blocking(move || {
            db.with_conn(|c| {
                c.execute(
                    "DELETE FROM memories WHERE id = ?1",
                    rusqlite::params![id.to_string()],
                )
            })
            .map_err(to_err)
        })
        .await
        .map_err(to_err)??;
        Ok(n > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memories_save_search_delete() {
        let db = Database::in_memory().unwrap();
        let store = SqliteMemoryStore::new(db);
        let m = Memory::new(
            "L'utilisateur préfère le français",
            MemoryCategory::Preference,
            0.9,
        );
        let id = m.id;
        store.save(m).await.unwrap();
        let found = store.search("français", 10).await.unwrap();
        assert_eq!(found.len(), 1);
        assert!(store.delete(id).await.unwrap());
        assert!(store.search("français", 10).await.unwrap().is_empty());
    }
}
