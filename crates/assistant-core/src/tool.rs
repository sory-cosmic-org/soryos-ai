//! Tool and security-gate abstractions (implemented elsewhere).

use async_trait::async_trait;
use thiserror::Error;

/// Failure of a single tool execution. Transport/authorization problems are
/// reported through [`crate::AssistantError`] instead.
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("execution failed: {0}")]
    Execution(String),
    #[error("tool not available: {0}")]
    Unavailable(String),
}

/// A capability the model may invoke. Implementations live in `soryos-tools`.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    /// JSON Schema (object) describing the expected input.
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError>;
}

/// Decision returned by a [`ToolGate`] before any tool runs.
#[derive(Debug, Clone)]
pub enum ToolDecision {
    Allow,
    RequireConfirmation { reason: String },
    Deny { reason: String },
}

/// Policy checkpoint. The model can never bypass this layer: the
/// orchestrator consults the gate for **every** requested call.
#[async_trait]
pub trait ToolGate: Send + Sync {
    async fn authorize(&self, tool: &str, input: &serde_json::Value) -> ToolDecision;
}

/// Asks the human user for confirmation. UI-bound implementations live in
/// the desktop app; tests use scripted doubles.
#[async_trait]
pub trait Confirmer: Send + Sync {
    async fn confirm(&self, tool: &str, input: &serde_json::Value, reason: &str) -> bool;
}

/// Confirmer that approves everything (demos and tests only).
pub struct AllowAllConfirmer;

#[async_trait]
impl Confirmer for AllowAllConfirmer {
    async fn confirm(&self, _tool: &str, _input: &serde_json::Value, _reason: &str) -> bool {
        true
    }
}

/// Confirmer that denies everything (safe default for non-interactive runs).
pub struct DenyAllConfirmer;

#[async_trait]
impl Confirmer for DenyAllConfirmer {
    async fn confirm(&self, _tool: &str, _input: &serde_json::Value, _reason: &str) -> bool {
        false
    }
}
