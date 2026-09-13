//! Generic OpenAI-compatible `/chat/completions` client (JSON + SSE).
//!
//! Used directly by OpenRouter, Mistral and local servers (Ollama,
//! llama.cpp, vLLM, ...). Each vendor module only supplies the base URL,
//! credentials and defaults.

use assistant_core::{
    AiProvider, BoxStream, ChatChunk, ChatRequest, ChatResponse, MessageRole, ProviderError,
    ProviderStatus, ToolCall, Usage,
};
use async_trait::async_trait;
use futures::stream;
use serde_json::Value;
use std::time::Duration;

/// Configuration for one OpenAI-compatible backend.
#[derive(Debug, Clone)]
pub struct OpenAiCompatConfig {
    pub provider_name: &'static str,
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
    pub extra_headers: Vec<(String, String)>,
    pub timeout_secs: u64,
}

impl OpenAiCompatConfig {
    pub fn api_key_or_not_configured(&self) -> Result<(), ProviderError> {
        if self.api_key.trim().is_empty() {
            Err(ProviderError::NotConfigured(format!(
                "no API key configured for {}",
                self.provider_name
            )))
        } else {
            Ok(())
        }
    }
}

pub struct OpenAiCompatProvider {
    config: OpenAiCompatConfig,
    client: reqwest::Client,
}

impl OpenAiCompatProvider {
    pub fn new(config: OpenAiCompatConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs.max(10)))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { config, client }
    }

    pub fn config(&self) -> &OpenAiCompatConfig {
        &self.config
    }

    pub fn http_client(&self) -> reqwest::Client {
        self.client.clone()
    }

    fn url(&self) -> String {
        format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        )
    }

    fn request_body(&self, request: &ChatRequest, stream: bool) -> Value {
        let model = if request.params.model.is_empty() {
            self.config.default_model.clone()
        } else {
            request.params.model.clone()
        };
        let mut body = serde_json::json!({
            "model": model,
            "messages": to_openai_messages(request),
            "temperature": request.params.temperature,
            "max_tokens": request.params.max_tokens,
            "stream": stream,
        });
        if !request.tools.is_empty() {
            body["tools"] = request
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
        }
        body
    }

    async fn post(&self, body: &Value, stream: bool) -> Result<reqwest::Response, ProviderError> {
        self.config.api_key_or_not_configured()?;
        let mut req = self
            .client
            .post(self.url())
            .bearer_auth(&self.config.api_key)
            .json(body);
        for (k, v) in &self.config.extra_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let fut = req.send();
        let resp = if stream {
            fut.await
        } else {
            tokio::time::timeout(Duration::from_secs(self.config.timeout_secs.max(10)), fut)
                .await
                .map_err(|_| ProviderError::Timeout)?
        }
        .map_err(map_transport)?;
        if !resp.status().is_success() {
            return Err(map_http_error(resp).await);
        }
        Ok(resp)
    }
}

fn to_openai_messages(request: &ChatRequest) -> Vec<Value> {
    request
        .messages
        .iter()
        .map(|m| {
            let role = match m.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            };
            let mut obj = serde_json::json!({ "role": role, "content": m.content });
            if !m.tool_calls.is_empty() {
                obj["tool_calls"] = m
                    .tool_calls
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "id": c.id,
                            "type": "function",
                            "function": {
                                "name": c.name,
                                "arguments": c.arguments.to_string(),
                            }
                        })
                    })
                    .collect();
            }
            if let Some(id) = &m.tool_call_id {
                obj["tool_call_id"] = Value::String(id.clone());
            }
            obj
        })
        .collect()
}

fn map_transport(e: reqwest::Error) -> ProviderError {
    if e.is_timeout() {
        ProviderError::Timeout
    } else if e.is_connect() {
        ProviderError::Transport(format!("cannot reach backend: {e}"))
    } else {
        ProviderError::Transport(e.to_string())
    }
}

async fn map_http_error(resp: reqwest::Response) -> ProviderError {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let short = body.chars().take(300).collect::<String>();
    match status.as_u16() {
        401 | 403 => ProviderError::Auth(format!(
            "HTTP {status}: clé API invalide ou refusée — vérifiez la clé. {short}"
        )),
        429 => ProviderError::Api(format!(
            "HTTP {status}: quota dépassé / rate limit — réessayez plus tard ou vérifiez votre abonnement. {short}"
        )),
        _ => ProviderError::Api(format!("HTTP {status}: {short}")),
    }
}

/// Parse a finished (non-streaming) chat-completions payload.
fn parse_response(data: &Value) -> Result<ChatResponse, ProviderError> {
    let choice = data
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| ProviderError::Protocol("missing choices[0]".to_string()))?;
    let message = choice
        .get("message")
        .ok_or_else(|| ProviderError::Protocol("missing choices[0].message".to_string()))?;
    let content = message
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    let mut tool_calls = vec![];
    if let Some(calls) = message.get("tool_calls").and_then(|c| c.as_array()) {
        for c in calls {
            let id = c
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("call-0")
                .to_string();
            let func = c.get("function").unwrap_or(&Value::Null);
            let name = func
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let args_str = func
                .get("arguments")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let arguments: Value =
                serde_json::from_str(args_str).unwrap_or(Value::Object(Default::default()));
            if !name.is_empty() {
                tool_calls.push(ToolCall::new(id, name, arguments));
            }
        }
    }
    let model = data
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let usage = Usage {
        prompt_tokens: data
            .pointer("/usage/prompt_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        completion_tokens: data
            .pointer("/usage/completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
    };
    Ok(ChatResponse {
        content,
        tool_calls,
        model,
        usage,
    })
}

#[derive(Debug, Default)]
struct PendingCall {
    id: String,
    name: String,
    arguments: String,
}

/// Parse one SSE `data:` payload, accumulating tool-call deltas.
fn apply_sse_data(data: &str, text: &mut String, pending: &mut Vec<PendingCall>) -> bool {
    if data.trim() == "[DONE]" {
        return true;
    }
    let Ok(json) = serde_json::from_str::<Value>(data) else {
        return false;
    };
    let Some(delta) = json
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("delta"))
    else {
        // Some servers send usage-only final events.
        return json.get("choices").is_some();
    };
    if let Some(t) = delta.get("content").and_then(|v| v.as_str()) {
        text.push_str(t);
    }
    if let Some(calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
        for c in calls {
            let index = c.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            while pending.len() <= index {
                pending.push(PendingCall::default());
            }
            let slot = &mut pending[index];
            if let Some(id) = c.get("id").and_then(|v| v.as_str()) {
                slot.id = id.to_string();
            }
            if let Some(func) = c.get("function") {
                if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                    slot.name.push_str(name);
                }
                if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                    slot.arguments.push_str(args);
                }
            }
        }
    }
    if json
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("finish_reason"))
        .and_then(|r| r.as_str())
        .is_some()
    {
        return true;
    }
    false
}

/// Turn an SSE byte stream into [`ChatChunk`]s: periodic text flushes plus
/// one final chunk carrying the finished tool calls.
async fn sse_to_chunks(
    mut bytes: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
    model: String,
) -> Result<BoxStream<'static, Result<ChatChunk, ProviderError>>, ProviderError> {
    use futures::StreamExt;
    let mut text = String::new();
    let mut pending: Vec<PendingCall> = vec![];
    let mut buffer = String::new();
    let mut out: Vec<Result<ChatChunk, ProviderError>> = vec![];

    while let Some(item) = bytes.next().await {
        let bytes = item.map_err(map_transport)?;
        buffer.push_str(&String::from_utf8_lossy(&bytes));
        // Drain complete lines.
        while let Some(pos) = buffer.find('\n') {
            let line: String = buffer.drain(..=pos).collect();
            let line = line.trim();
            let Some(data) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data.is_empty() {
                continue;
            }
            let before = text.len();
            let finished = apply_sse_data(data, &mut text, &mut pending);
            if text.len() > before {
                out.push(Ok(ChatChunk {
                    delta: text[before..].to_string(),
                    tool_calls: vec![],
                    finished: false,
                }));
            }
            if finished {
                // Drain the rest silently, then stop.
                let tool_calls = pending
                    .drain(..)
                    .enumerate()
                    .filter(|(_, p)| !p.name.is_empty())
                    .map(|(i, p)| {
                        let args: Value = serde_json::from_str(&p.arguments)
                            .unwrap_or(Value::Object(Default::default()));
                        ToolCall::new(
                            if p.id.is_empty() {
                                format!("call-{i}")
                            } else {
                                p.id
                            },
                            p.name,
                            args,
                        )
                    })
                    .collect();
                out.push(Ok(ChatChunk {
                    delta: String::new(),
                    tool_calls,
                    finished: true,
                }));
                let _ = model;
                return Ok(Box::pin(stream::iter(out)));
            }
        }
    }
    // Stream ended without explicit finish: flush what we have.
    let tool_calls = pending
        .drain(..)
        .enumerate()
        .filter(|(_, p)| !p.name.is_empty())
        .map(|(i, p)| {
            let args: Value =
                serde_json::from_str(&p.arguments).unwrap_or(Value::Object(Default::default()));
            ToolCall::new(
                if p.id.is_empty() {
                    format!("call-{i}")
                } else {
                    p.id
                },
                p.name,
                args,
            )
        })
        .collect();
    out.push(Ok(ChatChunk {
        delta: String::new(),
        tool_calls,
        finished: true,
    }));
    Ok(Box::pin(stream::iter(out)))
}

#[async_trait]
impl AiProvider for OpenAiCompatProvider {
    fn name(&self) -> &str {
        self.config.provider_name
    }

    fn models(&self) -> Vec<String> {
        if self.config.default_model.is_empty() {
            vec![]
        } else {
            vec![self.config.default_model.clone()]
        }
    }

    fn status(&self) -> ProviderStatus {
        if self.config.api_key.trim().is_empty() {
            ProviderStatus::NotConfigured
        } else {
            ProviderStatus::Ready
        }
    }

    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let body = self.request_body(&request, false);
        let resp = self.post(&body, false).await?;
        let data: Value = resp.json().await.map_err(|e| {
            if e.is_timeout() {
                ProviderError::Timeout
            } else {
                ProviderError::Protocol(e.to_string())
            }
        })?;
        if let Some(err) = data.get("error") {
            return Err(ProviderError::Api(err.to_string()));
        }
        parse_response(&data)
    }

    async fn stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatChunk, ProviderError>>, ProviderError> {
        let body = self.request_body(&request, true);
        let resp = self.post(&body, true).await?;
        let model = request.params.model.clone();
        sse_to_chunks(resp.bytes_stream(), model).await
    }

    async fn fetch_models(&self) -> Result<Vec<String>, ProviderError> {
        self.config.api_key_or_not_configured()?;
        let url = format!("{}/models", self.config.base_url.trim_end_matches('/'));
        let mut req = self.client.get(url).bearer_auth(&self.config.api_key);
        for (k, v) in &self.config.extra_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let resp = tokio::time::timeout(std::time::Duration::from_secs(20), req.send())
            .await
            .map_err(|_| ProviderError::Timeout)?
            .map_err(map_transport)?;
        if !resp.status().is_success() {
            return Err(map_http_error(resp).await);
        }
        let data: Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::Protocol(e.to_string()))?;
        let mut models: Vec<String> = data
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
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
