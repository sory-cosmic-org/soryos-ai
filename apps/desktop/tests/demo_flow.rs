//! End-to-end flow: User -> Assistant -> MockProvider -> Response,
//! and the secured tool loop: Assistant -> Tool -> Gate -> Confirmer.

use std::sync::Arc;

use assistant_core::{AllowAllConfirmer, Assistant, AssistantConfig, Conversation, MockProvider};
use soryos_security::SecurityPolicy;
use soryos_tools::SystemInfoTool;

fn assistant_with_tools(policy: SecurityPolicy) -> Assistant {
    let mut a = Assistant::new(
        Arc::new(MockProvider::new()),
        Arc::new(policy),
        AssistantConfig::default(),
    );
    a.register_tool(Arc::new(SystemInfoTool));
    a
}

#[tokio::test]
async fn demo_greeting_flow() {
    let a = assistant_with_tools(SecurityPolicy::permissive_for_tests());
    let mut conv = Conversation::new("Nouvelle conversation");
    let reply = a
        .respond_simple(&mut conv, "Bonjour", &AllowAllConfirmer)
        .await
        .unwrap();
    assert!(!reply.is_empty());
    // User + assistant messages persisted in the conversation.
    assert_eq!(conv.messages.len(), 2);
}

#[tokio::test]
async fn tool_loop_runs_through_security_gate() {
    // Default policy: system_info is read-only -> allowed without confirmation.
    let a = assistant_with_tools(SecurityPolicy::default());
    let mut conv = Conversation::new("Nouvelle conversation");
    let reply = a
        .respond_simple(
            &mut conv,
            "Quelle heure est-il ? [use-tool]",
            &AllowAllConfirmer,
        )
        .await
        .unwrap();
    assert!(
        reply.contains("outil") || reply.contains("résultat"),
        "unexpected reply: {reply}"
    );
    // The history contains the tool round-trip (user, assistant+call, tool, final).
    assert!(conv.messages.len() >= 4);
}

#[tokio::test]
async fn denied_tool_yields_error_result_not_execution() {
    use assistant_core::{Confirmer, ToolDecision, ToolGate};

    struct DenyAll;
    #[async_trait::async_trait]
    impl ToolGate for DenyAll {
        async fn authorize(&self, _tool: &str, _input: &serde_json::Value) -> ToolDecision {
            ToolDecision::Deny {
                reason: "test policy".to_string(),
            }
        }
    }
    struct NoConfirm;
    #[async_trait::async_trait]
    impl Confirmer for NoConfirm {
        async fn confirm(&self, _t: &str, _i: &serde_json::Value, _r: &str) -> bool {
            false
        }
    }

    let mut a = Assistant::new(
        Arc::new(MockProvider::new()),
        Arc::new(DenyAll),
        AssistantConfig::default(),
    );
    a.register_tool(Arc::new(SystemInfoTool));
    let mut conv = Conversation::new("Nouvelle conversation");
    let reply = a
        .respond_simple(&mut conv, "système [use-tool]", &NoConfirm)
        .await
        .unwrap();
    assert!(reply.contains("test policy"), "unexpected reply: {reply}");
}
