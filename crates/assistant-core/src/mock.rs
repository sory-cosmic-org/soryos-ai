//! [`MockProvider`]: a dependency-free provider used for demos and tests.
//!
//! It answers with a configurable message and can simulate tool calls when
//! the user message contains a marker, which makes the whole
//! `Assistant -> Tool -> Security` loop testable without any API key.

use async_trait::async_trait;
use futures::stream;
use std::sync::Mutex;

use crate::{
    AiProvider, BoxStream, ChatChunk, ChatRequest, ChatResponse, ProviderError, ProviderStatus,
    ToolCall,
};

/// Marker: when the latest user message contains this text, the mock emits
/// a `system_info` tool call instead of a plain answer.
pub const MOCK_TOOL_MARKER: &str = "[use-tool]";

pub struct MockProvider {
    name: String,
    response: Mutex<String>,
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            name: "mock".to_string(),
            response: Mutex::new(
                "Bonjour ! Je suis SoryOS AI (mode démo, sans clé API). \
                 Posez-moi une question, ou demandez « aide » pour découvrir mes capacités."
                    .to_string(),
            ),
        }
    }

    pub fn with_response(text: impl Into<String>) -> Self {
        Self {
            name: "mock".to_string(),
            response: Mutex::new(text.into()),
        }
    }

    pub fn set_response(&self, text: impl Into<String>) {
        *self.response.lock().unwrap() = text.into();
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn reply_for(text: &str, configured: &str) -> (String, Vec<ToolCall>) {
    let lower = text.to_lowercase();
    if lower.contains(MOCK_TOOL_MARKER) || lower.contains("heure") || lower.contains("système") {
        let content = "Je consulte les informations système.".to_string();
        let call = ToolCall::new("mock-call-1", "system_info", serde_json::json!({}));
        return (content, vec![call]);
    }
    if lower.contains("aide") || lower.contains("help") {
        return (
            "Je peux discuter, exécuter des outils (fichiers, shell, système) avec votre autorisation, \
             et mémoriser vos préférences. Essayez : « Quelle heure est-il ? » ou « Liste les fichiers »."
                .to_string(),
            vec![],
        );
    }
    if configured.is_empty() {
        (format!("Écho (mode démo) : {text}"), vec![])
    } else {
        (configured.to_string(), vec![])
    }
}

#[async_trait]
impl AiProvider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn models(&self) -> Vec<String> {
        vec!["mock-1".to_string()]
    }

    fn status(&self) -> ProviderStatus {
        ProviderStatus::Ready
    }

    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let last_user = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == crate::MessageRole::User)
            .map(|m| m.content.clone())
            .unwrap_or_default();
        // If the model previously produced tool results, acknowledge them.
        if request
            .messages
            .iter()
            .any(|m| m.role == crate::MessageRole::Tool)
        {
            let summary = request
                .messages
                .iter()
                .rev()
                .find(|m| m.role == crate::MessageRole::Tool)
                .map(|m| m.content.clone())
                .unwrap_or_default();
            return Ok(ChatResponse {
                content: format!("Voici le résultat de l'outil : {summary}"),
                tool_calls: vec![],
                model: "mock-1".to_string(),
                usage: crate::Usage::default(),
            });
        }
        let configured = self.response.lock().unwrap().clone();
        let (content, tool_calls) = reply_for(&last_user, &configured);
        Ok(ChatResponse {
            content,
            tool_calls,
            model: "mock-1".to_string(),
            usage: crate::Usage::default(),
        })
    }

    async fn stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatChunk, ProviderError>>, ProviderError> {
        let response = self.complete(request).await?;
        // Emit word-by-word so UIs can exercise the streaming path.
        let mut chunks: Vec<Result<ChatChunk, ProviderError>> = response
            .content
            .split_inclusive(' ')
            .map(|w| {
                Ok(ChatChunk {
                    delta: w.to_string(),
                    tool_calls: vec![],
                    finished: false,
                })
            })
            .collect();
        chunks.push(Ok(ChatChunk {
            delta: String::new(),
            tool_calls: response.tool_calls,
            finished: true,
        }));
        Ok(Box::pin(stream::iter(chunks)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChatMessage, GenerationParams};
    use futures::StreamExt;

    #[tokio::test]
    async fn mock_answers_without_api_key() {
        let p = MockProvider::with_response("Bonjour !");
        let req = ChatRequest::new(
            vec![ChatMessage::user("Bonjour")],
            GenerationParams {
                model: "mock-1".to_string(),
                ..Default::default()
            },
        );
        let resp = p.complete(req).await.unwrap();
        assert_eq!(resp.content, "Bonjour !");
    }

    #[tokio::test]
    async fn mock_streams_chunks_then_finishes() {
        let p = MockProvider::with_response("Bonjour !");
        let req = ChatRequest::new(vec![ChatMessage::user("hi")], GenerationParams::default());
        let mut s = p.stream(req).await.unwrap();
        let mut text = String::new();
        let mut finished = false;
        while let Some(chunk) = s.next().await {
            let chunk = chunk.unwrap();
            text.push_str(&chunk.delta);
            finished = chunk.finished;
        }
        assert!(finished);
        assert_eq!(text, "Bonjour !");
    }
}
