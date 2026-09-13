//! Short-term context window construction.

use crate::{ChatMessage, Memory, MessageRole};

/// Everything the orchestrator hands to a provider for one inference step.
#[derive(Debug, Clone)]
pub struct ChatContext {
    pub system_prompt: String,
    /// Most relevant long-term memories, rendered before history.
    pub memories: Vec<Memory>,
    /// Recent history, oldest first, already truncated to fit.
    pub window: Vec<ChatMessage>,
}

impl ChatContext {
    /// Build a context window: optional system prompt + memories summary +
    /// the last `max_history` messages (always keeping message order).
    pub fn build(
        system_prompt: &str,
        memories: Vec<Memory>,
        history: &[ChatMessage],
        max_history: usize,
    ) -> Self {
        let start = history.len().saturating_sub(max_history);
        Self {
            system_prompt: system_prompt.to_string(),
            memories,
            window: history[start..].to_vec(),
        }
    }

    /// Flatten into the message list sent to the provider.
    pub fn into_messages(self) -> Vec<ChatMessage> {
        let mut out = Vec::new();
        if !self.system_prompt.trim().is_empty() {
            out.push(ChatMessage::system(self.system_prompt));
        }
        if !self.memories.is_empty() {
            let summary = self
                .memories
                .iter()
                .map(|m| format!("- [{}] {}", m.category_label(), m.content))
                .collect::<Vec<_>>()
                .join("\n");
            out.push(ChatMessage::system(format!(
                "Relevant long-term memories:\n{summary}"
            )));
        }
        out.extend(self.window);
        out
    }

    pub fn contains_role(&self, role: MessageRole) -> bool {
        self.window.iter().any(|m| m.role == role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_keeps_only_recent_messages() {
        let history: Vec<ChatMessage> = (0..10)
            .map(|i| ChatMessage::user(format!("message {i}")))
            .collect();
        let ctx = ChatContext::build("sys", vec![], &history, 4);
        assert_eq!(ctx.window.len(), 4);
        assert_eq!(ctx.window[0].content, "message 6");
        let flat = ctx.into_messages();
        assert_eq!(flat[0].role, MessageRole::System);
        assert_eq!(flat.len(), 5);
    }
}
