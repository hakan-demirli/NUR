pub mod builtin;
pub mod dialogs;
pub mod input;
pub mod messages;
pub mod models;
pub mod permission;
pub mod prompt_ui;
pub mod question;
pub mod runtime;
pub mod scroll;
pub mod session;
pub mod sidebar_state;
pub mod slash;
pub mod theme_state;
pub mod transcript;

pub use builtin::{Agent, AgentIndex, Agents, Command, EmptyAgentsError, PromptInfo};
pub use dialogs::DialogState;
pub use input::{CompletionOutcome, InputState, PromptPart, PromptPartKind};
pub use messages::MessageStore;
pub use models::{ModelState, RECENT_MODELS_CAP};
pub use permission::{PermissionModalState, PermissionStage};
pub use prompt_ui::PromptUiState;
pub use question::QuestionModalState;
pub use runtime::{Clock, FixedClock, RuntimeState, SystemClock};
pub use scroll::ScrollState;
pub use session::SessionState;
pub use sidebar_state::SidebarUiState;
pub use slash::SlashCommand;
pub use theme_state::ThemeState;

use std::collections::{HashMap, HashSet, VecDeque};

use crossterm::event::{KeyCode, KeyModifiers};

use crate::action::{Action, HostAction, Lifecycle, Toast, ToastVariant, UserAction, ViewAction};
use crate::dialog::{Dialog, DialogAction, DialogKind, DialogOption, DialogPayload};
use crate::event::Event;
use crate::model::{Message, Sender};
use crate::provider::ModelRef;
use crate::ui::theme::{Mode as ThemeMode, ThemeName};

pub(crate) const RENDER_MESSAGE_TAIL_LIMIT: usize = 100;
const TRANSCRIPT_CACHE_SESSION_LIMIT: usize = 8;
const RENDER_CACHE_RETAIN_MESSAGE_LIMIT: usize = RENDER_MESSAGE_TAIL_LIMIT;

struct TranscriptSnapshot {
    messages: Vec<Message>,
    compaction_message_ids: HashSet<String>,
}

pub struct App {
    pub input: InputState,
    pub messages: MessageStore,
    pub scroll: ScrollState,
    pub theme: ThemeState,
    pub dialogs: DialogState,
    pub agents: Agents,
    pub models: ModelState,
    pub sidebar: SidebarUiState,
    pub prompt: PromptUiState,
    pub sessions: SessionState,
    pub permissions: PermissionModalState,
    pub questions: QuestionModalState,
    pub runtime: RuntimeState,
    pub last_text_width: usize,
    input_has_paste: bool,
    transcript_cache: HashMap<String, TranscriptSnapshot>,
    transcript_cache_lru: VecDeque<String>,
}

impl App {
    pub fn new() -> Self {
        Self::with_clock(Box::new(SystemClock))
    }

    pub fn with_clock(clock: Box<dyn Clock>) -> Self {
        let mut app = Self {
            input: InputState::new(),
            messages: MessageStore::new(),
            scroll: ScrollState::new(),
            theme: ThemeState::new(),
            dialogs: DialogState::new(builtin::builtin_commands()),
            agents: builtin::default_agents(),
            models: ModelState::new(),
            sidebar: SidebarUiState::new(),
            prompt: PromptUiState::new(),
            sessions: SessionState::new(),
            permissions: PermissionModalState::new(),
            questions: QuestionModalState::new(),
            runtime: RuntimeState::new(clock),
            last_text_width: 80,
            input_has_paste: false,
            transcript_cache: HashMap::new(),
            transcript_cache_lru: VecDeque::new(),
        };
        app.dialogs
            .rebuild_slash_completion(&mut app.input.completion);
        app.models.load_from_disk();
        app.sessions.load_from_disk();
        let persisted_hidden = app.models.thinking_hidden.unwrap_or(true);
        app.messages
            .set_thinking_hidden_from_persisted(persisted_hidden);
        app
    }

    pub fn with_user_themes() -> Self {
        let mut app = Self::new();
        app.theme = ThemeState::with_user_themes();
        app
    }

    pub fn with_user_themes_and_mode(mode: ThemeMode) -> Self {
        let mut app = Self::new();
        app.theme = ThemeState::with_user_themes_and_mode(mode);
        app
    }

    pub fn with_user_themes_and_detection(
        theme_name: Option<&str>,
        mode: Option<ThemeMode>,
    ) -> Self {
        let mut app = Self::new();
        app.theme = ThemeState::with_user_themes_and_detection(theme_name, mode);
        app
    }

    pub fn should_quit(&self) -> bool {
        self.runtime.should_quit()
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        self.runtime.take_events()
    }

    pub fn dispatch(&mut self, action: Action) {
        match action {
            Action::User(a) => self.dispatch_user(a),
            Action::View(a) => self.dispatch_view(a),
            Action::Host(a) => self.dispatch_host(a),
            Action::Lifecycle(a) => self.dispatch_lifecycle(a),
        }
    }

    fn dispatch_user(&mut self, action: UserAction) {
        match action {
            UserAction::Key { code, mods } => {
                if self.permissions.is_active()
                    && self
                        .permissions
                        .handle_key(&mut self.runtime.events, code, mods)
                {
                    return;
                }
                if self.questions.is_active()
                    && self
                        .questions
                        .handle_key(&mut self.runtime.events, code, mods)
                {
                    return;
                }
                if self.dialogs.dialog.is_some() {
                    self.handle_dialog_key(code, mods);
                } else {
                    self.handle_key(code, mods);
                }
            }
            UserAction::SubmitInput => self.submit_input(),
            UserAction::Interrupt => self.runtime.push(Event::Interrupt),
            UserAction::PasteText(text) => self.handle_paste(text),
            UserAction::ClearInput => {
                self.clear_input();
            }
            UserAction::MouseScroll { lines } => self.scroll.on_mouse_scroll(lines),
        }
    }

    fn dispatch_view(&mut self, action: ViewAction) {
        match action {
            ViewAction::OpenCommandPalette => self.open_command_palette(),
            ViewAction::OpenHelp => self.open_help(),
            ViewAction::OpenThemePicker => self.open_theme_picker(),
            ViewAction::OpenAgentPicker => self.open_agent_picker(),
            ViewAction::OpenModelPicker => self.open_model_picker(),
            ViewAction::OpenVariantPicker => self.open_variant_picker(),
            ViewAction::OpenSessionPicker => self.open_session_picker(),
            ViewAction::OpenSessionRename(session_id) => self.open_session_rename(session_id),
            ViewAction::OpenMessageActions(id) => self.open_message_actions(id),
            ViewAction::OpenForkPicker => self.open_fork_picker(),
            ViewAction::CloseDialog => self.close_dialog(false),

            ViewAction::SetTheme(name) => self.apply_theme(&name, true),
            ViewAction::SetThemeMode(mode) => self.set_theme_mode(mode),
            ViewAction::ToggleThemeMode => self.toggle_theme_mode(),

            ViewAction::CycleAgent(delta) => self.cycle_agent(delta),
            ViewAction::SetAgent(name) => self.set_agent(&name),

            ViewAction::SetModel(m) => self.set_model(m, true),
            ViewAction::SetVariant(v) => self.set_variant(v),
            ViewAction::CycleModelRecent(d) => self.cycle_model_recent(d),
            ViewAction::CycleModelFavorite(d) => self.cycle_model_favorite(d),
            ViewAction::CycleVariant => self.cycle_variant(),

            ViewAction::ToggleSidebar => self.sidebar.toggle_visible(),
            ViewAction::ScrollSidebar(delta) => self.sidebar.scroll_sidebar(delta as isize),
            ViewAction::ToggleSidebarSection(slot) => self.sidebar.toggle_section(slot),
            ViewAction::ToggleTimestamps => self.messages.toggle_timestamps(),
            ViewAction::ToggleToolExpanded { id } => {
                self.messages.toggle_tool_expanded(&id);
            }

            ViewAction::CopyLastAssistantMessage => self.copy_last_assistant_message(),
            ViewAction::CopySessionTranscript => self.copy_session_transcript(),
            ViewAction::ExportSession => self.export_session(),
            ViewAction::OpenDocs => self
                .runtime
                .push(Event::OpenUrl("https://opencode.ai/docs".into())),

            ViewAction::SwitchSession(id) => self.switch_session(id),
            ViewAction::PluginNavigateSession(id) => self.plugin_navigate_session(id),

            ViewAction::SubagentEnterFirstChild => self.subagent_enter_first_child(),
            ViewAction::SubagentGoToParent => self.subagent_go_to_parent(),
            ViewAction::SubagentCycleSibling(delta) => self.subagent_cycle_sibling(delta),

            ViewAction::Command(cmd) => self.run_command(cmd),
            ViewAction::ShowToast(toast) => self.dialogs.show_toast(toast),
            ViewAction::CopyToClipboard {
                text,
                success_message,
                error_message,
            } => self.runtime.push(Event::CopyToClipboard {
                text,
                success_message,
                error_message,
            }),
            ViewAction::ClearMessages => self.clear_messages(),
        }
    }

    fn dispatch_host(&mut self, action: HostAction) {
        match action {
            HostAction::SetSessions(entries) => self.sessions.set_sessions(entries),
            HostAction::RegisterPluginCommands(commands) => {
                if self.dialogs.register_plugin_commands(commands) {
                    self.dialogs
                        .rebuild_slash_completion(&mut self.input.completion);
                }
            }
            HostAction::OpenPluginSelect {
                callback_id,
                title,
                placeholder,
                options,
            } => self.open_plugin_select(callback_id, title, placeholder, options),
            HostAction::OpenPluginAlert { title, message } => {
                self.open_plugin_alert(title, message)
            }
            HostAction::ClearPluginDialog => self.clear_plugin_dialog(),
            HostAction::SetCurrentSession(id) => self.host_set_current_session(id),
            HostAction::ReplaceMessages(msgs) => self.host_replace_messages(msgs),
            HostAction::BindLastUserMessage { server_id, agent } => {
                self.messages.bind_first_untagged_user(server_id, agent);
            }
            HostAction::AppendMessage(msg) => self.host_append_message(msg),
            HostAction::UpsertToolCall(tool) => {
                self.messages.upsert_tool_call(*tool);
            }
            HostAction::UpdateTaskChild {
                parent_tool_id,
                child,
                child_tool_count,
            } => self
                .messages
                .update_task_child(&parent_tool_id, child, child_tool_count),
            HostAction::MarkCompaction { message_id, marker } => {
                let _ = self
                    .messages
                    .mark_compaction(message_id, marker, || self.runtime.now_hhmm());
            }
            HostAction::UpdateLastAssistantMeta {
                agent,
                model,
                provider_id,
                duration,
            } => self
                .messages
                .update_last_assistant_meta(agent, model, provider_id, duration),
            HostAction::SetLastAssistantError(err) => {
                let _ = self.messages.set_last_assistant_error(err);
            }
            HostAction::MarkAssistantInterrupted { message_id } => {
                self.messages.mark_assistant_interrupted(&message_id);
            }
            HostAction::UpsertSession(entry) => self.sessions.upsert_session(entry),
            HostAction::RemoveSession(id) => {
                self.sessions.remove_session(&id);
                self.remove_cached_transcript(&id);
            }
            HostAction::SetSessionBusy { session_id, busy } => {
                self.sessions.set_session_busy(&session_id, busy)
            }
            HostAction::SetSessionStatus { session_id, status } => {
                self.sessions.set_session_status(&session_id, status)
            }
            HostAction::SetVcsBranch(branch) => self.set_vcs_branch(branch),
            HostAction::SetWorkspaceCwd(cwd) => self.set_workspace_cwd(cwd),
            HostAction::RemoveMessage(id) => {
                self.messages.remove_by_server_id(&id);
            }
            HostAction::RemoveToolCall(id) => self.messages.remove_tool_call_by_id(&id),
            HostAction::SetSidebarTitle(title) => self.sidebar.set_title(title),
            HostAction::SetSidebarSubtitle(subtitle) => self.sidebar.set_subtitle(subtitle),
            HostAction::SetSidebarSections(sections) => self.sidebar.set_sections(sections),
            HostAction::SetSidebarVisible(visible) => self.sidebar.set_visible(visible),
            HostAction::SetSidebarFooterPath(path) => self.sidebar.set_footer_path(path),
            HostAction::SetBusy(busy) => self.prompt.set_busy(busy),
            HostAction::SetUsage(usage) => self.prompt.set_usage(usage),
            HostAction::SetCatalog(catalog) => self.set_catalog(catalog),
            HostAction::SetCurrentModel(model) => {
                let before = self.models.current_model.clone();
                let suggestion = model.clone();
                self.models.set_current_model(model);
                let after = self.models.current_model.clone();
                let ignored_suggestion = before == after && suggestion != after;
                if ignored_suggestion {
                    if let Some(kept) = after {
                        self.runtime.push(Event::ModelChanged {
                            model: kept,
                            variant: self.models.current_variant.clone(),
                        });
                    }
                }
            }

            HostAction::PermissionAsked(p) => self.permissions.host_asked(p),
            HostAction::PermissionDismissed(id) => self.permissions.host_dismissed(id),
            HostAction::QuestionAsked(p) => self.questions.host_asked(p),
            HostAction::QuestionDismissed(id) => self.questions.host_dismissed(id),

            HostAction::SystemMessage(content) => self.push_system_message(content),
            HostAction::AssistantDelta {
                text,
                thoughts,
                message_id,
            } => self.append_assistant_delta(&text, thoughts, message_id.as_deref()),
            HostAction::AssistantDone { message_id } => self
                .messages
                .finish_streaming_assistant(message_id.as_deref()),
        }
    }

    fn dispatch_lifecycle(&mut self, action: Lifecycle) {
        match action {
            Lifecycle::Tick => {
                self.input.cursor_visible = !self.input.cursor_visible;
                self.dialogs.tick_toast();
            }
            Lifecycle::Resize { cols, rows } => self.scroll.on_resize(cols, rows),
            Lifecycle::Quit => self.runtime.request_quit(),
        }
    }

    pub fn current_agent(&self) -> &Agent {
        self.agents.current()
    }

    pub fn set_agents(&mut self, agents: Vec<Agent>) {
        let _ = self.agents.try_replace(agents);
    }

    fn cycle_agent(&mut self, delta: i32) {
        if let Some(agent) = self.agents.cycle(delta) {
            let name = agent.name.clone();
            self.runtime.push(Event::AgentChanged(name));
        }
    }

    fn set_agent(&mut self, name: &str) {
        let Some(idx) = self.agents.iter().position(|a| a.name == name) else {
            self.push_system_message(format!("unknown agent: {name}"));
            return;
        };
        let cur = self.agents.current_index().get();
        if idx == cur {
            return;
        }
        let delta = idx as i32 - cur as i32;
        if let Some(agent) = self.agents.cycle(delta) {
            let chosen = agent.name.clone();
            self.runtime.push(Event::AgentChanged(chosen));
        }
    }

    pub fn push_system_message(&mut self, content: impl Into<String>) {
        let ts = self.runtime.now_hhmm();
        self.messages.push(Message::system(content, ts));
    }

    pub fn handle_paste(&mut self, text: String) {
        let normalised = text.replace("\r\n", "\n").replace('\r', "\n");
        let trimmed = normalised.trim();
        if !normalised.is_empty() {
            self.input_has_paste = true;
        }

        if !trimmed.is_empty() && !trimmed.contains('\n') {
            let candidate = unwrap_filepath_paste(trimmed);
            if let Some(filepath) = candidate {
                if let Some(part) = try_file_part_from_path(&filepath, &self.input.parts) {
                    let placeholder = part.0;
                    let kind = part.1;
                    self.input.insert_prompt_part(kind, placeholder);
                    return;
                }
            }
        }

        let line_count = normalised.matches('\n').count() + 1;
        if !trimmed.is_empty() && (line_count >= 3 || trimmed.len() > 150) {
            let placeholder = format!("[Pasted ~{line_count} lines]");
            self.input
                .insert_prompt_part(PromptPartKind::Text(normalised), placeholder);
            return;
        }

        self.input.paste_text(&normalised);
    }

    fn clear_input(&mut self) {
        self.input.clear();
        self.input_has_paste = false;
    }

    fn reset_paste_tracking_if_input_empty(&mut self) {
        if self.input.input.is_empty() {
            self.input_has_paste = false;
        }
    }

    pub fn set_workspace_cwd(&mut self, cwd: Option<String>) {
        self.sidebar.set_workspace_cwd(cwd);
        let composed = self.sidebar.recompose_workspace_footer();
        self.prompt.set_footer_right(composed);
    }

    pub fn set_vcs_branch(&mut self, branch: Option<String>) {
        self.sidebar.set_vcs_branch(branch);
        let composed = self.sidebar.recompose_workspace_footer();
        self.prompt.set_footer_right(composed);
    }

    fn host_set_current_session(&mut self, id: Option<String>) {
        match id {
            Some(id) => self.switch_current_session(id),
            None => {
                if self.sessions.sessions.current.is_some() {
                    self.stash_current_transcript();
                }
                self.sessions.set_current(None);
                self.messages.clear();
                self.messages.tool_block_rects.clear();
                self.messages.user_message_rects.clear();
                self.scroll.reset();
            }
        }
    }

    fn switch_current_session(&mut self, id: String) {
        if self.sessions.sessions.current.as_deref() == Some(id.as_str()) {
            return;
        }
        let adopting_new_session = self.sessions.sessions.current.is_none()
            && !self.messages.messages.is_empty()
            && !self.transcript_cache.contains_key(&id);
        self.stash_current_transcript();
        self.sessions.set_current(Some(id.clone()));
        if adopting_new_session {
            self.messages.tool_block_rects.clear();
            self.messages.user_message_rects.clear();
            self.scroll.reset();
        } else {
            self.restore_transcript_for_session(&id);
        }
    }

    fn stash_current_transcript(&mut self) {
        let Some(current) = self.sessions.sessions.current.clone() else {
            return;
        };
        let mut messages = std::mem::take(&mut self.messages.messages);
        trim_render_caches_to_tail(&mut messages);
        self.transcript_cache.insert(
            current.clone(),
            TranscriptSnapshot {
                messages,
                compaction_message_ids: std::mem::take(&mut self.messages.compaction_message_ids),
            },
        );
        self.touch_cached_transcript(current);
        self.messages.tool_block_rects.clear();
        self.messages.user_message_rects.clear();
    }

    fn restore_transcript_for_session(&mut self, id: &str) {
        if let Some(snapshot) = self.transcript_cache.remove(id) {
            self.transcript_cache_lru
                .retain(|cached_id| cached_id != id);
            self.messages.messages = snapshot.messages;
            self.messages.compaction_message_ids = snapshot.compaction_message_ids;
        } else {
            self.messages.clear();
        }
        self.messages.tool_block_rects.clear();
        self.messages.user_message_rects.clear();
        self.scroll.reset();
    }

    fn touch_cached_transcript(&mut self, id: String) {
        self.transcript_cache_lru
            .retain(|cached_id| cached_id != &id);
        self.transcript_cache_lru.push_back(id);
        while self.transcript_cache_lru.len() > TRANSCRIPT_CACHE_SESSION_LIMIT {
            if let Some(evicted) = self.transcript_cache_lru.pop_front() {
                self.transcript_cache.remove(&evicted);
            }
        }
    }

    fn remove_cached_transcript(&mut self, id: &str) {
        self.transcript_cache.remove(id);
        self.transcript_cache_lru
            .retain(|cached_id| cached_id != id);
    }

    fn invalidate_cached_transcript_render_caches(&mut self) {
        for snapshot in self.transcript_cache.values_mut() {
            for msg in &mut snapshot.messages {
                clear_message_render_caches(msg);
            }
        }
    }

    fn host_replace_messages(&mut self, messages: Vec<crate::action::HostMessage>) {
        let ts = self.runtime.now_hhmm();
        self.messages.host_replace(messages, || ts.clone());
        self.scroll.reset();
    }

    fn host_append_message(&mut self, message: crate::action::HostMessage) {
        let ts = self.runtime.now_hhmm();
        self.messages.host_append(message, || ts.clone());
    }

    fn append_assistant_delta(
        &mut self,
        text: &str,
        thoughts: bool,
        server_message_id: Option<&str>,
    ) {
        let ts = self.runtime.now_hhmm();
        let agent = Some(self.agents.current().name.clone());
        let model = self
            .models
            .current_model
            .as_ref()
            .map(|m| m.model_id.clone());
        let provider_id = self
            .models
            .current_model
            .as_ref()
            .map(|m| m.provider_id.clone());
        self.messages
            .append_assistant_delta(text, thoughts, server_message_id, || {
                Message::assistant_streaming_with_meta(ts, agent, model, provider_id)
            });
    }

    pub fn submit_input(&mut self) {
        if self.sessions.sessions.current_is_child() {
            return;
        }

        let raw = self.input.expand_for_submit();
        let raw = raw.trim().to_string();
        let raw_for_history = self.input.input.trim().to_string();
        if raw.is_empty() {
            return;
        }

        let is_command = raw_for_history.starts_with('/') && !self.input_has_paste;

        if !is_command && self.models.current_model.is_none() {
            self.push_system_message("Pick a model with /models before sending a message");
            if !self.models.catalog.is_empty() {
                self.open_model_picker();
            }
            self.scroll.stick_to_bottom();
            return;
        }

        let parts_for_history = self.input.parts.clone();
        self.input.push_history(&raw_for_history, parts_for_history);
        let file_parts = self.input.take_file_parts();
        self.input.input.clear();
        self.input.cursor_position = 0;
        self.input.parts.clear();
        self.input_has_paste = false;
        self.scroll.stick_to_bottom();

        if is_command {
            self.run_command(raw);
        } else {
            let ts = self.runtime.now_hhmm();
            self.messages.push(Message::user(&raw_for_history, &ts));
            let agent = Some(self.agents.current().name.clone());
            let model = self
                .models
                .current_model
                .as_ref()
                .map(|m| m.model_id.clone());
            let provider_id = self
                .models
                .current_model
                .as_ref()
                .map(|m| m.provider_id.clone());
            self.messages.push(Message::assistant_streaming_with_meta(
                ts,
                agent,
                model,
                provider_id,
            ));
            if file_parts.is_empty() {
                self.runtime.push(Event::UserMessage(raw));
            } else {
                let files = file_parts
                    .into_iter()
                    .filter_map(|p| match p.kind {
                        PromptPartKind::File {
                            mime,
                            filename,
                            filepath,
                            base64,
                        } => Some(crate::event::UserFileAttachment {
                            mime,
                            filename,
                            filepath,
                            base64,
                        }),
                        _ => None,
                    })
                    .collect();
                self.runtime
                    .push(Event::UserMessageWithFiles { text: raw, files });
            }
            self.scroll.stick_to_bottom();
        }
    }

    pub fn run_command(&mut self, cmd_input: String) {
        match SlashCommand::parse(&cmd_input) {
            SlashCommand::Empty => {}
            SlashCommand::Action(action) => self.dispatch(*action),
            SlashCommand::InvalidArg { detail, .. } => {
                self.push_system_message(detail);
            }
            SlashCommand::Unknown { name, args } => {
                if name == "undo" {
                    let id = self
                        .messages
                        .iter()
                        .rev()
                        .find(|m| m.sender == Sender::User && m.server_id.is_some())
                        .and_then(|m| m.server_id.clone());
                    match id {
                        Some(message_id) => {
                            self.runtime.push(Event::Undo { message_id });
                        }
                        None => {
                            self.push_system_message(
                                "Nothing to undo — no prior user message in this session."
                                    .to_string(),
                            );
                        }
                    }
                    return;
                }
                if name == "redo" {
                    self.runtime.push(Event::Redo);
                    return;
                }
                if name == "thinking" {
                    self.messages.toggle_thinking();
                    self.models.thinking_hidden = Some(self.messages.thinking_hidden);
                    let _ = self.models.save_to_disk();
                    return;
                }
                if name == "rename" {
                    let title = args.trim().to_string();
                    if title.is_empty() {
                        self.push_system_message("Usage: /rename <new title>".to_string());
                        return;
                    }
                    self.runtime.push(Event::Command {
                        name: "rename".into(),
                        args: title,
                    });
                    return;
                }
                if let Some(command) = self.dialogs.plugin_command_for_slash(&name) {
                    self.runtime.push(Event::PluginCommand {
                        name: command.name.clone(),
                        args,
                    });
                    return;
                }
                self.runtime.push(Event::Command { name, args });
            }
        }
    }

    fn clear_messages(&mut self) {
        self.messages.clear();
        self.scroll.reset();
        self.runtime.push(Event::Command {
            name: "clear".into(),
            args: String::new(),
        });
    }

    fn set_catalog(&mut self, catalog: crate::provider::ModelCatalog) {
        self.models.set_catalog(catalog);
        self.refresh_completion_sources();
    }

    fn refresh_completion_sources(&mut self) {
        self.input
            .completion
            .set_models(self.models.catalog_wire_refs());
        self.input
            .completion
            .set_variants(self.models.current_variant_list());
    }

    fn set_model(&mut self, model: ModelRef, emit_event: bool) {
        if !self.models.catalog.is_empty() && !self.models.catalog.has(&model) {
            self.push_system_message(format!(
                "unknown model: {}/{}",
                model.provider_id, model.model_id
            ));
            return;
        }
        let changed = self.models.current_model.as_ref() != Some(&model);
        self.models.current_model = Some(model.clone());
        if changed {
            self.models.current_variant = self.models.variant_map.get(&model.wire()).cloned();
        }
        self.models.touch_recent(&model);
        self.refresh_completion_sources();
        let _ = self.models.save_to_disk();
        if emit_event {
            self.runtime.push(Event::ModelChanged {
                model,
                variant: self.models.current_variant.clone(),
            });
        }
    }

    fn set_variant(&mut self, variant: Option<String>) {
        let Some(m) = self.models.current_model.clone() else {
            self.push_system_message("no model selected; pick one with /models first");
            return;
        };
        if let Some(v) = &variant {
            let known = self
                .models
                .catalog
                .find(&m)
                .map(|(_, mi)| mi.variants.iter().any(|x| x == v))
                .unwrap_or(true);
            if !known {
                self.push_system_message(format!("unknown variant: {v}"));
                return;
            }
        }
        if self.models.current_variant == variant {
            return;
        }
        self.models.set_current_variant(variant.clone());
        let _ = self.models.save_to_disk();
        self.runtime.push(Event::VariantChanged(variant));
    }

    fn cycle_model_recent(&mut self, delta: i32) {
        if let Some(pick) = self.models.pick_recent(delta) {
            self.set_model(pick, true);
        }
    }

    fn cycle_model_favorite(&mut self, delta: i32) {
        match self.models.pick_favorite(delta) {
            Ok(pick) => self.set_model(pick, true),
            Err(()) => self.push_system_message("no favorite models"),
        }
    }

    fn cycle_variant(&mut self) {
        let Some(next) = self.models.next_variant() else {
            return;
        };
        self.set_variant(next);
    }

    fn toggle_favorite_for_selected(&mut self) {
        let Some(dialog) = self.dialogs.dialog.as_ref() else {
            return;
        };
        if dialog.kind() != DialogKind::ModelPicker {
            return;
        }
        let Some(opt) = dialog.selected_option() else {
            return;
        };
        if opt.is_header || opt.value.is_empty() {
            return;
        }
        let Some(model) = ModelRef::parse(&opt.value) else {
            return;
        };
        self.models.toggle_favorite(model);
        let _ = self.models.save_to_disk();
        let options = self.models.build_picker_options();
        if let Some(dialog) = self.dialogs.dialog.as_mut() {
            dialog.replace_options(options);
        }
    }

    fn apply_theme(&mut self, name: &str, emit_event: bool) {
        let Some(validated) = self.theme.lookup(name) else {
            self.push_system_message(format!("unknown theme: {name}"));
            return;
        };
        self.apply_theme_name(validated, emit_event);
    }

    fn apply_theme_name(&mut self, name: ThemeName, emit_event: bool) {
        if self.theme.apply_theme_name(name.clone()) {
            for msg in self.messages.iter_mut() {
                msg.invalidate_render_cache();
            }
            self.invalidate_cached_transcript_render_caches();
        }
        if emit_event {
            self.runtime
                .push(Event::ThemeChanged(name.as_str().to_string()));
        }
    }

    fn toggle_theme_mode(&mut self) {
        let next = match self.theme.mode() {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        };
        self.set_theme_mode(next);
    }

    fn set_theme_mode(&mut self, mode: ThemeMode) {
        if self.theme.set_mode(mode) {
            for msg in self.messages.iter_mut() {
                msg.invalidate_render_cache();
            }
            self.invalidate_cached_transcript_render_caches();
            self.runtime.push(Event::ThemeModeChanged(mode));
        }
    }

    fn switch_session(&mut self, id: String) {
        if id.is_empty() {
            return;
        }
        if !self.sessions.has_session(&id) {
            self.push_system_message(format!("unknown session: {id}"));
            return;
        }
        self.permissions.clear();
        self.questions.clear();
        self.switch_current_session(id.clone());
        self.runtime.push(Event::SessionSwitched(id));
    }

    fn plugin_navigate_session(&mut self, id: String) {
        if id.is_empty() {
            return;
        }
        self.permissions.clear();
        self.questions.clear();
        self.switch_current_session(id.clone());
        self.runtime.push(Event::SessionSwitched(id));
    }

    fn subagent_enter_first_child(&mut self) {
        let Some(current) = self.sessions.sessions.current.clone() else {
            return;
        };
        let children = self.sessions.sessions.children_of(&current);
        let Some(first) = children.first() else {
            self.dialogs.show_toast(Toast::new(
                "No subagents to view from this session.",
                ToastVariant::Info,
            ));
            return;
        };
        let target = first.id.clone();
        self.subagent_navigate(target);
    }

    fn subagent_go_to_parent(&mut self) {
        let parent_id = match self.sessions.sessions.current_parent_id() {
            Some(p) => p.to_string(),
            None => return,
        };
        self.subagent_navigate(parent_id);
    }

    fn subagent_cycle_sibling(&mut self, delta: i32) {
        let Some(current) = self.sessions.sessions.current.clone() else {
            return;
        };
        let Some(parent_id) = self
            .sessions
            .sessions
            .get(&current)
            .and_then(|e| e.parent_id.clone())
        else {
            return;
        };
        let siblings_owned: Vec<String> = self
            .sessions
            .sessions
            .children_of(&parent_id)
            .iter()
            .map(|e| e.id.clone())
            .collect();
        if siblings_owned.len() <= 1 {
            return;
        }
        let idx = match siblings_owned.iter().position(|s| s == &current) {
            Some(i) => i,
            None => return,
        };
        let n = siblings_owned.len() as i32;
        let raw = idx as i32 + delta;
        let next = ((raw % n) + n) % n;
        let target = siblings_owned[next as usize].clone();
        if target == current {
            return;
        }
        self.subagent_navigate(target);
    }

    fn subagent_navigate(&mut self, id: String) {
        if id.is_empty() {
            return;
        }
        self.permissions.clear();
        self.questions.clear();
        self.switch_current_session(id.clone());
        self.runtime.push(Event::SubagentNavigate(id));
    }

    fn export_session(&mut self) {
        let markdown = transcript::export_markdown(&self.messages.messages);
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let suggested_filename = format!("session-{stamp}.md");
        self.runtime.push(Event::Export {
            suggested_filename,
            markdown,
        });
    }

    fn copy_session_transcript(&mut self) {
        let markdown = transcript::export_markdown(&self.messages.messages);
        self.runtime.push(Event::CopyToClipboard {
            text: markdown,
            success_message: "Session transcript copied to clipboard!".into(),
            error_message: "Failed to copy session transcript".into(),
        });
    }

    fn copy_last_assistant_message(&mut self) {
        let text = self
            .messages
            .iter()
            .rev()
            .find(|m| matches!(m.sender, Sender::Assistant))
            .map(|m| m.content.trim().to_string());
        let Some(text) = text else {
            self.dialogs.show_toast(Toast::new(
                "No assistant messages found",
                ToastVariant::Error,
            ));
            return;
        };
        if text.is_empty() {
            self.dialogs.show_toast(Toast::new(
                "No text content found in last assistant message",
                ToastVariant::Error,
            ));
            return;
        }
        self.runtime.push(Event::CopyToClipboard {
            text,
            success_message: "Message copied to clipboard!".into(),
            error_message: "Failed to copy to clipboard".into(),
        });
    }

    pub fn open_command_palette(&mut self) {
        let mut options: Vec<DialogOption> = self
            .dialogs
            .commands
            .iter()
            .map(|c| {
                let title = match &c.slash_name {
                    Some(s) => format!("{}  (/{})", c.title, s),
                    None => c.title.clone(),
                };
                DialogOption::new(title, c.name.clone())
            })
            .collect();
        options.extend(self.dialogs.plugin_commands.iter().map(|c| {
            let title = match c
                .slash_name
                .as_ref()
                .map(|slash| slash.trim().trim_start_matches('/'))
                .filter(|slash| !slash.is_empty())
            {
                Some(slash) => format!("{}  (/{})", c.title, slash),
                None => c.title.clone(),
            };
            DialogOption {
                title,
                value: c.name.clone(),
                description: c.description.clone(),
                category: c.category.clone(),
                disabled: false,
                is_header: false,
            }
        }));
        let parser: Box<dyn Fn(&str) -> DialogPayload + Send + Sync> =
            Box::new(|v: &str| DialogPayload::CommandPalette {
                current: v.to_string(),
            });
        self.dialogs.dialog = Some(Dialog::new(
            "Commands",
            DialogPayload::CommandPalette {
                current: String::new(),
            },
            options,
            parser,
        ));
    }

    fn open_help(&mut self) {
        self.open_plugin_alert(
            "Help".into(),
            "Press Ctrl+P to see all available actions and commands in any context.".into(),
        );
    }

    fn open_theme_picker(&mut self) {
        self.theme.snapshot_for_preview();
        let known_names: std::collections::HashSet<String> =
            self.theme.theme_registry.names().into_iter().collect();
        let options: Vec<DialogOption> = self
            .theme
            .theme_registry
            .names()
            .into_iter()
            .map(|n| DialogOption::new(n.clone(), n))
            .collect();
        let parser: Box<dyn Fn(&str) -> DialogPayload + Send + Sync> = Box::new(move |v: &str| {
            let current = if known_names.contains(v) {
                ThemeName::opencode_default_with(v)
            } else {
                ThemeName::opencode_default()
            };
            DialogPayload::ThemePicker { current }
        });
        let payload = DialogPayload::ThemePicker {
            current: self.theme.theme.name.clone(),
        };
        self.dialogs.dialog = Some(Dialog::new("Themes", payload, options, parser));
        self.preview_current_dialog_theme();
    }

    fn open_agent_picker(&mut self) {
        let current_name = self.current_agent().name.clone();
        let options: Vec<DialogOption> = self
            .agents
            .iter()
            .map(|a| DialogOption::new(a.name.clone(), a.name.clone()))
            .collect();
        let payload = DialogPayload::AgentPicker {
            current: current_name,
        };
        let parser: Box<dyn Fn(&str) -> DialogPayload + Send + Sync> =
            Box::new(|v: &str| DialogPayload::AgentPicker {
                current: v.to_string(),
            });
        self.dialogs.dialog = Some(Dialog::new("Select agent", payload, options, parser));
    }

    fn open_model_picker(&mut self) {
        if self.models.catalog.is_empty() {
            self.push_system_message(
                "Model catalog hasn't arrived yet — wait a moment and try again",
            );
            return;
        }
        let options = self.models.build_picker_options();
        let payload = DialogPayload::ModelPicker {
            current: self.models.current_model.clone(),
        };
        let parser: Box<dyn Fn(&str) -> DialogPayload + Send + Sync> =
            Box::new(|v: &str| DialogPayload::ModelPicker {
                current: ModelRef::parse(v),
            });
        let actions = vec![
            DialogAction {
                label: "Favorite".to_string(),
                key_hint: "ctrl+f".to_string(),
            },
            DialogAction {
                label: "Connect provider".to_string(),
                key_hint: "ctrl+a".to_string(),
            },
        ];
        self.dialogs.dialog =
            Some(Dialog::new("Select model", payload, options, parser).with_actions(actions));
    }

    fn open_variant_picker(&mut self) {
        let variants = self.models.current_variant_list();
        if variants.is_empty() {
            self.push_system_message("current model has no variants");
            return;
        }
        let mut options = vec![DialogOption::new("(default)", String::new())];
        for v in &variants {
            options.push(DialogOption::new(v.clone(), v.clone()));
        }
        let payload = DialogPayload::VariantPicker {
            current: self.models.current_variant.clone(),
        };
        let parser: Box<dyn Fn(&str) -> DialogPayload + Send + Sync> =
            Box::new(|v: &str| DialogPayload::VariantPicker {
                current: if v.is_empty() {
                    None
                } else {
                    Some(v.to_string())
                },
            });
        self.dialogs.dialog = Some(Dialog::new("Variants", payload, options, parser));
    }

    fn open_session_picker(&mut self) {
        if self.sessions.is_empty() {
            self.push_system_message(
                "no session list yet; host hasn't pushed one (bridge not connected)",
            );
            return;
        }
        self.sessions.session_delete_armed = None;
        let options = self.sessions.build_picker_options();
        let payload = DialogPayload::SessionPicker {
            current: self.sessions.sessions.current.clone(),
        };
        let parser: Box<dyn Fn(&str) -> DialogPayload + Send + Sync> =
            Box::new(|v: &str| DialogPayload::SessionPicker {
                current: if v.is_empty() {
                    None
                } else {
                    Some(v.to_string())
                },
            });
        let actions = vec![
            DialogAction {
                label: "pin/unpin".to_string(),
                key_hint: "ctrl+f".to_string(),
            },
            DialogAction {
                label: "delete".to_string(),
                key_hint: "ctrl+d".to_string(),
            },
            DialogAction {
                label: "rename".to_string(),
                key_hint: "ctrl+r".to_string(),
            },
        ];
        self.dialogs.dialog =
            Some(Dialog::new("Sessions", payload, options, parser).with_actions(actions));
    }

    fn open_session_rename(&mut self, session_id: Option<String>) {
        let Some(id) = session_id.or_else(|| self.sessions.sessions.current.clone()) else {
            self.dialogs.show_toast(Toast::new(
                "No active session to rename.",
                ToastVariant::Error,
            ));
            return;
        };
        let title = self
            .sessions
            .sessions
            .get(&id)
            .map(|s| s.title.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| id.clone());
        let payload = DialogPayload::SessionRename {
            session_id: id.clone(),
            title,
        };
        let parser_id = id.clone();
        let parser: Box<dyn Fn(&str) -> DialogPayload + Send + Sync> =
            Box::new(move |v: &str| DialogPayload::SessionRename {
                session_id: parser_id.clone(),
                title: v.to_string(),
            });
        self.dialogs.dialog = Some(Dialog::prompt("Rename Session", payload, parser));
    }

    pub fn open_fork_picker(&mut self) {
        if self.sessions.sessions.current.is_none() {
            self.push_system_message("No active session to fork.".to_string());
            return;
        }
        let mut options: Vec<DialogOption> = Vec::new();
        let mut full = DialogOption::new("Full session", "");
        full.description = Some("Fork at the current HEAD".to_string());
        options.push(full);

        let mut user_msgs: Vec<DialogOption> = Vec::new();
        for m in self.messages.iter() {
            if m.sender != Sender::User {
                continue;
            }
            let Some(sid) = m.server_id.as_ref() else {
                continue;
            };
            let preview: String = m
                .content
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(120)
                .collect();
            let title = if preview.trim().is_empty() {
                "(empty message)".to_string()
            } else {
                preview
            };
            user_msgs.push(DialogOption::new(title, sid.clone()));
        }
        user_msgs.reverse();
        options.extend(user_msgs);

        let payload = DialogPayload::ForkPicker { current: None };
        let parser: Box<dyn Fn(&str) -> DialogPayload + Send + Sync> =
            Box::new(|v: &str| DialogPayload::ForkPicker {
                current: if v.is_empty() {
                    None
                } else {
                    Some(v.to_string())
                },
            });
        self.dialogs.dialog = Some(Dialog::new("Fork session", payload, options, parser));
    }

    pub fn open_message_actions(&mut self, message_id: String) {
        if message_id.is_empty() {
            return;
        }
        let options = vec![
            {
                let mut o = DialogOption::new("Revert", "revert");
                o.description = Some("undo messages and file changes".to_string());
                o
            },
            {
                let mut o = DialogOption::new("Copy", "copy");
                o.description = Some("message text to clipboard".to_string());
                o
            },
            {
                let mut o = DialogOption::new("Fork", "fork");
                o.description = Some("create a new session".to_string());
                o
            },
        ];
        let payload = DialogPayload::MessageActions {
            message_id: message_id.clone(),
        };
        let mid_for_parser = message_id.clone();
        let parser: Box<dyn Fn(&str) -> DialogPayload + Send + Sync> =
            Box::new(move |_v: &str| DialogPayload::MessageActions {
                message_id: mid_for_parser.clone(),
            });
        self.dialogs.dialog = Some(Dialog::new("Message Actions", payload, options, parser));
    }

    fn open_plugin_select(
        &mut self,
        callback_id: u64,
        title: String,
        _placeholder: Option<String>,
        options: Vec<crate::action::PluginDialogOption>,
    ) {
        let options: Vec<DialogOption> = options
            .into_iter()
            .map(|option| DialogOption {
                title: option.title,
                value: option.value,
                description: option.description,
                category: option.category,
                disabled: option.disabled,
                is_header: false,
            })
            .collect();
        let parser: Box<dyn Fn(&str) -> DialogPayload + Send + Sync> =
            Box::new(move |v: &str| DialogPayload::PluginSelect {
                callback_id,
                current: if v.is_empty() {
                    None
                } else {
                    Some(v.to_string())
                },
            });
        self.dialogs.dialog = Some(Dialog::new(
            title,
            DialogPayload::PluginSelect {
                callback_id,
                current: None,
            },
            options,
            parser,
        ));
    }

    fn open_plugin_alert(&mut self, title: String, message: String) {
        let parser_message = message.clone();
        let parser: Box<dyn Fn(&str) -> DialogPayload + Send + Sync> =
            Box::new(move |_| DialogPayload::PluginAlert {
                message: parser_message.clone(),
            });
        self.dialogs.dialog = Some(Dialog::new(
            title,
            DialogPayload::PluginAlert { message },
            Vec::new(),
            parser,
        ));
    }

    fn clear_plugin_dialog(&mut self) {
        let is_plugin_dialog = self
            .dialogs
            .dialog
            .as_ref()
            .map(|dialog| {
                matches!(
                    dialog.kind(),
                    DialogKind::PluginSelect | DialogKind::PluginAlert
                )
            })
            .unwrap_or(false);
        if is_plugin_dialog {
            self.dialogs.dialog = None;
        }
    }

    fn close_dialog(&mut self, confirmed: bool) {
        let Some(d) = self.dialogs.dialog.take() else {
            return;
        };
        match d.payload {
            DialogPayload::ThemePicker { current } => {
                if !confirmed {
                    if let Some(restored) = self.theme.restore_preview() {
                        let _ = restored;
                        for msg in self.messages.iter_mut() {
                            msg.invalidate_render_cache();
                        }
                        self.invalidate_cached_transcript_render_caches();
                    }
                } else {
                    self.theme.clear_preview_snapshot();
                    self.runtime
                        .push(Event::ThemeChanged(current.as_str().to_string()));
                }
            }
            DialogPayload::CommandPalette { .. }
            | DialogPayload::ModelPicker { .. }
            | DialogPayload::VariantPicker { .. }
            | DialogPayload::SessionPicker { .. }
            | DialogPayload::SessionRename { .. }
            | DialogPayload::AgentPicker { .. }
            | DialogPayload::PluginAlert { .. }
            | DialogPayload::MessageActions { .. }
            | DialogPayload::ForkPicker { .. } => {}
            DialogPayload::PluginSelect { callback_id, .. } => {
                if !confirmed {
                    self.runtime
                        .push(Event::PluginDialogDismissed { callback_id });
                }
            }
        }
    }

    fn confirm_dialog(&mut self) {
        let Some(d) = self.dialogs.dialog.take() else {
            return;
        };
        match d.payload {
            DialogPayload::ThemePicker { current } => {
                self.theme.clear_preview_snapshot();
                self.runtime
                    .push(Event::ThemeChanged(current.as_str().to_string()));
            }
            DialogPayload::CommandPalette { current } => {
                if !self.run_builtin_command_by_name(&current) {
                    self.runtime.push(Event::PluginCommand {
                        name: current,
                        args: String::new(),
                    });
                }
            }
            DialogPayload::ModelPicker { current } => {
                if let Some(m) = current {
                    self.set_model(m, true);
                }
            }
            DialogPayload::VariantPicker { current } => {
                self.set_variant(current);
            }
            DialogPayload::SessionPicker { current } => {
                if let Some(id) = current {
                    self.switch_session(id);
                }
            }
            DialogPayload::SessionRename { session_id, title } => {
                let title = title.trim().to_string();
                if title.is_empty() {
                    self.dialogs.show_toast(Toast::new(
                        "Session title cannot be empty",
                        ToastVariant::Error,
                    ));
                } else {
                    self.runtime
                        .push(Event::RenameSession { session_id, title });
                }
            }
            DialogPayload::AgentPicker { current } => {
                self.set_agent(&current);
            }
            DialogPayload::PluginSelect {
                callback_id,
                current,
            } => match current {
                Some(value) => self
                    .runtime
                    .push(Event::PluginDialogSelected { callback_id, value }),
                None => self
                    .runtime
                    .push(Event::PluginDialogDismissed { callback_id }),
            },
            DialogPayload::PluginAlert { .. } => {}
            DialogPayload::MessageActions { message_id } => match d.current_value.as_str() {
                "revert" => self.runtime.push(Event::Undo {
                    message_id: message_id.clone(),
                }),
                "copy" => {
                    let text = self
                        .messages
                        .iter()
                        .find(|m| m.server_id.as_deref() == Some(message_id.as_str()))
                        .map(|m| m.content.clone())
                        .unwrap_or_default();
                    if !text.is_empty() {
                        self.runtime.push(Event::CopyToClipboard {
                            text,
                            success_message: "Copied message to clipboard".to_string(),
                            error_message: "Failed to copy message".to_string(),
                        });
                    }
                }
                "fork" => self.runtime.push(Event::ForkSession {
                    message_id: Some(message_id.clone()),
                }),
                _ => {}
            },
            DialogPayload::ForkPicker { current } => {
                self.runtime.push(Event::ForkSession {
                    message_id: current,
                });
            }
        }
    }

    fn run_builtin_command_by_name(&mut self, name: &str) -> bool {
        let cmd = self
            .dialogs
            .commands
            .iter()
            .find(|c| c.name == name)
            .cloned();
        if let Some(cmd) = cmd {
            self.dispatch(cmd.action);
            true
        } else {
            false
        }
    }

    fn preview_current_dialog_theme(&mut self) {
        let preview = match self.dialogs.dialog.as_ref().map(|d| &d.payload) {
            Some(DialogPayload::ThemePicker { current }) => current.clone(),
            _ => return,
        };
        if preview != self.theme.theme.name {
            self.apply_theme_name(preview, false);
        }
    }

    fn handle_dialog_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        if matches!(code, KeyCode::Char('c')) && mods.contains(KeyModifiers::CONTROL) {
            self.close_dialog(false);
            return;
        }
        if matches!(
            self.dialogs.dialog.as_ref().map(|dialog| dialog.kind()),
            Some(DialogKind::PluginAlert)
        ) {
            match code {
                KeyCode::Esc | KeyCode::Enter => self.close_dialog(false),
                _ => {}
            }
            return;
        }
        match code {
            KeyCode::Esc => self.close_dialog(false),
            KeyCode::Up | KeyCode::BackTab => {
                if let Some(d) = self.dialogs.dialog.as_mut() {
                    d.move_prev();
                }
                self.preview_current_dialog_theme();
            }
            KeyCode::Char('p') if mods.contains(KeyModifiers::CONTROL) => {
                if let Some(d) = self.dialogs.dialog.as_mut() {
                    d.move_prev();
                }
                self.preview_current_dialog_theme();
            }
            KeyCode::Down | KeyCode::Tab => {
                if let Some(d) = self.dialogs.dialog.as_mut() {
                    d.move_next();
                }
                self.preview_current_dialog_theme();
            }
            KeyCode::Char('n') if mods.contains(KeyModifiers::CONTROL) => {
                if let Some(d) = self.dialogs.dialog.as_mut() {
                    d.move_next();
                }
                self.preview_current_dialog_theme();
            }
            KeyCode::Char('f')
                if mods.contains(KeyModifiers::CONTROL)
                    && self.dialogs.dialog_kind() == Some(DialogKind::ModelPicker) =>
            {
                self.toggle_favorite_for_selected();
            }
            KeyCode::Char('a')
                if mods.contains(KeyModifiers::CONTROL)
                    && self.dialogs.dialog_kind() == Some(DialogKind::ModelPicker) =>
            {
                self.close_dialog(false);
                self.run_builtin_command_by_name("provider.connect");
            }
            KeyCode::Char('f')
                if mods.contains(KeyModifiers::CONTROL)
                    && self.dialogs.dialog_kind() == Some(DialogKind::SessionPicker) =>
            {
                self.toggle_pin_for_selected_session();
            }
            KeyCode::Char('d')
                if mods.contains(KeyModifiers::CONTROL)
                    && self.dialogs.dialog_kind() == Some(DialogKind::SessionPicker) =>
            {
                self.delete_selected_session();
            }
            KeyCode::Char('r')
                if mods.contains(KeyModifiers::CONTROL)
                    && self.dialogs.dialog_kind() == Some(DialogKind::SessionPicker) =>
            {
                self.rename_selected_session();
            }
            KeyCode::Enter => self.confirm_dialog(),
            KeyCode::Backspace => {
                if let Some(d) = self.dialogs.dialog.as_mut() {
                    d.backspace_filter();
                }
                self.preview_current_dialog_theme();
            }
            KeyCode::Delete => {
                if let Some(d) = self.dialogs.dialog.as_mut() {
                    d.delete_filter_char();
                }
                self.preview_current_dialog_theme();
            }
            KeyCode::Left => {
                if let Some(d) = self.dialogs.dialog.as_mut() {
                    d.move_filter_cursor_left();
                }
            }
            KeyCode::Right => {
                if let Some(d) = self.dialogs.dialog.as_mut() {
                    d.move_filter_cursor_right();
                }
            }
            KeyCode::Char(c) => {
                if let Some(d) = self.dialogs.dialog.as_mut() {
                    d.insert_filter_char(c);
                }
                self.preview_current_dialog_theme();
            }
            _ => {}
        }
    }

    pub fn dialog_kind(&self) -> Option<DialogKind> {
        self.dialogs.dialog_kind()
    }

    fn toggle_pin_for_selected_session(&mut self) {
        let Some(dialog) = self.dialogs.dialog.as_ref() else {
            return;
        };
        if dialog.kind() != DialogKind::SessionPicker {
            return;
        }
        let Some(opt) = dialog.selected_option() else {
            return;
        };
        if opt.is_header || opt.value.is_empty() {
            return;
        }
        let id = opt.value.clone();
        self.sessions.toggle_pin(id);
        let _ = self.sessions.save_to_disk();
        let options = self.sessions.build_picker_options();
        if let Some(dialog) = self.dialogs.dialog.as_mut() {
            dialog.replace_options(options);
        }
    }

    fn delete_selected_session(&mut self) {
        let Some(dialog) = self.dialogs.dialog.as_ref() else {
            return;
        };
        if dialog.kind() != DialogKind::SessionPicker {
            return;
        }
        let Some(opt) = dialog.selected_option() else {
            return;
        };
        if opt.is_header || opt.value.is_empty() {
            return;
        }
        let id = opt.value.clone();
        if self.sessions.session_delete_armed.as_deref() == Some(id.as_str()) {
            self.sessions.session_delete_armed = None;
            self.sessions.remove_session(&id);
            let _ = self.sessions.save_to_disk();
            let options = self.sessions.build_picker_options();
            if let Some(dialog) = self.dialogs.dialog.as_mut() {
                dialog.replace_options(options);
            }
            self.runtime.push(Event::DeleteSession { session_id: id });
        } else {
            self.sessions.session_delete_armed = Some(id);
            self.dialogs.show_toast(Toast::new(
                "Press ctrl+d again to delete",
                ToastVariant::Warning,
            ));
        }
    }

    fn rename_selected_session(&mut self) {
        let Some(dialog) = self.dialogs.dialog.as_ref() else {
            return;
        };
        if dialog.kind() != DialogKind::SessionPicker {
            return;
        }
        let Some(opt) = dialog.selected_option() else {
            return;
        };
        if opt.is_header || opt.value.is_empty() {
            return;
        }
        let id = opt.value.clone();
        self.sessions.session_delete_armed = None;
        self.open_session_rename(Some(id));
    }

    pub fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        if self.runtime.is_leader_armed() {
            let _ = self.runtime.take_leader_armed();
            if matches!(code, KeyCode::Down) {
                self.dispatch(Action::View(ViewAction::SubagentEnterFirstChild));
            }
            return;
        }

        if matches!(code, KeyCode::Char('c')) && mods.contains(KeyModifiers::CONTROL) {
            if !self.input.input.is_empty() {
                self.clear_input();
            } else {
                self.runtime.request_quit();
            }
            return;
        }

        if matches!(code, KeyCode::Char('x')) && mods.contains(KeyModifiers::CONTROL) {
            self.runtime.arm_leader();
            return;
        }

        if matches!(code, KeyCode::Char('p')) && mods.contains(KeyModifiers::CONTROL) {
            self.open_command_palette();
            return;
        }

        if matches!(code, KeyCode::Char('r')) && mods.contains(KeyModifiers::CONTROL) {
            self.open_session_rename(None);
            return;
        }

        if matches!(code, KeyCode::Char('b')) && mods.contains(KeyModifiers::CONTROL) {
            self.sidebar.toggle_visible();
            return;
        }

        if self.sessions.sessions.current_is_child() {
            if self.input.input.is_empty() {
                match (code, mods) {
                    (KeyCode::Up, m) if m.is_empty() => {
                        self.dispatch(Action::View(ViewAction::SubagentGoToParent));
                    }
                    (KeyCode::Right, m) if m.is_empty() => {
                        self.dispatch(Action::View(ViewAction::SubagentCycleSibling(1)));
                    }
                    (KeyCode::Left, m) if m.is_empty() => {
                        self.dispatch(Action::View(ViewAction::SubagentCycleSibling(-1)));
                    }
                    _ => {}
                }
            }
            match code {
                KeyCode::Esc => self.runtime.push(Event::Interrupt),
                KeyCode::PageUp => self.scroll.scroll_messages(-5),
                KeyCode::PageDown => self.scroll.scroll_messages(5),
                _ => {}
            }
            return;
        }

        if matches!(code, KeyCode::Char('u')) && mods.contains(KeyModifiers::CONTROL) {
            self.clear_input();
            return;
        }

        if matches!(code, KeyCode::Char('w')) && mods.contains(KeyModifiers::CONTROL) {
            self.input.delete_word_back();
            self.reset_paste_tracking_if_input_empty();
            return;
        }

        if matches!(code, KeyCode::Char('a')) && mods.contains(KeyModifiers::CONTROL) {
            self.input.move_cursor_home();
            return;
        }
        if matches!(code, KeyCode::Char('e')) && mods.contains(KeyModifiers::CONTROL) {
            self.input.move_cursor_end();
            return;
        }

        if matches!(code, KeyCode::Char('k')) && mods.contains(KeyModifiers::CONTROL) {
            self.input.kill_to_end();
            self.reset_paste_tracking_if_input_empty();
            return;
        }

        if self.input.completion.active {
            match self.input.handle_completion_key(code, mods) {
                CompletionOutcome::Consumed => return,
                CompletionOutcome::SubmitNow => {
                    self.submit_input();
                    return;
                }
                CompletionOutcome::NotConsumed => {}
            }
        }

        match code {
            KeyCode::Tab => {
                if mods.contains(KeyModifiers::SHIFT) {
                    self.cycle_agent(-1);
                } else {
                    self.cycle_agent(1);
                }
            }
            KeyCode::BackTab => self.cycle_agent(-1),
            KeyCode::Esc => {
                if !self.input.input.is_empty() {
                    self.clear_input();
                } else {
                    self.runtime.push(Event::Interrupt);
                }
            }
            KeyCode::Up => {
                let w = self.last_text_width;
                let row = self.input.cursor_visual_row(w);
                if row == 0 {
                    self.input.history_prev();
                    self.input_has_paste = self.input.has_text_parts();
                } else {
                    self.input.move_cursor_up(w);
                }
            }
            KeyCode::Down => {
                let w = self.last_text_width;
                let row = self.input.cursor_visual_row(w);
                let total = self.input.total_visual_rows(w);
                if row >= total.saturating_sub(1) {
                    self.input.history_next();
                    self.input_has_paste = self.input.has_text_parts();
                } else {
                    self.input.move_cursor_down(w);
                }
            }
            KeyCode::PageUp => self.scroll.scroll_messages(-5),
            KeyCode::PageDown => self.scroll.scroll_messages(5),
            KeyCode::Left => {
                if mods.contains(KeyModifiers::CONTROL) || mods.contains(KeyModifiers::ALT) {
                    self.input.move_cursor_word_left();
                } else {
                    self.input.move_cursor_left();
                }
            }
            KeyCode::Right => {
                if mods.contains(KeyModifiers::CONTROL) || mods.contains(KeyModifiers::ALT) {
                    self.input.move_cursor_word_right();
                } else {
                    self.input.move_cursor_right();
                }
            }
            KeyCode::Home if mods.contains(KeyModifiers::CONTROL) => {
                self.scroll.scroll_messages(isize::MIN / 2);
            }
            KeyCode::End if mods.contains(KeyModifiers::CONTROL) => {
                self.scroll.scroll_messages(isize::MAX / 2);
            }
            KeyCode::Home => self.input.move_cursor_home(),
            KeyCode::End => self.input.move_cursor_end(),
            KeyCode::Delete => {
                self.input.delete_forward();
                self.reset_paste_tracking_if_input_empty();
            }
            KeyCode::Backspace => {
                if mods.contains(KeyModifiers::ALT) {
                    self.input.delete_word_back();
                } else {
                    self.input.backspace();
                }
                self.reset_paste_tracking_if_input_empty();
            }
            KeyCode::Enter => {
                let newline = mods.contains(KeyModifiers::ALT)
                    || mods.contains(KeyModifiers::SHIFT)
                    || mods.contains(KeyModifiers::CONTROL);
                if newline {
                    self.input.insert_newline();
                } else {
                    self.submit_input();
                }
            }
            KeyCode::Char('j') if mods.contains(KeyModifiers::CONTROL) => {
                self.input.insert_newline();
            }
            KeyCode::Char(c) => self.input.insert_char(c),
            _ => {}
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn trim_render_caches_to_tail(messages: &mut [Message]) {
    let keep_start = messages
        .len()
        .saturating_sub(RENDER_CACHE_RETAIN_MESSAGE_LIMIT);
    for msg in &mut messages[..keep_start] {
        clear_message_render_caches(msg);
    }
}

fn clear_message_render_caches(msg: &mut Message) {
    msg.rendered_content_cache = None;
    msg.rendered_thoughts_cache = None;
    msg.last_render_width = 0;
    msg.content_fingerprint = 0;
    msg.thoughts_fingerprint = 0;
    msg.tool_render_cache.clear();
    msg.part_render_cache.clear();
}

fn unwrap_filepath_paste(trimmed: &str) -> Option<String> {
    let mut s = trimmed.trim();
    while s.len() >= 2 {
        let first = s.chars().next().unwrap();
        let last = s.chars().last().unwrap();
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            s = s[1..s.len() - 1].trim();
        } else {
            break;
        }
    }
    if let Some(rest) = s.strip_prefix("file://") {
        let trimmed_rest = rest.trim_end();
        if trimmed_rest.is_empty() {
            return None;
        }
        return Some(trimmed_rest.to_string());
    }
    if s.is_empty() {
        return None;
    }
    Some(s.to_string())
}

fn try_file_part_from_path(
    filepath: &str,
    existing_parts: &[PromptPart],
) -> Option<(String, PromptPartKind)> {
    let lower = filepath.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return None;
    }
    let path = std::path::Path::new(filepath);
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    const MAX_BYTES: u64 = 32 * 1024 * 1024;
    if metadata.len() > MAX_BYTES {
        return None;
    }
    let mime = guess_mime_from_extension(filepath)?;
    let is_image = mime.starts_with("image/");
    let is_pdf = mime == "application/pdf";
    if !is_image && !is_pdf {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let base64 = base64_encode(&bytes);
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("attachment")
        .to_string();
    let count_same_kind = existing_parts
        .iter()
        .filter(|p| match &p.kind {
            PromptPartKind::File { mime: m, .. } => {
                if is_pdf {
                    m == "application/pdf"
                } else {
                    m.starts_with("image/")
                }
            }
            _ => false,
        })
        .count();
    let placeholder = if is_pdf {
        format!("[PDF {}]", count_same_kind + 1)
    } else {
        format!("[Image {}]", count_same_kind + 1)
    };
    let kind = PromptPartKind::File {
        mime,
        filename,
        filepath: filepath.to_string(),
        base64,
    };
    Some((placeholder, kind))
}

fn guess_mime_from_extension(path: &str) -> Option<String> {
    let lower = path.to_ascii_lowercase();
    let dot = lower.rfind('.')?;
    let ext = &lower[dot + 1..];
    Some(
        match ext {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "bmp" => "image/bmp",
            "svg" => "image/svg+xml",
            "tif" | "tiff" => "image/tiff",
            "heic" => "image/heic",
            "heif" => "image/heif",
            "avif" => "image/avif",
            "pdf" => "application/pdf",
            _ => return None,
        }
        .to_string(),
    )
}

fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut chunks = input.chunks_exact(3);
    for chunk in &mut chunks {
        let b0 = chunk[0] as u32;
        let b1 = chunk[1] as u32;
        let b2 = chunk[2] as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3F) as usize] as char);
        out.push(ALPHABET[(n & 0x3F) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        0 => {}
        1 => {
            let n = (rem[0] as u32) << 16;
            out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
            out.push(ALPHABET[((n >> 6) & 0x3F) as usize] as char);
            out.push('=');
        }
        _ => unreachable!("chunks_exact remainder is at most 2"),
    }
    out
}
