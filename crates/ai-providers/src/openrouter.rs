//! OpenRouter provider — **FREE ONLY**.
//!
//! ```text
//! User selects model → SoryOS FreeModelPolicy → check pricing
//!   → prompt == 0 && completion == 0 → ALLOW → request
//!   → otherwise → BLOCK (ModelNotFree, no HTTP sent)
//! ```
//!
//! - Model catalogue is fetched live from `GET /v1/models`; the `pricing`
//!   object is the **only** source of truth (never the `:free` suffix).
//! - Default model is the official free router [`OPENROUTER_FREE_ROUTER`].
//! - There is deliberately **no paid fallback**, in complete() or stream().

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::openai_compat::{OpenAiCompatConfig, OpenAiCompatProvider};
use assistant_core::{AiProvider, BoxStream, ChatChunk, ChatRequest, ChatResponse, ProviderError};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// Official OpenRouter router across currently free models.
/// Recommended default: always free, chosen automatically.
pub const OPENROUTER_FREE_ROUTER: &str = "openrouter/free";

/// Previous auto router (may route to paid models) — never used by default.
pub const OPENROUTER_AUTO_MODEL: &str = "openrouter/auto";

/// Catalogue entries are revalidated after this long. A stale cache is
/// **never** used to authorize: on refresh failure past the TTL, requests
/// are denied instead of sent.
const MODELS_TTL: Duration = Duration::from_secs(5 * 60);

/// One OpenRouter catalogue entry with its prices.
///
/// Prices come from the API as decimal strings (`"0"`, `"0.000002"`).
/// `f64` is used instead of a decimal type: only exact-zero comparison
/// matters for the free-only policy.
#[derive(Debug, Clone)]
pub struct OpenRouterModel {
    pub id: String,
    pub name: String,
    pub context_length: Option<u64>,
    pub prompt_price: f64,
    pub completion_price: f64,
    /// Whether the model advertises tool calling (`supported_parameters`).
    /// Unknown capabilities never block pricing approval, only the
    /// tool-compatibility check.
    pub supports_tools: bool,
}

impl OpenRouterModel {
    /// Free **iff** both prices are exactly zero — the single definition
    /// used everywhere (UI filter and request guard alike).
    pub fn is_free(&self) -> bool {
        soryos_security::check_free_only(&self.id, self.prompt_price, self.completion_price).is_ok()
    }

    fn from_api(id: String, value: &serde_json::Value) -> Option<Self> {
        let pricing = value.get("pricing")?;
        let prompt_price = pricing
            .get("prompt")
            .and_then(soryos_security::parse_price)?;
        let completion_price = pricing
            .get("completion")
            .and_then(soryos_security::parse_price)?;
        // Missing/unparsable prices => entry rejected (fail closed).
        Some(Self {
            name: value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string(),
            context_length: value.get("context_length").and_then(|v| v.as_u64()),
            supports_tools: value
                .get("supported_parameters")
                .and_then(|v| v.as_array())
                .map(|params| params.iter().any(|p| p.as_str() == Some("tools")))
                .unwrap_or(false),
            id,
            prompt_price,
            completion_price,
        })
    }
}

/// Dedicated free-model registry: the rest of SoryOS never touches the
/// OpenRouter API shape directly.
#[derive(Debug, Default)]
pub struct FreeModelRegistry {
    models: Vec<OpenRouterModel>,
    fetched_at: Option<Instant>,
}

impl FreeModelRegistry {
    /// All entries (free and paid) currently known.
    pub fn all(&self) -> &[OpenRouterModel] {
        &self.models
    }

    /// Only free entries — what the UI may display.
    pub fn free_models(&self) -> Vec<OpenRouterModel> {
        self.models
            .iter()
            .filter(|m| m.is_free())
            .cloned()
            .collect()
    }

    pub fn find(&self, id: &str) -> Option<&OpenRouterModel> {
        self.models.iter().find(|m| m.id == id)
    }

    /// True when the entry exists **and** is priced at zero.
    pub fn is_free(&self, id: &str) -> bool {
        self.find(id).map(|m| m.is_free()).unwrap_or(false)
    }

    pub fn is_fresh(&self) -> bool {
        self.fetched_at
            .map(|t| t.elapsed() < MODELS_TTL)
            .unwrap_or(false)
    }

    fn replace(&mut self, models: Vec<OpenRouterModel>) {
        self.models = models;
        self.fetched_at = Some(Instant::now());
    }
}

/// Fetch the live catalogue and parse every priced entry.
/// Entries without usable pricing are dropped (fail closed).
pub fn parse_catalogue(data: &serde_json::Value) -> Vec<OpenRouterModel> {
    data.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?.to_string();
                    OpenRouterModel::from_api(id, m)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// OpenRouter configuration (TOML shape documented in the README):
///
/// ```toml
/// [providers.openrouter]
/// enabled = true
/// pricing_policy = "free_only"
/// default_model = "openrouter/free"
/// dynamic_model_list = true
/// allow_paid_models = false
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub pricing_policy: soryos_security::ModelPricingPolicy,
    #[serde(default = "default_free_router")]
    pub default_model: String,
    #[serde(default = "default_true")]
    pub dynamic_model_list: bool,
    #[serde(default)]
    pub allow_paid_models: bool,
}

fn default_true() -> bool {
    true
}

fn default_free_router() -> String {
    OPENROUTER_FREE_ROUTER.to_string()
}

impl Default for OpenRouterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            pricing_policy: soryos_security::ModelPricingPolicy::FreeOnly,
            default_model: default_free_router(),
            dynamic_model_list: true,
            allow_paid_models: false,
        }
    }
}

impl OpenRouterConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(m) = std::env::var("OPENROUTER_MODEL") {
            if !m.trim().is_empty() {
                cfg.default_model = m;
            }
        }
        cfg
    }
}

pub struct OpenRouterProvider {
    inner: OpenAiCompatProvider,
    registry: Arc<RwLock<FreeModelRegistry>>,
    config: OpenRouterConfig,
}

impl OpenRouterProvider {
    pub fn new(api_key: String, model: String) -> Self {
        let mut config = OpenRouterConfig::from_env();
        if !model.is_empty() {
            config.default_model = model;
        }
        Self {
            inner: OpenAiCompatProvider::new(OpenAiCompatConfig {
                provider_name: "openrouter",
                base_url: std::env::var("OPENROUTER_BASE_URL")
                    .unwrap_or_else(|_| OPENROUTER_BASE_URL.to_string()),
                api_key,
                default_model: config.default_model.clone(),
                extra_headers: vec![
                    (
                        "HTTP-Referer".to_string(),
                        "https://soryos.local/ai-assistant".to_string(),
                    ),
                    ("X-Title".to_string(), "SoryOS AI Assistant".to_string()),
                ],
                timeout_secs: 60,
            }),
            registry: Arc::new(RwLock::new(FreeModelRegistry::default())),
            config,
        }
    }

    pub fn from_env() -> Self {
        Self::new(
            std::env::var("OPENROUTER_API_KEY").unwrap_or_default(),
            String::new(),
        )
    }

    pub fn config(&self) -> &OpenRouterConfig {
        &self.config
    }

    /// Test hook: seed the registry without network.
    #[cfg(test)]
    pub async fn seed_for_tests(&self, models: Vec<OpenRouterModel>) {
        self.registry.write().await.replace(models);
    }

    /// Refresh the catalogue unless the cache is still fresh.
    async fn ensure_fresh(&self) -> Result<(), ProviderError> {
        if self.registry.read().await.is_fresh() {
            return Ok(());
        }
        self.refresh().await
    }

    /// Fetch the live catalogue (public endpoint; key sent when configured).
    async fn refresh(&self) -> Result<(), ProviderError> {
        let url = format!(
            "{}/models",
            self.inner.config().base_url.trim_end_matches('/')
        );
        let mut req = self.inner.http_client().get(url);
        let key = self.inner.config().api_key.clone();
        if !key.trim().is_empty() {
            req = req.bearer_auth(key);
        }
        let resp = tokio::time::timeout(Duration::from_secs(20), req.send())
            .await
            .map_err(|_| ProviderError::Timeout)?
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::Transport(e.to_string())
                }
            })?;
        if !resp.status().is_success() {
            return Err(ProviderError::Api(format!("HTTP {}", resp.status())));
        }
        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::Protocol(e.to_string()))?;
        let mut models = parse_catalogue(&data);
        models.sort_by(|a, b| a.id.cmp(&b.id));
        self.registry.write().await.replace(models);
        Ok(())
    }

    /// The request guard: revalidates the free-only policy **before any**
    /// generation HTTP call is made, in both complete() and stream().
    ///
    /// - `openrouter/free` (the official free router) is always allowed.
    /// - Known entries must price at exactly zero (prompt AND completion).
    /// - Unknown entries are denied, even after a refresh (fail closed).
    /// - A stale cache is never used to authorize: refresh failure past the
    ///   TTL denies the request instead of sending it.
    /// - When tools are requested, a known entry lacking tool support is
    ///   rejected without ever falling back to a paid model.
    async fn validate_model(
        &self,
        model: &str,
        tools_requested: bool,
    ) -> Result<(), ProviderError> {
        if model == OPENROUTER_FREE_ROUTER {
            return Ok(());
        }
        if let Err(e) = self.ensure_fresh().await {
            // Refresh failed: only a still-fresh cache may authorize.
            if !self.registry.read().await.is_fresh() {
                return Err(ProviderError::Api(format!(
                    "cannot verify free-only policy (catalogue unreachable: {e}); request blocked"
                )));
            }
        }
        let registry = self.registry.read().await;
        match registry.find(model) {
            Some(entry) if entry.is_free() => {
                if tools_requested && !entry.supports_tools {
                    return Err(ProviderError::Api(format!(
                        "No compatible free model is currently available: '{model}' is free but does not support tool calling, and paid models are forbidden"
                    )));
                }
                Ok(())
            }
            Some(entry) => Err(ProviderError::ModelNotFree(format!(
                "{model} (prompt={}, completion={})",
                entry.prompt_price, entry.completion_price
            ))),
            None => Err(ProviderError::ModelNotFree(format!(
                "{model} is not a verified free OpenRouter model"
            ))),
        }
    }

    fn resolve_model(&self, request: &ChatRequest) -> String {
        if request.params.model.is_empty() {
            self.config.default_model.clone()
        } else {
            request.params.model.clone()
        }
    }
}

#[async_trait::async_trait]
impl AiProvider for OpenRouterProvider {
    fn name(&self) -> &str {
        "openrouter"
    }

    fn models(&self) -> Vec<String> {
        // Static fallback; the UI prefers fetch_models() (live, free-only).
        vec![OPENROUTER_FREE_ROUTER.to_string()]
    }

    fn status(&self) -> assistant_core::provider::ProviderStatus {
        self.inner.status()
    }

    async fn complete(&self, mut request: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let model = self.resolve_model(&request);
        self.validate_model(&model, !request.tools.is_empty())
            .await?;
        request.params.model = model;
        self.inner.complete(request).await
    }

    async fn stream(
        &self,
        mut request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatChunk, ProviderError>>, ProviderError> {
        let model = self.resolve_model(&request);
        self.validate_model(&model, !request.tools.is_empty())
            .await?;
        request.params.model = model;
        self.inner.stream(request).await
    }

    async fn fetch_models(&self) -> Result<Vec<String>, ProviderError> {
        // The free router first, then every live free model. Paid models
        // are never exposed here, so the UI cannot even offer them.
        if self.config.dynamic_model_list {
            if let Err(e) = self.refresh().await {
                // Catalogue unreachable: fall back to the free router only —
                // never to a paid model, never to a stale list.
                tracing::warn!(error = %e, "openrouter catalogue refresh failed");
                return Ok(vec![OPENROUTER_FREE_ROUTER.to_string()]);
            }
        }
        let registry = self.registry.read().await;
        let mut models = vec![OPENROUTER_FREE_ROUTER.to_string()];
        models.extend(registry.free_models().iter().map(|m| m.id.clone()));
        if models.len() == 1 {
            return Err(ProviderError::Api(
                "No free OpenRouter models are currently available.".to_string(),
            ));
        }
        Ok(models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn free_entry(id: &str) -> OpenRouterModel {
        OpenRouterModel {
            id: id.into(),
            name: id.into(),
            context_length: None,
            prompt_price: 0.0,
            completion_price: 0.0,
            supports_tools: true,
        }
    }

    #[test]
    fn prompt_zero_completion_zero_is_free() {
        assert!(free_entry("m:x").is_free());
    }

    #[test]
    fn positive_prompt_price_is_not_free() {
        let mut m = free_entry("m:x");
        m.prompt_price = 0.000001;
        assert!(!m.is_free());
    }

    #[test]
    fn positive_completion_price_is_not_free() {
        let mut m = free_entry("m:x");
        m.completion_price = 2.0;
        assert!(!m.is_free());
    }

    #[test]
    fn both_positive_is_not_free() {
        let m = OpenRouterModel {
            prompt_price: 1.0,
            completion_price: 1.0,
            ..free_entry("m:x")
        };
        assert!(!m.is_free());
    }

    #[test]
    fn free_suffix_does_not_grant_free_status() {
        // A `:free`-suffixed id with non-zero pricing is NOT free: pricing
        // is the only source of truth.
        let m = OpenRouterModel {
            prompt_price: 0.5,
            ..free_entry("sneaky-model:free")
        };
        assert!(!m.is_free());
    }

    #[test]
    fn catalogue_filter_keeps_only_priced_zero() {
        let data = serde_json::json!({
            "data": [
                {"id": "a:free", "pricing": {"prompt": "0", "completion": "0"}},
                {"id": "b:free", "pricing": {"prompt": "0.001", "completion": "0"}},
                {"id": "c", "pricing": {"prompt": "0", "completion": "0.002"}},
                {"id": "d", "pricing": {"prompt": "1", "completion": "1"}},
                {"id": "e-no-pricing"},
                {"id": "f-bad-pricing", "pricing": {"prompt": "n/a", "completion": "0"}}
            ]
        });
        let mut registry = FreeModelRegistry::default();
        registry.replace(parse_catalogue(&data));
        let free: Vec<String> = registry
            .free_models()
            .iter()
            .map(|m| m.id.clone())
            .collect();
        assert_eq!(free, vec!["a:free".to_string()]);
        assert!(registry.is_free("a:free"));
        assert!(!registry.is_free("b:free"));
        assert!(!registry.is_free("nope"));
    }

    #[tokio::test]
    async fn paid_model_request_is_blocked_before_http() {
        let provider = OpenRouterProvider::new("test-key".to_string(), String::new());
        provider
            .seed_for_tests(vec![OpenRouterModel {
                prompt_price: 0.002,
                ..free_entry("paid-model")
            }])
            .await;
        // Fresh seed => no network involved; the guard must reject.
        let request = ChatRequest::new(
            vec![],
            assistant_core::GenerationParams {
                model: "paid-model".to_string(),
                ..Default::default()
            },
        );
        let err = provider.complete(request).await.unwrap_err();
        assert!(
            matches!(err, ProviderError::ModelNotFree(_)),
            "unexpected: {err}"
        );
    }

    #[tokio::test]
    async fn unknown_model_is_blocked() {
        let provider = OpenRouterProvider::new("test-key".to_string(), String::new());
        provider.seed_for_tests(vec![free_entry("some-free")]).await;
        // Seed is fresh, so no refresh happens; unknown id must be denied
        // without any HTTP generation call.
        let request = ChatRequest::new(
            vec![],
            assistant_core::GenerationParams {
                model: "attacker-chosen-paid".to_string(),
                ..Default::default()
            },
        );
        let err = provider.complete(request).await.unwrap_err();
        assert!(matches!(err, ProviderError::ModelNotFree(_)));
    }

    #[tokio::test]
    async fn free_router_and_free_model_pass_validation() {
        // `openrouter/free` never needs catalogue access.
        let provider = OpenRouterProvider::new(String::new(), String::new());
        provider
            .validate_model(OPENROUTER_FREE_ROUTER, false)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn tools_without_support_are_rejected_not_rerouted() {
        let provider = OpenRouterProvider::new("test-key".to_string(), String::new());
        provider
            .seed_for_tests(vec![OpenRouterModel {
                supports_tools: false,
                ..free_entry("no-tools-free")
            }])
            .await;
        let err = provider
            .validate_model("no-tools-free", true)
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::Api(_)));
    }
}
