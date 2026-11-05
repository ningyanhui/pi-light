// SPDX-License-Identifier: Apache-2.0
// pi-light — 内嵌 Pi 的轻量 Rust Shell
// M2′ 聊天 + M3′ Trace/Token + M4′+ 设置面 + M5′ # 会话引用 + 多会话/cwd/重试

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use eframe::egui;
use pi_light::{
    admin_panel::{
        self, write_settings_model, write_settings_thinking, AdminPanelReturn, AdminPanelState,
        AdminTab, SetModelRequest,
    },
    app_icon,
    assets::ShellTextures,
    chat::{ChatMessage, ChatState, SessionHub},
    composer_invoke::{self, InvokeAction, InvokeChooser, InvokeKind, PiCommand},
    fonts,
    icons::{self, Icon},
    markdown_ui::{self, MarkdownView},
    message_tree::{self, NodeId, NodeRole, BRANCH_HONESTY},
    onboarding::{self, FirstRunSurface},
    pi_path::{self, PiProbe},
    pi_rpc::{self, PiRpcClient, PiRpcEvent},
    provider_catalog,
    remote_access::RemoteAccessState,
    remote_gateway::{CompanionChatEvent, SnapshotTool},
    session_insights::StatsFetchState,
    session_ref::{self, RefChip},
    session_store::{self, RpcAttach},
    theme, timeline_ui,
    workspace::{self, CwdSwitchPlan, DockAction, WorkspaceUi},
    workspace_git,
};

mod app_screens;
mod chat_view;
use chat_view::*;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("pi-light")
            .with_decorations(false)
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([880.0, 560.0])
            .with_icon(app_icon::icon_data()),
        ..Default::default()
    };

    eframe::run_native(
        "pi-light",
        options,
        Box::new(|cc| Ok(Box::new(PiLightApp::new(cc)))),
    )
}

// ── App state ─────────────────────────────────────────────────────────────

struct PiLightApp {
    pi_probe: PiProbe,
    rpc_client: Option<PiRpcClient>,
    sessions: SessionHub,
    spawn_error: Option<String>,
    egui_ctx: egui::Context,
    /// M4′ 管理面 / 设置面状态。
    admin: AdminPanelState,
    /// 引导期间只把设置切到「模型」一次。
    primed_model_tab: bool,
    /// Composer `/` `@` `#` 唤起。
    invoke: InvokeChooser,
    invoke_cache: Vec<composer_invoke::InvokeCandidate>,
    hash_file_cache: Vec<composer_invoke::InvokeCandidate>,
    pi_commands: Vec<PiCommand>,
    /// M5′ 已确认待附带发送的引用 chips。
    ref_chips: Vec<RefChip>,
    textures: ShellTextures,
    /// 当前活动工作区（写入 `Command::current_dir`；同时只有一个）。
    workdir: PathBuf,
    welcome_action: Option<DockAction>,
    /// Right context inspector; `None` = decide from window width.
    inspector_open: Option<bool>,
    workdir_error: Option<String>,
    /// 最近一次成功 spawn 的 sidecar cwd。同路径切会话时复用进程。
    sidecar_cwd: PathBuf,
    workspace: WorkspaceUi,
    /// 壳侧会话文件。None 表示没有可写的用户目录。
    state_path: Option<PathBuf>,
    persist_dirty: bool,
    persist_retry_at: Option<std::time::Instant>,
    /// v3 M5：远程访问（配对 / Cloudflare Tunnel / 设备列表）。
    remote: RemoteAccessState,
    /// M6-lite：当前 companion HTTP 回合的回包通道。
    companion_reply: Option<std::sync::mpsc::Sender<CompanionChatEvent>>,
    /// 正在编辑的用户消息节点。
    editing_node: Option<NodeId>,
    edit_draft: String,
    /// 消息树导航是否展开。
    tree_open: bool,
}

impl PiLightApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        fonts::install_system_cjk(&cc.egui_ctx);
        theme::apply_theme(&cc.egui_ctx);
        let probe = pi_path::probe();
        let egui_ctx = cc.egui_ctx.clone();
        let fallback_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let state_path = session_store::default_path();
        let (mut sessions, fallback_workdir) = match state_path
            .as_ref()
            .and_then(|path| session_store::load(path))
        {
            Some(loaded) => {
                let workdir = loaded
                    .workdir
                    .filter(|path| path.is_dir())
                    .unwrap_or(fallback_dir);
                (loaded.sessions, workdir)
            }
            None => (SessionHub::new(), fallback_dir),
        };
        sessions.fill_missing_workdirs(&fallback_workdir);
        let workdir = sessions
            .active()
            .workdir
            .clone()
            .filter(|path| path.is_dir())
            .unwrap_or(fallback_workdir);
        let mut workspace = WorkspaceUi::default();
        workspace.bind_root(workdir.clone());
        let stored_onboarding = cc
            .storage
            .and_then(|storage| storage.get_string(onboarding::SHOW_ONBOARDING_KEY));
        let admin = AdminPanelState {
            show_onboarding: onboarding::show_onboarding_from_storage(stored_onboarding.as_deref()),
            ..AdminPanelState::default()
        };
        let mut app = Self {
            pi_probe: probe,
            rpc_client: None,
            sessions,
            spawn_error: None,
            egui_ctx,
            admin,
            primed_model_tab: false,
            invoke: InvokeChooser::default(),
            invoke_cache: Vec::new(),
            hash_file_cache: Vec::new(),
            pi_commands: Vec::new(),
            ref_chips: Vec::new(),
            textures: ShellTextures::load(&cc.egui_ctx),
            workdir: workdir.clone(),
            welcome_action: None,
            inspector_open: None,
            workdir_error: None,
            sidecar_cwd: workdir,
            workspace,
            state_path,
            persist_dirty: false,
            persist_retry_at: None,
            remote: RemoteAccessState::load_default(),
            companion_reply: None,
            editing_node: None,
            edit_draft: String::new(),
            tree_open: false,
        };
        app.admin.discover_all();
        app.ensure_shell_path();
        let pending_model = app.admin.persist_inferred_default();
        app.try_spawn_rpc();
        if let Some(req) = pending_model {
            app.handle_set_model_request(req);
        }
        app
    }

    fn try_spawn_rpc(&mut self) {
        let _ = self.spawn_rpc(false);
    }

    fn spawn_rpc(&mut self, workspace_switch: bool) -> bool {
        self.rpc_client = None;
        self.spawn_error = None;
        self.invoke.close();
        if !workspace_switch {
            self.ref_chips.clear();
        }
        self.ensure_shell_path();

        if let Some(path) = self.pi_probe.path() {
            let ctx = self.egui_ctx.clone();
            match PiRpcClient::spawn(path, Some(&self.workdir), move || ctx.request_repaint()) {
                Ok(mut client) => {
                    let attach =
                        session_store::rpc_attach(self.sessions.active().session_file.as_deref());
                    let detached = matches!(attach, RpcAttach::NewSession)
                        && !self.sessions.active_chat().messages.is_empty();
                    match attach {
                        RpcAttach::SwitchTo(session_path) => {
                            let _ = client.send_switch_session(&session_path);
                        }
                        RpcAttach::NewSession => {
                            let _ = client.send_new_session();
                        }
                    }
                    let _ = client.request_get_state();
                    let _ = client.request_available_models();
                    let _ = client.request_available_thinking_levels();
                    let _ = client.request_commands();
                    self.rpc_client = Some(client);
                    self.sidecar_cwd = self.workdir.clone();
                    if detached {
                        self.sessions.active_chat_mut().set_error(
                            "已恢复本机会话记录。Pi 侧会话文件不在，模型上下文从新会话开始。"
                                .to_owned(),
                        );
                    }
                    return true;
                }
                Err(e) => {
                    self.spawn_error = Some(if workspace_switch {
                        workspace::SWITCH_FAIL_COPY.to_owned()
                    } else {
                        format!("启动 Pi 失败：{e}")
                    });
                    return false;
                }
            }
        }
        if workspace_switch {
            self.spawn_error = Some(workspace::SWITCH_FAIL_COPY.to_owned());
        }
        false
    }

    /// Re-probe Pi binary and re-spawn RPC if found.
    fn reprobe(&mut self) {
        self.rpc_client = None;
        self.pi_probe = pi_path::probe();
        self.try_spawn_rpc();
    }

    /// 重启 sidecar（不重新探测二进制）：用于 Skills/MCP/模型配置变更后使新配置生效。
    fn restart_sidecar(&mut self) {
        self.rpc_client = None;
        self.try_spawn_rpc();
        self.admin.rediscover();
    }

    fn request_workdir_change(&mut self, raw: &str) {
        match pi_rpc::validate_cwd(raw) {
            Ok(path) => self.apply_workdir_path(path),
            Err(_) => {
                self.workdir_error = Some(workspace::SWITCH_FAIL_COPY.to_owned());
                self.workspace.switch_error = Some(workspace::SWITCH_FAIL_COPY.to_owned());
                let retry = PathBuf::from(raw.trim());
                self.workspace.retry_path = if retry.as_os_str().is_empty() {
                    None
                } else {
                    Some(retry)
                };
            }
        }
    }

    fn apply_workdir_path(&mut self, path: PathBuf) {
        self.workspace.cwd_popover = false;
        self.workdir_error = None;
        self.workspace.switch_error = None;
        match workspace::plan_cwd_switch(&self.sidecar_cwd, &path) {
            CwdSwitchPlan::Reuse => {
                self.commit_workdir_success(path);
            }
            CwdSwitchPlan::Respawn => {
                if self.sessions.any_streaming() {
                    self.workspace.switch_error = Some(workspace::SWITCH_FAIL_COPY.to_owned());
                    self.workspace.retry_path = Some(path);
                    return;
                }
                let previous = self.workdir.clone();
                self.workspace.switching = true;
                self.workdir = path.clone();
                let ok = self.spawn_rpc(true);
                self.workspace.switching = false;
                if ok {
                    self.commit_workdir_success(path);
                } else {
                    self.workdir = previous.clone();
                    let _ = self.spawn_rpc(false);
                    self.workspace.switch_error = Some(workspace::SWITCH_FAIL_COPY.to_owned());
                    self.workspace.retry_path = Some(path);
                }
            }
        }
    }

    fn commit_workdir_success(&mut self, path: PathBuf) {
        self.workdir = path.clone();
        self.sessions.set_active_workdir(path.clone());
        self.workspace.bind_root(path.clone());
        self.workspace.retry_path = None;
        self.touch_sessions();
        self.persist_sessions(true);
        self.workspace.show_toast(format!(
            "已切换到 {} · 树与 Git 已刷新",
            workspace::display_workdir(&path)
        ));
    }

    fn insert_composer_path(&mut self, rel: &str) {
        let input = &mut self.sessions.active_chat_mut().input;
        let token = if rel.starts_with('@') {
            rel.to_owned()
        } else {
            format!("@{rel}")
        };
        if input.split_whitespace().any(|part| part == token) {
            return;
        }
        if !input.is_empty() && !input.ends_with([' ', '\n']) {
            input.push(' ');
        }
        input.push_str(&token);
    }

    /// 发送当前输入（含 chips 引用上下文）给 Pi。
    fn do_send(&mut self) {
        let user_text = self.sessions.active_chat().input.trim().to_owned();
        if user_text.is_empty() && self.ref_chips.is_empty() {
            return;
        }
        if self.rpc_client.is_none() || self.workspace.switching {
            return;
        }
        self.touch_sessions();
        let chips = std::mem::take(&mut self.ref_chips);
        let effective_prompt = session_ref::build_prompt_text(&chips, &user_text);

        let display_text = if chips.is_empty() {
            user_text.clone()
        } else {
            let labels: Vec<&str> = chips.iter().map(|c| c.display.as_str()).collect();
            format!("[引用: {}]\n{}", labels.join(", "), user_text)
        };

        self.sessions.set_active_title_from_user(&user_text);
        let prefix = self.sessions.active_chat().messages.clone();
        let stale = self.sessions.active_chat().pi_stale;
        {
            let chat = self.sessions.active_chat_mut();
            chat.push_user(display_text);
            chat.last_sent_prompt = Some(effective_prompt.clone());
            chat.input.clear();
            chat.begin_streaming();
        }
        self.invoke.close();
        self.editing_node = None;
        self.send_prompt_maybe_replay(&effective_prompt, &prefix, stale);
        self.persist_sessions(true);
    }

    fn do_abort(&mut self) {
        if let Some(client) = &mut self.rpc_client {
            let _ = client.send_abort();
        }
    }

    fn do_retry(&mut self) {
        if self.sessions.active_is_streaming() {
            return;
        }
        if let Some(id) = self.sessions.active_chat().tree.last_assistant_on_path() {
            self.do_regenerate(id);
            return;
        }
        let prompt = match self.sessions.active_chat().last_sent_prompt.clone() {
            Some(p) => p,
            None => return,
        };
        if self.rpc_client.is_none() {
            return;
        }
        self.touch_sessions();
        let prefix = self
            .sessions
            .active_chat()
            .tree
            .last_user_on_path()
            .map(|id| self.sessions.active_chat().tree.ancestor_messages(id))
            .unwrap_or_default();
        self.sessions.active_chat_mut().begin_streaming();
        self.send_prompt_maybe_replay(&prompt, &prefix, true);
        self.persist_sessions(true);
    }

    fn do_regenerate(&mut self, assistant: NodeId) {
        if self.sessions.active_is_streaming() || self.rpc_client.is_none() {
            return;
        }
        self.touch_sessions();
        let Some((prefix, user_text)) = self
            .sessions
            .active_chat_mut()
            .prepare_regenerate(assistant)
        else {
            return;
        };
        {
            let chat = self.sessions.active_chat_mut();
            chat.last_sent_prompt = Some(user_text.clone());
            chat.begin_streaming();
        }
        self.send_prompt_maybe_replay(&user_text, &prefix, true);
        self.persist_sessions(true);
    }

    fn do_edit_user(&mut self, user: NodeId, new_text: String) {
        let new_text = new_text.trim().to_owned();
        if new_text.is_empty() || self.sessions.active_is_streaming() || self.rpc_client.is_none() {
            return;
        }
        self.touch_sessions();
        let Some(prefix) = self
            .sessions
            .active_chat_mut()
            .prepare_user_branch(user, new_text.clone())
        else {
            return;
        };
        self.sessions.set_active_title_from_user(&new_text);
        {
            let chat = self.sessions.active_chat_mut();
            chat.last_sent_prompt = Some(new_text.clone());
            chat.begin_streaming();
        }
        self.editing_node = None;
        self.send_prompt_maybe_replay(&new_text, &prefix, true);
        self.persist_sessions(true);
    }

    fn do_select_sibling(&mut self, id: NodeId, delta: i32) {
        if self.sessions.active_is_streaming() {
            return;
        }
        if self.sessions.active_chat_mut().select_sibling(id, delta) {
            self.editing_node = None;
            self.touch_sessions();
            self.persist_sessions(true);
        }
    }

    fn do_navigate(&mut self, id: NodeId) {
        if self.sessions.active_is_streaming() {
            return;
        }
        if self.sessions.active_chat_mut().navigate_to(id) {
            self.editing_node = None;
            self.touch_sessions();
            self.persist_sessions(true);
        }
    }

    /// 活动路径与 Pi 不一致时：new_session + 回放前缀后再发当前用户消息。
    fn send_prompt_maybe_replay(
        &mut self,
        user_prompt: &str,
        prefix: &[ChatMessage],
        replay: bool,
    ) {
        let Some(client) = self.rpc_client.as_mut() else {
            return;
        };
        let payload = if replay {
            let _ = client.send_new_session();
            message_tree::replay_prompt(prefix, user_prompt)
        } else {
            user_prompt.to_owned()
        };
        if let Err(e) = client.send_prompt(&payload) {
            self.sessions
                .active_chat_mut()
                .set_error(format!("发送失败：{e}"));
            return;
        }
        if replay {
            let _ = client.request_get_state();
        }
        self.sessions.active_chat_mut().pi_stale = false;
    }

    fn do_new_session(&mut self) {
        if self.sessions.any_streaming() || self.workspace.switching {
            return;
        }
        self.invoke.close();
        self.ref_chips.clear();
        self.editing_node = None;
        self.edit_draft.clear();
        self.sessions.create_new();
        self.sessions.set_active_workdir(self.workdir.clone());
        self.touch_sessions();
        if let Some(client) = &mut self.rpc_client {
            if let Err(e) = client.send_new_session() {
                self.sessions
                    .active_chat_mut()
                    .set_error(format!("新建会话失败：{e}"));
                return;
            }
            let _ = client.request_get_state();
        }
        self.persist_sessions(true);
    }

    fn do_switch_session(&mut self, index: usize) {
        if index == self.sessions.active_index() {
            return;
        }
        if self.sessions.any_streaming() || self.workspace.switching {
            return;
        }
        let target_wd = self
            .sessions
            .list()
            .get(index)
            .and_then(|s| s.workdir.clone())
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| self.workdir.clone());
        let file = self
            .sessions
            .list()
            .get(index)
            .and_then(|s| s.session_file.clone());
        if !self.sessions.switch_to(index) {
            return;
        }
        self.touch_sessions();
        self.invoke.close();
        self.ref_chips.clear();
        self.editing_node = None;
        self.edit_draft.clear();
        match workspace::plan_cwd_switch(&self.sidecar_cwd, &target_wd) {
            CwdSwitchPlan::Reuse => {
                self.workdir = target_wd.clone();
                self.sessions.set_active_workdir(target_wd.clone());
                self.workspace.bind_root(target_wd);
                if let Some(path) = file {
                    if let Some(client) = &mut self.rpc_client {
                        if let Err(e) = client.send_switch_session(&path) {
                            self.sessions
                                .active_chat_mut()
                                .set_error(format!("切换会话失败：{e}"));
                            return;
                        }
                        let _ = client.request_get_state();
                    }
                }
                self.persist_sessions(true);
            }
            CwdSwitchPlan::Respawn => {
                self.apply_workdir_path(target_wd);
            }
        }
    }

    fn do_fork_session(&mut self, index: usize) {
        if self.sessions.any_streaming() || self.workspace.switching {
            return;
        }
        let source_active = index == self.sessions.active_index();
        let source_file = self
            .sessions
            .list()
            .get(index)
            .and_then(|s| s.session_file.clone());
        if source_active {
            if let Some(client) = &mut self.rpc_client {
                if client.send_clone_session().is_ok() && self.sessions.fork_slot(index).is_some() {
                    self.workspace.show_toast("已 Fork（Pi clone）");
                    self.invoke.close();
                    self.ref_chips.clear();
                    if let Some(client) = &mut self.rpc_client {
                        let _ = client.request_get_state();
                    }
                    self.touch_sessions();
                    self.persist_sessions(true);
                    return;
                }
            }
        } else if let (Some(file), Some(client)) =
            (source_file.as_deref(), self.rpc_client.as_mut())
        {
            if client.send_switch_session(file).is_ok()
                && client.send_clone_session().is_ok()
                && self.sessions.fork_slot(index).is_some()
            {
                self.workspace.show_toast("已 Fork（先切换再 Pi clone）");
                self.invoke.close();
                self.ref_chips.clear();
                if let Some(client) = &mut self.rpc_client {
                    let _ = client.request_get_state();
                }
                self.touch_sessions();
                self.persist_sessions(true);
                return;
            }
        }
        if self.sessions.fork_slot(index).is_none() {
            return;
        }
        self.invoke.close();
        self.ref_chips.clear();
        if let Some(client) = &mut self.rpc_client {
            if let Err(e) = client.send_new_session() {
                self.sessions
                    .active_chat_mut()
                    .set_error(format!("壳侧复制后 new_session 失败：{e}"));
            } else {
                let _ = client.request_get_state();
            }
        }
        self.workspace.show_toast("已壳侧复制会话（未走 Pi clone）");
        self.touch_sessions();
        self.persist_sessions(true);
    }

    fn do_quote_session(&mut self, index: usize) {
        let Some(session) = self.sessions.list().get(index).cloned() else {
            return;
        };
        let preview = self
            .sessions
            .chat_at(index)
            .and_then(|chat| chat.messages.last())
            .map(|msg| match msg {
                ChatMessage::User(t) | ChatMessage::Assistant(t) => t.clone(),
            })
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| session.title.clone());
        let chip = composer_invoke::quote_hub_session(&session, &preview);
        if !self.ref_chips.iter().any(|c| c.id == chip.id) {
            self.ref_chips.push(chip);
        }
    }

    fn do_delete_session(&mut self, index: usize) {
        if self.sessions.any_streaming() || self.workspace.switching {
            return;
        }
        if !self.sessions.remove(index) {
            return;
        }
        self.invoke.close();
        self.ref_chips.clear();
        self.editing_node = None;
        if let Some(client) = &mut self.rpc_client {
            if let Some(file) = self.sessions.active().session_file.clone() {
                let _ = client.send_switch_session(&file);
            } else {
                let _ = client.send_new_session();
            }
            let _ = client.request_get_state();
        }
        self.touch_sessions();
        self.persist_sessions(true);
    }

    fn handle_set_thinking_level(&mut self, level: &str) {
        if let Some(client) = &mut self.rpc_client {
            if let Err(e) = client.send_set_thinking_level(level) {
                self.admin.last_error = Some(format!("set_thinking_level 发送失败：{e}"));
                return;
            }
            self.admin.model_config.thinking_level = Some(level.to_owned());
            let _ = client.request_get_state();
        } else {
            self.admin.last_error = Some("尚未连接 Pi，无法设置思考强度。".to_owned());
        }
        if self.admin.model_config.thinking_set_default {
            if let Some(path) = self.admin.model_config.settings_path.clone() {
                if write_settings_thinking(&path, level).is_ok() {
                    self.admin.model_config.default_thinking_level = Some(level.to_owned());
                }
            }
        }
    }

    fn drain_rpc_events(&mut self) {
        let mut refresh_state = false;
        let mut refresh_thinking = false;
        if let Some(client) = &mut self.rpc_client {
            let events = client.drain_events();
            let had_events = !events.is_empty();
            for event in events {
                let is_exit = matches!(&event, PiRpcEvent::ProcessExited { .. });
                let request_stats = matches!(&event, PiRpcEvent::AgentSettled { aborted: false });
                let companion_note = match &event {
                    PiRpcEvent::TextDelta(delta) => Some(CompanionChatEvent::Delta {
                        text: delta.clone(),
                    }),
                    PiRpcEvent::ToolTraceStart {
                        tool_call_id,
                        tool_name,
                        args_preview,
                        ..
                    } => Some(CompanionChatEvent::Tool(SnapshotTool {
                        id: tool_call_id.clone(),
                        name: tool_name.clone(),
                        status: "running".into(),
                        args: args_preview.clone(),
                        duration_ms: None,
                    })),
                    PiRpcEvent::ToolTraceEnd {
                        tool_call_id,
                        tool_name,
                        is_error,
                        duration_ms,
                    } => Some(CompanionChatEvent::Tool(SnapshotTool {
                        id: tool_call_id.clone(),
                        name: tool_name.clone(),
                        status: if *is_error {
                            "error".into()
                        } else {
                            "done".into()
                        },
                        args: None,
                        duration_ms: *duration_ms,
                    })),
                    PiRpcEvent::AgentSettled { aborted } => Some(CompanionChatEvent::Settled {
                        text: String::new(),
                        aborted: *aborted,
                    }),
                    PiRpcEvent::ProcessExited { .. } => Some(CompanionChatEvent::Error {
                        code: "pi_exited".into(),
                        message: "Pi 进程已退出".into(),
                    }),
                    PiRpcEvent::CommandError { command, error } => {
                        if matches!(
                            command.as_str(),
                            "set_model"
                                | "get_available_models"
                                | "get_state"
                                | "get_session_stats"
                                | "set_thinking_level"
                                | "get_available_thinking_levels"
                                | "get_commands"
                                | "clone"
                                | "fork"
                                | "set_session_name"
                        ) {
                            if command == "get_available_thinking_levels" {
                                self.admin.model_config.thinking_rpc_failed = true;
                                self.admin.model_config.thinking_levels.clear();
                            }
                            None
                        } else {
                            Some(CompanionChatEvent::Error {
                                code: "command_error".into(),
                                message: error.clone(),
                            })
                        }
                    }
                    _ => None,
                };

                match &event {
                    PiRpcEvent::StateInfo {
                        model,
                        session_file,
                        session_id,
                        thinking_level,
                        context_window,
                        max_tokens,
                        ..
                    } => {
                        self.admin.update_live_model(model.clone(), Vec::new());
                        self.sessions
                            .set_active_session_file(session_file.clone(), session_id.clone());
                        if let Some(level) = thinking_level {
                            self.admin.model_config.thinking_level = Some(level.clone());
                        }
                        if let Some(window) = *context_window {
                            self.admin.model_config.live_context_window = Some(window);
                            if self.admin.model_config.context_window_draft.is_empty() {
                                self.admin.model_config.context_window_draft = window.to_string();
                            }
                        }
                        if let Some(tokens) = *max_tokens {
                            self.admin.model_config.live_max_tokens = Some(tokens);
                            if self.admin.model_config.max_tokens_draft.is_empty() {
                                self.admin.model_config.max_tokens_draft = tokens.to_string();
                            }
                        }
                        if let Some(req) = self.admin.persist_inferred_default() {
                            self.handle_set_model_request(req);
                        }
                    }
                    PiRpcEvent::AvailableModels(models) => {
                        self.admin.update_live_model(None, models.clone());
                        if let Some(req) = self.admin.persist_inferred_default() {
                            self.handle_set_model_request(req);
                        }
                    }
                    PiRpcEvent::ModelChanged(model) => {
                        self.admin
                            .update_live_model(Some(model.clone()), Vec::new());
                        refresh_thinking = true;
                        refresh_state = true;
                        if let Some(req) = self.admin.persist_inferred_default() {
                            self.handle_set_model_request(req);
                        }
                    }
                    PiRpcEvent::ThinkingLevels(levels) => {
                        self.admin.model_config.thinking_rpc_failed = false;
                        self.admin.model_config.thinking_levels = levels.clone();
                    }
                    PiRpcEvent::ThinkingLevelChanged(level) => {
                        if !level.is_empty() {
                            self.admin.model_config.thinking_level = Some(level.clone());
                        }
                        refresh_state = true;
                    }
                    PiRpcEvent::AvailableCommands(commands) => {
                        self.pi_commands = commands.clone();
                    }
                    PiRpcEvent::SessionStats(stats) => {
                        if let Some(window) = stats.context_window {
                            self.admin.model_config.live_context_window = Some(window);
                        }
                    }
                    PiRpcEvent::SessionCloned {
                        cancelled: true, ..
                    } => {
                        self.sessions
                            .active_chat_mut()
                            .set_error("Pi 拒绝了 clone/fork（扩展取消）。".to_owned());
                    }
                    PiRpcEvent::SessionCommandDone {
                        cancelled: true, ..
                    } => {
                        self.sessions
                            .active_chat_mut()
                            .set_error("Pi 拒绝了会话切换（扩展取消）。".to_owned());
                    }
                    _ => {}
                }

                self.sessions.active_chat_mut().handle_rpc_event(event);
                if let Some(note) = companion_note {
                    match &note {
                        CompanionChatEvent::Settled { .. } | CompanionChatEvent::Error { .. } => {
                            self.finish_companion_turn(note);
                        }
                        CompanionChatEvent::Delta { .. } | CompanionChatEvent::Tool(_) => {
                            self.forward_companion_event(note);
                        }
                    }
                }
                if request_stats {
                    self.request_session_stats();
                }
                if is_exit {
                    self.rpc_client = None;
                    break;
                }
            }
            if had_events {
                self.touch_sessions();
            }
        }
        if let Some(client) = &mut self.rpc_client {
            if refresh_thinking {
                let _ = client.request_available_thinking_levels();
            }
            if refresh_state {
                let _ = client.request_get_state();
            }
        }
    }

    fn forward_companion_event(&mut self, note: CompanionChatEvent) {
        let Some(tx) = self.companion_reply.as_ref() else {
            return;
        };
        if tx.send(note).is_err() {
            self.companion_reply = None;
        }
    }

    fn finish_companion_turn(&mut self, note: CompanionChatEvent) {
        let Some(tx) = self.companion_reply.take() else {
            return;
        };
        let event = match note {
            CompanionChatEvent::Settled { aborted, .. } => {
                let text = match self.sessions.active_chat().messages.last() {
                    Some(ChatMessage::Assistant(t)) => t.clone(),
                    _ => self.sessions.active_chat().streaming_buf.clone(),
                };
                CompanionChatEvent::Settled { text, aborted }
            }
            other => other,
        };
        let _ = tx.send(event);
    }

    fn drain_companion_inbox(&mut self) {
        let Some(pending) = self.remote.take_pending_chat() else {
            return;
        };
        if self.companion_reply.is_some() || self.sessions.active_is_streaming() {
            let _ = pending.reply.send(CompanionChatEvent::Error {
                code: "busy".into(),
                message: "desktop is already generating".into(),
            });
            return;
        }
        if self.rpc_client.is_none() {
            let _ = pending.reply.send(CompanionChatEvent::Error {
                code: "pi_unavailable".into(),
                message: "Pi sidecar is not running".into(),
            });
            return;
        }
        let text = pending.text;
        self.touch_sessions();
        self.sessions.set_active_title_from_user(&text);
        let prefix = self.sessions.active_chat().messages.clone();
        let stale = self.sessions.active_chat().pi_stale;
        {
            let chat = self.sessions.active_chat_mut();
            chat.push_user(text.clone());
            chat.last_sent_prompt = Some(text.clone());
            chat.begin_streaming();
        }
        self.send_prompt_maybe_replay(&text, &prefix, stale);
        if self.sessions.active_chat().error.is_some() {
            let msg = self
                .sessions
                .active_chat()
                .error
                .clone()
                .unwrap_or_else(|| "发送失败".into());
            let _ = pending.reply.send(CompanionChatEvent::Error {
                code: "send_failed".into(),
                message: msg,
            });
            return;
        }
        self.companion_reply = Some(pending.reply);
        self.persist_sessions(true);
    }

    fn drain_companion_stop(&mut self) {
        if !self.remote.take_pending_stop() {
            return;
        }
        if self.sessions.active_is_streaming() || self.companion_reply.is_some() {
            self.do_abort();
        }
    }

    fn touch_sessions(&mut self) {
        self.sessions.touch_active();
        self.persist_dirty = true;
    }

    fn persist_sessions(&mut self, force: bool) {
        if !self.persist_dirty {
            return;
        }
        if self.sessions.any_streaming() && !force {
            return;
        }
        if !force {
            if let Some(when) = self.persist_retry_at {
                if std::time::Instant::now() < when {
                    return;
                }
            }
        }
        let Some(path) = self.state_path.clone() else {
            self.persist_dirty = false;
            return;
        };
        match session_store::save(&path, &self.sessions, &self.workdir) {
            Ok(()) => {
                self.persist_dirty = false;
                self.persist_retry_at = None;
            }
            Err(_) => {
                self.persist_retry_at =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
            }
        }
    }

    fn ensure_shell_path(&mut self) {
        let Some(dir) = admin_panel::pi_agent_dir() else {
            return;
        };
        let path = dir.join("settings.json");
        let probe = pi_light::bash_path::probe();
        if let Err(e) = admin_panel::ensure_settings_shell_path(&path, &probe) {
            self.admin.last_error = Some(format!("写入 shellPath 失败：{e}"));
        }
    }

    /// 向 Pi 发送 set_model 命令。
    fn handle_set_model_request(&mut self, req: SetModelRequest) {
        if let Some(client) = &mut self.rpc_client {
            if let Err(e) = client.send_set_model(&req.provider, &req.model_id) {
                self.admin.last_error = Some(format!("set_model 发送失败：{e}"));
            }
        } else {
            self.admin.last_error =
                Some("当前没有连上 Pi，不能立即切换。请改用「保存为默认」。".to_owned());
        }
    }

    fn request_session_stats(&mut self) {
        if let Some(client) = &mut self.rpc_client {
            self.sessions
                .active_chat_mut()
                .insights
                .on_stats_request_sent();
            if client.request_session_stats().is_err() {
                self.sessions.active_chat_mut().insights.stats_fetch = StatsFetchState::Failed;
            }
        }
    }

    fn update_invoke_chooser(&mut self) {
        let input = self.sessions.active_chat().input.clone();
        let trigger = composer_invoke::find_invoke_trigger(&input);
        let was_active = self.invoke.active;
        match trigger {
            Some((kind, pos)) => {
                let kind_changed = !was_active || self.invoke.kind != Some(kind);
                // `/` `@` 每帧重建（命令列表/文件树可能晚到）。`#` 会话文件只在打开时读盘。
                self.invoke_cache = match kind {
                    InvokeKind::Slash => composer_invoke::slash_candidates(&self.pi_commands),
                    InvokeKind::At => {
                        let files = self.workspace.tree.collect_file_rels(80);
                        composer_invoke::at_candidates(&files)
                    }
                    InvokeKind::Hash => {
                        if kind_changed {
                            self.hash_file_cache = composer_invoke::hash_session_file_candidates();
                        }
                        let mut items =
                            composer_invoke::hash_candidates(&self.sessions.active_chat().messages);
                        items.extend(self.hash_file_cache.clone());
                        items
                    }
                };
                let query = composer_invoke::extract_query(&input, pos).to_owned();
                self.invoke.activate(kind, &self.invoke_cache, &query, pos);
            }
            None => {
                if was_active {
                    self.invoke.close();
                }
            }
        }
    }

    fn apply_invoke_candidate(&mut self, item: composer_invoke::InvokeCandidate) {
        let pos = self.invoke.trigger_byte_offset;
        match item.action {
            InvokeAction::Insert => {
                composer_invoke::replace_trigger(
                    &mut self.sessions.active_chat_mut().input,
                    pos,
                    &item.insert,
                );
            }
            InvokeAction::SendPrompt => {
                composer_invoke::replace_trigger(
                    &mut self.sessions.active_chat_mut().input,
                    pos,
                    &item.insert,
                );
                self.do_send();
            }
            InvokeAction::NewSession => {
                self.sessions.active_chat_mut().input.clear();
                self.do_new_session();
            }
            InvokeAction::Quote => {
                if let Some(chip) = item.chip {
                    if !self.ref_chips.iter().any(|c| c.id == chip.id) {
                        self.ref_chips.push(chip);
                    }
                }
                composer_invoke::replace_trigger(
                    &mut self.sessions.active_chat_mut().input,
                    pos,
                    "",
                );
            }
        }
    }

    fn paint_main_backdrop(&self, _ui: &egui::Ui) {
        // Keep the conversation surface calm and readable; no decorative image overlay.
    }
}

// ── eframe::App ───────────────────────────────────────────────────────────

impl eframe::App for PiLightApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(
            onboarding::SHOW_ONBOARDING_KEY,
            onboarding::show_onboarding_storage_value(self.admin.show_onboarding).to_owned(),
        );
        self.persist_sessions(true);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_rpc_events();
        self.drain_companion_inbox();
        self.drain_companion_stop();
        self.update_invoke_chooser();
        if self.remote.poll() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        self.remote.publish_chat(
            self.sessions.active().title.as_str(),
            self.sessions.active_chat(),
        );
        if self.remote.persist.enabled {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        if self.companion_reply.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(80));
        }

        // Minimized windows report a zero-sized screen; laying out panels then
        // yields negative widths and trips egui's size assertions.
        let minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        let screen = ctx.screen_rect();
        if minimized || screen.width() < 64.0 || screen.height() < 64.0 {
            self.persist_sessions(false);
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }

        // M5′: 键盘导航（在面板渲染前消费按键，避免被 TextEdit 截获）
        if self.invoke.active {
            let mut up = false;
            let mut down = false;
            let mut confirmed = false;
            let mut closed = false;
            ctx.input_mut(|i| {
                up = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
                down = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                confirmed = i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    || i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
                closed = i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
            });
            if up {
                self.invoke.move_up();
            }
            if down {
                self.invoke.move_down();
            }
            if confirmed {
                if let Some(item) = self.invoke.confirm() {
                    self.apply_invoke_candidate(item);
                }
            } else if closed {
                self.invoke.close();
            }
        }

        let surface =
            onboarding::first_run_surface(self.pi_probe.is_ready(), self.admin.show_onboarding);
        if matches!(surface, FirstRunSurface::ModelSetup) && !self.primed_model_tab {
            self.admin.add_provider.open_builtin(None);
            self.primed_model_tab = true;
        }

        self.workspace.poll_toast();
        let chat_surface = matches!(surface, FirstRunSurface::Chat);
        self.show_title_bar(ctx, chat_surface);
        if chat_surface {
            self.handle_folder_input(ctx);
            self.show_status_bar(ctx);
            self.show_icon_rail(ctx);
            if self.workspace.dock_open {
                self.show_workspace_dock(ctx);
            }
        }

        self.admin.memory_insights = Some(self.sessions.active_chat().insights.clone());
        let panel_ret = if matches!(surface, FirstRunSurface::ModelSetup) {
            AdminPanelReturn::default()
        } else {
            self.admin.show_panel(
                ctx,
                None,
                self.rpc_client.is_some(),
                self.pi_probe.is_ready(),
                self.pi_probe.path(),
                &mut self.remote,
            )
        };
        if let Some(req) = panel_ret.set_model_request {
            self.handle_set_model_request(req);
        }
        if let Some(level) = panel_ret.set_thinking_level {
            self.handle_set_thinking_level(&level);
        }
        if panel_ret.restart_sidecar {
            self.restart_sidecar();
        }

        match surface {
            FirstRunSurface::ModelSetup => self.show_model_onboarding(ctx),
            FirstRunSurface::PiMissing => self.show_pi_missing(ctx),
            FirstRunSurface::Chat => {
                if self.rpc_client.is_some() {
                    self.show_chat_ui(ctx);
                } else if let Some(err) = self.spawn_error.clone() {
                    self.show_spawn_error(ctx, &err);
                } else {
                    self.show_pi_disconnected(ctx);
                }
            }
        }
        if let Some(action) = self.welcome_action.take() {
            self.apply_dock_action(ctx, action);
        }
        self.handle_window_frame(ctx, chat_surface);
        self.persist_sessions(false);
    }
}

impl PiLightApp {
    // ── Chat UI ───────────────────────────────────────────────────────────

    fn show_workspace_dock(&mut self, ctx: &egui::Context) {
        let streaming = self.sessions.any_streaming();
        let switching = self.workspace.switching;
        let workdir = self.workdir.clone();
        let mut action = None;
        egui::SidePanel::left("workspace_dock")
            .resizable(false)
            .exact_width(248.0)
            .frame(
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(10.0, 10.0))
                    .fill(theme::sidebar_bg())
                    .stroke(egui::Stroke::new(1.0_f32, theme::hairline())),
            )
            .show(ctx, |ui| {
                action = workspace::show_dock(
                    ui,
                    &mut self.workspace,
                    &self.sessions,
                    &workdir,
                    streaming,
                    switching,
                    Some(&self.textures),
                );
            });
        if let Some(next) = action {
            self.apply_dock_action(ctx, next);
        }
    }

    fn show_icon_rail(&mut self, ctx: &egui::Context) {
        let mut action = None;
        egui::SidePanel::left("icon_rail")
            .resizable(false)
            .exact_width(48.0)
            .frame(
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(8.0, 10.0))
                    .fill(theme::rail_bg())
                    .stroke(egui::Stroke::new(1.0_f32, theme::hairline())),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                for (tab, icon, tip) in [
                    (workspace::DockTab::Sessions, Icon::Chat, "会话"),
                    (workspace::DockTab::Files, Icon::File, "文件"),
                    (workspace::DockTab::Git, Icon::Git, "Git"),
                ] {
                    let active = self.workspace.dock_open && self.workspace.dock == tab;
                    let resp = icons::icon_button_sized(ui, icon, 32.0, 18.0, tip, active);
                    if active {
                        let r = resp.rect;
                        ui.painter().rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(r.left() - 8.0, r.top() + 7.0),
                                egui::pos2(r.left() - 6.0, r.bottom() - 7.0),
                            ),
                            egui::Rounding::same(1.0),
                            theme::accent(),
                        );
                    }
                    if resp.clicked() {
                        if active {
                            self.workspace.dock_open = false;
                        } else {
                            self.workspace.dock = tab;
                            self.workspace.dock_open = true;
                        }
                    }
                }
                if icons::icon_button_sized(ui, Icon::Puzzle, 32.0, 18.0, "扩展市场", false)
                    .clicked()
                {
                    action = Some(DockAction::OpenMarketplace);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    if icons::icon_button_sized(ui, Icon::Gear, 32.0, 18.0, "设置", false).clicked()
                    {
                        action = Some(DockAction::OpenSettings);
                    }
                });
            });
        if let Some(next) = action {
            self.apply_dock_action(ctx, next);
        }
    }

    fn apply_dock_action(&mut self, ctx: &egui::Context, action: DockAction) {
        match action {
            DockAction::NewSession => self.do_new_session(),
            DockAction::SwitchSession(i) => self.do_switch_session(i),
            DockAction::ForkSession(i) => self.do_fork_session(i),
            DockAction::QuoteSession(i) => self.do_quote_session(i),
            DockAction::DeleteSession(i) => self.do_delete_session(i),
            DockAction::StartRename(i) => {
                if let Some(session) = self.sessions.list().get(i) {
                    self.workspace.rename_draft = session.title.clone();
                    self.workspace.rename_index = Some(i);
                }
            }
            DockAction::ConfirmRename => {
                if let Some(i) = self.workspace.rename_index.take() {
                    let title = self.workspace.rename_draft.trim().to_owned();
                    if self.sessions.rename(i, &title) {
                        if i == self.sessions.active_index() {
                            if let Some(client) = &mut self.rpc_client {
                                let _ = client.send_set_session_name(&title);
                            }
                        }
                        self.touch_sessions();
                        self.persist_sessions(true);
                    }
                }
                self.workspace.rename_draft.clear();
            }
            DockAction::CancelRename => {
                self.workspace.rename_index = None;
                self.workspace.rename_draft.clear();
            }
            DockAction::OpenSettings => {
                self.admin.open = !self.admin.open;
                if self.admin.open {
                    self.admin.opened_pass = Some(ctx.cumulative_pass_nr());
                } else {
                    self.admin.opened_pass = None;
                }
            }
            DockAction::OpenMarketplace => {
                self.admin.open = true;
                self.admin.active_tab = AdminTab::Marketplace;
                self.admin.opened_pass = Some(ctx.cumulative_pass_nr());
            }
            DockAction::OpenRemote => {
                self.admin.open = true;
                self.admin.active_tab = AdminTab::RemoteAccess;
                self.admin.opened_pass = Some(ctx.cumulative_pass_nr());
            }
            DockAction::InsertPath(rel) => self.insert_composer_path(&rel),
            DockAction::RequestWorkdir(raw) => self.request_workdir_change(&raw),
            DockAction::PickFolder => self.pick_folder(),
            DockAction::OpenCwdPopover => {
                self.workspace.cwd_popover = true;
            }
            DockAction::CloseCwdPopover => {
                self.workspace.cwd_popover = false;
            }
            DockAction::GitStage => {
                if let Some(path) = self.workspace.selected_git.clone() {
                    match self.workspace.git.stage(&path) {
                        Ok(()) => self.workspace.git_action_error = None,
                        Err(err) => self.workspace.git_action_error = Some(err),
                    }
                }
            }
            DockAction::GitUnstage => {
                if let Some(path) = self.workspace.selected_git.clone() {
                    match self.workspace.git.unstage(&path) {
                        Ok(()) => self.workspace.git_action_error = None,
                        Err(err) => self.workspace.git_action_error = Some(err),
                    }
                }
            }
            DockAction::GitCommit => match self
                .workspace
                .git
                .commit(&self.workspace.commit_draft.clone())
            {
                Ok(()) => {
                    self.workspace.commit_draft.clear();
                    self.workspace.commit_confirm = false;
                    self.workspace.git_action_error = None;
                }
                Err(err) => {
                    self.workspace.git_action_error = Some(err);
                    self.workspace.commit_confirm = false;
                }
            },
            DockAction::GitReveal => {
                if let Some(rel) = self.workspace.selected_git.clone() {
                    let abs = self.workdir.join(&rel);
                    if let Err(err) = workspace_git::reveal_in_file_manager(&abs) {
                        self.workspace.git_action_error = Some(err);
                    }
                }
            }
            DockAction::SelectGitPath(path) => {
                self.workspace.selected_git = Some(path);
            }
            DockAction::RetrySwitch => {
                if let Some(path) = self.workspace.retry_path.clone() {
                    self.request_workdir_change(&path.to_string_lossy());
                }
            }
            DockAction::DismissError => {
                self.workspace.switch_error = None;
                self.workdir_error = None;
            }
        }
    }

    fn pick_folder(&mut self) {
        self.workspace.cwd_popover = false;
        let picked = rfd::FileDialog::new()
            .set_title("打开文件夹")
            .set_directory(&self.workdir)
            .pick_folder();
        if let Some(path) = picked {
            self.apply_workdir_path(path);
        }
    }

    /// Ctrl+O and folders dropped onto the window both switch the workspace.
    fn handle_folder_input(&mut self, ctx: &egui::Context) {
        let open = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::O,
            ))
        });
        if open {
            self.pick_folder();
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(path) = dropped.into_iter().next() {
            if path.is_dir() {
                self.apply_workdir_path(path);
            } else {
                self.workspace.show_toast("请拖入文件夹以切换工作区");
            }
        }
    }

    fn show_title_bar(&mut self, ctx: &egui::Context, chat: bool) {
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let streaming = chat && self.sessions.active_is_streaming();
        let recent = workspace::recent_workdirs(&self.sessions, &self.workdir);
        let mut action = None;
        let mut reprobe = false;
        egui::TopBottomPanel::top("title_bar")
            .exact_height(44.0)
            .frame(
                egui::Frame::none()
                    .fill(theme::rail_bg())
                    .inner_margin(egui::Margin {
                        left: 14.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }),
            )
            .show(ctx, |ui| {
                ui.style_mut().interaction.selectable_labels = false;
                let bar = ui.max_rect();
                let drag = ui.interact(
                    bar,
                    ui.id().with("title_drag"),
                    egui::Sense::click_and_drag(),
                );
                if drag.double_clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                } else if drag.drag_started_by(egui::PointerButton::Primary) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                ui.horizontal_centered(|ui| {
                    theme::pi_avatar(ui, 22.0);
                    ui.label(
                        egui::RichText::new("pi-light")
                            .strong()
                            .size(13.5)
                            .color(theme::text_primary()),
                    );
                    if chat {
                        ui.add_space(10.0);
                        action = workspace::show_workspace_switcher(
                            ui,
                            &self.workdir,
                            self.workspace.switching,
                            self.workspace.cwd_popover,
                            &recent,
                            self.workspace
                                .switch_error
                                .as_deref()
                                .or(self.workdir_error.as_deref()),
                        );
                        if theme::toolbar_button(ui, Icon::FolderOpen, "打开文件夹")
                            .on_hover_text("Ctrl+O")
                            .clicked()
                        {
                            action = Some(DockAction::PickFolder);
                        }
                        if streaming {
                            ui.add_space(4.0);
                            theme::status_pill(ui, "生成中", theme::warn());
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        if theme::window_button(ui, Icon::Close, true).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        let max_icon = if maximized {
                            Icon::Restore
                        } else {
                            Icon::Maximize
                        };
                        if theme::window_button(ui, max_icon, false).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        }
                        if theme::window_button(ui, Icon::Minimize, false).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        if !chat {
                            return;
                        }
                        ui.add_space(10.0);
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if icons::icon_button(ui, Icon::Gear, "设置").clicked() {
                            action = Some(DockAction::OpenSettings);
                        }
                        if icons::icon_button(ui, Icon::Refresh, "重新扫描 Pi 并重连").clicked()
                        {
                            reprobe = true;
                        }
                        if icons::icon_button(ui, Icon::Globe, "远程访问").clicked() {
                            action = Some(DockAction::OpenRemote);
                        }
                    });
                });
            });
        if let Some(action) = action {
            self.apply_dock_action(ctx, action);
        }
        if reprobe {
            self.reprobe();
        }
    }

    /// Frameless window chrome: 1px border, edge resize handles, and the
    /// drop-a-folder overlay.
    fn handle_window_frame(&mut self, ctx: &egui::Context, chat: bool) {
        let screen = ctx.screen_rect();
        let (maximized, fullscreen) = ctx.input(|i| {
            let vp = i.viewport();
            (
                vp.maximized.unwrap_or(false),
                vp.fullscreen.unwrap_or(false),
            )
        });
        let fg = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("window_chrome"),
        ));
        if chat && ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            fg.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(170));
            let card = egui::Rect::from_center_size(screen.center(), egui::vec2(360.0, 150.0));
            fg.rect(
                card,
                theme::radius::XL,
                theme::sidebar_bg(),
                egui::Stroke::new(1.5_f32, theme::accent()),
            );
            icons::paint(
                &fg,
                egui::Rect::from_center_size(
                    card.center() - egui::vec2(0.0, 26.0),
                    egui::vec2(34.0, 34.0),
                ),
                Icon::FolderOpen,
                theme::accent_bright(),
            );
            fg.text(
                card.center() + egui::vec2(0.0, 18.0),
                egui::Align2::CENTER_CENTER,
                "松开以打开此文件夹",
                egui::FontId::proportional(16.0),
                theme::text_primary(),
            );
            fg.text(
                card.center() + egui::vec2(0.0, 44.0),
                egui::Align2::CENTER_CENTER,
                "将作为新的工作区",
                egui::FontId::proportional(12.0),
                theme::text_dim(),
            );
        }
        if maximized || fullscreen {
            return;
        }
        fg.rect_stroke(
            screen.shrink(0.5),
            0.0,
            egui::Stroke::new(1.0_f32, theme::hairline_strong()),
        );
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        const EDGE: f32 = 5.0;
        let left = pos.x - screen.left() <= EDGE;
        let right = screen.right() - pos.x <= EDGE;
        let top = pos.y - screen.top() <= EDGE;
        let bottom = screen.bottom() - pos.y <= EDGE;
        use egui::{CursorIcon as C, ResizeDirection as R};
        let hit = match (left, right, top, bottom) {
            (true, _, true, _) => Some((R::NorthWest, C::ResizeNorthWest)),
            (_, true, true, _) => Some((R::NorthEast, C::ResizeNorthEast)),
            (true, _, _, true) => Some((R::SouthWest, C::ResizeSouthWest)),
            (_, true, _, true) => Some((R::SouthEast, C::ResizeSouthEast)),
            (true, ..) => Some((R::West, C::ResizeWest)),
            (_, true, ..) => Some((R::East, C::ResizeEast)),
            (_, _, true, _) => Some((R::North, C::ResizeNorth)),
            (.., true) => Some((R::South, C::ResizeSouth)),
            _ => None,
        };
        if let Some((dir, cursor)) = hit {
            ctx.set_cursor_icon(cursor);
            if ctx.input(|i| i.pointer.primary_pressed()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
            }
        }
    }

    fn apply_model_spec(&mut self, spec: &str) {
        if let Some((provider, model_id)) = provider_catalog::parse_model_spec(spec) {
            if let Some(path) = self.admin.model_config.settings_path.clone() {
                if write_settings_model(&path, &provider, &model_id).is_ok() {
                    self.admin.model_config.default_provider = Some(provider.clone());
                    self.admin.model_config.default_model = Some(model_id.clone());
                }
            }
            self.handle_set_model_request(SetModelRequest { provider, model_id });
        }
    }

    fn show_chat_ui(&mut self, ctx: &egui::Context) {
        // Copy flags before any mutable borrow of active_chat().input.
        let is_streaming = self.sessions.active_is_streaming();
        let can_retry = self.sessions.active_chat().can_retry();
        let chips_empty = self.ref_chips.is_empty();
        let current_spec = provider_catalog::display_model_spec(
            self.admin.model_config.live_model.as_deref(),
            self.admin.model_config.default_provider.as_deref(),
            self.admin.model_config.default_model.as_deref(),
            &self.admin.model_config.available_models,
            &self.admin.model_config.custom_providers,
        );
        let usable_models = provider_catalog::usable_model_specs(
            &self.admin.model_config.available_models,
            &self.admin.model_config.custom_providers,
        );
        let mut send_now = false;
        let mut abort_now = false;
        let mut retry_now = false;
        let mut apply_model: Option<String> = None;

        let inspector_open = self
            .inspector_open
            .unwrap_or(ctx.screen_rect().width() >= 1280.0);
        if inspector_open {
            let chat = self.sessions.active_chat();
            let stats = chat.insights.session_stats.as_ref();
            let data = timeline_ui::InspectorData {
                context_used: stats
                    .and_then(|s| s.context_tokens)
                    .or(chat.insights.effective_usage().total),
                context_window: stats.and_then(|s| s.context_window),
                model: current_spec.clone(),
                thinking: self
                    .admin
                    .model_config
                    .thinking_level
                    .clone()
                    .unwrap_or_else(|| "off".to_owned()),
                steps: &chat.insights.trace_steps,
                tps: chat.insights.last_tps,
                cost: stats.and_then(|s| s.cost),
            };
            egui::SidePanel::right("context_inspector")
                .resizable(false)
                .exact_width(272.0)
                .frame(
                    egui::Frame::none()
                        .fill(theme::sidebar_bg())
                        .stroke(egui::Stroke::new(1.0_f32, theme::hairline()))
                        .inner_margin(egui::Margin::same(12.0)),
                )
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| timeline_ui::show_inspector(ui, &data));
                });
        }
        self.show_session_bar(ctx, &current_spec, inspector_open);

        egui::TopBottomPanel::bottom("chat_input")
            .frame(
                egui::Frame::none()
                    .fill(theme::chrome_bg())
                    .inner_margin(egui::Margin {
                        left: 24.0,
                        right: 24.0,
                        top: 4.0,
                        bottom: 14.0,
                    }),
            )
            .show(ctx, |ui| {
                let mut chip_to_remove: Option<usize> = None;
                let mut insert_at = false;
                theme::centered_column(ui, 820.0, |ui| {
                    egui::Frame::none()
                        .fill(theme::card_bg())
                        .stroke(egui::Stroke::new(
                            1.0_f32,
                            theme::accent().gamma_multiply(0.45),
                        ))
                        .rounding(egui::Rounding::same(theme::radius::XL))
                        .shadow(egui::epaint::Shadow {
                            offset: egui::vec2(0.0, 6.0),
                            blur: 24.0,
                            spread: 0.0,
                            color: egui::Color32::from_black_alpha(110),
                        })
                        .inner_margin(egui::Margin {
                            left: 12.0,
                            right: 10.0,
                            top: 10.0,
                            bottom: 8.0,
                        })
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if !self.ref_chips.is_empty() {
                                ui.horizontal_wrapped(|ui| {
                                    for (i, chip) in self.ref_chips.iter().enumerate() {
                                        let label =
                                            format!("# {}  ×", truncate_display(&chip.display, 40));
                                        let btn = egui::Button::new(
                                            egui::RichText::new(label)
                                                .size(12.0)
                                                .color(theme::accent_bright()),
                                        )
                                        .fill(theme::accent().gamma_multiply(0.18))
                                        .stroke(egui::Stroke::new(
                                            1.0_f32,
                                            theme::accent().gamma_multiply(0.35),
                                        ))
                                        .rounding(egui::Rounding::same(theme::radius::SM));
                                        if ui.add(btn).on_hover_text("点击移除此引用").clicked()
                                        {
                                            chip_to_remove = Some(i);
                                        }
                                    }
                                });
                                ui.add_space(6.0);
                            }
                            let (input_response, can_send) = {
                                let input = &mut self.sessions.active_chat_mut().input;
                                let can_send = !is_streaming
                                    && !self.workspace.switching
                                    && (!input.trim().is_empty() || !chips_empty);
                                let resp = ui.add(
                                    egui::TextEdit::singleline(input)
                                        .hint_text("给 Pi 发消息…  / 命令 · @ 文件 · # 引用会话")
                                        .font(egui::FontId::proportional(14.5))
                                        .frame(false)
                                        .margin(egui::vec2(4.0, 6.0))
                                        .desired_width(ui.available_width().max(0.0))
                                        .interactive(!is_streaming),
                                );
                                (resp, can_send)
                            };
                            if input_response.lost_focus()
                                && ctx.input(|i| i.key_pressed(egui::Key::Enter))
                                && can_send
                                && !self.invoke.active
                            {
                                send_now = true;
                                input_response.request_focus();
                            }
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                if icons::icon_button(ui, Icon::File, "引用文件 (@)").clicked()
                                {
                                    insert_at = true;
                                    input_response.request_focus();
                                }
                                if let Some(spec) = show_model_chip(
                                    ui,
                                    "composer_model_combo",
                                    &current_spec,
                                    &usable_models,
                                    150.0,
                                ) {
                                    apply_model = Some(spec);
                                }
                                self.show_thinking_chip(ui);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if is_streaming {
                                            let stop = egui::Button::new(
                                                egui::RichText::new("停止")
                                                    .size(12.5)
                                                    .color(theme::danger()),
                                            )
                                            .fill(theme::danger().gamma_multiply(0.12))
                                            .stroke(egui::Stroke::new(
                                                1.0_f32,
                                                theme::danger().gamma_multiply(0.5),
                                            ))
                                            .rounding(egui::Rounding::same(theme::radius::SM))
                                            .min_size(egui::vec2(60.0, 30.0));
                                            if ui
                                                .add(stop)
                                                .on_hover_text("中止当前生成（发送 abort 命令）")
                                                .clicked()
                                            {
                                                abort_now = true;
                                            }
                                        } else {
                                            if theme::send_button(ui, can_send).clicked() {
                                                send_now = true;
                                                input_response.request_focus();
                                            }
                                            let retry = ui
                                                .add_enabled_ui(can_retry, |ui| {
                                                    icons::icon_button(
                                                        ui,
                                                        Icon::Refresh,
                                                        "重新生成最后一条回复（同会话分支）",
                                                    )
                                                })
                                                .inner;
                                            if retry.clicked() {
                                                retry_now = true;
                                            }
                                        }
                                    },
                                );
                            });
                        });
                });
                if let Some(idx) = chip_to_remove {
                    self.ref_chips.remove(idx);
                }
                if insert_at {
                    self.sessions.active_chat_mut().input.push('@');
                }
            });

        if send_now {
            self.do_send();
        }
        if abort_now {
            self.do_abort();
        }
        if retry_now {
            self.do_retry();
        }
        if let Some(spec) = apply_model {
            self.apply_model_spec(&spec);
        }
        if self.invoke.active {
            self.show_invoke_popup(ctx);
        }

        let is_streaming = self.sessions.active_is_streaming();
        let empty = self.sessions.active_chat().messages.is_empty();

        let mut action: Option<ChatUiAction> = None;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(theme::chrome_bg())
                    .inner_margin(egui::Margin::symmetric(24.0, 12.0)),
            )
            .show(ctx, |ui| {
                self.paint_main_backdrop(ui);
                if empty && !is_streaming {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| self.show_chat_welcome(ui));
                    return;
                }

                if let Some(err) = self.sessions.active_chat().error.clone() {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.colored_label(theme::danger(), format!("⚠ {err}"));
                        if ui.small_button("关闭").clicked() {
                            self.sessions.active_chat_mut().error = None;
                        }
                    });
                    ui.separator();
                }

                if let Some(tree_action) = self.show_message_tree_panel(ui, is_streaming) {
                    action = Some(tree_action);
                }

                let scroll_to_bottom = self.sessions.active_chat().scroll_to_bottom;
                self.sessions.active_chat_mut().scroll_to_bottom = false;

                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        theme::centered_column(ui, 820.0, |ui| {
                            ui.add_space(8.0);

                            let streaming = self.sessions.active_chat().is_streaming;
                            let streaming_buf = self.sessions.active_chat().streaming_buf.clone();
                            let has_tools =
                                !self.sessions.active_chat().insights.trace_steps.is_empty();
                            let rows = self.sessions.active_chat().path_rows();
                            let last_assistant =
                                rows.iter().rposition(|row| row.role == NodeRole::Assistant);
                            let editing_node = self.editing_node;
                            for (i, row) in rows.iter().enumerate() {
                                match row.role {
                                    NodeRole::User => {
                                        let editing = editing_node == Some(row.id);
                                        if let Some(next) = show_user_message(
                                            ui,
                                            row,
                                            editing,
                                            &mut self.edit_draft,
                                            !streaming,
                                        ) {
                                            action = Some(next);
                                        }
                                    }
                                    NodeRole::Assistant => {
                                        if Some(i) == last_assistant && has_tools && !streaming {
                                            show_thinking_block(ui, self.sessions.active_chat());
                                            ui.add_space(8.0);
                                        }
                                        if let Some(next) =
                                            show_assistant_message(ui, row, !streaming)
                                        {
                                            action = Some(next);
                                        }
                                    }
                                }
                                ui.add_space(14.0);
                            }

                            if streaming {
                                if has_tools {
                                    show_thinking_block(ui, self.sessions.active_chat());
                                    ui.add_space(8.0);
                                }
                                show_streaming_assistant(ui, &streaming_buf);
                                ui.add_space(6.0);
                            }
                        });

                        if scroll_to_bottom {
                            ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                        }
                    });
            });
        if let Some(next) = action {
            self.apply_chat_action(next);
        }
        if let Some(toast) = self.workspace.toast.as_ref() {
            workspace::show_toast_overlay(ctx, &toast.text);
        }
    }

    fn show_message_tree_panel(
        &mut self,
        ui: &mut egui::Ui,
        streaming: bool,
    ) -> Option<ChatUiAction> {
        let rows = self.sessions.active_chat().tree_rows();
        if rows.is_empty() {
            return None;
        }
        let mut action = None;
        let has_branches = self.sessions.active_chat().tree.has_branches();
        if !has_branches && !self.tree_open {
            return None;
        }
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("消息树 · {} 节点 · 有分支", rows.len()))
                    .size(12.0)
                    .color(theme::text_muted()),
            )
            .on_hover_text(BRANCH_HONESTY);
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new(if self.tree_open { "收起" } else { "展开" })
                            .size(12.0)
                            .color(theme::accent_soft()),
                    )
                    .frame(false),
                )
                .clicked()
            {
                self.tree_open = !self.tree_open;
            }
        });
        if self.tree_open {
            theme::card_frame().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("message_tree_nav")
                    .max_height(160.0)
                    .show(ui, |ui| {
                        for row in &rows {
                            let indent = "    ".repeat(row.depth);
                            let role = match row.role {
                                NodeRole::User => "用户",
                                NodeRole::Assistant => "助手",
                            };
                            let mark = if row.on_active_path { "●" } else { "○" };
                            let snippet = truncate_display(&row.text.replace('\n', " "), 36);
                            let text = format!("{indent}{mark} {role}  {snippet}");
                            let color = if row.on_active_path {
                                theme::text_primary()
                            } else {
                                theme::text_muted()
                            };
                            let resp = ui.add_enabled(
                                !streaming,
                                egui::SelectableLabel::new(
                                    row.on_active_path,
                                    egui::RichText::new(text).size(12.0).color(color),
                                ),
                            );
                            if resp.clicked() {
                                action = Some(ChatUiAction::Navigate(row.id));
                            }
                        }
                    });
            });
            ui.add_space(8.0);
        }
        action
    }

    fn apply_chat_action(&mut self, action: ChatUiAction) {
        match action {
            ChatUiAction::Prev(id) => self.do_select_sibling(id, -1),
            ChatUiAction::Next(id) => self.do_select_sibling(id, 1),
            ChatUiAction::EditStart(id) => {
                if let Some(text) = self.sessions.active_chat().tree.text(id) {
                    self.editing_node = Some(id);
                    self.edit_draft = text.to_owned();
                }
            }
            ChatUiAction::EditConfirm => {
                if let Some(id) = self.editing_node {
                    let draft = self.edit_draft.clone();
                    self.do_edit_user(id, draft);
                }
            }
            ChatUiAction::EditCancel => {
                self.editing_node = None;
                self.edit_draft.clear();
            }
            ChatUiAction::Regen(id) => self.do_regenerate(id),
            ChatUiAction::Navigate(id) => self.do_navigate(id),
        }
    }

    fn show_session_bar(&mut self, ctx: &egui::Context, spec: &str, inspector_open: bool) {
        let title = self.sessions.active().title.clone();
        let id = self.sessions.active().id;
        let turns = self.sessions.active_chat().path_rows().len();
        let streaming = self.sessions.active_is_streaming();
        let thinking = self
            .admin
            .model_config
            .thinking_level
            .clone()
            .unwrap_or_else(|| "off".to_owned());
        let mut banner_action = None;
        let mut toggle_inspector = false;

        egui::TopBottomPanel::top("session_bar")
            .exact_height(if self.workspace.switch_error.is_some() {
                84.0
            } else {
                44.0
            })
            .frame(
                egui::Frame::none()
                    .fill(theme::chrome_bg())
                    .stroke(egui::Stroke::new(1.0_f32, theme::hairline()))
                    .inner_margin(egui::Margin::symmetric(20.0, 8.0)),
            )
            .show(ctx, |ui| {
                if let Some(err) = self.workspace.switch_error.clone() {
                    banner_action = workspace::show_switch_error_banner(ui, &err);
                    ui.add_space(4.0);
                }
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::icon_button_sized(
                            ui,
                            Icon::PanelRight,
                            28.0,
                            16.0,
                            "上下文面板",
                            inspector_open,
                        )
                        .clicked()
                        {
                            toggle_inspector = true;
                        }
                        if ui.available_width() > 360.0 {
                            let model = spec.rsplit('/').next().unwrap_or(spec);
                            let model = if model.is_empty() {
                                "未选择模型"
                            } else {
                                model
                            };
                            egui::Frame::none()
                                .stroke(egui::Stroke::new(1.0_f32, theme::hairline_strong()))
                                .rounding(egui::Rounding::same(theme::radius::SM))
                                .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{model} · 思考 {thinking}"))
                                            .monospace()
                                            .size(11.0)
                                            .color(theme::text_muted()),
                                    );
                                });
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let meta_w = if streaming { 96.0 } else { 90.0 };
                            ui.scope(|ui| {
                                ui.set_max_width((ui.available_width() - meta_w).max(40.0));
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&title)
                                            .strong()
                                            .size(14.0)
                                            .color(theme::text_primary()),
                                    )
                                    .truncate(),
                                );
                            });
                            if streaming {
                                theme::status_pill(ui, "Agent 运行中", theme::accent_cyan());
                            } else {
                                let meta = if turns == 0 {
                                    format!("#{id}")
                                } else {
                                    format!("#{id} · {turns} 条消息")
                                };
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(meta)
                                            .size(11.5)
                                            .color(theme::text_dim()),
                                    )
                                    .truncate(),
                                );
                            }
                        });
                    });
                });
            });

        if toggle_inspector {
            self.inspector_open = Some(!inspector_open);
        }
        if let Some(action) = banner_action {
            self.apply_dock_action(ctx, action);
        }
    }

    fn show_thinking_chip(&mut self, ui: &mut egui::Ui) {
        let levels = self.admin.model_config.thinking_levels.clone();
        let current = self
            .admin
            .model_config
            .thinking_level
            .clone()
            .unwrap_or_else(|| "off".to_owned());
        let usable = composer_invoke::thinking_levels_usable(&levels)
            && !self.admin.model_config.thinking_rpc_failed;
        let label = if usable {
            format!("思考 · {current}")
        } else {
            "思考 · 不可用".to_owned()
        };
        let resp = ui.add_enabled(
            usable,
            egui::Button::new(
                egui::RichText::new(label)
                    .size(12.0)
                    .color(theme::text_muted()),
            )
            .fill(egui::Color32::TRANSPARENT)
            .stroke(egui::Stroke::new(1.0_f32, theme::hairline()))
            .rounding(egui::Rounding::same(13.0))
            .min_size(egui::vec2(0.0, 26.0)),
        );
        if !usable {
            resp.on_hover_text("当前模型不支持思考强度");
            return;
        }
        resp.on_hover_text("会话态 thinkingLevel；选项来自 RPC")
            .context_menu(|ui| {
                for level in &levels {
                    if ui.selectable_label(level == &current, level).clicked() {
                        self.handle_set_thinking_level(level);
                        ui.close_menu();
                    }
                }
            });
    }

    fn show_invoke_popup(&mut self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let chips_extra = if self.ref_chips.is_empty() { 0.0 } else { 28.0 };
        let popup_height = 220.0f32;
        let popup_width = (screen.width() - 280.0).clamp(320.0, 560.0);
        let pos = egui::pos2(
            268.0,
            screen.bottom() - 32.0 - 108.0 - chips_extra - popup_height - 8.0,
        );
        let title = match self.invoke.kind {
            Some(InvokeKind::Slash) => "/ 命令",
            Some(InvokeKind::At) => "@ 文件",
            Some(InvokeKind::Hash) => "# 引用会话",
            None => "唤起",
        };
        let empty = match self.invoke.kind {
            Some(InvokeKind::Slash) => {
                "没有可执行的 Pi 命令。TUI 内置项（如 /settings）不会经 RPC 执行。"
            }
            Some(InvokeKind::At) => "当前工作区没有可引用的文件（空目录 / 无权限 / 已忽略）。",
            Some(InvokeKind::Hash) => "暂无引用。开始对话后，历史消息会出现在此列表。",
            None => "暂无候选",
        };
        let mut click_idx = None;
        egui::Area::new(egui::Id::new("composer_invoke_popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .interactable(true)
            .show(ctx, |ui| {
                theme::glass_frame()
                    .inner_margin(egui::Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.set_min_width(popup_width);
                        ui.set_max_width(popup_width);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(title).strong().size(12.0));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new("↑↓ 选择  Enter/Tab 确认  Esc 关闭")
                                            .weak()
                                            .size(10.0),
                                    );
                                },
                            );
                        });
                        ui.separator();
                        if self.invoke.filtered.is_empty() {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(empty).weak().italics().size(12.0));
                        } else {
                            egui::ScrollArea::vertical()
                                .max_height(popup_height - 52.0)
                                .show(ui, |ui| {
                                    let cursor = self.invoke.cursor;
                                    for (i, candidate) in self.invoke.filtered.iter().enumerate() {
                                        let selected = i == cursor;
                                        let resp = ui.selectable_label(
                                            selected,
                                            egui::RichText::new(&candidate.display).size(12.5),
                                        );
                                        if !candidate.detail.is_empty() {
                                            resp.clone().on_hover_text(&candidate.detail);
                                        }
                                        if resp.clicked() {
                                            click_idx = Some(i);
                                        }
                                        if selected {
                                            resp.scroll_to_me(None);
                                        }
                                    }
                                });
                        }
                    });
            });
        if let Some(idx) = click_idx {
            self.invoke.cursor = idx;
            if let Some(item) = self.invoke.confirm() {
                self.apply_invoke_candidate(item);
            }
        }
    }

    fn show_chat_welcome(&mut self, ui: &mut egui::Ui) {
        let top = ((ui.available_height() - 300.0) * 0.4).clamp(24.0, 160.0);
        ui.add_space(top);
        theme::centered_column(ui, 520.0, |ui| {
            ui.vertical_centered(|ui| {
                theme::pi_avatar(ui, 48.0);
                ui.add_space(14.0);
                ui.label(
                    egui::RichText::new("今天想做点什么？")
                        .strong()
                        .size(24.0)
                        .color(theme::text_primary()),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Pi 会在当前工作区中读写文件、运行命令。")
                        .size(13.5)
                        .color(theme::text_muted()),
                );
            });
            ui.add_space(18.0);
            let recent = workspace::recent_workdirs(&self.sessions, &self.workdir);
            let current = workspace::display_workdir(&self.workdir);
            let mut picked = None;
            egui::Frame::none()
                .fill(theme::card_bg())
                .stroke(egui::Stroke::new(1.0_f32, theme::hairline()))
                .rounding(egui::Rounding::same(theme::radius::LG))
                .inner_margin(egui::Margin::same(6.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let title = format!("工作区 · {}", workspace::workdir_name(&self.workdir));
                    if theme::menu_row(
                        ui,
                        Icon::FolderOpen,
                        &title,
                        Some(&current),
                        Some("打开文件夹  Ctrl+O"),
                        true,
                    )
                    .clicked()
                    {
                        picked = Some(DockAction::PickFolder);
                    }
                    for path in recent.iter().skip(1).take(3) {
                        let detail = workspace::display_workdir(path);
                        if theme::menu_row(
                            ui,
                            Icon::Clock,
                            &workspace::workdir_name(path),
                            Some(&detail),
                            None,
                            false,
                        )
                        .clicked()
                        {
                            picked = Some(DockAction::RequestWorkdir(
                                path.to_string_lossy().into_owned(),
                            ));
                        }
                    }
                });
            if picked.is_some() {
                self.welcome_action = picked;
            }
            ui.add_space(22.0);
            for (key, title, detail) in [
                ("/", "调用命令", "执行 Pi 提供的斜杠命令"),
                ("@", "引用文件", "把工作区文件加入上下文"),
                ("#", "引用会话", "带上其他会话里的消息"),
            ] {
                egui::Frame::none()
                    .fill(theme::card_bg())
                    .rounding(egui::Rounding::same(10.0))
                    .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            theme::keycap(ui, key);
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(title)
                                    .size(13.5)
                                    .color(theme::text_primary()),
                            );
                            ui.label(
                                egui::RichText::new(detail)
                                    .size(12.5)
                                    .color(theme::text_dim()),
                            );
                        });
                    });
                ui.add_space(6.0);
            }
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("消息可编辑、重新生成，并在同会话分支间切换。")
                        .size(12.0)
                        .color(theme::text_dim()),
                );
            });
        });
    }

    fn show_status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("app_footer")
            .exact_height(32.0)
            .frame(
                theme::glass_bar()
                    .fill(theme::sidebar_bg())
                    .rounding(egui::Rounding::ZERO)
                    .inner_margin(egui::Margin::symmetric(14.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let online = self.rpc_client.is_some() && self.pi_probe.is_ready();
                    let (dot_color, dot_tip) =
                        pi_dot_color(&self.pi_probe, self.rpc_client.is_some());
                    let color = if online { theme::ok() } else { dot_color };
                    let (dot, _) =
                        ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                    ui.painter().circle_filled(dot.center(), 3.5, color);
                    ui.label(
                        egui::RichText::new(if online { "Pi 已连接" } else { "Pi 离线" })
                            .size(11.5)
                            .color(color),
                    )
                    .on_hover_text(dot_tip);
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(workspace::display_workdir(&self.workdir))
                            .monospace()
                            .size(11.0)
                            .color(theme::text_dim()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let remote_on = self.remote.persist.enabled;
                        ui.label(
                            egui::RichText::new(if remote_on {
                                "远程访问 开"
                            } else {
                                "远程访问 关"
                            })
                            .size(11.0)
                            .color(if remote_on {
                                theme::accent_cyan()
                            } else {
                                theme::text_dim()
                            }),
                        );
                        ui.separator();
                        let (tokens, trace, tps) =
                            self.sessions.active_chat().insights.footer_metric_parts();
                        ui.label(
                            egui::RichText::new(tps)
                                .size(11.0)
                                .color(theme::text_muted()),
                        );
                        ui.separator();
                        ui.label(
                            egui::RichText::new(trace)
                                .size(11.0)
                                .color(theme::text_muted()),
                        );
                        ui.separator();
                        ui.label(
                            egui::RichText::new(tokens)
                                .size(11.0)
                                .color(theme::text_muted()),
                        );
                    });
                });
            });
    }
}

// ── Pi status dot helpers ─────────────────────────────────────────────────

fn pi_dot_color(probe: &PiProbe, rpc_alive: bool) -> (egui::Color32, &'static str) {
    if !probe.is_ready() {
        return (
            theme::danger(),
            "Pi 未就绪。恢复说明在主界面折叠区和设置「通用」页。",
        );
    }
    if rpc_alive {
        match probe {
            PiProbe::BundledReady(_) => (theme::ok(), "Pi 就绪 — 内嵌 sidecar（Bundled Ready）"),
            PiProbe::DetectedLocal { .. } => (
                theme::accent_bright(),
                "Pi 就绪 — 本机安装（Detected Local）",
            ),
            PiProbe::NotReady => unreachable!(),
        }
    } else {
        (theme::warn(), "Pi 路径已找到但 RPC 尚未启动")
    }
}

// ── M5′ 辅助函数 ──────────────────────────────────────────────────────────

/// 截断显示文本到指定字符数（超出则追加 "…"）。
fn truncate_display(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_owned()
    } else {
        format!("{}…", chars[..max_chars].iter().collect::<String>())
    }
}

// ── Process hygiene: Drop ensures clean shutdown ──────────────────────────
//
// `PiRpcClient` implements Drop itself (close stdin → wait → kill), so
// simply dropping `PiLightApp` is sufficient.  No extra on_exit needed.
