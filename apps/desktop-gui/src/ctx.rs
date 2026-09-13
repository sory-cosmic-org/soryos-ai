//! Shared GUI context: assistant handle, event queue, confirmation slot.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use assistant_core::{Assistant, AssistantEvent, ChatResponse, Confirmer, Conversation};
use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

use soryos_storage::{SettingsStore, SqliteConversationStore};

/// Outcome of a finished turn, delivered to the UI thread.
#[derive(Debug)]
pub struct TurnOutcome {
    pub conversation: Conversation,
    pub result: Result<ChatResponse, String>,
}

/// Events produced by background turn tasks, drained on each UI tick.
#[derive(Debug)]
pub enum GuiEvent {
    Core(AssistantEvent),
    TurnFinished(TurnOutcome),
}

pub type EventQueue = Arc<Mutex<VecDeque<GuiEvent>>>;

/// Rendez-vous between the turn task (awaiting a decision) and the dialog.
#[derive(Debug, Default)]
pub struct ConfirmSlot {
    pub sender: Option<oneshot::Sender<bool>>,
    /// Decision clicked before the task stored its sender (rare race).
    pub pre: Option<bool>,
}

/// Everything background tasks need. `Assistant` is cheap to clone.
pub struct GuiCtx {
    pub assistant: Mutex<Assistant>,
    pub queue: EventQueue,
    pub confirm: Mutex<ConfirmSlot>,
    pub turn: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub store: Arc<SqliteConversationStore>,
    pub settings: SettingsStore,
    pub tts: Arc<dyn assistant_core::TextToSpeech>,
}

/// [`Confirmer`] implementation wired to the GUI dialog.
pub struct GuiConfirmer {
    pub ctx: Arc<GuiCtx>,
}

#[async_trait]
impl Confirmer for GuiConfirmer {
    async fn confirm(&self, _tool: &str, _input: &serde_json::Value, _reason: &str) -> bool {
        let rx = {
            let mut slot = self.ctx.confirm.lock().unwrap();
            if let Some(decided) = slot.pre.take() {
                return decided;
            }
            let (tx, rx) = oneshot::channel();
            slot.sender = Some(tx);
            rx
        };
        rx.await.unwrap_or(false)
    }
}

/// Resolve a pending confirmation from the dialog buttons.
pub fn resolve_confirm(ctx: &GuiCtx, approved: bool) {
    let mut slot = ctx.confirm.lock().unwrap();
    if let Some(tx) = slot.sender.take() {
        let _ = tx.send(approved);
    } else {
        slot.pre = Some(approved);
    }
}

pub fn push(ctx: &GuiCtx, event: GuiEvent) {
    ctx.queue.lock().unwrap().push_back(event);
}

/// Run one assistant turn in the background, forwarding progress events.
pub async fn run_turn(ctx: Arc<GuiCtx>, mut conversation: Conversation, text: String) {
    use assistant_core::ConversationStore as _;

    // Upsert the conversation so turns on a fresh chat persist it.
    let _ = ctx.store.save(&conversation).await;

    let assistant = ctx.assistant.lock().unwrap().clone();
    let confirmer = GuiConfirmer { ctx: ctx.clone() };
    let (tx, mut rx) = mpsc::channel::<AssistantEvent>(64);
    let queue = ctx.queue.clone();
    let pump = tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            queue.lock().unwrap().push_back(GuiEvent::Core(event));
        }
    });
    let result = assistant
        .respond(&mut conversation, &text, &confirmer, &tx)
        .await
        .map_err(|e| e.to_string());
    drop(tx);
    let _ = pump.await;
    push(
        &ctx,
        GuiEvent::TurnFinished(TurnOutcome {
            conversation,
            result,
        }),
    );
    *ctx.turn.lock().unwrap() = None;
}
