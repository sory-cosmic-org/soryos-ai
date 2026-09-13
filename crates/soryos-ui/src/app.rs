//! [`App`]: event reducer producing side [`Effect`]s for the frontend.

use assistant_core::{ConversationId, VoiceState};

use crate::state::{AppState, ConversationSummary};

/// Events the frontend forwards to the state machine.
#[derive(Debug, Clone)]
pub enum UiEvent {
    NewConversation,
    SelectConversation(ConversationId),
    InputChanged(String),
    SendMessage,
    StreamChunk(String),
    StreamDone {
        content: String,
    },
    StreamError(String),
    StopRequested,
    ToggleVoice,
    VoiceStateChanged(VoiceState),
    ToggleSettings,
    SelectProvider(String),
    SetModel(String),
    ConversationsLoaded(Vec<ConversationSummary>),
    ConversationOpened {
        id: ConversationId,
        messages: Vec<assistant_core::ChatMessage>,
    },
    ToolCallStarted {
        name: String,
    },
}

/// Side effects the frontend must execute (I/O lives outside the reducer).
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    SendToAssistant(String),
    CancelGeneration,
    StartListening,
    StopVoice,
    PersistSettings,
    None,
}

/// Pure state machine over [`AppState`].
pub struct App {
    state: AppState,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::default(),
        }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn update(&mut self, event: UiEvent) -> Effect {
        match event {
            UiEvent::NewConversation => {
                self.state.active_id = None;
                self.state.chat = crate::state::ChatView::default();
                self.state.input.clear();
                Effect::None
            }
            UiEvent::SelectConversation(id) => {
                self.state.active_id = Some(id);
                self.state.chat = crate::state::ChatView::default();
                Effect::None
            }
            UiEvent::InputChanged(text) => {
                self.state.input = text;
                Effect::None
            }
            UiEvent::SendMessage => {
                let text = self.state.input.trim().to_string();
                if text.is_empty() || self.state.chat.generating {
                    return Effect::None;
                }
                self.state.input.clear();
                self.state
                    .chat
                    .messages
                    .push(assistant_core::ChatMessage::user(&text));
                self.state.chat.streaming_text.clear();
                self.state.chat.generating = true;
                self.state.chat.last_error = None;
                Effect::SendToAssistant(text)
            }
            UiEvent::StreamChunk(delta) => {
                self.state.chat.streaming_text.push_str(&delta);
                Effect::None
            }
            UiEvent::StreamDone { content } => {
                self.state.chat.streaming_text.clear();
                if !content.is_empty() {
                    self.state
                        .chat
                        .messages
                        .push(assistant_core::ChatMessage::assistant(content));
                }
                self.state.chat.generating = false;
                Effect::None
            }
            UiEvent::StreamError(message) => {
                self.state.chat.generating = false;
                self.state.chat.streaming_text.clear();
                self.state.chat.last_error = Some(message);
                Effect::None
            }
            UiEvent::StopRequested => {
                self.state.chat.generating = false;
                Effect::CancelGeneration
            }
            UiEvent::ToggleVoice => {
                if self.state.voice == VoiceState::Idle {
                    self.state.voice = VoiceState::Listening;
                    Effect::StartListening
                } else {
                    self.state.voice = VoiceState::Idle;
                    Effect::StopVoice
                }
            }
            UiEvent::VoiceStateChanged(s) => {
                self.state.voice = s;
                Effect::None
            }
            UiEvent::ToggleSettings => {
                self.state.settings_open = !self.state.settings_open;
                Effect::None
            }
            UiEvent::SelectProvider(name) => {
                self.state.settings.selected_provider = name;
                Effect::PersistSettings
            }
            UiEvent::SetModel(model) => {
                self.state.settings.model = model;
                Effect::PersistSettings
            }
            UiEvent::ConversationsLoaded(list) => {
                self.state.conversations = list;
                Effect::None
            }
            UiEvent::ConversationOpened { id, messages } => {
                self.state.active_id = Some(id);
                self.state.chat.messages = messages;
                self.state.chat.streaming_text.clear();
                self.state.chat.generating = false;
                Effect::None
            }
            UiEvent::ToolCallStarted { name } => {
                self.state
                    .chat
                    .streaming_text
                    .push_str(&format!("\n⚙️ Exécution de l'outil « {name} »…\n"));
                Effect::None
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_message_clears_input_and_starts_generation() {
        let mut app = App::new();
        app.update(UiEvent::InputChanged("Bonjour".to_string()));
        let fx = app.update(UiEvent::SendMessage);
        assert_eq!(fx, Effect::SendToAssistant("Bonjour".to_string()));
        assert!(app.state().chat.generating);
        assert!(app.state().input.is_empty());
    }

    #[test]
    fn streaming_accumulates_then_commits() {
        let mut app = App::new();
        app.update(UiEvent::InputChanged("hi".to_string()));
        app.update(UiEvent::SendMessage);
        app.update(UiEvent::StreamChunk("Bon".to_string()));
        app.update(UiEvent::StreamChunk("jour".to_string()));
        assert_eq!(app.state().chat.streaming_text, "Bonjour");
        app.update(UiEvent::StreamDone {
            content: "Bonjour".to_string(),
        });
        assert!(!app.state().chat.generating);
        assert_eq!(app.state().chat.messages.len(), 2);
    }

    #[test]
    fn voice_toggle_cycles_listening() {
        let mut app = App::new();
        assert_eq!(app.update(UiEvent::ToggleVoice), Effect::StartListening);
        assert_eq!(app.update(UiEvent::ToggleVoice), Effect::StopVoice);
    }
}
