//! Framework-agnostic UI layer for SoryOS AI Assistant.
//!
//! [`App`] is a pure state machine: frontends (COSMIC/libcosmic, egui, the
//! terminal REPL in `apps/desktop`) feed it [`UiEvent`]s and execute the
//! resulting [`Effect`]s. No toolkit import lives here, so binding a real
//! desktop shell later requires no changes to `assistant-core`.

pub mod app;
pub mod chat;
pub mod settings;
pub mod sidebar;
pub mod state;
pub mod voice;

pub use app::{App, Effect, UiEvent};
pub use sidebar::{group_history, HistoryGroup};
pub use state::{AppState, ChatView, ConversationSummary, SettingsView};
