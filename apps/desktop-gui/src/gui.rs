//! SoryOS AI Assistant — Desktop GUI.
//!
//! Three-panel layout: sidebar | center | right panel.
//! Dark theme matching the reference design.

use std::sync::Arc;
use std::time::Duration;

use assistant_core::{
    AssistantEvent, ChatMessage, Conversation, ConversationId, ConversationStore, MessageRole,
    VoiceState,
};
use cosmic::app::{ApplicationExt, Core, Task};
use cosmic::iced::{Length, Subscription};
use cosmic::{executor, iced, widget, Action, Application, Element};
use soryos_ui::{group_history, sidebar::summarize, App as UiApp, Effect, UiEvent};

use crate::ctx::{self, GuiCtx, GuiEvent};
use crate::envfile;
use crate::sory_theme as st;

pub struct Flags {
    pub ctx: Arc<GuiCtx>,
    pub provider_states: Vec<ProviderLite>,
}

#[derive(Debug, Clone)]
pub struct ProviderLite {
    pub name: String,
    pub status: String,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    InputChanged(String),
    Send,
    Stop,
    NewChat,
    OpenConversation(ConversationId),
    ConversationLoaded(Result<Conversation, String>),
    SidebarLoaded(Vec<soryos_ui::ConversationSummary>),
    ToggleProviderPanel(String),
    ApiKeyChanged(String),
    ToggleShowKey,
    ProviderModelChanged(String),
    ProviderBaseUrlChanged(String),
    SaveProviderKey,
    ClearProviderKey,
    RefreshModels,
    ModelsLoaded(String, Result<Vec<String>, String>),
    UseModel(String),
    ReloadProviders,
    HistoryFilterChanged(String),
    DeleteConversation(ConversationId),
    PromptSuggestion(String),
    VoicePressed,
    VoiceDoneBridge(usize),
    ConfirmAllow,
    ConfirmDeny,
    ToggleSidebar,
    ToggleModelSelector,
    NavigateTo(String),
    Ignore,
}

struct PendingConfirm {
    tool: String,
    reason: String,
}

pub struct SoryApp {
    core: Core,
    ctx: Arc<GuiCtx>,
    ui: UiApp,
    current: Conversation,
    pending_text: Option<String>,
    pending_confirm: Option<PendingConfirm>,
    providers: Vec<ProviderLite>,
    expanded_provider: Option<String>,
    api_key_input: String,
    show_key: bool,
    provider_model_input: String,
    provider_base_url_input: String,
    history_filter: String,
    provider_models: std::collections::HashMap<String, Vec<String>>,
    models_loading: Option<String>,
    models_error: Option<String>,
    status: String,
    show_model_selector: bool,
    sidebar_visible: bool,
    active_nav: String,
    title_set: bool,
    smoke_done: bool,
}

// ── Helpers ──

fn spacer_h(h: f32) -> Element<'static, Message> {
    widget::space().height(Length::Fixed(h)).into()
}

fn spacer_w(w: f32) -> Element<'static, Message> {
    widget::space().width(Length::Fixed(w)).into()
}

fn spacer_fill() -> Element<'static, Message> {
    widget::space().width(Length::Fill).into()
}

fn divider() -> Element<'static, Message> {
    widget::container(spacer_h(1.0))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(
            |_t: &cosmic::Theme| cosmic::iced::widget::container::Style {
                background: Some(cosmic::iced::Background::Color(st::DIVIDER)),
                ..Default::default()
            },
        )
        .into()
}

impl SoryApp {
    fn drain_queue(&mut self) -> Task<Message> {
        let events: Vec<GuiEvent> = {
            let mut q = self.ctx.queue.lock().unwrap();
            q.drain(..).collect()
        };
        let mut tasks = vec![];
        for event in events {
            match event {
                GuiEvent::Core(e) => self.on_core_event(e),
                GuiEvent::TurnFinished(outcome) => tasks.push(self.on_turn_finished(outcome)),
            }
        }
        Task::batch(tasks)
    }

    fn on_core_event(&mut self, event: AssistantEvent) {
        match event {
            AssistantEvent::TextChunk(delta) => {
                self.ui.update(UiEvent::StreamChunk(delta));
            }
            AssistantEvent::ToolRequested { name, .. } => {
                self.status = format!("Outil : {name}");
                self.ui.update(UiEvent::ToolCallStarted { name });
            }
            AssistantEvent::ConfirmationRequired { tool, reason } => {
                self.pending_confirm = Some(PendingConfirm { tool, reason });
            }
            AssistantEvent::ConfirmationDecided { tool, approved } => {
                self.pending_confirm = None;
                self.status = if approved {
                    format!("{tool} approuvé")
                } else {
                    format!("{tool} refusé")
                };
            }
            AssistantEvent::ToolCompleted { name, is_error } => {
                self.status = if is_error {
                    format!("{name} erreur")
                } else {
                    format!("{name} ok")
                };
            }
            AssistantEvent::Done(_) => {}
        }
    }

    fn on_turn_finished(&mut self, outcome: crate::ctx::TurnOutcome) -> Task<Message> {
        self.current = outcome.conversation;
        self.pending_text = None;
        match outcome.result {
            Ok(response) => {
                self.status.clear();
                self.ui.update(UiEvent::StreamDone {
                    content: response.content,
                });
            }
            Err(error) => {
                self.status.clear();
                self.ui.update(UiEvent::StreamError(error));
            }
        }
        self.refresh_sidebar()
    }

    fn refresh_sidebar(&self) -> Task<Message> {
        let store = self.ctx.store.clone();
        Task::perform(
            async move { summarize(&store.list(50).await.unwrap_or_default()) },
            |list| Action::App(Message::SidebarLoaded(list)),
        )
    }

    fn refresh_providers(&mut self) {
        let active = self.ctx.assistant.lock().unwrap().provider_name();
        self.providers = ai_providers::config::all_provider_states()
            .into_iter()
            .map(|s| ProviderLite {
                active: s.name == active,
                name: s.name,
                status: s.status.label().to_string(),
            })
            .collect();
    }

    fn select_provider(&mut self, name: &str, announce: bool) -> Task<Message> {
        let already_active = self.ctx.assistant.lock().unwrap().provider_name() == name;
        if !already_active {
            if let Some(kind) = ai_providers::ProviderKind::parse(name) {
                let provider = ai_providers::provider_from_env(kind);
                if provider.status() == assistant_core::provider::ProviderStatus::Ready {
                    self.ctx.assistant.lock().unwrap().set_provider(provider);
                    if announce {
                        self.status = format!("Provider actif : {name}");
                    }
                } else if announce {
                    self.status = format!("« {name} » non configuré.");
                }
            }
        }
        let active = self.ctx.assistant.lock().unwrap().provider_name();
        for row in &mut self.providers {
            row.active = row.name == active;
        }
        let settings = self.ctx.settings.clone();
        let name = name.to_string();
        Task::perform(
            async move {
                settings.set("provider", &name).await.ok();
            },
            |_| Action::App(Message::Ignore),
        )
    }

    fn save_provider_key(&mut self) -> Task<Message> {
        let Some(name) = self.expanded_provider.clone() else {
            return Task::none();
        };
        let Some(kind) = ai_providers::ProviderKind::parse(&name) else {
            return Task::none();
        };
        if let Some((key_var, model_var)) = envfile::provider_env_vars(&name) {
            let key = self.api_key_input.trim().to_string();
            if !key.is_empty() {
                if let Err(e) = envfile::write_env_key(key_var, &key) {
                    self.status = format!("Erreur .env : {e}");
                    return Task::none();
                }
            }
            let model = self.provider_model_input.trim().to_string();
            if !model.is_empty() {
                if let Err(e) = envfile::write_env_key(model_var, &model) {
                    self.status = format!("Erreur .env : {e}");
                    return Task::none();
                }
            }
            if name == "local" {
                let base = self.provider_base_url_input.trim().to_string();
                if !base.is_empty() {
                    if let Err(e) = envfile::write_env_key(envfile::LOCAL_BASE_URL_VAR, &base) {
                        self.status = format!("Erreur .env : {e}");
                        return Task::none();
                    }
                }
            }
        }
        self.api_key_input.clear();
        let provider = ai_providers::provider_from_env(kind);
        if provider.status() == assistant_core::provider::ProviderStatus::Ready {
            self.ctx.assistant.lock().unwrap().set_provider(provider);
            let model = self.provider_model_input.trim().to_string();
            if !model.is_empty() {
                let (temperature, max_tokens) = {
                    let a = self.ctx.assistant.lock().unwrap();
                    (a.config().temperature, a.config().max_tokens)
                };
                self.ctx.assistant.lock().unwrap().set_generation_params(
                    model,
                    temperature,
                    max_tokens,
                );
            }
            self.status = format!("« {name} » configuré.");
        } else {
            self.status = format!("Clé enregistrée, « {name} » indisponible.");
        }
        self.refresh_providers();
        let settings = self.ctx.settings.clone();
        Task::perform(
            async move {
                settings.set("provider", &name).await.ok();
            },
            |_| Action::App(Message::Ignore),
        )
    }

    fn spawn_turn(&mut self, text: String) -> Task<Message> {
        self.pending_text = Some(text.clone());
        let ctx = self.ctx.clone();
        let conversation = self.current.clone();
        let handle = tokio::spawn(async move { ctx::run_turn(ctx, conversation, text).await });
        *self.ctx.turn.lock().unwrap() = Some(handle);
        Task::none()
    }

    fn active_provider_name(&self) -> String {
        self.ctx.assistant.lock().unwrap().provider_name()
    }

    fn active_model_label(&self) -> String {
        let model = self
            .ctx
            .assistant
            .lock()
            .unwrap()
            .config()
            .default_model
            .clone();
        let provider = self.active_provider_name();
        if provider == "openrouter" && model == "openrouter/free" {
            "Automatic — Free".to_string()
        } else if model.is_empty() {
            "modèle par défaut".to_string()
        } else {
            model
        }
    }

    // ─────────── SIDEBAR ───────────

    fn sidebar_view(&self) -> Element<'_, Message> {
        let state = self.ui.state();

        let logo = widget::Row::new()
            .push(
                widget::container(widget::text::heading("S").class(st::ACCENT))
                    .width(Length::Fixed(36.0))
                    .height(Length::Fixed(36.0))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill),
            )
            .push(
                widget::Column::new()
                    .push(widget::text::heading("SoryOS"))
                    .push(widget::text::caption(
                        "Votre assistant IA, plus intelligent.",
                    ))
                    .spacing(2),
            )
            .spacing(10)
            .align_y(iced::Alignment::Center);

        let nav_items = [
            ("accueil", "🏠 Accueil", true),
            ("conversations", "💬 Conversations", false),
            ("outils", "🔧 Outils", false),
            ("fichiers", "📁 Fichiers", false),
            ("parametres", "⚙ Paramètres", false),
        ];

        let mut nav_column = widget::Column::new().spacing(2);
        for (id, label, _) in nav_items {
            let _is_active = self.active_nav == id;
            let btn = widget::button::text(label)
                .on_press(Message::NavigateTo(id.to_string()))
                .width(Length::Fill);
            nav_column = nav_column.push(btn);
        }

        let shortcuts_label = widget::text::caption("RACCOURCIS").class(st::TEXT_TERTIARY);

        let shortcut_items = [
            ("new_chat", "+ Nouveau chat"),
            ("search", "🔍 Rechercher"),
            ("voice", "🎤 Mode vocal"),
            ("images", "🖼 Images"),
            ("code", "</> Code"),
            ("docs", "📄 Documents"),
            ("web", "🌐 Web"),
        ];

        let mut shortcuts_col = widget::Column::new().spacing(2);
        for (id, label) in shortcut_items {
            shortcuts_col = shortcuts_col.push(
                widget::button::text(label)
                    .on_press(Message::NavigateTo(id.to_string()))
                    .width(Length::Fill),
            );
        }

        let history_label = widget::text::caption("CONVERSATIONS").class(st::TEXT_TERTIARY);
        let search = widget::text_input::search_input("Rechercher…", &self.history_filter)
            .on_input(Message::HistoryFilterChanged);
        let history = history_view(&state.conversations, &self.history_filter, state.active_id);

        let user_area = widget::container(
            widget::Row::new()
                .push(
                    widget::container(widget::text::body("S").class(st::ACCENT))
                        .width(Length::Fixed(32.0))
                        .height(Length::Fixed(32.0))
                        .center_x(Length::Fill)
                        .center_y(Length::Fill),
                )
                .push(
                    widget::Column::new()
                        .push(widget::text::body("SoryOS"))
                        .push(widget::text::caption("Toujours là pour vous."))
                        .spacing(2),
                )
                .spacing(10)
                .align_y(iced::Alignment::Center),
        )
        .padding(10)
        .width(Length::Fill)
        .style(st::user_area);

        let sidebar_content = widget::Column::new()
            .push(logo)
            .push(spacer_h(8.0))
            .push(nav_column)
            .push(spacer_h(12.0))
            .push(shortcuts_label)
            .push(shortcuts_col)
            .push(spacer_h(12.0))
            .push(history_label)
            .push(search)
            .push(spacer_h(4.0))
            .push(history)
            .push(spacer_h(8.0))
            .push(user_area)
            .spacing(6)
            .padding(14)
            .width(Length::Fixed(st::SIDEBAR_WIDTH));

        widget::container(widget::scrollable(sidebar_content).height(Length::Fill))
            .width(Length::Fixed(st::SIDEBAR_WIDTH))
            .height(Length::Fill)
            .style(st::sidebar_bg)
            .into()
    }

    // ─────────── CENTER ───────────

    fn center_view(&self) -> Element<'_, Message> {
        let state = self.ui.state();

        let top_bar = widget::container(
            widget::Row::new()
                .push(widget::text::body("🔍").class(st::TEXT_SECONDARY))
                .push(spacer_w(4.0))
                .push(
                    widget::text_input::search_input("Rechercher sur SoryOS…", String::new())
                        .on_input(|_| Message::Ignore),
                )
                .push(spacer_fill())
                .push(widget::text::body("☀").class(st::TEXT_SECONDARY))
                .push(spacer_w(8.0))
                .push(widget::text::body("🔔").class(st::TEXT_SECONDARY))
                .push(spacer_w(8.0))
                .push(widget::text::body("Utilisateur").class(st::TEXT_PRIMARY))
                .push(spacer_w(4.0))
                .push(widget::text::caption("En ligne").class(st::STATUS_GREEN))
                .spacing(4)
                .align_y(iced::Alignment::Center),
        )
        .padding(iced::Padding::from([8, 16]))
        .width(Length::Fill)
        .height(Length::Fixed(st::TOP_BAR_HEIGHT));

        let is_welcome = state.chat.messages.is_empty() && state.chat.streaming_text.is_empty();

        let center_content = if is_welcome {
            self.welcome_view()
        } else {
            self.chat_view()
        };

        let input_bar = self.input_bar_view();
        let status_bar = self.status_bar_view();

        widget::Column::new()
            .push(top_bar)
            .push(
                widget::container(center_content)
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .push(input_bar)
            .push(status_bar)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn welcome_view(&self) -> Element<'_, Message> {
        let logo = widget::container(widget::text::heading("S").class(st::ACCENT))
            .width(Length::Fixed(120.0))
            .height(Length::Fixed(120.0))
            .center_x(Length::Fill)
            .center_y(Length::Fill);

        let cards = widget::Row::new()
            .push(self.action_card(
                "💬",
                "Discuter",
                "Posez vos questions, obtenez des réponses précises et utiles.",
            ))
            .push(self.action_card(
                "</>",
                "Coder",
                "Développez plus rapidement avec l'aide de l'IA.",
            ))
            .push(self.action_card(
                "💡",
                "Explorer",
                "Découvrez, apprenez, soyez plus productif.",
            ))
            .push(self.action_card(
                "✨",
                "Créer",
                "Générez du contenu, des idées et bien plus encore.",
            ))
            .spacing(16)
            .width(Length::Fill);

        widget::Column::new()
            .push(spacer_h(40.0))
            .push(logo)
            .push(spacer_h(16.0))
            .push(widget::text::heading("SoryOS").class(st::TEXT_PRIMARY))
            .push(spacer_h(6.0))
            .push(
                widget::text::body("Votre assistant IA, plus intelligent.")
                    .class(st::TEXT_SECONDARY),
            )
            .push(spacer_h(32.0))
            .push(cards)
            .spacing(8)
            .align_x(iced::Alignment::Center)
            .width(Length::Fill)
            .into()
    }

    fn action_card<'a>(
        &self,
        icon: &'a str,
        title: &'a str,
        desc: &'a str,
    ) -> Element<'a, Message> {
        let content = widget::Column::new()
            .push(widget::text::heading(icon).class(st::ACCENT))
            .push(spacer_h(12.0))
            .push(widget::text::body(title).class(st::TEXT_PRIMARY))
            .push(spacer_h(4.0))
            .push(widget::text::caption(desc).class(st::TEXT_SECONDARY))
            .spacing(0)
            .width(Length::Fill);

        widget::button::custom(content)
            .on_press(Message::PromptSuggestion(title.to_string()))
            .width(Length::Fill)
            .height(Length::Fixed(160.0))
            .into()
    }

    fn chat_view(&self) -> Element<'_, Message> {
        let state = self.ui.state();
        let mut messages = widget::Column::new().spacing(12).padding(18);

        for message in state.chat.visible_messages() {
            if message.role == MessageRole::System {
                continue;
            }
            messages = messages.push(message_bubble(&message));
        }

        if let Some(pending) = &self.pending_confirm {
            messages = messages.push(confirm_card(pending));
        }

        widget::scrollable(messages)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    fn input_bar_view(&self) -> Element<'_, Message> {
        let state = self.ui.state();
        let generating = state.chat.generating;

        let input = widget::text_input::text_input("Écrivez votre message à SoryOS…", &state.input)
            .on_input(Message::InputChanged)
            .on_submit(|_| Message::Send);

        let send_btn = if generating {
            widget::button::custom(widget::text::body("⏹").class(st::TEXT_PRIMARY))
                .on_press(Message::Stop)
        } else {
            widget::button::custom(widget::text::body("➤").class(st::TEXT_PRIMARY))
                .on_press(Message::Send)
        };

        let input_row = widget::Row::new()
            .push(widget::text::body("📎").class(st::TEXT_SECONDARY))
            .push(spacer_w(4.0))
            .push(input.width(Length::Fill))
            .push(spacer_w(4.0))
            .push(widget::text::body("⚙").class(st::TEXT_SECONDARY))
            .push(spacer_w(4.0))
            .push(
                widget::button::custom(widget::text::body("🎤").class(st::TEXT_SECONDARY))
                    .on_press(Message::VoicePressed),
            )
            .push(spacer_w(4.0))
            .push(send_btn)
            .spacing(4)
            .align_y(iced::Alignment::Center);

        widget::container(input_row)
            .padding(iced::Padding::from([10, 14]))
            .width(Length::Fill)
            .height(Length::Fixed(st::INPUT_BAR_HEIGHT))
            .style(st::input_bar_bg)
            .into()
    }

    fn status_bar_view(&self) -> Element<'_, Message> {
        let state = self.ui.state();
        let generating = state.chat.generating;
        let status_text = if generating {
            "Génération…"
        } else if !self.status.is_empty() {
            &self.status
        } else {
            "Système prêt"
        };

        let dot_color = if generating {
            st::ACCENT
        } else {
            st::STATUS_GREEN
        };

        widget::container(
            widget::Row::new()
                .push(
                    widget::Row::new()
                        .push(widget::text::body("●").class(dot_color))
                        .push(spacer_w(6.0))
                        .push(widget::text::caption(status_text).class(st::TEXT_SECONDARY))
                        .align_y(iced::Alignment::Center),
                )
                .push(spacer_fill())
                .push(widget::text::caption("SoryOS v1.0").class(st::TEXT_TERTIARY))
                .align_y(iced::Alignment::Center),
        )
        .padding(iced::Padding::from([4, 16]))
        .width(Length::Fill)
        .height(Length::Fixed(st::STATUS_BAR_HEIGHT))
        .style(st::status_bar_bg)
        .into()
    }

    // ─────────── RIGHT PANEL ───────────

    fn right_panel_view(&self) -> Element<'_, Message> {
        let active_provider = self.active_provider_name();
        let _active_model = self.active_model_label();

        let provider_label = widget::text::caption("Modèle IA").class(st::TEXT_SECONDARY);

        let provider_selector = widget::button::custom(
            widget::Row::new()
                .push(widget::text::body("⇄").class(st::TEXT_PRIMARY))
                .push(spacer_w(6.0))
                .push(widget::text::body(active_provider.clone()).class(st::TEXT_PRIMARY))
                .push(spacer_fill())
                .push(widget::text::body("▾").class(st::TEXT_SECONDARY))
                .align_y(iced::Alignment::Center),
        )
        .on_press(Message::ToggleModelSelector)
        .width(Length::Fill);

        let auto_free = widget::container(
            widget::Row::new()
                .push(
                    widget::container(widget::text::body("⚡").class(st::ACCENT))
                        .width(Length::Fixed(40.0))
                        .height(Length::Fixed(40.0))
                        .center_x(Length::Fill)
                        .center_y(Length::Fill),
                )
                .push(
                    widget::Column::new()
                        .push(
                            widget::Row::new()
                                .push(
                                    widget::text::body("Automatic — Free").class(st::TEXT_PRIMARY),
                                )
                                .push(spacer_w(6.0))
                                .push(widget::text::caption("FREE").class(st::SUCCESS))
                                .align_y(iced::Alignment::Center),
                        )
                        .push(
                            widget::text::caption(
                                "OpenRouter choisit automatiquement parmi les modèles gratuits",
                            )
                            .class(st::TEXT_SECONDARY),
                        )
                        .spacing(4),
                )
                .spacing(10)
                .align_y(iced::Alignment::Center),
        )
        .padding(10)
        .width(Length::Fill)
        .style(st::model_card_highlighted);

        let model_search = widget::container(
            widget::Row::new()
                .push(widget::text::body("🔍").class(st::TEXT_SECONDARY))
                .push(spacer_w(6.0))
                .push(
                    widget::text_input::text_input("Rechercher un modèle…", "")
                        .on_input(|_| Message::Ignore),
                )
                .align_y(iced::Alignment::Center),
        )
        .padding(8)
        .width(Length::Fill)
        .style(st::search_bar);

        let models_label = widget::text::caption("Modèles gratuits").class(st::TEXT_SECONDARY);

        let mut models_list = widget::Column::new().spacing(4);

        if let Some(models) = self.provider_models.get("openrouter") {
            for model in models.iter().filter(|m| *m != "openrouter/free").take(8) {
                let display_name = model.split('/').last().unwrap_or(model);
                models_list = models_list.push(
                    widget::container(
                        widget::Row::new()
                            .push(widget::text::body("●").class(st::ACCENT))
                            .push(spacer_w(6.0))
                            .push(
                                widget::Column::new()
                                    .push(
                                        widget::text::body(format!("{display_name} (free)"))
                                            .class(st::TEXT_PRIMARY),
                                    )
                                    .push(
                                        widget::text::caption(model_info(model))
                                            .class(st::TEXT_SECONDARY),
                                    )
                                    .spacing(2)
                                    .width(Length::Fill),
                            )
                            .push(spacer_fill())
                            .push(
                                widget::button::custom(
                                    widget::text::caption("FREE").class(st::SUCCESS),
                                )
                                .on_press(Message::UseModel(model.clone())),
                            )
                            .spacing(8)
                            .align_y(iced::Alignment::Center),
                    )
                    .padding(iced::Padding::from([6, 8]))
                    .width(Length::Fill)
                    .style(st::card_bg),
                );
            }
        } else {
            models_list = models_list.push(
                widget::button::text("Charger les modèles gratuits")
                    .on_press(Message::RefreshModels)
                    .width(Length::Fill),
            );
        }

        let free_note = widget::text::caption(
            "Seuls les modèles gratuits sont affichés.\nAucun modèle payant n'est disponible.",
        )
        .class(st::TEXT_TERTIARY);

        let tools_section = self.section_row("🔧", "Outils", "8 outils activés", st::SUCCESS);
        let files_section =
            self.section_row("📁", "Fichiers", "Accès aux fichiers activé", st::SUCCESS);
        let voice_section = self.section_row("🎤", "Mode vocal", "Désactivé", st::TEXT_TERTIARY);
        let web_section = self.section_row("🌐", "Recherche web", "Activé", st::SUCCESS);

        let panel_content = widget::Column::new()
            .push(provider_label)
            .push(spacer_h(6.0))
            .push(provider_selector)
            .push(spacer_h(10.0))
            .push(auto_free)
            .push(spacer_h(10.0))
            .push(model_search)
            .push(spacer_h(10.0))
            .push(models_label)
            .push(models_list)
            .push(spacer_h(8.0))
            .push(free_note)
            .push(spacer_h(16.0))
            .push(divider())
            .push(spacer_h(12.0))
            .push(tools_section)
            .push(spacer_h(6.0))
            .push(files_section)
            .push(spacer_h(6.0))
            .push(voice_section)
            .push(spacer_h(6.0))
            .push(web_section)
            .spacing(0)
            .padding(14)
            .width(Length::Fixed(st::RIGHT_PANEL_WIDTH));

        widget::container(widget::scrollable(panel_content).height(Length::Fill))
            .width(Length::Fixed(st::RIGHT_PANEL_WIDTH))
            .height(Length::Fill)
            .style(st::right_panel_bg)
            .into()
    }

    fn section_row<'a>(
        &self,
        icon: &'a str,
        title: &'a str,
        subtitle: &'a str,
        color: iced::Color,
    ) -> Element<'a, Message> {
        let content = widget::Row::new()
            .push(widget::text::body(icon).class(color))
            .push(spacer_w(10.0))
            .push(
                widget::Column::new()
                    .push(widget::text::body(title).class(st::TEXT_PRIMARY))
                    .push(widget::text::caption(subtitle).class(st::TEXT_SECONDARY))
                    .spacing(2)
                    .width(Length::Fill),
            )
            .push(spacer_fill())
            .push(widget::text::body("›").class(st::TEXT_SECONDARY))
            .spacing(4)
            .align_y(iced::Alignment::Center);

        widget::button::custom(content)
            .on_press(Message::Ignore)
            .width(Length::Fill)
            .into()
    }

    // ─────────── MODEL SELECTOR POPUP ───────────

    fn model_selector_panel(&self) -> Element<'_, Message> {
        let expanded = self.expanded_provider.clone().unwrap_or_default();
        let active = self.active_provider_name();

        let mut panel = widget::Column::new().spacing(6);
        for provider in &self.providers {
            let mark = if provider.name == active {
                "●"
            } else {
                "○"
            };
            let display = if provider.name == "openrouter" {
                format!("{mark} OpenRouter · Free only")
            } else {
                format!("{mark} {}", provider.name)
            };
            panel = panel.push(
                widget::button::text(display)
                    .on_press(Message::ToggleProviderPanel(provider.name.clone()))
                    .width(Length::Fill),
            );
            if expanded == provider.name {
                panel = panel.push(self.provider_config_panel(&provider.name));
            }
        }

        widget::container(panel)
            .padding(12)
            .width(Length::Fill)
            .style(st::elevated_bg)
            .into()
    }

    fn provider_config_panel(&self, name: &str) -> Element<'_, Message> {
        let mut panel = widget::Column::new().spacing(8).padding(8);
        if name == "mock" {
            return panel.push(widget::text::caption("Mode démo.")).into();
        }
        if name == "openrouter" {
            panel = panel.push(
                widget::text::caption("Free only : seuls les modèles gratuits seront utilisés.")
                    .class(st::TEXT_SECONDARY),
            );
        }
        let key_set = envfile::provider_env_vars(name)
            .map(|(key_var, _)| envfile::has_key(key_var))
            .unwrap_or(false);
        panel = panel.push(
            widget::text::caption(if key_set {
                "Clé API : enregistrée"
            } else {
                "Clé API : absente"
            })
            .class(st::TEXT_SECONDARY),
        );

        let key_field = if self.show_key {
            widget::text_input::text_input("sk-…", &self.api_key_input)
                .on_input(Message::ApiKeyChanged)
                .on_submit(|_| Message::SaveProviderKey)
        } else {
            widget::text_input::secure_input(
                "Coller la clé API…",
                &self.api_key_input,
                Some(Message::ToggleShowKey),
                true,
            )
            .on_input(Message::ApiKeyChanged)
            .on_submit(|_| Message::SaveProviderKey)
        };
        panel = panel.push(
            widget::Row::new()
                .push(key_field.width(Length::Fill))
                .push(spacer_w(4.0))
                .push(
                    widget::button::text(if self.show_key { "masquer" } else { "voir" })
                        .on_press(Message::ToggleShowKey),
                )
                .spacing(4),
        );
        if name == "local" {
            panel = panel.push(
                widget::text_input::text_input(
                    "URL (ex. http://localhost:11434/v1)",
                    &self.provider_base_url_input,
                )
                .on_input(Message::ProviderBaseUrlChanged)
                .on_submit(|_| Message::SaveProviderKey),
            );
        }
        let placeholder = if name == "openrouter" {
            "openrouter/free"
        } else {
            "modèle (optionnel)"
        };
        panel = panel.push(
            widget::text_input::text_input(placeholder, &self.provider_model_input)
                .on_input(Message::ProviderModelChanged)
                .on_submit(|_| Message::SaveProviderKey),
        );
        if name == "openrouter" {
            if let Some(models) = self.provider_models.get("openrouter") {
                panel = panel.push(
                    widget::text::caption(format!("{} modèles gratuits", models.len()))
                        .class(st::TEXT_SECONDARY),
                );
            }
        }
        panel = panel.push(
            widget::Row::new()
                .push(widget::button::suggested("Enregistrer").on_press(Message::SaveProviderKey))
                .push(spacer_w(8.0))
                .push(widget::button::text("Effacer").on_press(Message::ClearProviderKey))
                .spacing(8),
        );
        panel.into()
    }
}

impl Application for SoryApp {
    type Executor = executor::Default;
    type Flags = Flags;
    type Message = Message;
    const APP_ID: &'static str = "com.soryos.AiAssistant";

    fn core(&self) -> &Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let app = SoryApp {
            core,
            ctx: flags.ctx,
            ui: UiApp::new(),
            current: Conversation::new("Nouvelle conversation"),
            pending_text: None,
            pending_confirm: None,
            providers: flags.provider_states,
            expanded_provider: None,
            api_key_input: String::new(),
            show_key: false,
            provider_model_input: String::new(),
            provider_base_url_input: String::new(),
            history_filter: String::new(),
            provider_models: std::collections::HashMap::new(),
            models_loading: None,
            models_error: None,
            status: String::new(),
            show_model_selector: false,
            sidebar_visible: std::env::var("SORYOS_GUI_NOSIDEBAR").is_err(),
            smoke_done: std::env::var("SORYOS_GUI_SMOKE").is_err(),
            active_nav: "accueil".to_string(),
            title_set: false,
        };
        let sidebar = app.refresh_sidebar();
        (app, Task::batch(vec![Task::none(), sidebar]))
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        iced::time::every(Duration::from_millis(60)).map(|_| Message::Tick)
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::Ignore => Task::none(),
            Message::Tick => {
                if !self.smoke_done {
                    self.smoke_done = true;
                    self.ui.update(UiEvent::InputChanged("Bonjour".to_string()));
                    return self.update(Message::Send);
                }
                if !self.title_set {
                    if let Some(id) = self.core.main_window_id() {
                        self.title_set = true;
                        self.set_header_title("SoryOS AI Assistant".to_string());
                        let events = self.drain_queue();
                        return Task::batch(vec![
                            self.set_window_title("SoryOS AI Assistant".to_string(), id),
                            events,
                        ]);
                    }
                }
                self.drain_queue()
            }
            Message::InputChanged(text) => {
                self.ui.update(UiEvent::InputChanged(text));
                Task::none()
            }
            Message::Send => match self.ui.update(UiEvent::SendMessage) {
                Effect::SendToAssistant(text) => {
                    self.status = "Génération…".into();
                    self.spawn_turn(text)
                }
                _ => Task::none(),
            },
            Message::Stop => {
                if let Some(handle) = self.ctx.turn.lock().unwrap().take() {
                    handle.abort();
                }
                if let Some(text) = self.pending_text.take() {
                    self.current.push(ChatMessage::user(text));
                }
                self.status = "Interrompu.".into();
                self.ui.update(UiEvent::StreamError("Interrompu.".into()));
                Task::none()
            }
            Message::NewChat => {
                self.current = Conversation::new("Nouvelle conversation");
                self.pending_text = None;
                self.pending_confirm = None;
                self.ui.update(UiEvent::NewConversation);
                self.status.clear();
                Task::none()
            }
            Message::OpenConversation(id) => {
                let store = self.ctx.store.clone();
                Task::perform(
                    async move { store.get(id).await.map_err(|e| e.to_string()) },
                    |r| Action::App(Message::ConversationLoaded(r)),
                )
            }
            Message::ConversationLoaded(Ok(c)) => {
                let id = c.id;
                self.current = c.clone();
                self.ui.update(UiEvent::ConversationOpened {
                    id,
                    messages: c.messages,
                });
                Task::none()
            }
            Message::ConversationLoaded(Err(e)) => {
                self.status = format!("Erreur : {e}");
                Task::none()
            }
            Message::SidebarLoaded(list) => {
                self.ui.update(UiEvent::ConversationsLoaded(list));
                Task::none()
            }
            Message::ToggleProviderPanel(name) => {
                if self.expanded_provider.as_deref() == Some(&name) {
                    self.expanded_provider = None;
                    return Task::none();
                }
                self.api_key_input.clear();
                self.show_key = false;
                let active_model = self
                    .ctx
                    .assistant
                    .lock()
                    .unwrap()
                    .config()
                    .default_model
                    .clone();
                let stored = envfile::read_env_file(&envfile::env_path());
                let (model_default, base_default) = match name.as_str() {
                    "local" => (
                        stored
                            .get("LOCAL_AI_MODEL")
                            .cloned()
                            .unwrap_or_else(|| "qwen2.5:7b".into()),
                        stored
                            .get(envfile::LOCAL_BASE_URL_VAR)
                            .cloned()
                            .unwrap_or_else(|| "http://localhost:11434/v1".into()),
                    ),
                    _ => (
                        envfile::provider_env_vars(&name)
                            .and_then(|(_, m)| stored.get(m).cloned())
                            .unwrap_or_default(),
                        String::new(),
                    ),
                };
                self.provider_model_input = if self.ctx.assistant.lock().unwrap().provider_name()
                    == name
                    && !active_model.is_empty()
                {
                    active_model
                } else {
                    model_default
                };
                self.provider_base_url_input = base_default;
                self.expanded_provider = Some(name.clone());
                self.select_provider(&name, false)
            }
            Message::ApiKeyChanged(t) => {
                self.api_key_input = t;
                Task::none()
            }
            Message::ToggleShowKey => {
                self.show_key = !self.show_key;
                Task::none()
            }
            Message::ProviderModelChanged(t) => {
                self.provider_model_input = t;
                Task::none()
            }
            Message::ProviderBaseUrlChanged(t) => {
                self.provider_base_url_input = t;
                Task::none()
            }
            Message::SaveProviderKey => self.save_provider_key(),
            Message::RefreshModels => {
                let Some(name) = self.expanded_provider.clone() else {
                    return Task::none();
                };
                if ai_providers::ProviderKind::parse(&name).is_none() {
                    return Task::none();
                }
                self.models_loading = Some(name.clone());
                self.models_error = None;
                Task::perform(
                    async move {
                        let kind = ai_providers::ProviderKind::parse(&name).unwrap();
                        let p = ai_providers::provider_from_env(kind);
                        (name, p.fetch_models().await.map_err(|e| e.to_string()))
                    },
                    |(n, r)| Action::App(Message::ModelsLoaded(n, r)),
                )
            }
            Message::ModelsLoaded(name, result) => {
                self.models_loading = None;
                match result {
                    Ok(m) => {
                        self.models_error = None;
                        self.provider_models.insert(name, m);
                    }
                    Err(e) => {
                        self.models_error = Some(e);
                    }
                }
                Task::none()
            }
            Message::UseModel(model) => {
                let (temp, max) = {
                    let a = self.ctx.assistant.lock().unwrap();
                    (a.config().temperature, a.config().max_tokens)
                };
                self.ctx
                    .assistant
                    .lock()
                    .unwrap()
                    .set_generation_params(model.clone(), temp, max);
                self.provider_model_input = model.clone();
                self.status = format!("Modèle actif : {model}");
                if let Some(name) = self.expanded_provider.clone() {
                    if let Some((_, mv)) = envfile::provider_env_vars(&name) {
                        if let Err(e) = envfile::write_env_key(mv, &model) {
                            self.status = format!("Modèle actif : {model} (.{e})");
                        }
                    }
                }
                let settings = self.ctx.settings.clone();
                Task::perform(
                    async move {
                        settings.set("model", &model).await.ok();
                    },
                    |_| Action::App(Message::Ignore),
                )
            }
            Message::ReloadProviders => {
                let count = envfile::reload_env_file();
                self.refresh_providers();
                let current = self.ctx.assistant.lock().unwrap().provider_name();
                let current_ready = ai_providers::ProviderKind::parse(&current)
                    .map(|k| ai_providers::provider_from_env(k).status())
                    == Some(assistant_core::provider::ProviderStatus::Ready);
                if !current_ready {
                    let best = ai_providers::config::default_provider_from_env();
                    if best.name() != current {
                        self.ctx
                            .assistant
                            .lock()
                            .unwrap()
                            .set_provider(best.clone());
                        self.refresh_providers();
                        self.status =
                            format!("Clés rechargées ({count}). Provider : {}.", best.name());
                        return Task::none();
                    }
                }
                self.status = format!("Clés rechargées ({count}).");
                Task::none()
            }
            Message::ClearProviderKey => {
                if let Some(name) = self.expanded_provider.clone() {
                    if let Some((kv, _)) = envfile::provider_env_vars(&name) {
                        if envfile::write_env_key(kv, "").is_ok() {
                            self.status = format!("Clé {kv} supprimée.");
                        }
                    }
                    self.api_key_input.clear();
                    self.refresh_providers();
                }
                Task::none()
            }
            Message::HistoryFilterChanged(t) => {
                self.history_filter = t;
                Task::none()
            }
            Message::DeleteConversation(id) => {
                if self.current.id == id {
                    self.current = Conversation::new("Nouvelle conversation");
                    self.pending_text = None;
                    self.pending_confirm = None;
                    self.ui.update(UiEvent::NewConversation);
                }
                let store = self.ctx.store.clone();
                Task::perform(
                    async move {
                        store.delete(id).await.ok();
                        summarize(&store.list(50).await.unwrap_or_default())
                    },
                    |l| Action::App(Message::SidebarLoaded(l)),
                )
            }
            Message::PromptSuggestion(text) => {
                self.ui.update(UiEvent::InputChanged(text));
                Task::none()
            }
            Message::VoicePressed => {
                let last = self
                    .ui
                    .state()
                    .chat
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == MessageRole::Assistant)
                    .map(|m| m.content.clone())
                    .unwrap_or_default();
                if last.is_empty() {
                    self.status = "Rien à lire.".into();
                    return Task::none();
                }
                self.ui
                    .update(UiEvent::VoiceStateChanged(VoiceState::Speaking));
                self.status = "Lecture…".into();
                let ctx = self.ctx.clone();
                Task::perform(
                    async move {
                        ctx.tts
                            .synthesize(&last)
                            .await
                            .map(|w| w.len())
                            .unwrap_or(0)
                    },
                    |b| Action::App(Message::VoiceDoneBridge(b)),
                )
            }
            Message::VoiceDoneBridge(b) => {
                self.status = format!("TTS : {b} octets.");
                self.ui.update(UiEvent::VoiceStateChanged(VoiceState::Idle));
                Task::none()
            }
            Message::ConfirmAllow => {
                ctx::resolve_confirm(&self.ctx, true);
                self.pending_confirm = None;
                Task::none()
            }
            Message::ConfirmDeny => {
                ctx::resolve_confirm(&self.ctx, false);
                self.pending_confirm = None;
                Task::none()
            }
            Message::ToggleSidebar => {
                self.sidebar_visible = !self.sidebar_visible;
                Task::none()
            }
            Message::ToggleModelSelector => {
                self.show_model_selector = !self.show_model_selector;
                Task::none()
            }
            Message::NavigateTo(nav) => {
                self.active_nav = nav;
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let sidebar: Element<'_, Message> = if self.sidebar_visible {
            self.sidebar_view()
        } else {
            widget::container(spacer_h(0.0))
                .width(Length::Fixed(0.0))
                .into()
        };

        let right = self.right_panel_view();
        let mut center = self.center_view();

        if self.show_model_selector {
            let selector = self.model_selector_panel();
            center = widget::Column::new()
                .push(selector)
                .push(center)
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }

        widget::Row::new()
            .push(sidebar)
            .push(center)
            .push(right)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

// ── Free functions ──

fn model_info(model: &str) -> String {
    if model.contains("gpt") || model.contains("oss") {
        "OpenAI · gratuit".into()
    } else if model.contains("llama") {
        "Meta · gratuit".into()
    } else if model.contains("qwen") {
        "Alibaba · gratuit".into()
    } else if model.contains("deepseek") {
        "DeepSeek · gratuit".into()
    } else if model.contains("gemma") {
        "Google · gratuit".into()
    } else {
        "modèle gratuit".into()
    }
}

fn history_view<'a>(
    rows: &'a [soryos_ui::ConversationSummary],
    filter: &'a str,
    active: Option<ConversationId>,
) -> Element<'a, Message> {
    let groups = group_history(rows, filter);
    if groups.is_empty() {
        return widget::text::caption(if filter.trim().is_empty() {
            "Aucune conversation."
        } else {
            "Aucun résultat."
        })
        .class(st::TEXT_TERTIARY)
        .into();
    }
    let mut column = widget::Column::new().spacing(4);
    for group in groups {
        column = column.push(widget::text::caption(group.label).class(st::TEXT_TERTIARY));
        for item in group.items {
            let mark = if Some(item.id) == active { "▌" } else { " " };
            column = column.push(
                widget::Row::new()
                    .push(
                        widget::button::text(format!("{mark} {}", item.title))
                            .on_press(Message::OpenConversation(item.id))
                            .width(Length::Fill),
                    )
                    .push(widget::button::text("×").on_press(Message::DeleteConversation(item.id)))
                    .spacing(2),
            );
        }
    }
    column.into()
}

fn message_bubble(message: &ChatMessage) -> Element<'static, Message> {
    let label = match message.role {
        MessageRole::User => "Vous",
        MessageRole::Assistant => "SoryOS",
        MessageRole::Tool => "Outil",
        MessageRole::System => "Système",
    };

    let mut content = widget::Column::new()
        .push(
            widget::Row::new()
                .push(widget::text::caption(label).class(st::TEXT_SECONDARY))
                .spacing(10)
                .align_y(iced::Alignment::Center),
        )
        .push(spacer_h(4.0))
        .push(widget::text::body(message.content.clone()).class(st::TEXT_PRIMARY))
        .spacing(4);

    for call in &message.tool_calls {
        content = content.push(
            widget::text::caption(format!("outil: {} {}", call.name, call.arguments))
                .class(st::TEXT_TERTIARY),
        );
    }

    widget::container(content)
        .padding(iced::Padding::from([12, 14]))
        .width(Length::Fill)
        .style(st::card_bg)
        .into()
}

fn confirm_card(pending: &PendingConfirm) -> Element<'_, Message> {
    let card = widget::Column::new()
        .push(widget::text::body(format!(
            "L'assistant veut exécuter « {} ».",
            pending.tool
        )))
        .push(widget::text::caption(pending.reason.clone()).class(st::TEXT_SECONDARY))
        .push(spacer_h(8.0))
        .push(
            widget::Row::new()
                .push(widget::button::suggested("Autoriser").on_press(Message::ConfirmAllow))
                .push(spacer_w(8.0))
                .push(widget::button::text("Refuser").on_press(Message::ConfirmDeny))
                .spacing(8),
        )
        .spacing(8)
        .padding(12);
    widget::container(card)
        .width(Length::Fill)
        .style(st::card_bg)
        .into()
}
