//! Filesystem tools: read, write and list. Writes go through the
//! security policy (confirmation) before reaching this code.

use assistant_core::{Tool, ToolError};
use async_trait::async_trait;

fn required<'a>(input: &'a serde_json::Value, field: &str) -> Result<&'a str, ToolError> {
    input
        .get(field)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::InvalidInput(format!("missing required field '{field}'")))
}

fn schema_for(properties: serde_json::Value, required: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

/// Read a UTF-8 text file (capped at 256 KiB to bound model context).
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read a UTF-8 text file and return its content."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_for(
            serde_json::json!({ "path": { "type": "string", "description": "Absolute file path" } }),
            &["path"],
        )
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let path = required(&input, "path")?.to_string();
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ToolError::Execution(format!("cannot read '{path}': {e}")))?;
        const MAX: usize = 256 * 1024;
        let (content, truncated) = if content.len() > MAX {
            (content[..MAX].to_string(), true)
        } else {
            (content, false)
        };
        Ok(serde_json::json!({ "path": path, "content": content, "truncated": truncated }))
    }
}

/// Create or overwrite a text file.
pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Create or overwrite a UTF-8 text file. Requires user confirmation by default."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_for(
            serde_json::json!({
                "path": { "type": "string", "description": "Absolute file path" },
                "content": { "type": "string", "description": "File content" },
            }),
            &["path", "content"],
        )
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let path = required(&input, "path")?.to_string();
        let content = required(&input, "content")?.to_string();
        if let Some(parent) = std::path::Path::new(&path).parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| ToolError::Execution(format!("cannot create dir: {e}")))?;
            }
        }
        tokio::fs::write(&path, content.as_bytes())
            .await
            .map_err(|e| ToolError::Execution(format!("cannot write '{path}': {e}")))?;
        Ok(serde_json::json!({ "path": path, "bytes_written": content.len() }))
    }
}

/// List a directory (names + kinds, capped at 500 entries).
pub struct ListDirTool;

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn description(&self) -> &str {
        "List entries of a directory."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        schema_for(
            serde_json::json!({ "path": { "type": "string", "description": "Directory path" } }),
            &["path"],
        )
    }

    async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let path = required(&input, "path")?.to_string();
        let mut dir = tokio::fs::read_dir(&path)
            .await
            .map_err(|e| ToolError::Execution(format!("cannot list '{path}': {e}")))?;
        let mut entries = vec![];
        while entries.len() < 500 {
            match dir.next_entry().await {
                Ok(Some(e)) => {
                    let kind = e
                        .file_type()
                        .await
                        .map(|t| {
                            if t.is_dir() {
                                "dir"
                            } else if t.is_symlink() {
                                "symlink"
                            } else {
                                "file"
                            }
                        })
                        .unwrap_or("unknown");
                    entries.push(serde_json::json!({
                        "name": e.file_name().to_string_lossy(),
                        "kind": kind,
                    }));
                }
                Ok(None) => break,
                Err(e) => return Err(ToolError::Execution(e.to_string())),
            }
        }
        Ok(serde_json::json!({ "path": path, "entries": entries }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_then_read_round_trip() {
        let dir = std::env::temp_dir().join("soryos-tools-test");
        let path = dir.join("hello.txt");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        let w = WriteFileTool;
        let out = w
            .execute(serde_json::json!({ "path": path.to_string_lossy(), "content": "bonjour" }))
            .await
            .unwrap();
        assert_eq!(out["bytes_written"], 7);
        let r = ReadFileTool;
        let out = r
            .execute(serde_json::json!({ "path": path.to_string_lossy() }))
            .await
            .unwrap();
        assert_eq!(out["content"], "bonjour");
        let l = ListDirTool;
        let out = l
            .execute(serde_json::json!({ "path": dir.to_string_lossy() }))
            .await
            .unwrap();
        assert!(!out["entries"].as_array().unwrap().is_empty());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn missing_field_is_invalid_input() {
        let r = ReadFileTool;
        let err = r.execute(serde_json::json!({})).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }
}
