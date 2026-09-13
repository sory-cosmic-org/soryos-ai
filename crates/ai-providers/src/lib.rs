//! Concrete [`assistant_core::AiProvider`] implementations.
//!
//! Each module hides one vendor's HTTP details. The rest of the app only
//! sees trait objects obtained from [`ProviderRegistry`] or [`provider_from_env`].
//!
//! Adding a new provider:
//! 1. Create `src/myprovider.rs` with a struct implementing `AiProvider`
//!    (reuse [`openai_compat`] if the API is OpenAI-compatible).
//! 2. Re-export it here and register it in [`provider_from_env`].
//! 3. Document the env vars in `.env.example` and `README.md`.

pub mod config;
pub mod gemini;
pub mod local;
pub mod mistral;
pub mod openai_compat;
pub mod openrouter;
pub mod registry;

pub use config::{provider_from_env, ProviderKind, ProviderState};
pub use gemini::GeminiProvider;
pub use local::LocalProvider;
pub use mistral::MistralProvider;
pub use openai_compat::OpenAiCompatProvider;
pub use openrouter::{
    parse_catalogue, FreeModelRegistry, OpenRouterConfig, OpenRouterModel, OpenRouterProvider,
    OPENROUTER_AUTO_MODEL, OPENROUTER_BASE_URL, OPENROUTER_FREE_ROUTER,
};
pub use registry::ProviderRegistry;
