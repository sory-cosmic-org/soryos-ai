//! Local persistence for SoryOS AI Assistant (SQLite via `rusqlite`).
//!
//! The rest of the application only depends on the [`assistant_core`]
//! traits [`ConversationStore`](assistant_core::ConversationStore) and
//! [`MemoryStore`](assistant_core::MemoryStore); swapping SQLite for another
//! backend never touches the core or the UI.

pub mod conversations;
pub mod database;
pub mod memories;
pub mod settings;

pub use conversations::SqliteConversationStore;
pub use database::Database;
pub use memories::SqliteMemoryStore;
pub use settings::SettingsStore;
