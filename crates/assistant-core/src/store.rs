//! Conversation persistence abstraction (implemented by `soryos-storage`).

use async_trait::async_trait;
use thiserror::Error;

use crate::{Conversation, ConversationId};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("storage failure: {0}")]
    Backend(String),
    #[error("conversation not found: {0}")]
    NotFound(ConversationId),
}

#[async_trait]
pub trait ConversationStore: Send + Sync {
    async fn create(&self, conversation: Conversation) -> Result<(), StoreError>;
    async fn get(&self, id: ConversationId) -> Result<Conversation, StoreError>;
    /// Most recently updated first.
    async fn list(&self, limit: usize) -> Result<Vec<Conversation>, StoreError>;
    async fn save(&self, conversation: &Conversation) -> Result<(), StoreError>;
    async fn delete(&self, id: ConversationId) -> Result<bool, StoreError>;
}
