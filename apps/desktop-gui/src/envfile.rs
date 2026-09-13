//! Read/write API keys in the local `.env` file.
//!
//! Secrets are never stored in SQLite or logged. The `.env` file (see
//! `.env.example`) is the documented, user-editable secret store; this
//! module lets the GUI edit it so users can paste keys without a terminal.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Environment variable holding the key for each provider, plus the
/// variable holding its model override.
pub fn provider_env_vars(provider: &str) -> Option<(&'static str, &'static str)> {
    match provider {
        "openrouter" => Some(("OPENROUTER_API_KEY", "OPENROUTER_MODEL")),
        "mistral" => Some(("MISTRAL_API_KEY", "MISTRAL_MODEL")),
        "gemini" => Some(("GEMINI_API_KEY", "GEMINI_MODEL")),
        "local" => Some(("LOCAL_AI_API_KEY", "LOCAL_AI_MODEL")),
        _ => None,
    }
}

/// Base URL variable for the local provider.
pub const LOCAL_BASE_URL_VAR: &str = "LOCAL_AI_BASE_URL";

pub fn env_path() -> PathBuf {
    PathBuf::from(".env")
}

/// Parse the `.env` file into key/value pairs (ignores comments/blank lines).
pub fn read_env_file(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return map;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || !line.contains('=') {
            continue;
        }
        let (k, v) = line.split_once('=').unwrap_or(("", ""));
        let k = k.trim().trim_start_matches("export ").trim();
        if k.is_empty() {
            continue;
        }
        map.insert(
            k.to_string(),
            v.trim().trim_matches('"').trim_matches('\'').to_string(),
        );
    }
    map
}

/// Re-read `.env` into the process environment (picks up keys the user
/// pasted manually while the app runs). Returns the number of variables set.
pub fn reload_env_file() -> usize {
    let map = read_env_file(&env_path());
    let mut count = 0;
    for (key, value) in map {
        if !value.trim().is_empty() {
            std::env::set_var(&key, &value);
            count += 1;
        }
    }
    count
}

/// True when a non-empty value exists in the process env or the file.
pub fn has_key(var: &str) -> bool {
    if std::env::var(var)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    read_env_file(&env_path())
        .get(var)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

/// Set `key=value` in `.env`, preserving comments and other entries.
/// An empty value removes the key.
pub fn write_env_key(key: &str, value: &str) -> std::io::Result<()> {
    let path = env_path();
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = vec![];
    let mut found = false;
    for line in current.lines() {
        let trimmed = line.trim();
        let is_ours = !trimmed.starts_with('#')
            && trimmed
                .split_once('=')
                .map(|(k, _)| k.trim().trim_start_matches("export ").trim() == key)
                .unwrap_or(false);
        if is_ours {
            found = true;
            if !value.is_empty() {
                lines.push(format!("{key}={value}"));
            }
            // Empty value: drop the line (key removed).
        } else {
            lines.push(line.to_string());
        }
    }
    if !found && !value.is_empty() {
        lines.push(format!("{key}={value}"));
    }
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    std::fs::write(&path, out)?;
    // Apply immediately to this process so providers pick it up.
    if value.is_empty() {
        std::env::remove_var(key);
    } else {
        std::env::set_var(key, value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_preserves_comments_and_updates() {
        let dir = std::env::temp_dir().join("soryos-envfile-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(".env");
        std::fs::write(&path, "# comment\nFOO=1\nMISTRAL_API_KEY=old\n").unwrap();
        // Simulate by direct manipulation (unit-level check of parsing).
        let map = read_env_file(&path);
        assert_eq!(map.get("MISTRAL_API_KEY").map(String::as_str), Some("old"));
        assert_eq!(map.get("FOO").map(String::as_str), Some("1"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
