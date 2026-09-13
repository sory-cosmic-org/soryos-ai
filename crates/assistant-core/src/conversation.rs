//! Conversations: ordered message histories with stable ids.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable, serializable conversation identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConversationId(pub Uuid);

impl ConversationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ConversationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An ordered message history plus metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: ConversationId,
    pub title: String,
    pub messages: Vec<crate::ChatMessage>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Conversation {
    pub fn new(title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: ConversationId::new(),
            title: title.into(),
            messages: vec![],
            created_at: now,
            updated_at: now,
        }
    }

    pub fn push(&mut self, message: crate::ChatMessage) {
        self.messages.push(message);
        self.updated_at = Utc::now();
    }

    pub fn user_turns(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.role == crate::MessageRole::User)
            .count()
    }

    /// Suggest a title from the first user message (used when the model
    /// hasn't provided one).
    pub fn derive_title(&mut self) {
        if self.title != "Nouvelle conversation" && !self.title.is_empty() {
            return;
        }
        if let Some(first) = self
            .messages
            .iter()
            .find(|m| m.role == crate::MessageRole::User)
        {
            let text: String = first.content.chars().take(48).collect();
            self.title = text;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_tracks_user_turns_and_title() {
        let mut c = Conversation::new("Nouvelle conversation");
        c.push(crate::ChatMessage::user("Bonjour, qui es-tu ?"));
        c.push(crate::ChatMessage::assistant("Je suis SoryOS AI."));
        assert_eq!(c.user_turns(), 1);
        c.derive_title();
        assert_eq!(c.title, "Bonjour, qui es-tu ?");
    }
}
