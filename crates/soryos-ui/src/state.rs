//! UI state: conversations, input, streaming and settings views.

use assistant_core::{ChatMessage, ConversationId, VoiceState};

/// One row of the conversation sidebar.
#[derive(Debug, Clone)]
pub struct ConversationSummary {
    pub id: ConversationId,
    pub title: String,
    pub updated: String,
    /// Whole days since last update (local time), for history grouping.
    pub updated_days_ago: i64,
}

/// Live view of the open conversation, including in-flight streaming text.
#[derive(Debug, Clone, Default)]
pub struct ChatView {
    pub messages: Vec<ChatMessage>,
    pub streaming_text: String,
    pub generating: bool,
    pub last_error: Option<String>,
}

impl ChatView {
    /// All messages plus the current partial response, for rendering.
    pub fn visible_messages(&self) -> Vec<ChatMessage> {
        let mut out = self.messages.clone();
        if !self.streaming_text.is_empty() {
            out.push(ChatMessage::assistant(&self.streaming_text));
        }
        out
    }
}

/// Provider/model selection state for the settings panel.
#[derive(Debug, Clone, Default)]
pub struct SettingsView {
    pub providers: Vec<ProviderRow>,
    pub selected_provider: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

/// One row of the provider list (status only — never secrets).
#[derive(Debug, Clone)]
pub struct ProviderRow {
    pub name: String,
    pub status: String,
    pub models: Vec<String>,
}

/// Whole application state. The [`crate::App`] reducer mutates this.
#[derive(Debug, Default)]
pub struct AppState {
    pub conversations: Vec<ConversationSummary>,
    pub active_id: Option<ConversationId>,
    pub chat: ChatView,
    pub input: String,
    pub voice: VoiceState,
    pub voice_enabled: bool,
    pub settings_open: bool,
    pub settings: SettingsView,
    pub tools: Vec<String>,
}

impl AppState {
    pub fn active_title(&self) -> &str {
        self.active_id
            .and_then(|id| self.conversations.iter().find(|c| c.id == id))
            .map(|c| c.title.as_str())
            .unwrap_or("Nouvelle conversation")
    }
}
