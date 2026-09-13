//! Permission levels and action classification.

use serde::{Deserialize, Serialize};

/// Sensitivity of an operation, from harmless to forbidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PermissionLevel {
    /// Pure reads with no side effects.
    ReadOnly,
    /// Writes considered safe by policy (sandboxed paths, ...).
    Safe,
    /// Allowed only after explicit user confirmation.
    RequiresConfirmation,
    /// Dangerous even with confirmation (destructive shell, ...).
    Restricted,
    /// Never allowed.
    Denied,
}

impl PermissionLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::Safe => "safe",
            Self::RequiresConfirmation => "requires-confirmation",
            Self::Restricted => "restricted",
            Self::Denied => "denied",
        }
    }
}

/// What kind of action a tool call represents. The policy maps
/// `(tool, kind)` to a [`PermissionLevel`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionKind {
    Read,
    Write,
    Delete,
    Execute,
    SystemInfo,
    Other(String),
}

impl ActionKind {
    /// Heuristic classification from the tool name.
    /// The call payload is inspected separately by the policy.
    pub fn classify(tool: &str, _input: &serde_json::Value) -> Self {
        match tool {
            "read_file" | "list_dir" => Self::Read,
            "write_file" => Self::Write,
            "delete_file" => Self::Delete,
            "run_shell" => Self::Execute,
            "system_info" => Self::SystemInfo,
            other => Self::Other(other.to_string()),
        }
    }
}
