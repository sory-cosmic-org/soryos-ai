//! SoryOS AI Assistant — core abstractions and orchestration.
//!
//! This crate knows **no** concrete AI vendor, no HTTP client, no database
//! and no UI toolkit. It only defines the traits and types that concrete
//! crates (`ai-providers`, `voice`, `soryos-tools`, `soryos-security`,
//! `soryos-storage`, `soryos-ui`) implement or consume:
//!
//! ```text
//! User -> Assistant -> ChatContext -> AiProvider -> Tool* -> ToolGate -> Tool
//! ```

pub mod assistant;
pub mod config;
pub mod context;
pub mod conversation;
pub mod error;
pub mod memory;
pub mod message;
pub mod mock;
pub mod provider;
pub mod store;
pub mod tool;
pub mod voice;

pub use assistant::{Assistant, AssistantEvent};
pub use config::{AppConfig, AssistantConfig, SecurityConfig, VoiceConfig};
pub use context::ChatContext;
pub use conversation::{Conversation, ConversationId};
pub use error::AssistantError;
pub use memory::{Memory, MemoryCategory, MemoryStore};
pub use message::{ChatMessage, MessageRole, ToolCall, ToolResult};
pub use mock::MockProvider;
pub use provider::{
    AiProvider, BoxStream, ChatChunk, ChatRequest, ChatResponse, GenerationParams, ProviderError,
    ProviderInfo, ProviderStatus, ToolSchema, Usage,
};
pub use store::ConversationStore;
pub use tool::{
    AllowAllConfirmer, Confirmer, DenyAllConfirmer, Tool, ToolDecision, ToolError, ToolGate,
};
pub use voice::{SpeechToText, TextToSpeech, VoiceError, VoiceState};
