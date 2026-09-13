//! Mistral AI provider (OpenAI-compatible chat endpoint).

use assistant_core::{
    AiProvider, BoxStream, ChatChunk, ChatRequest, ChatResponse, ProviderError, ProviderStatus,
};

use crate::openai_compat::{OpenAiCompatConfig, OpenAiCompatProvider};

pub const MISTRAL_BASE_URL: &str = "https://api.mistral.ai/v1";

pub struct MistralProvider(OpenAiCompatProvider);

impl MistralProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self(OpenAiCompatProvider::new(OpenAiCompatConfig {
            provider_name: "mistral",
            base_url: std::env::var("MISTRAL_BASE_URL")
                .unwrap_or_else(|_| MISTRAL_BASE_URL.to_string()),
            api_key,
            default_model: if model.is_empty() {
                std::env::var("MISTRAL_MODEL")
                    .unwrap_or_else(|_| "mistral-small-latest".to_string())
            } else {
                model
            },
            extra_headers: vec![],
            timeout_secs: 60,
        }))
    }

    pub fn from_env() -> Self {
        Self::new(
            std::env::var("MISTRAL_API_KEY").unwrap_or_default(),
            std::env::var("MISTRAL_MODEL").unwrap_or_default(),
        )
    }
}

#[async_trait::async_trait]
impl AiProvider for MistralProvider {
    fn name(&self) -> &str {
        "mistral"
    }

    fn models(&self) -> Vec<String> {
        self.0.models()
    }

    fn status(&self) -> ProviderStatus {
        self.0.status()
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
}
