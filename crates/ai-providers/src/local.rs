//! Local OpenAI-compatible server (Ollama, llama.cpp server, vLLM, ...).
//!
//! Configure with `LOCAL_AI_BASE_URL` (default `http://localhost:11434/v1`)
//! and `LOCAL_AI_MODEL`. No key is required; `LOCAL_AI_API_KEY` is sent when
//! the server expects one.

use assistant_core::{
    AiProvider, BoxStream, ChatChunk, ChatRequest, ChatResponse, ProviderError, ProviderStatus,
};

use crate::openai_compat::{OpenAiCompatConfig, OpenAiCompatProvider};

pub struct LocalProvider(OpenAiCompatProvider);

impl LocalProvider {
    pub fn new(base_url: String, model: String, api_key: String) -> Self {
        Self(OpenAiCompatProvider::new(OpenAiCompatConfig {
            provider_name: "local",
            base_url,
            api_key,
            default_model: model,
            extra_headers: vec![],
            timeout_secs: 120,
        }))
    }

    pub fn from_env() -> Self {
        Self::new(
            std::env::var("LOCAL_AI_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:11434/v1".to_string()),
            std::env::var("LOCAL_AI_MODEL").unwrap_or_else(|_| "qwen2.5:7b".to_string()),
            std::env::var("LOCAL_AI_API_KEY").unwrap_or_else(|_| "ollama".to_string()),
        )
    }
}

#[async_trait::async_trait]
impl AiProvider for LocalProvider {
    fn name(&self) -> &str {
        "local"
    }

    fn models(&self) -> Vec<String> {
        self.0.models()
    }

    fn status(&self) -> ProviderStatus {
        // Local servers need no key: report Ready and let connection errors
        // surface at request time with a clear message.
        ProviderStatus::Ready
    }

    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.0.complete(request).await
    }

    async fn stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatChunk, ProviderError>>, ProviderError> {
        self.0.stream(request).await
    }

    async fn fetch_models(&self) -> Result<Vec<String>, ProviderError> {
        // Prefer the OpenAI-compatible endpoint, fall back to Ollama's
        // native `/api/tags` so every locally pulled model is listed.
        match self.0.fetch_models().await {
            Ok(models) if !models.is_empty() && models != self.0.models() => Ok(models),
            _ => fetch_ollama_tags(self.0.http_client(), &self.0.config().base_url).await,
        }
    }
}

/// List models via Ollama's native API (`/api/tags`).
async fn fetch_ollama_tags(
    client: reqwest::Client,
    base_url: &str,
) -> Result<Vec<String>, ProviderError> {
    // `base_url` points at `.../v1`; tags live one level up.
    let root = base_url
        .trim_end_matches('/')
        .strip_suffix("/v1")
        .unwrap_or(base_url.trim_end_matches('/'));
    let url = format!("{root}/api/tags");
    let resp = tokio::time::timeout(std::time::Duration::from_secs(10), client.get(url).send())
        .await
        .map_err(|_| ProviderError::Timeout)?
        .map_err(|e| ProviderError::Transport(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(ProviderError::Transport(format!(
            "local server unreachable (HTTP {})",
            resp.status()
        )));
    }
    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ProviderError::Protocol(e.to_string()))?;
    let mut models: Vec<String> = data
        .get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name").and_then(|v| v.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    models.sort();
    models.dedup();
    Ok(models)
}
