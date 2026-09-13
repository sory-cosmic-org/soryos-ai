//! The [`Assistant`] orchestrator: context -> provider -> tools -> security.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::{
    provider::ChatResponse, AiProvider, AssistantConfig, AssistantError, ChatContext, ChatMessage,
    ChatRequest, Confirmer, Conversation, ConversationStore, GenerationParams, MemoryStore, Tool,
    ToolDecision, ToolGate, ToolResult,
};

/// Events emitted while a turn runs. The UI consumes these to render
/// progressive output without waiting for the full response.
#[derive(Debug, Clone)]
pub enum AssistantEvent {
    TextChunk(String),
    ToolRequested {
        name: String,
        input: serde_json::Value,
    },
    ConfirmationRequired {
        tool: String,
        reason: String,
    },
    ConfirmationDecided {
        tool: String,
        approved: bool,
    },
    ToolCompleted {
        name: String,
        is_error: bool,
    },
    Done(ChatResponse),
}

/// Central orchestrator. Holds only trait objects — never a concrete vendor,
/// database or UI type.
///
/// Cheap to clone (all shared handles); turns clone it to run in background
/// tasks while the UI keeps its own copy.
#[derive(Clone)]
pub struct Assistant {
    provider: Arc<dyn AiProvider>,
    tools: HashMap<String, Arc<dyn Tool>>,
    gate: Arc<dyn ToolGate>,
    memory: Option<Arc<dyn MemoryStore>>,
    store: Option<Arc<dyn ConversationStore>>,
    config: AssistantConfig,
}

impl Assistant {
    pub fn new(
        provider: Arc<dyn AiProvider>,
        gate: Arc<dyn ToolGate>,
        config: AssistantConfig,
    ) -> Self {
        Self {
            provider,
            tools: HashMap::new(),
            gate,
            memory: None,
            store: None,
            config,
        }
    }

    pub fn provider_name(&self) -> String {
        self.provider.name().to_string()
    }

    pub fn set_provider(&mut self, provider: Arc<dyn AiProvider>) {
        info!(provider = provider.name(), "switching AI provider");
        self.provider = provider;
    }

    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) {
        info!(tool = tool.name(), "registering tool");
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn tool_schemas(&self) -> Vec<crate::ToolSchema> {
        self.tools
            .values()
            .map(|t| crate::ToolSchema {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect()
    }

    pub fn set_memory(&mut self, memory: Arc<dyn MemoryStore>) {
        self.memory = Some(memory);
    }

    pub fn set_store(&mut self, store: Arc<dyn ConversationStore>) {
        self.store = Some(store);
    }

    pub fn config(&self) -> &AssistantConfig {
        &self.config
    }

    pub fn set_generation_params(&mut self, model: String, temperature: f32, max_tokens: u32) {
        self.config.default_model = model;
        self.config.temperature = temperature;
        self.config.max_tokens = max_tokens;
    }

    fn generation_params(&self) -> GenerationParams {
        let model = if self.config.default_model.is_empty() {
            self.provider.models().first().cloned().unwrap_or_default()
        } else {
            self.config.default_model.clone()
        };
        GenerationParams {
            model,
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
        }
    }

    async fn recall(&self, query: &str) -> Vec<crate::Memory> {
        match &self.memory {
            Some(m) => m.search(query, 5).await.unwrap_or_default(),
            None => vec![],
        }
    }

    /// Run one user turn, streaming progress events.
    ///
    /// Flow: `User -> Context -> Provider -> (Tool -> Gate -> Confirmer -> Tool)* -> Response`.
    /// Every tool call passes through the [`ToolGate`]; the model can never
    /// execute anything directly.
    pub async fn respond(
        &self,
        conversation: &mut Conversation,
        user_text: &str,
        confirmer: &dyn Confirmer,
        tx: &mpsc::Sender<AssistantEvent>,
    ) -> Result<ChatResponse, AssistantError> {
        conversation.push(ChatMessage::user(user_text));
        let memories = self.recall(user_text).await;

        let mut final_response = None;

        for _ in 0..=self.config.max_tool_iterations {
            let ctx = ChatContext::build(
                &self.config.system_prompt,
                memories.clone(),
                &conversation.messages,
                self.config.max_history,
            );
            let request = ChatRequest::new(ctx.into_messages(), self.generation_params())
                .with_tools(self.tool_schemas());

            let (text, tool_calls) = self.stream_once(&request, tx).await?;

            if tool_calls.is_empty() {
                let response = ChatResponse {
                    content: text.clone(),
                    tool_calls: vec![],
                    model: request.params.model.clone(),
                    usage: crate::Usage::default(),
                };
                conversation.push(ChatMessage::assistant(&text));
                final_response = Some(response);
                break;
            }

            conversation.push(ChatMessage::assistant_with_tools(&text, tool_calls.clone()));

            let mut results = Vec::new();
            for call in &tool_calls {
                tx.send(AssistantEvent::ToolRequested {
                    name: call.name.clone(),
                    input: call.arguments.clone(),
                })
                .await
                .map_err(|_| AssistantError::Cancelled)?;

                let Some(tool) = self.tools.get(&call.name) else {
                    warn!(tool = %call.name, "model requested unknown tool");
                    results.push(ToolResult::err(&call.id, &call.name, "unknown tool"));
                    continue;
                };

                match self.gate.authorize(&call.name, &call.arguments).await {
                    ToolDecision::Allow => {}
                    ToolDecision::RequireConfirmation { reason } => {
                        tx.send(AssistantEvent::ConfirmationRequired {
                            tool: call.name.clone(),
                            reason: reason.clone(),
                        })
                        .await
                        .map_err(|_| AssistantError::Cancelled)?;
                        let approved = confirmer
                            .confirm(&call.name, &call.arguments, &reason)
                            .await;
                        tx.send(AssistantEvent::ConfirmationDecided {
                            tool: call.name.clone(),
                            approved,
                        })
                        .await
                        .map_err(|_| AssistantError::Cancelled)?;
                        if !approved {
                            results.push(ToolResult::err(&call.id, &call.name, "denied by user"));
                            continue;
                        }
                    }
                    ToolDecision::Deny { reason } => {
                        results.push(ToolResult::err(&call.id, &call.name, reason));
                        continue;
                    }
                }

                match tool.execute(call.arguments.clone()).await {
                    Ok(value) => {
                        tx.send(AssistantEvent::ToolCompleted {
                            name: call.name.clone(),
                            is_error: false,
                        })
                        .await
                        .map_err(|_| AssistantError::Cancelled)?;
                        results.push(ToolResult::ok(&call.id, &call.name, value));
                    }
                    Err(e) => {
                        tx.send(AssistantEvent::ToolCompleted {
                            name: call.name.clone(),
                            is_error: true,
                        })
                        .await
                        .map_err(|_| AssistantError::Cancelled)?;
                        results.push(ToolResult::err(&call.id, &call.name, e.to_string()));
                    }
                }
            }

            for r in &results {
                conversation.push(ChatMessage::tool_result(r));
            }
        }

        let response = final_response.ok_or_else(|| {
            AssistantError::Tool("max tool iterations exceeded without final answer".to_string())
        })?;

        conversation.derive_title();
        if let Some(store) = &self.store {
            if let Err(e) = store.save(conversation).await {
                warn!(error = %e, "could not persist conversation");
            }
        }
        let _ = tx.send(AssistantEvent::Done(response.clone())).await;
        Ok(response)
    }

    /// Non-streaming convenience wrapper (tests, scripting).
    pub async fn respond_simple(
        &self,
        conversation: &mut Conversation,
        user_text: &str,
        confirmer: &dyn Confirmer,
    ) -> Result<String, AssistantError> {
        let (tx, mut rx) = mpsc::channel(64);
        // Drain events in the background so `respond` never blocks.
        let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
        let out = self.respond(conversation, user_text, confirmer, &tx).await;
        drop(tx);
        let _ = drain.await;
        out.map(|r| r.content)
    }

    /// Stream one provider pass, forwarding text deltas. Returns the full
    /// text plus any tool calls announced on the final chunk.
    async fn stream_once(
        &self,
        request: &ChatRequest,
        tx: &mpsc::Sender<AssistantEvent>,
    ) -> Result<(String, Vec<crate::ToolCall>), AssistantError> {
        use futures::StreamExt;
        let mut stream = self
            .provider
            .stream(request.clone())
            .await
            .map_err(AssistantError::from)?;
        let mut text = String::new();
        let mut tool_calls = vec![];
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(AssistantError::from)?;
            if !chunk.delta.is_empty() {
                text.push_str(&chunk.delta);
                let _ = tx.send(AssistantEvent::TextChunk(chunk.delta)).await;
            }
            if chunk.finished {
                tool_calls = chunk.tool_calls;
                break;
            }
        }
        Ok((text, tool_calls))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AllowAllConfirmer, MockProvider};
    use std::sync::Arc;

    struct PermissiveGate;
    #[async_trait::async_trait]
    impl ToolGate for PermissiveGate {
        async fn authorize(&self, _tool: &str, _input: &serde_json::Value) -> ToolDecision {
            ToolDecision::Allow
        }
    }

    fn assistant_with_mock() -> (Assistant, AllowAllConfirmer) {
        let config = AssistantConfig::default();
        let a = Assistant::new(
            Arc::new(MockProvider::new()),
            Arc::new(PermissiveGate),
            config,
        );
        (a, AllowAllConfirmer)
    }

    #[tokio::test]
    async fn user_message_yields_assistant_reply() {
        let (a, c) = assistant_with_mock();
        let mut conv = Conversation::new("Nouvelle conversation");
        let reply = a.respond_simple(&mut conv, "Bonjour", &c).await.unwrap();
        assert!(!reply.is_empty());
        assert_eq!(conv.messages.len(), 2);
    }

    #[tokio::test]
    async fn full_turn_emits_streaming_events() {
        let (a, c) = assistant_with_mock();
        let mut conv = Conversation::new("Nouvelle conversation");
        let (tx, mut rx) = mpsc::channel(64);
        let handle = tokio::spawn(async move {
            let mut chunks = 0;
            let mut done = false;
            while let Some(e) = rx.recv().await {
                match e {
                    AssistantEvent::TextChunk(_) => chunks += 1,
                    AssistantEvent::Done(_) => {
                        done = true;
                    }
                    _ => {}
                }
            }
            (chunks, done)
        });
        a.respond(&mut conv, "Bonjour", &c, &tx).await.unwrap();
        drop(tx);
        let (chunks, done) = handle.await.unwrap();
        assert!(chunks > 0);
        assert!(done);
    }
}
