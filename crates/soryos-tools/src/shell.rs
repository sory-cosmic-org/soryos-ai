//! Shell execution tool. The security policy decides whether a command
//! needs confirmation or is blocked; this module only executes what it is
//! asked, with a timeout and bounded output.

use assistant_core::{Tool, ToolError};
use async_trait::async_trait;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_OUTPUT: usize = 64 * 1024;

pub struct ShellTool {
    pub timeout_secs: u64,
}

impl Default for ShellTool {
    fn default() -> Self {
        Self {
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        }
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "run_shell"
    }

    fn description(&self) -> &str {
        "Run a shell command (sh -c) and capture stdout/stderr. Requires user confirmation by default; destructive commands are blocked."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to run" },
                "workdir": { "type": "string", "description": "Working directory (optional)" },
            },
            "required": ["command"],
        })
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| ToolError::InvalidInput("missing required field 'command'".to_string()))?
            .to_string();
        let workdir = input
            .get("workdir")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let timeout = self.timeout_secs;
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            run_command(command.clone(), workdir),
        )
        .await
        .map_err(|_| ToolError::Execution(format!("command timed out after {timeout}s")))?
        .map_err(ToolError::Execution)?;

        Ok(output)
    }
}

async fn run_command(
    command: String,
    workdir: Option<String>,
) -> Result<serde_json::Value, String> {
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(&command);
    if let Some(dir) = workdir {
        cmd.current_dir(dir);
    }
    let out = cmd.output().await.map_err(|e| e.to_string())?;
    let mut stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let mut stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let mut truncated = false;
    if stdout.len() + stderr.len() > MAX_OUTPUT {
        stdout.truncate(MAX_OUTPUT / 2);
        stderr.truncate(MAX_OUTPUT / 2);
        truncated = true;
    }
    Ok(serde_json::json!({
        "command": command,
        "exit_code": out.status.code(),
        "success": out.status.success(),
        "stdout": stdout,
        "stderr": stderr,
        "truncated": truncated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn echo_works() {
        let t = ShellTool::default();
        let out = t
            .execute(serde_json::json!({ "command": "echo hello" }))
            .await
            .unwrap();
        assert!(out["success"].as_bool().unwrap());
        assert!(out["stdout"].as_str().unwrap().contains("hello"));
    }
}
