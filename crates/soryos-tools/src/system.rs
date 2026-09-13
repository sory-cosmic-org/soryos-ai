//! Read-only system information tool (always safe, never needs confirmation).

use assistant_core::{Tool, ToolError};
use async_trait::async_trait;

pub struct SystemInfoTool;

#[async_trait]
impl Tool for SystemInfoTool {
    fn name(&self) -> &str {
        "system_info"
    }

    fn description(&self) -> &str {
        "Return read-only system information (OS, architecture, time, uptime)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {}, "required": [] })
    }

    async fn execute(&self, _input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let uptime = std::fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|s| s.split_whitespace().next().map(|v| v.to_string()));
        Ok(serde_json::json!({
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "family": std::env::consts::FAMILY,
            "uptime_seconds": uptime,
            "timestamp_utc": chrono_now(),
            "exe": std::env::current_exe().ok().map(|p| p.to_string_lossy().into_owned()),
        }))
    }
}

fn chrono_now() -> String {
    // std-only timestamp (RFC3339-ish) to avoid pulling chrono into this crate.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reports_os() {
        let t = SystemInfoTool;
        let out = t.execute(serde_json::json!({})).await.unwrap();
        assert_eq!(out["os"], std::env::consts::OS);
    }
}
