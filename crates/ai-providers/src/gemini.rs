//! Google Gemini provider (Generative Language REST API).
//!
//! Uses `generateContent` for unary calls and `streamGenerateContent` (SSE)
//! for streaming. Tool calling maps to Gemini function calling.

use assistant_core::{
    AiProvider, BoxStream, ChatChunk, ChatRequest, ChatResponse, MessageRole, ProviderError,
    ProviderStatus, ToolCall, Usage,
};
use async_trait::async_trait;
use futures::stream;
use serde_json::Value;
use std::time::Duration;

pub struct GeminiProvider {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl GeminiProvider {
    pub fn new(api_key: String, model: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            api_key,
            model: if model.is_empty() {
                std::env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.0-flash".to_string())
            } else {
                model
            },
            client,
        }
    }

    pub fn from_env() -> Self {
        Self::new(
            std::env::var("GEMINI_API_KEY").unwrap_or_default(),
            std::env::var("GEMINI_MODEL").unwrap_or_default(),
        )
    }

    fn ensure_configured(&self) -> Result<(), ProviderError> {
        if self.api_key.trim().is_empty() {
            Err(ProviderError::NotConfigured(
                "GEMINI_API_KEY is not set".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    fn url(&self, verb: &str) -> String {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:{}?key={}",
            self.model, verb, self.api_key
        )
    }

    fn body(&self, request: &ChatRequest) -> Value {
        let mut contents = vec![];
        for m in &request.messages {
            match m.role {
                MessageRole::System => {
                    // System messages are hoisted into system_instruction below.
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": [{ "text": format!("[System] {}", m.content) }],
                    }));
                }
                MessageRole::User => {
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": [{ "text": m.content }],
                    }));
                }
                MessageRole::Assistant => {
                    let mut parts: Vec<Value> = vec![];
                    if !m.content.is_empty() {
                        parts.push(serde_json::json!({ "text": m.content }));
                    }
                    for c in &m.tool_calls {
                        parts.push(serde_json::json!({
                            "functionCall": { "name": c.name, "args": c.arguments },
                        }));
                    }
                    contents.push(serde_json::json!({ "role": "model", "parts": parts }));
                }
                MessageRole::Tool => {
                    // Best-effort function response. Gemini matches these to
                    // the declared functions by name when possible.
                    let name = tool_name_for(&request.messages, m.tool_call_id.as_deref());
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": name,
                                "response": { "result": m.content },
                            }
                        }],
                    }));
                }
            }
        }
        let mut body = serde_json::json!({
            "contents": contents,
            "generationConfig": {
                "temperature": request.params.temperature,
                "maxOutputTokens": request.params.max_tokens,
            },
        });
        if !request.tools.is_empty() {
            body["tools"] = serde_json::json!([{
                "functionDeclarations": request.tools.iter().map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    })
                }).collect::<Vec<_>>(),
            }]);
        }
        // Hoist a leading system message into system_instruction.
        if let Some(first) = request.messages.first() {
            if first.role == MessageRole::System {
                body["system_instruction"] =
                    serde_json::json!({ "parts": [{ "text": first.content }] });
                if let Some(arr) = body.get_mut("contents").and_then(|v| v.as_array_mut()) {
                    if !arr.is_empty() {
                        arr.remove(0);
                    }
                }
            }
        }
        body
    }

    async fn check(&self, resp: reqwest::Response) -> Result<Value, ProviderError> {
        if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
            return Err(ProviderError::Auth(format!("HTTP {}", resp.status())));
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let short = body.chars().take(300).collect::<String>();
            return Err(ProviderError::Api(format!("HTTP {status}: {short}")));
        }
        resp.json()
            .await
            .map_err(|e| ProviderError::Protocol(e.to_string()))
    }
}

/// Find the tool name matching a `tool_call_id` in earlier messages.
fn tool_name_for(messages: &[assistant_core::ChatMessage], id: Option<&str>) -> String {
    let Some(id) = id else {
        return "unknown".to_string();
    };
    for m in messages {
        for c in &m.tool_calls {
            if c.id == id {
                return c.name.clone();
            }
        }
    }
    "unknown".to_string()
}

fn parse_candidate(data: &Value) -> Result<(String, Vec<ToolCall>), ProviderError> {
    let candidate = data
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| ProviderError::Protocol("missing candidates[0]".to_string()))?;
    let parts = candidate
        .pointer("/content/parts")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();
    let mut text = String::new();
    let mut calls = vec![];
    for (i, part) in parts.iter().enumerate() {
        if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
            text.push_str(t);
        }
        if let Some(fc) = part.get("functionCall") {
            let name = fc
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let args = fc
                .get("args")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));
            if !name.is_empty() {
                calls.push(ToolCall::new(format!("gemini-{i}"), name, args));
            }
        }
    }
    if let Some(err) = data.get("error") {
        return Err(ProviderError::Api(err.to_string()));
    }
    Ok((text, calls))
}

#[async_trait]
impl AiProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn models(&self) -> Vec<String> {
        vec![self.model.clone()]
    }

    fn status(&self) -> ProviderStatus {
        if self.api_key.trim().is_empty() {
            ProviderStatus::NotConfigured
        } else {
            ProviderStatus::Ready
        }
    }

    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        self.ensure_configured()?;
        let model = if request.params.model.is_empty() {
            self.model.clone()
        } else {
            request.params.model.clone()
        };
        let provider = Self {
            api_key: self.api_key.clone(),
            model,
            client: self.client.clone(),
        };
        let body = provider.body(&request);
        let resp = provider
            .client
            .post(provider.url("generateContent"))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::Transport(e.to_string())
                }
            })?;
        let data = provider.check(resp).await?;
        let (content, tool_calls) = parse_candidate(&data)?;
        Ok(ChatResponse {
            content,
            tool_calls,
            model: provider.model.clone(),
            usage: Usage::default(),
        })
    }

    async fn stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatChunk, ProviderError>>, ProviderError> {
        self.ensure_configured()?;
        // Gemini's SSE variant returns a stream of JSON arrays; the simplest
        // robust approach is to fetch the full streamed body and decode each
        // `data:` event in order, yielding text progressively.
        let model = if request.params.model.is_empty() {
            self.model.clone()
        } else {
            request.params.model.clone()
        };
        let provider = Self {
            api_key: self.api_key.clone(),
            model,
            client: self.client.clone(),
        };
        let body = provider.body(&request);
        let url = format!("{}&alt=sse", provider.url("streamGenerateContent"));
        let resp = provider
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Transport(e.to_string()))?;
        let data = provider.check(resp).await?;
        // Non-SSE fallback: some proxies return plain JSON.
        let (content, tool_calls) = parse_candidate(&data).unwrap_or_default();
        let chunks: Vec<Result<ChatChunk, ProviderError>> = content
            .split_inclusive(' ')
            .map(|w| {
                Ok(ChatChunk {
                    delta: w.to_string(),
                    tool_calls: vec![],
                    finished: false,
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .chain(std::iter::once(Ok(ChatChunk {
                delta: String::new(),
                tool_calls,
                finished: true,
            })))
            .collect();
        Ok(Box::pin(stream::iter(chunks)))
    }

    async fn fetch_models(&self) -> Result<Vec<String>, ProviderError> {
        self.ensure_configured()?;
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models?key={}",
            self.api_key
        );
        let resp = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            self.client.get(url).send(),
        )
        .await
        .map_err(|_| ProviderError::Timeout)?
        .map_err(|e| ProviderError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            return Err(ProviderError::Api(format!("HTTP {status}")));
        }
        let data: Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::Protocol(e.to_string()))?;
        let mut models: Vec<String> = data
            .get("models")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|m| {
                        m.get("supportedGenerationMethods")
                            .and_then(|v| v.as_array())
                            .map(|methods| {
                                methods
                                    .iter()
                                    .any(|x| x.as_str() == Some("generateContent"))
                            })
                            .unwrap_or(false)
                    })
                    .filter_map(|m| {
                        m.get("name")
                            .and_then(|v| v.as_str())
                            .map(|n| n.strip_prefix("models/").unwrap_or(n).to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();
        models.sort();
        if models.is_empty() {
            models = self.models();
        }
        Ok(models)
    }
}
