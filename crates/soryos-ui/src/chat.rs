//! Chat rendering helpers (shared by every frontend).

use assistant_core::{ChatMessage, MessageRole};

/// One-line prefix per role for plain-text frontends.
pub fn role_label(role: MessageRole) -> &'static str {
    match role {
        MessageRole::System => "⚙️ Système",
        MessageRole::User => "🧑 Vous",
        MessageRole::Assistant => "🤖 Assistant",
        MessageRole::Tool => "🔧 Outil",
    }
}

/// Render a message as plain text with fenced code blocks preserved.
pub fn format_message(message: &ChatMessage) -> String {
    let mut out = format!("{}:\n{}", role_label(message.role), message.content);
    if !message.tool_calls.is_empty() {
        for call in &message.tool_calls {
            out.push_str(&format!("\n  ⚙️ {} {}", call.name, call.arguments));
        }
    }
    out
}

/// Render the full application state as a terminal snapshot (used by the
/// desktop REPL and by tests).
pub fn render_snapshot(state: &crate::state::AppState) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "╭─ SoryOS AI Assistant ─ {} ─────────",
        state.active_title()
    );
    let _ = writeln!(out, "│ Conversations: {}", state.conversations.len());
    for m in state.chat.visible_messages() {
        if m.role == MessageRole::System {
            continue;
        }
        let _ = writeln!(out, "├─ {}", format_message(&m));
    }
    if state.chat.generating {
        let _ = writeln!(out, "│ … génération en cours (streaming) …");
    }
    if let Some(err) = &state.chat.last_error {
        let _ = writeln!(out, "│ ⚠️ Erreur : {err}");
    }
    let _ = writeln!(
        out,
        "╰─ {} {} ─────────────────────────",
        state.voice.icon(),
        state.voice.label()
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_contains_messages() {
        let state = crate::state::AppState {
            chat: crate::state::ChatView {
                messages: vec![ChatMessage::user("hello")],
                ..Default::default()
            },
            ..Default::default()
        };
        let snap = render_snapshot(&state);
        assert!(snap.contains("hello"));
    }
}
