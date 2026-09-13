//! [`assistant_core::Confirmer`] implementations.

use assistant_core::Confirmer;
use async_trait::async_trait;

/// Confirmer driven by a closure — handy for wiring any UI prompt.
pub struct ConfirmerFn<F>(pub F);

#[async_trait]
impl<F> Confirmer for ConfirmerFn<F>
where
    F: Fn(String, serde_json::Value, String) -> bool + Send + Sync,
{
    async fn confirm(&self, tool: &str, input: &serde_json::Value, reason: &str) -> bool {
        (self.0)(tool.to_string(), input.clone(), reason.to_string())
    }
}

/// Always approves (demos/tests only — never the default in the app).
pub struct AlwaysApprove;

#[async_trait]
impl Confirmer for AlwaysApprove {
    async fn confirm(&self, _tool: &str, _input: &serde_json::Value, _reason: &str) -> bool {
        true
    }
}

/// Always denies (safe non-interactive default).
pub struct AlwaysDeny;

#[async_trait]
impl Confirmer for AlwaysDeny {
    async fn confirm(&self, _tool: &str, _input: &serde_json::Value, _reason: &str) -> bool {
        false
    }
}

/// Blocking terminal prompt (`o/N`). Runs on a blocking thread so the
/// Tokio runtime stays responsive.
pub struct TerminalConfirmer;

#[async_trait]
impl Confirmer for TerminalConfirmer {
    async fn confirm(&self, tool: &str, input: &serde_json::Value, reason: &str) -> bool {
        let (tool, input, reason) = (tool.to_string(), input.clone(), reason.to_string());
        tokio::task::spawn_blocking(move || {
            use std::io::{self, Write};
            println!("\n🔐 L'assistant veut exécuter l'outil « {tool} ».");
            println!("   Motif : {reason}");
            println!("   Entrée : {input}");
            print!("   Autoriser ? [o/N] ");
            let _ = io::stdout().flush();
            let mut line = String::new();
            if io::stdin().read_line(&mut line).is_err() {
                return false;
            }
            matches!(
                line.trim().to_lowercase().as_str(),
                "o" | "oui" | "y" | "yes"
            )
        })
        .await
        .unwrap_or(false)
    }
}
