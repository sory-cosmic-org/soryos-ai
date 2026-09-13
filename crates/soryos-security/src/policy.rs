//! [`SecurityPolicy`] and its [`assistant_core::ToolGate`] implementation.

use assistant_core::{ToolDecision, ToolGate};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::permissions::{ActionKind, PermissionLevel};

/// Shell command fragments that are blocked outright, even with confirmation.
const BLOCKED_SHELL_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf ~",
    "mkfs",
    ":(){",
    "dd if=",
    "dd of=/dev",
    "> /dev/sd",
    "shutdown",
    "reboot",
    "halt",
    "poweroff",
    "chmod -R 777 /",
    "chown -R",
    "curl|sh",
    "wget|sh",
];

/// Returns true when a shell command looks destructive.
pub fn is_destructive_shell_command(command: &str) -> bool {
    let lower = command.to_lowercase();
    BLOCKED_SHELL_PATTERNS
        .iter()
        .any(|pat| lower.contains(&pat.to_lowercase()))
}

/// Tunable policy. The desktop app builds it from [`assistant_core::AppConfig`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    #[serde(default = "default_true")]
    pub shell_requires_confirmation: bool,
    #[serde(default = "default_true")]
    pub filesystem_write_requires_confirmation: bool,
    #[serde(default)]
    pub filesystem_sandbox: String,
    /// When true, destructive shell commands are denied instead of merely
    /// requiring confirmation.
    #[serde(default = "default_true")]
    pub block_destructive_shell: bool,
}

fn default_true() -> bool {
    true
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            shell_requires_confirmation: true,
            filesystem_write_requires_confirmation: true,
            filesystem_sandbox: String::new(),
            block_destructive_shell: true,
        }
    }
}

impl SecurityPolicy {
    pub fn permissive_for_tests() -> Self {
        Self {
            shell_requires_confirmation: false,
            filesystem_write_requires_confirmation: false,
            filesystem_sandbox: String::new(),
            block_destructive_shell: true,
        }
    }

    /// Map a tool call to a permission level.
    pub fn classify(&self, tool: &str, input: &serde_json::Value) -> PermissionLevel {
        let kind = ActionKind::classify(tool, input);
        match kind {
            ActionKind::Read | ActionKind::SystemInfo => PermissionLevel::ReadOnly,
            ActionKind::Write => {
                if self.filesystem_write_requires_confirmation {
                    PermissionLevel::RequiresConfirmation
                } else {
                    PermissionLevel::Safe
                }
            }
            ActionKind::Delete => PermissionLevel::RequiresConfirmation,
            ActionKind::Execute => {
                let cmd = input
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if self.block_destructive_shell && is_destructive_shell_command(cmd) {
                    return PermissionLevel::Restricted;
                }
                if self.shell_requires_confirmation {
                    PermissionLevel::RequiresConfirmation
                } else {
                    PermissionLevel::Safe
                }
            }
            ActionKind::Other(_) => PermissionLevel::RequiresConfirmation,
        }
    }

    /// Enforce the filesystem sandbox, if configured.
    fn check_sandbox(&self, input: &serde_json::Value) -> Option<String> {
        if self.filesystem_sandbox.is_empty() {
            return None;
        }
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if path.is_empty() {
            return None;
        }
        // Normalize `..` lexically (no fs access needed).
        let mut parts: Vec<&str> = vec![];
        for comp in path.split('/') {
            match comp {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                c => parts.push(c),
            }
        }
        let normalized = format!("/{}", parts.join("/"));
        if !normalized.starts_with(&self.filesystem_sandbox) {
            return Some(format!(
                "path '{path}' is outside sandbox '{}'",
                self.filesystem_sandbox
            ));
        }
        None
    }

    fn decide(&self, tool: &str, input: &serde_json::Value) -> ToolDecision {
        if let Some(reason) = self.check_sandbox(input) {
            warn!(tool, "sandbox violation blocked");
            return ToolDecision::Deny { reason };
        }
        match self.classify(tool, input) {
            PermissionLevel::ReadOnly | PermissionLevel::Safe => ToolDecision::Allow,
            PermissionLevel::RequiresConfirmation => ToolDecision::RequireConfirmation {
                reason: format!(
                    "tool '{tool}' needs your approval (input: {})",
                    truncate_input(input)
                ),
            },
            PermissionLevel::Restricted => ToolDecision::Deny {
                reason: format!("tool '{tool}' is blocked by policy (destructive action)"),
            },
            PermissionLevel::Denied => ToolDecision::Deny {
                reason: format!("tool '{tool}' is denied by policy"),
            },
        }
    }
}

fn truncate_input(input: &serde_json::Value) -> String {
    let s = input.to_string();
    if s.len() > 160 {
        format!("{}…", &s[..160])
    } else {
        s
    }
}

#[async_trait]
impl ToolGate for SecurityPolicy {
    async fn authorize(&self, tool: &str, input: &serde_json::Value) -> ToolDecision {
        self.decide(tool, input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_are_allowed_without_confirmation() {
        let p = SecurityPolicy::default();
        let d = p
            .authorize("read_file", &serde_json::json!({"path": "/tmp/x"}))
            .await;
        assert!(matches!(d, ToolDecision::Allow));
    }

    #[tokio::test]
    async fn shell_requires_confirmation_by_default() {
        let p = SecurityPolicy::default();
        let d = p
            .authorize("run_shell", &serde_json::json!({"command": "ls -la"}))
            .await;
        assert!(matches!(d, ToolDecision::RequireConfirmation { .. }));
    }

    #[tokio::test]
    async fn destructive_shell_is_blocked() {
        let p = SecurityPolicy::default();
        let d = p
            .authorize(
                "run_shell",
                &serde_json::json!({"command": "rm -rf / --no-preserve-root"}),
            )
            .await;
        assert!(matches!(d, ToolDecision::Deny { .. }));
    }

    #[tokio::test]
    async fn sandbox_blocks_escapes() {
        let p = SecurityPolicy {
            filesystem_sandbox: "/home/sory".to_string(),
            ..Default::default()
        };
        let d = p
            .authorize("read_file", &serde_json::json!({"path": "/etc/passwd"}))
            .await;
        assert!(matches!(d, ToolDecision::Deny { .. }));
    }
}
