//! Provider-agnostic chat types and the [`AiProvider`] trait.

use async_trait::async_trait;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use thiserror::Error;

/// A stream of response chunks. Must be `'static` so it can cross task bounds.
pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + 'a>>;

/// Generation parameters, provider-independent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationParams {
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_temperature() -> f32 {
    0.7
}

fn default_max_tokens() -> u32 {
    1024
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            model: String::new(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
        }
    }
}

/// JSON-schema description of a tool exposed to the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Full request sent to a provider.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub messages: Vec<crate::ChatMessage>,
    pub params: GenerationParams,
    pub tools: Vec<ToolSchema>,
}

impl ChatRequest {
    pub fn new(messages: Vec<crate::ChatMessage>, params: GenerationParams) -> Self {
        Self {
            messages,
            params,
            tools: vec![],
        }
    }

    pub fn with_tools(mut self, tools: Vec<ToolSchema>) -> Self {
        self.tools = tools;
        self
    }
}

/// Token usage reported by a provider (best effort, may be zero).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

/// Complete, non-streaming response.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub tool_calls: Vec<crate::ToolCall>,
    pub model: String,
    pub usage: Usage,
}

impl ChatResponse {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            tool_calls: vec![],
            model: String::new(),
            usage: Usage::default(),
        }
    }
}

/// Incremental chunk of a streaming response.
///
/// Providers accumulate tool-call argument deltas internally and expose the
/// finished calls on the final chunk, so orchestrators never have to parse
/// partial JSON.
#[derive(Debug, Clone, Default)]
pub struct ChatChunk {
    pub delta: String,
    pub tool_calls: Vec<crate::ToolCall>,
    pub finished: bool,
}

/// Static metadata describing a provider implementation.
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: &'static str,
    pub models: Vec<String>,
}

/// Everything that can go wrong while talking to a model backend.
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("request error: {0}")]
    Request(String),
    #[error("provider returned an error: {0}")]
    Api(String),
    #[error("invalid response from provider: {0}")]
    Protocol(String),
    #[error("request timed out")]
    Timeout,
    #[error("provider not configured: {0}")]
    NotConfigured(String),
    #[error("model is not free and paid models are forbidden: {0}")]
    ModelNotFree(String),
    #[error("transport error: {0}")]
    Transport(String),
}

/// The single abstraction the core holds over every model backend.
///
/// Cloud APIs, local servers and test doubles all implement this trait.
/// Nothing here mentions HTTP, endpoints or API keys.
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Stable identifier, e.g. `"openrouter"`, `"mock"`.
    fn name(&self) -> &str;

    /// Known model ids (may be empty when the backend lists them dynamically).
    fn models(&self) -> Vec<String> {
        vec![]
    }

    /// Human-readable status, used by the settings UI.
    fn status(&self) -> ProviderStatus {
        ProviderStatus::Ready
    }

    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError>;

    async fn stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatChunk, ProviderError>>, ProviderError>;

    /// Live model list from the backend (model picker UIs). Defaults to the
    /// statically known [`AiProvider::models`]; vendors with a list endpoint
    /// override it.
    async fn fetch_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(self.models())
    }
}

/// Connectivity state of a provider, without leaking secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderStatus {
    Ready,
    NotConfigured,
    Error,
}

impl ProviderStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Connected",
            Self::NotConfigured => "Not configured",
            Self::Error => "Error",
        }
    }
}
