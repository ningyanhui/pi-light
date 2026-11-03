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
    theme,
    workspace::{self, CwdSwitchPlan, DockAction, WorkspaceUi},
    workspace_git,
};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("pi-light")
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
    workdir_draft: String,
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
        let workdir_draft = workdir.to_string_lossy().into_owned();
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
            workdir_draft,
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
                self.workdir_draft = path.to_string_lossy().into_owned();
                let ok = self.spawn_rpc(true);
                self.workspace.switching = false;
                if ok {
                    self.commit_workdir_success(path);
                } else {
                    self.workdir = previous.clone();
                    self.workdir_draft = previous.to_string_lossy().into_owned();
                    let _ = self.spawn_rpc(false);
                    self.workspace.switch_error = Some(workspace::SWITCH_FAIL_COPY.to_owned());
                    self.workspace.retry_path = Some(path);
                }
            }
        }
    }

    fn commit_workdir_success(&mut self, path: PathBuf) {
        self.workdir = path.clone();
        self.workdir_draft = path.to_string_lossy().into_owned();
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
                self.workdir_draft = target_wd.to_string_lossy().into_owned();
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
        if matches!(surface, FirstRunSurface::Chat) {
            self.show_status_bar(ctx);
            self.show_workspace_dock(ctx);
            self.show_header(ctx);
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
            .exact_width(256.0)
            .frame(
                theme::glass_frame()
                    .inner_margin(egui::Margin::symmetric(12.0, 12.0))
                    .fill(theme::glass_strong()),
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
            DockAction::OpenCwdPopover => {
                self.workdir_draft = self.workdir.to_string_lossy().into_owned();
                self.workspace.cwd_popover = true;
            }
            DockAction::CloseCwdPopover => {
                self.workspace.cwd_popover = false;
                self.workdir_draft = self.workdir.to_string_lossy().into_owned();
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

    fn show_header(&mut self, ctx: &egui::Context) {
        let mut reprobe = false;
        let mut open_models = false;
        let mut cwd_action = None;
        let recent = workspace::recent_workdirs(&self.sessions, &self.workdir);
        egui::TopBottomPanel::top("header")
            .frame(
                theme::glass_bar()
                    .fill(theme::glass_strong())
                    .inner_margin(egui::Margin::symmetric(14.0, 8.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    cwd_action = workspace::show_topbar_cwd(
                        ui,
                        &self.workdir,
                        &mut self.workdir_draft,
                        self.workspace.switching,
                        self.workspace.cwd_popover,
                        &recent,
                        self.workspace
                            .switch_error
                            .as_deref()
                            .or(self.workdir_error.as_deref()),
                    );
                    ui.add_space(8.0);
                    if let Some(spec) = self.show_model_picker(ui, "header_model_combo", 188.0) {
                        self.apply_model_spec(&spec);
                    }
                    self.show_thinking_chip(ui);
                    if self.sessions.active_is_streaming() {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("生成中")
                                .size(11.0)
                                .color(theme::warn()),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::icon_button(ui, "👤", "账户与通用设置").clicked() {
                            open_models = true;
                        }
                        if theme::icon_button(ui, "↻", "重新扫描 Pi 并重连").clicked() {
                            reprobe = true;
                        }
                    });
                });
            });
        if let Some(action) = cwd_action {
            self.apply_dock_action(ctx, action);
        }
        if reprobe {
            self.reprobe();
        }
        if open_models {
            self.admin.open = true;
            self.admin.active_tab = AdminTab::General;
            self.admin.opened_pass = Some(ctx.cumulative_pass_nr());
        }
    }

    fn show_model_picker(
        &self,
        ui: &mut egui::Ui,
        id_salt: &'static str,
        width: f32,
    ) -> Option<String> {
        let current = provider_catalog::display_model_spec(
            self.admin.model_config.live_model.as_deref(),
            self.admin.model_config.default_provider.as_deref(),
            self.admin.model_config.default_model.as_deref(),
            &self.admin.model_config.available_models,
            &self.admin.model_config.custom_providers,
        );
        let models = provider_catalog::usable_model_specs(
            &self.admin.model_config.available_models,
            &self.admin.model_config.custom_providers,
        );
        show_model_chip(ui, id_salt, &current, &models, width)
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
        self.show_session_bar(ctx);

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
        let mut open_settings = false;
        let mut send_now = false;
        let mut abort_now = false;
        let mut retry_now = false;
        let mut apply_model: Option<String> = None;

        egui::TopBottomPanel::bottom("chat_input")
            .frame(
                theme::glass_bar()
                    .fill(theme::glass_strong())
                    .inner_margin(egui::Margin::symmetric(18.0, 10.0)),
            )
            .min_height(96.0)
            .show(ctx, |ui| {
                let mut chip_to_remove: Option<usize> = None;
                if !self.ref_chips.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for (i, chip) in self.ref_chips.iter().enumerate() {
                            let label = format!("# {}  ×", truncate_display(&chip.display, 40));
                            let btn = egui::Button::new(
                                egui::RichText::new(label)
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(184, 204, 255)),
                            )
                            .fill(egui::Color32::from_rgb(35, 51, 73))
                            .rounding(egui::Rounding::same(10.0));
                            if ui.add(btn).on_hover_text("点击移除此引用").clicked() {
                                chip_to_remove = Some(i);
                            }
                        }
                    });
                    ui.add_space(4.0);
                }
                if let Some(idx) = chip_to_remove {
                    self.ref_chips.remove(idx);
                }

                theme::emphasis_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let input = &mut self.sessions.active_chat_mut().input;
                        let can_send = !is_streaming
                            && !self.workspace.switching
                            && (!input.trim().is_empty() || !chips_empty);
                        let input_response = ui.add_sized(
                            [ui.available_width() - 168.0, 36.0],
                            egui::TextEdit::singleline(input)
                                .hint_text("输入消息，/ 命令，@ 文件，# 引用会话")
                                .frame(false)
                                .interactive(!is_streaming),
                        );
                        if input_response.lost_focus()
                            && ctx.input(|i| i.key_pressed(egui::Key::Enter))
                            && can_send
                            && !self.invoke.active
                        {
                            send_now = true;
                            input_response.request_focus();
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if theme::send_button(ui, can_send).clicked() {
                                send_now = true;
                                input_response.request_focus();
                            }
                            if let Some(spec) = show_model_chip(
                                ui,
                                "composer_model_combo",
                                &current_spec,
                                &usable_models,
                                120.0,
                            ) {
                                apply_model = Some(spec);
                            }
                        });
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let _ = theme::icon_button(ui, "📎", "附件：用 # 引用会话消息");
                        let _ = theme::icon_button(ui, "🌐", "网络与工具由 Skills / MCP 提供");
                        if theme::icon_button(ui, "⚙", "打开设置").clicked() {
                            open_settings = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if is_streaming {
                                if ui
                                    .add(egui::Button::new("■ 停止").min_size([60.0, 26.0].into()))
                                    .on_hover_text("中止当前生成（发送 abort 命令）")
                                    .clicked()
                                {
                                    abort_now = true;
                                }
                            } else if ui
                                .add_enabled(
                                    can_retry,
                                    egui::Button::new("重试").min_size([60.0, 26.0].into()),
                                )
                                .on_hover_text(
                                    "重新生成当前活动路径的最后一条助手回复（同会话分支）",
                                )
                                .clicked()
                            {
                                retry_now = true;
                            }
                        });
                    });
                });
                ui.add_space(6.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("pi-light 可能会犯错，请仔细辨别关键信息。")
                            .size(10.0)
                            .color(theme::text_dim()),
                    );
                });
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
        if open_settings {
            self.admin.open = true;
            self.admin.opened_pass = Some(ctx.cumulative_pass_nr());
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
                    .fill(egui::Color32::TRANSPARENT)
                    .inner_margin(16.0),
            )
            .show(ctx, |ui| {
                self.paint_main_backdrop(ui);
                if empty && !is_streaming {
                    self.show_chat_welcome(ui);
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
                                    if let Some(next) = show_assistant_message(ui, row, !streaming)
                                    {
                                        action = Some(next);
                                    }
                                }
                            }
                            ui.add_space(10.0);
                        }

                        if streaming {
                            if has_tools {
                                show_thinking_block(ui, self.sessions.active_chat());
                                ui.add_space(8.0);
                            }
                            show_streaming_assistant(ui, &streaming_buf);
                            ui.add_space(6.0);
                        }

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
        ui.horizontal(|ui| {
            let label = if has_branches {
                format!("消息树  {} 节点 · 有分支", rows.len())
            } else {
                format!("消息树  {} 节点", rows.len())
            };
            ui.strong(label);
            if ui
                .small_button(if self.tree_open { "收起" } else { "展开" })
                .clicked()
            {
                self.tree_open = !self.tree_open;
            }
        });
        ui.label(
            egui::RichText::new(BRANCH_HONESTY)
                .size(10.0)
                .color(theme::text_dim()),
        );
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

    fn show_session_bar(&mut self, ctx: &egui::Context) {
        let streaming = self.sessions.any_streaming();
        let switching = self.workspace.switching;
        let title = self.sessions.active().title.clone();
        let id = self.sessions.active().id;
        let mut want_new = false;
        let mut banner_action = None;

        egui::TopBottomPanel::top("session_bar")
            .frame(
                theme::glass_bar()
                    .fill(theme::glass_fill())
                    .inner_margin(egui::Margin::symmetric(16.0, 8.0)),
            )
            .show(ctx, |ui| {
                if let Some(err) = self.workspace.switch_error.clone() {
                    banner_action = workspace::show_switch_error_banner(ui, &err);
                    ui.add_space(4.0);
                }
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("#sess-{id}  {title}"))
                            .strong()
                            .size(14.0)
                            .color(theme::text_primary()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                !streaming && !switching,
                                egui::Button::new(
                                    egui::RichText::new("+ 新会话")
                                        .color(egui::Color32::WHITE)
                                        .size(12.0),
                                )
                                .fill(theme::accent())
                                .rounding(egui::Rounding::same(12.0)),
                            )
                            .clicked()
                        {
                            want_new = true;
                        }
                    });
                });
            });

        if want_new {
            self.do_new_session();
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
            format!("思考强度 {current}")
        } else {
            "思考强度 不可用".to_owned()
        };
        let resp = ui.add_enabled(
            usable,
            egui::Button::new(
                egui::RichText::new(label)
                    .size(11.0)
                    .color(theme::text_primary()),
            )
            .fill(egui::Color32::from_rgb(29, 38, 52))
            .stroke(egui::Stroke::new(1.0_f32, theme::accent_stroke_think()))
            .rounding(egui::Rounding::same(12.0)),
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

    fn show_chat_welcome(&self, ui: &mut egui::Ui) {
        ui.add_space(72.0);
        theme::centered_column(ui, 460.0, |ui| {
            theme::soft_card(ui, |ui| {
                ui.label(
                    egui::RichText::new("可以开始了")
                        .strong()
                        .size(22.0)
                        .color(theme::text_primary()),
                );
                ui.add_space(8.0);
                ui.label(egui::RichText::new("在下方输入消息。").size(14.0));
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("要改模型或查看密钥，用左下角的「设置」。")
                        .weak()
                        .size(13.0),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "输入 / 调用 Pi 命令，@ 引用工作区文件，# 引用会话消息。消息上可编辑 / 重新生成并切换同会话分支。",
                    )
                    .weak()
                    .size(13.0),
                );
            });
        });
    }

    fn show_status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("app_footer")
            .exact_height(32.0)
            .frame(
                theme::glass_bar()
                    .fill(theme::glass_strong())
                    .inner_margin(egui::Margin::symmetric(14.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let online = self.rpc_client.is_some() && self.pi_probe.is_ready();
                    let (dot_color, dot_tip) =
                        pi_dot_color(&self.pi_probe, self.rpc_client.is_some());
                    ui.colored_label(
                        if online { theme::ok() } else { dot_color },
                        if online { "● 在线" } else { "● 离线" },
                    )
                    .on_hover_text(dot_tip);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (tokens, trace, tps) =
                            self.sessions.active_chat().insights.footer_metric_parts();
                        ui.label(
                            egui::RichText::new(tps)
                                .size(11.0)
                                .color(theme::text_muted()),
                        );
                        ui.separator();
                        ui.label(
                            egui::RichText::new(format!("⚡ {trace}"))
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

    fn show_model_onboarding(&mut self, ctx: &egui::Context) {
        let spawn_note = self.spawn_error.clone();
        let mut dismiss = false;
        let mut focus_form = false;
        let mut form_result = provider_catalog::ModelsTabResult::default();
        let configured = self
            .admin
            .api_keys
            .rows
            .iter()
            .any(|row| !matches!(row.presence, pi_light::api_keys::KeyPresence::Absent));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::chrome_bg()))
            .show(ctx, |ui| {
                self.paint_main_backdrop(ui);
                let half = (ui.available_width() * 0.5).max(320.0);
                ui.columns(2, |cols| {
                    cols[0].vertical_centered(|ui| {
                        ui.set_min_width((half - 24.0).max(280.0));
                        ui.add_space(48.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("⚡").size(22.0).color(theme::accent()));
                            ui.label(
                                egui::RichText::new("PI-LIGHT")
                                    .strong()
                                    .size(13.0)
                                    .color(theme::text_muted()),
                            );
                        });
                        ui.label(
                            egui::RichText::new("欢迎使用 Pi-Light")
                                .strong()
                                .size(28.0)
                                .color(egui::Color32::WHITE),
                        );
                        ui.label(
                            egui::RichText::new("配置模型后，即可开始智能开发之旅")
                                .weak()
                                .size(14.0),
                        );
                        ui.add_space(20.0);
                        theme::soft_card(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("⚡")
                                        .size(16.0)
                                        .color(theme::accent()),
                                );
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("配置模型").strong().size(15.0),
                                    );
                                    ui.label(
                                        egui::RichText::new(
                                            "选择或添加 AI 模型提供商，开始使用 Pi-Light",
                                        )
                                        .weak()
                                        .size(12.0),
                                    );
                                });
                            });
                        });
                        ui.add_space(10.0);
                        theme::soft_card(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("▢").size(16.0).color(theme::text_muted()));
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(if configured {
                                            "已有可用提供商"
                                        } else {
                                            "暂无可用模型"
                                        })
                                        .strong()
                                        .size(14.0),
                                    );
                                    ui.label(
                                        egui::RichText::new(
                                            "添加模型提供商以开始对话与开发任务。密钥保存后只显示掩码。",
                                        )
                                        .weak()
                                        .size(12.0),
                                    );
                                });
                            });
                        });
                        if let Some(note) = &spawn_note {
                            ui.add_space(8.0);
                            ui.colored_label(
                                theme::warn(),
                                format!("程序已找到，但这次没有启动成功：{note}"),
                            );
                        }
                        ui.add_space(16.0);
                        if theme::primary_button(ui, "+  配置模型").clicked() {
                            focus_form = true;
                        }
                        ui.add_space(16.0);
                        ui.label(
                            egui::RichText::new("企业级安全 · 本地优先 · 完全掌控")
                                .weak()
                                .size(11.0),
                        );
                        ui.add_space(8.0);
                        if ui.small_button("进入对话").clicked() {
                            dismiss = true;
                        }
                    });

                    cols[1].add_space(36.0);
                    theme::soft_card(&mut cols[1], |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("⚡")
                                    .size(16.0)
                                    .color(theme::accent()),
                            );
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new("添加 Provider").strong().size(18.0),
                                );
                                ui.label(
                                    egui::RichText::new("选择第三方服务或自定义 API 接入")
                                        .weak()
                                        .size(12.0),
                                );
                            });
                        });
                        ui.add_space(10.0);
                        let auth_path = admin_panel::pi_agent_dir().map(|dir| dir.join("auth.json"));
                        let models_path =
                            admin_panel::pi_agent_dir().map(|dir| dir.join("models.json"));
                        form_result = provider_catalog::render_add_provider_inline(
                            ui,
                            &mut self.admin.add_provider,
                            &mut self.admin.api_keys,
                            auth_path.as_deref(),
                            models_path.as_deref(),
                            true,
                        );
                    });
                });
            });

        if focus_form {
            self.admin.add_provider.open_builtin(None);
        }
        if form_result.restart {
            self.restart_sidecar();
        }
        if let Some(err) = form_result.error {
            self.admin.last_error = Some(err);
        }
        if dismiss {
            self.admin.show_onboarding = false;
            self.primed_model_tab = false;
            self.admin.open = false;
            self.admin.opened_pass = None;
            self.admin.add_provider.close();
            if let Some(req) = self.admin.persist_inferred_default() {
                self.handle_set_model_request(req);
            }
        }
    }

    fn show_pi_missing(&mut self, ctx: &egui::Context) {
        let mut reprobe = false;
        let mut open_models = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            self.paint_main_backdrop(ui);
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.add_space(36.0);
                    theme::centered_column(ui, 560.0, |ui| {
                        theme::soft_card(ui, |ui| {
                            ui.label(
                                egui::RichText::new("还没有找到 Pi")
                                    .strong()
                                    .size(22.0)
                                    .color(theme::amber()),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(
                                    "平时只需要配置模型和 API 密钥。当前还没找到 Pi，所以暂时不能对话。",
                                )
                                .size(14.0),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(
                                    "官方 Windows 便携包已经把 Pi 放在程序旁边。请完整解压 pi-light-windows-x64.zip，直接运行 pi-light.exe，然后点「重新探测」。一般不用自己设环境变量或 PATH。",
                                )
                                .size(13.0),
                            );
                            ui.add_space(14.0);
                            ui.horizontal(|ui| {
                                if theme::primary_button(ui, "重新探测").clicked() {
                                    reprobe = true;
                                }
                                ui.add_space(8.0);
                                if ui.button("先填写模型配置").clicked() {
                                    open_models = true;
                                }
                            });
                            ui.add_space(16.0);
                            egui::CollapsingHeader::new("高级：自己指定 Pi 程序")
                                .default_open(false)
                                .id_salt("pi_recovery_center")
                                .show(ui, |ui| {
                                    admin_panel::show_pi_binary_recovery(ui);
                                });
                        });
                    });
                    ui.add_space(24.0);
                });
        });

        if open_models {
            self.admin.open = true;
            self.admin.active_tab = AdminTab::Models;
            self.admin.opened_pass = Some(ctx.cumulative_pass_nr());
        }
        if reprobe {
            self.reprobe();
        }
    }

    fn show_pi_disconnected(&mut self, ctx: &egui::Context) {
        let path = self
            .pi_probe
            .path()
            .map(|p| p.to_string_lossy().into_owned());
        let mut reprobe = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            self.paint_main_backdrop(ui);
            ui.add_space(72.0);
            theme::centered_column(ui, 480.0, |ui| {
                theme::soft_card(ui, |ui| {
                    ui.label(
                        egui::RichText::new("连接断开了")
                            .strong()
                            .size(20.0)
                            .color(theme::amber()),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Pi 已经找到，不用重新安装。再探测一次即可。")
                            .size(14.0),
                    );
                    if let Some(path) = &path {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(path).monospace().weak().size(11.0));
                    }
                    ui.add_space(14.0);
                    if theme::primary_button(ui, "重新探测").clicked() {
                        reprobe = true;
                    }
                });
            });
        });

        if reprobe {
            self.reprobe();
        }
    }

    fn show_spawn_error(&self, ctx: &egui::Context, error: &str) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.paint_main_backdrop(ui);
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.add_space(48.0);
                    theme::centered_column(ui, 520.0, |ui| {
                        theme::soft_card(ui, |ui| {
                            ui.label(
                                egui::RichText::new("Pi 没有启动")
                                    .color(egui::Color32::from_rgb(210, 120, 80))
                                    .strong()
                                    .size(20.0),
                            );
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new(error).size(13.0));
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(
                                    "确认程序可以运行后，点窗口底部的「重新探测」。",
                                )
                                .weak()
                                .size(13.0),
                            );
                            ui.add_space(12.0);
                            egui::CollapsingHeader::new("高级：换一个 Pi 程序")
                                .default_open(false)
                                .id_salt("pi_recovery_spawn")
                                .show(ui, |ui| {
                                    admin_panel::show_pi_binary_recovery(ui);
                                });
                        });
                    });
                });
        });
    }
}

fn show_model_chip(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    current: &str,
    models: &[String],
    width: f32,
) -> Option<String> {
    let mut selected = None;
    let chip = if current.is_empty() {
        "✦  模型 · 尚未选择".to_owned()
    } else {
        let short = current.rsplit('/').next().unwrap_or(current);
        format!("✦  模型 · {short}")
    };
    let mut choices: Vec<String> = models.to_vec();
    if !current.is_empty() && !choices.iter().any(|m| m == current) {
        choices.insert(0, current.to_owned());
    }
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(8.0, 2.0))
        .rounding(egui::Rounding::same(8.0))
        .fill(egui::Color32::from_rgb(28, 36, 48))
        .stroke(egui::Stroke::new(1.0_f32, theme::hairline()))
        .show(ui, |ui| {
            egui::ComboBox::from_id_salt(id_salt)
                .width(width)
                .selected_text(egui::RichText::new(chip).size(12.5))
                .show_ui(ui, |ui| {
                    if choices.is_empty() {
                        ui.weak("连上 Pi 后会列出可用模型");
                        return;
                    }
                    for model in &choices {
                        if ui.selectable_label(model == current, model).clicked() {
                            selected = Some(model.clone());
                        }
                    }
                });
        });
    selected.filter(|m| m != current)
}

// ── M3′ Trace / Token UI ──────────────────────────────────────────────────

fn show_thinking_block(ui: &mut egui::Ui, chat: &ChatState) {
    theme::think_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("✦  思考过程")
                    .strong()
                    .size(13.0)
                    .color(theme::accent_bright()),
            );
        });
        ui.add_space(6.0);
        show_tool_cards(ui, chat);
    });
}

fn show_tool_cards(ui: &mut egui::Ui, chat: &ChatState) {
    for step in &chat.insights.trace_steps {
        show_tool_card(ui, step);
        ui.add_space(4.0);
    }
}

fn show_tool_card(ui: &mut egui::Ui, step: &pi_light::pi_rpc::TraceStep) {
    let (status, color) = if step.running {
        ("运行中", theme::warn())
    } else if step.is_error {
        ("失败", theme::danger())
    } else {
        ("完成", theme::ok())
    };
    let dur = step
        .duration_ms
        .map(|ms| format!("{:.1}s", ms as f64 / 1000.0))
        .unwrap_or_else(|| "—".to_owned());
    theme::card_frame().show(ui, |ui| {
        ui.set_max_width(ui.available_width());
        ui.horizontal(|ui| {
            theme::glyph_circle(
                ui,
                theme::first_letter(&step.tool_name),
                theme::package_glyph_color(&step.tool_name),
            );
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&step.tool_name)
                            .strong()
                            .monospace()
                            .size(12.5)
                            .color(theme::text_primary()),
                    );
                    ui.label(
                        egui::RichText::new(" TOOL ")
                            .size(10.0)
                            .color(egui::Color32::WHITE)
                            .background_color(theme::accent()),
                    );
                });
                ui.label(
                    egui::RichText::new(step.args_preview.as_deref().unwrap_or(status))
                        .size(11.0)
                        .color(theme::text_muted()),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(dur)
                        .size(11.0)
                        .color(theme::text_muted()),
                );
                ui.colored_label(color, status);
            });
        });
    });
}

// ── Chat message renderers ────────────────────────────────────────────────

enum ChatUiAction {
    Prev(NodeId),
    Next(NodeId),
    EditStart(NodeId),
    EditConfirm,
    EditCancel,
    Regen(NodeId),
    Navigate(NodeId),
}

fn show_branch_nav(
    ui: &mut egui::Ui,
    id: NodeId,
    sibling_index: usize,
    sibling_count: usize,
    enabled: bool,
) -> Option<ChatUiAction> {
    if sibling_count <= 1 {
        return None;
    }
    let mut action = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                enabled,
                egui::Button::new("‹").min_size([22.0, 20.0].into()),
            )
            .on_hover_text("上一条兄弟分支")
            .clicked()
        {
            action = Some(ChatUiAction::Prev(id));
        }
        ui.label(
            egui::RichText::new(format!("{}/{}", sibling_index + 1, sibling_count))
                .size(11.0)
                .color(theme::text_muted()),
        );
        if ui
            .add_enabled(
                enabled,
                egui::Button::new("›").min_size([22.0, 20.0].into()),
            )
            .on_hover_text("下一条兄弟分支")
            .clicked()
        {
            action = Some(ChatUiAction::Next(id));
        }
    });
    action
}

fn show_user_message(
    ui: &mut egui::Ui,
    row: &pi_light::message_tree::PathRow,
    editing: bool,
    draft: &mut String,
    enabled: bool,
) -> Option<ChatUiAction> {
    let mut action = None;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(14.0, 10.0))
            .rounding(egui::Rounding::same(12.0))
            .fill(egui::Color32::from_rgb(36, 52, 75))
            .stroke(egui::Stroke::new(1.0_f32, theme::glass_stroke()))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width() * 0.72);
                if editing {
                    ui.add(
                        egui::TextEdit::multiline(draft)
                            .desired_width(ui.available_width())
                            .desired_rows(3),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                enabled && !draft.trim().is_empty(),
                                egui::Button::new("发送分支"),
                            )
                            .clicked()
                        {
                            action = Some(ChatUiAction::EditConfirm);
                        }
                        if ui.button("取消").clicked() {
                            action = Some(ChatUiAction::EditCancel);
                        }
                    });
                } else {
                    ui.label(
                        egui::RichText::new(&row.text)
                            .color(theme::text_primary())
                            .size(14.0),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if let Some(nav) = show_branch_nav(
                            ui,
                            row.id,
                            row.sibling_index,
                            row.sibling_count,
                            enabled,
                        ) {
                            action = Some(nav);
                        }
                        if ui
                            .add_enabled(
                                enabled,
                                egui::Button::new("编辑").min_size([40.0, 20.0].into()),
                            )
                            .on_hover_text("改写这条用户消息并分出同会话分支")
                            .clicked()
                        {
                            action = Some(ChatUiAction::EditStart(row.id));
                        }
                    });
                }
            });
    });
    action
}

fn show_assistant_message(
    ui: &mut egui::Ui,
    row: &pi_light::message_tree::PathRow,
    enabled: bool,
) -> Option<ChatUiAction> {
    let mut action = None;
    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(4.0, 6.0))
            .fill(egui::Color32::TRANSPARENT)
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width() * 0.92);
                markdown_ui::show_markdown(
                    ui,
                    &row.text,
                    MarkdownView {
                        streaming: false,
                        show_role: true,
                    },
                );
                if !row.text.is_empty() {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if let Some(nav) = show_branch_nav(
                            ui,
                            row.id,
                            row.sibling_index,
                            row.sibling_count,
                            enabled,
                        ) {
                            action = Some(nav);
                        }
                        if theme::icon_button(ui, "⧉", "复制回复").clicked() {
                            ui.ctx().copy_text(row.text.clone());
                        }
                        let _ = theme::icon_button(ui, "▲", "有帮助");
                        let _ = theme::icon_button(ui, "▼", "没帮助");
                        if theme::icon_button(ui, "↻", "重新生成").clicked() && enabled {
                            action = Some(ChatUiAction::Regen(row.id));
                        }
                    });
                }
            });
    });
    action
}

fn show_streaming_assistant(ui: &mut egui::Ui, text: &str) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(4.0, 6.0))
            .fill(egui::Color32::TRANSPARENT)
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width() * 0.92);
                markdown_ui::show_markdown(
                    ui,
                    text,
                    MarkdownView {
                        streaming: true,
                        show_role: true,
                    },
                );
            });
    });
}

// ── Pi status dot helpers ─────────────────────────────────────────────────

fn pi_dot_color(probe: &PiProbe, rpc_alive: bool) -> (egui::Color32, &'static str) {
    if !probe.is_ready() {
        return (
            egui::Color32::from_rgb(200, 90, 70),
            "Pi 未就绪。恢复说明在主界面折叠区和设置「通用」页。",
        );
    }
    if rpc_alive {
        match probe {
            PiProbe::BundledReady(_) => (
                egui::Color32::from_rgb(80, 200, 100),
                "Pi 就绪 — 内嵌 sidecar（Bundled Ready）",
            ),
            PiProbe::DetectedLocal { .. } => (
                egui::Color32::from_rgb(80, 160, 230),
                "Pi 就绪 — 本机安装（Detected Local）",
            ),
            PiProbe::NotReady => unreachable!(),
        }
    } else {
        (
            egui::Color32::from_rgb(210, 150, 50),
            "Pi 路径已找到但 RPC 尚未启动",
        )
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
