//! Long-term memory types and the [`MemoryStore`] abstraction.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Broad category used for filtering and display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryCategory {
    Preference,
    Fact,
    Project,
    Other(String),
}

impl MemoryCategory {
    pub fn label(&self) -> String {
        match self {
            Self::Preference => "preference".to_string(),
            Self::Fact => "fact".to_string(),
            Self::Project => "project".to_string(),
            Self::Other(s) => s.clone(),
        }
    }
}

/// A single remembered item. Semantic/vector search can be layered on top
/// later without changing this shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: Uuid,
    pub content: String,
    pub category: MemoryCategory,
    /// 0.0 (trivia) .. 1.0 (critical).
    pub importance: f32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Memory {
    pub fn new(content: impl Into<String>, category: MemoryCategory, importance: f32) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            category,
            importance: importance.clamp(0.0, 1.0),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn category_label(&self) -> String {
        self.category.label()
    }
}

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("storage failure: {0}")]
    Storage(String),
}

#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn save(&self, memory: Memory) -> Result<Memory, MemoryError>;
    async fn list(&self, limit: usize) -> Result<Vec<Memory>, MemoryError>;
    /// Best-effort keyword search, ordered by importance then recency.
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Memory>, MemoryError>;
    async fn delete(&self, id: Uuid) -> Result<bool, MemoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn importance_is_clamped() {
        let m = Memory::new("x", MemoryCategory::Fact, 5.0);
        assert_eq!(m.importance, 1.0);
    }
}
