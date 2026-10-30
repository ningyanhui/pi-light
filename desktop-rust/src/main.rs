// SPDX-License-Identifier: Apache-2.0
// pi-light — 内嵌 Pi 的轻量 Rust Shell
// M2′ 聊天 + M3′ Trace/Token + M4′+ 设置面 + M5′ # 会话引用 + 多会话/cwd/重试

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use eframe::egui;
use pi_light::{
    admin_panel::{self, write_settings_model, AdminPanelState, AdminTab, SetModelRequest},
    chat::{ChatMessage, ChatState, SessionHub},
    fonts,
    onboarding::{self, FirstRunSurface},
    pi_path::{self, PiProbe},
    pi_rpc::{self, PiRpcClient, PiRpcEvent},
    provider_catalog,
    remote_access::RemoteAccessState,
    session_insights::{StatsFetchState, TraceDisplayState, UsageDisplayState},
    session_ref::{self, RefCandidate, RefChip, RefChooserState},
    session_store::{self, RpcAttach},
};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("pi-light")
            .with_inner_size([920.0, 640.0])
            .with_min_inner_size([520.0, 360.0]),
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
    /// 用户已从首次模型引导进入对话。
    onboarding_dismissed: bool,
    /// 引导期间只把设置切到「模型」一次。
    primed_model_tab: bool,
    /// M5′ `#` 引用选择器状态。
    ref_chooser: RefChooserState,
    /// M5′ 已确认待附带发送的引用 chips。
    ref_chips: Vec<RefChip>,
    /// M5′ 候选缓存（在 # 触发时构建，每次触发刷新）。
    ref_candidates_cache: Vec<RefCandidate>,
    /// Pi sidecar 工作目录（写入 `Command::current_dir`）。
    workdir: PathBuf,
    workdir_draft: String,
    workdir_error: Option<String>,
    workdir_editing: bool,
    /// 壳侧会话文件。None 表示没有可写的用户目录。
    state_path: Option<PathBuf>,
    persist_dirty: bool,
    persist_retry_at: Option<std::time::Instant>,
    /// v3 M5：远程访问（配对 / Cloudflare Tunnel / 设备列表）。
    remote: RemoteAccessState,
}

impl PiLightApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        fonts::install_system_cjk(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx);
        let probe = pi_path::probe();
        let egui_ctx = cc.egui_ctx.clone();
        let fallback_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let state_path = session_store::default_path();
        let (sessions, workdir) = match state_path
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
        let workdir_draft = workdir.to_string_lossy().into_owned();
        let onboarding_dismissed = cc
            .storage
            .and_then(|storage| storage.get_string(onboarding::ONBOARDING_DISMISSED_KEY))
            .as_deref()
            == Some("1");
        let mut app = Self {
            pi_probe: probe,
            rpc_client: None,
            sessions,
            spawn_error: None,
            egui_ctx,
            admin: AdminPanelState::default(),
            onboarding_dismissed,
            primed_model_tab: false,
            ref_chooser: RefChooserState::default(),
            ref_chips: Vec::new(),
            ref_candidates_cache: Vec::new(),
            workdir,
            workdir_draft,
            workdir_error: None,
            workdir_editing: false,
            state_path,
            persist_dirty: false,
            persist_retry_at: None,
            remote: RemoteAccessState::load_default(),
        };
        app.try_spawn_rpc();
        app
    }

    fn try_spawn_rpc(&mut self) {
        self.rpc_client = None;
        self.spawn_error = None;
        self.ref_chooser.close();
        self.ref_chips.clear();

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
                    self.rpc_client = Some(client);
                    if detached {
                        self.sessions.active_chat_mut().set_error(
                            "已恢复本机会话记录。Pi 侧会话文件不在，模型上下文从新会话开始。"
                                .to_owned(),
                        );
                    }
                }
                Err(e) => {
                    self.spawn_error = Some(format!("启动 Pi 失败：{e}"));
                }
            }
        }
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

    fn apply_workdir(&mut self) {
        match pi_rpc::validate_cwd(&self.workdir_draft) {
            Ok(path) => {
                self.workdir = path;
                self.workdir_draft = self.workdir.to_string_lossy().into_owned();
                self.workdir_error = None;
                self.workdir_editing = false;
                self.touch_sessions();
                self.persist_sessions(true);
                self.try_spawn_rpc();
            }
            Err(e) => {
                self.workdir_error = Some(e);
            }
        }
    }

    /// 发送当前输入（含 chips 引用上下文）给 Pi。
    fn do_send(&mut self) {
        let user_text = self.sessions.active_chat().input.trim().to_owned();
        if user_text.is_empty() && self.ref_chips.is_empty() {
            return;
        }
        if self.rpc_client.is_none() {
            return;
        }
        self.touch_sessions();
        if let Some(client) = &mut self.rpc_client {
            let chips = std::mem::take(&mut self.ref_chips);
            let effective_prompt = session_ref::build_prompt_text(&chips, &user_text);

            let display_text = if chips.is_empty() {
                user_text.clone()
            } else {
                let labels: Vec<&str> = chips.iter().map(|c| c.display.as_str()).collect();
                format!("[引用: {}]\n{}", labels.join(", "), user_text)
            };

            self.sessions.set_active_title_from_user(&user_text);
            {
                let chat = self.sessions.active_chat_mut();
                chat.push_user(display_text);
                chat.last_sent_prompt = Some(effective_prompt.clone());
                chat.input.clear();
                chat.begin_streaming();
            }
            self.ref_chooser.close();
            if let Err(e) = client.send_prompt(&effective_prompt) {
                self.sessions
                    .active_chat_mut()
                    .set_error(format!("发送失败：{e}"));
            }
        }
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
        let prompt = match self.sessions.active_chat().last_sent_prompt.clone() {
            Some(p) => p,
            None => return,
        };
        if self.rpc_client.is_none() {
            return;
        }
        self.touch_sessions();
        if let Some(client) = &mut self.rpc_client {
            let chat = self.sessions.active_chat_mut();
            chat.drop_trailing_assistant();
            chat.begin_streaming();
            if let Err(e) = client.send_prompt(&prompt) {
                chat.set_error(format!("重试失败：{e}"));
            }
        }
    }

    fn do_new_session(&mut self) {
        if self.sessions.any_streaming() {
            return;
        }
        self.ref_chooser.close();
        self.ref_chips.clear();
        self.sessions.create_new();
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
        if self.sessions.any_streaming() {
            return;
        }
        let file = self
            .sessions
            .list()
            .get(index)
            .and_then(|s| s.session_file.clone());
        if !self.sessions.switch_to(index) {
            return;
        }
        self.touch_sessions();
        self.ref_chooser.close();
        self.ref_chips.clear();
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

    fn drain_rpc_events(&mut self) {
        if let Some(client) = &mut self.rpc_client {
            let events = client.drain_events();
            let had_events = !events.is_empty();
            for event in events {
                let is_exit = matches!(&event, PiRpcEvent::ProcessExited { .. });
                let request_stats = matches!(&event, PiRpcEvent::AgentSettled { aborted: false });

                match &event {
                    PiRpcEvent::StateInfo {
                        model,
                        session_file,
                        session_id,
                        ..
                    } => {
                        self.admin.update_live_model(model.clone(), Vec::new());
                        self.sessions
                            .set_active_session_file(session_file.clone(), session_id.clone());
                    }
                    PiRpcEvent::AvailableModels(models) => {
                        self.admin.update_live_model(None, models.clone());
                    }
                    PiRpcEvent::ModelChanged(model) => {
                        self.admin
                            .update_live_model(Some(model.clone()), Vec::new());
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
    }

    fn touch_sessions(&mut self) {
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

    /// M5′: 根据当前输入检测 `#` 触发词，刷新选择器候选。
    /// 每帧调用一次（非常廉价：仅字符串扫描 + 列表过滤）。
    fn update_ref_chooser(&mut self) {
        let trigger = session_ref::find_hash_trigger(&self.sessions.active_chat().input);
        let was_active = self.ref_chooser.active;

        match trigger {
            Some(hash_pos) => {
                // 刚刚激活：刷新候选缓存（含当前会话消息 + Pi 本地会话文件）
                if !was_active {
                    let mut cands = session_ref::candidates_from_messages(
                        &self.sessions.active_chat().messages,
                    );
                    cands.extend(session_ref::candidates_from_pi_sessions());
                    self.ref_candidates_cache = cands;
                }
                let query =
                    session_ref::extract_query(&self.sessions.active_chat().input, hash_pos)
                        .to_owned();
                // 分开借用：先取 hash_pos，再调用方法
                let all = self.ref_candidates_cache.as_slice();
                self.ref_chooser.activate(all, &query, hash_pos);
            }
            None => {
                if was_active {
                    self.ref_chooser.close();
                }
            }
        }
    }
}

// ── eframe::App ───────────────────────────────────────────────────────────

impl eframe::App for PiLightApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.onboarding_dismissed {
            storage.set_string(onboarding::ONBOARDING_DISMISSED_KEY, "1".to_owned());
        }
        self.persist_sessions(true);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_rpc_events();
        self.update_ref_chooser();
        if self.remote.poll() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        // M5′: 键盘导航（在面板渲染前消费按键，避免被 TextEdit 截获）
        if self.ref_chooser.active {
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
                self.ref_chooser.move_up();
            }
            if down {
                self.ref_chooser.move_down();
            }
            if confirmed {
                let hash_pos = self.ref_chooser.hash_byte_offset;
                if let Some(chip) = self.ref_chooser.confirm() {
                    self.ref_chips.push(chip);
                    session_ref::remove_hash_trigger(
                        &mut self.sessions.active_chat_mut().input,
                        hash_pos,
                    );
                }
            } else if closed {
                self.ref_chooser.close();
            }
        }

        self.show_footer(ctx);

        // ── Header ────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("⚡").size(22.0).color(amber()));
                ui.heading("pi-light");
                ui.weak("轻量桌面助手");

                let (dot_color, dot_tip) = pi_dot_color(&self.pi_probe, self.rpc_client.is_some());
                ui.colored_label(dot_color, "●").on_hover_text(dot_tip);
                if self.sessions.active_is_streaming() {
                    ui.weak("生成中");
                }
                ui.separator();
                let current = provider_catalog::current_model_spec(
                    self.admin.model_config.live_model.as_deref(),
                    self.admin.model_config.default_provider.as_deref(),
                    self.admin.model_config.default_model.as_deref(),
                );
                let models = self.admin.model_config.available_models.clone();
                if let Some(spec) = provider_catalog::show_model_combo(
                    ui,
                    "header_model_combo",
                    &current,
                    &models,
                    220.0,
                ) {
                    if let Some((provider, model_id)) = provider_catalog::parse_model_spec(&spec) {
                        if let Some(path) = self.admin.model_config.settings_path.clone() {
                            if write_settings_model(&path, &provider, &model_id).is_ok() {
                                self.admin.model_config.default_provider = Some(provider.clone());
                                self.admin.model_config.default_model = Some(model_id.clone());
                            }
                        }
                        self.handle_set_model_request(SetModelRequest { provider, model_id });
                    }
                }
                ui.separator();
                let folder = self
                    .workdir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(".");
                ui.weak(folder)
                    .on_hover_text(self.workdir.to_string_lossy());
            });
        });

        // ── 主路径：模型引导 / 缺程序说明 / 对话 ─────────────────────────
        let surface =
            onboarding::first_run_surface(self.pi_probe.is_ready(), self.onboarding_dismissed);
        if matches!(surface, FirstRunSurface::ModelSetup) && !self.primed_model_tab {
            self.admin.open = true;
            self.admin.active_tab = AdminTab::Models;
            self.admin.opened_pass = Some(ctx.cumulative_pass_nr());
            self.primed_model_tab = true;
        }

        let panel_ret = self.admin.show_panel(
            ctx,
            None,
            self.rpc_client.is_some(),
            self.pi_probe.is_ready(),
            self.pi_probe.path(),
            &mut self.remote,
        );
        if let Some(req) = panel_ret.set_model_request {
            self.handle_set_model_request(req);
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

    fn show_session_sidebar(&mut self, ctx: &egui::Context) {
        let streaming = self.sessions.any_streaming();
        let active = self.sessions.active_index();
        let mut switch_to: Option<usize> = None;
        let mut want_new = false;
        let mut apply_cwd = false;

        egui::SidePanel::left("session_list")
            .resizable(true)
            .default_width(176.0)
            .min_width(132.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("会话").strong().size(13.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = ui.add_enabled(!streaming, egui::Button::new("＋"));
                        if btn.on_hover_text("新建会话").clicked() {
                            want_new = true;
                        }
                    });
                });
                ui.label(
                    egui::RichText::new("关闭后保留，下次启动恢复")
                        .weak()
                        .size(10.0),
                );
                ui.separator();

                let list_height = (ui.available_height() - 96.0).max(48.0);
                egui::ScrollArea::vertical()
                    .id_salt("session_list_scroll")
                    .max_height(list_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (i, session) in self.sessions.list().iter().enumerate() {
                            let selected = i == active;
                            let resp = ui.add_enabled(
                                !streaming || selected,
                                egui::SelectableLabel::new(selected, &session.title),
                            );
                            if resp.clicked() {
                                switch_to = Some(i);
                            }
                        }
                    });

                ui.add_space(8.0);
                ui.separator();
                ui.label(egui::RichText::new("工作目录").strong().size(12.0));
                if self.workdir_editing {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.workdir_draft)
                            .desired_width(f32::INFINITY)
                            .hint_text("绝对路径"),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("应用").clicked() {
                            apply_cwd = true;
                        }
                        if ui.button("取消").clicked() {
                            self.workdir_draft = self.workdir.to_string_lossy().into_owned();
                            self.workdir_editing = false;
                            self.workdir_error = None;
                        }
                    });
                } else {
                    let path_label = ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(self.workdir.to_string_lossy().as_ref())
                                    .monospace()
                                    .size(10.0),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_text("点击以更改工作目录（将重启 Pi）");
                    if path_label.clicked() {
                        self.workdir_editing = true;
                    }
                }
                if let Some(err) = &self.workdir_error {
                    ui.colored_label(egui::Color32::from_rgb(210, 80, 60), err);
                }
            });

        if want_new {
            self.do_new_session();
        }
        if let Some(i) = switch_to {
            self.do_switch_session(i);
        }
        if apply_cwd {
            self.apply_workdir();
        }
    }

    fn show_chat_ui(&mut self, ctx: &egui::Context) {
        self.show_session_sidebar(ctx);

        // Copy flags before any mutable borrow of active_chat().input.
        let is_streaming = self.sessions.active_is_streaming();
        let can_retry = self.sessions.active_chat().can_retry();
        let chips_empty = self.ref_chips.is_empty();

        egui::TopBottomPanel::bottom("insights_bar")
            .min_height(28.0)
            .show(ctx, |ui| {
                show_session_insights_bar(ui, self.sessions.active_chat());
            });

        egui::TopBottomPanel::bottom("chat_input")
            .min_height(56.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);

                let mut chip_to_remove: Option<usize> = None;
                if !self.ref_chips.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for (i, chip) in self.ref_chips.iter().enumerate() {
                            let label = format!("# {}  ×", truncate_display(&chip.display, 40));
                            let btn = egui::Button::new(
                                egui::RichText::new(label)
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(180, 220, 255)),
                            )
                            .fill(egui::Color32::from_rgb(30, 60, 100))
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

                ui.horizontal(|ui| {
                    let input = &mut self.sessions.active_chat_mut().input;
                    let can_send = !is_streaming && (!input.trim().is_empty() || !chips_empty);

                    let input_response = ui.add_sized(
                        [ui.available_width() - 140.0, 32.0],
                        egui::TextEdit::singleline(input)
                            .hint_text("输入消息，按 Enter 发送；输入 # 引用会话…")
                            .interactive(!is_streaming),
                    );

                    if input_response.lost_focus()
                        && ctx.input(|i| i.key_pressed(egui::Key::Enter))
                        && can_send
                        && !self.ref_chooser.active
                    {
                        self.do_send();
                        input_response.request_focus();
                    }

                    if ui
                        .add_enabled(
                            can_send,
                            egui::Button::new("发送").min_size([60.0, 32.0].into()),
                        )
                        .clicked()
                    {
                        self.do_send();
                        input_response.request_focus();
                    }

                    if is_streaming {
                        let stop_btn = ui.add_enabled(
                            true,
                            egui::Button::new("■ 停止").min_size([60.0, 32.0].into()),
                        );
                        if stop_btn.clicked() {
                            self.do_abort();
                        }
                        stop_btn.on_hover_text("中止当前生成（发送 abort 命令）");
                    } else {
                        let retry_btn = ui.add_enabled(
                            can_retry,
                            egui::Button::new("重试").min_size([60.0, 32.0].into()),
                        );
                        if retry_btn.clicked() {
                            self.do_retry();
                        }
                        retry_btn.on_hover_text("重试上一条已发送的 prompt");
                    }
                });
                ui.add_space(4.0);
            });

        if self.ref_chooser.active {
            self.show_ref_chooser_popup(ctx);
        }

        let is_streaming = self.sessions.active_is_streaming();
        let empty = self.sessions.active_chat().messages.is_empty();

        egui::CentralPanel::default().show(ctx, |ui| {
            paint_lightning_backdrop(ui);
            if empty && !is_streaming {
                self.show_chat_welcome(ui);
                return;
            }

            if let Some(err) = self.sessions.active_chat().error.clone() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(210, 80, 60), format!("⚠ {err}"));
                    if ui.small_button("关闭").clicked() {
                        self.sessions.active_chat_mut().error = None;
                    }
                });
                ui.separator();
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
                    let has_tools = !self.sessions.active_chat().insights.trace_steps.is_empty();
                    for msg in &self.sessions.active_chat().messages {
                        match msg {
                            ChatMessage::User(text) => {
                                show_user_message(ui, text);
                            }
                            ChatMessage::Assistant(text) => {
                                show_assistant_message(ui, text, false);
                            }
                        }
                        ui.add_space(6.0);
                    }

                    if streaming {
                        if has_tools {
                            show_tool_cards(ui, self.sessions.active_chat());
                            ui.add_space(6.0);
                        }
                        show_assistant_message(ui, &streaming_buf, true);
                        ui.add_space(6.0);
                    }

                    if scroll_to_bottom {
                        ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                    }

                    show_trace_section(ui, self.sessions.active_chat());
                });
        });
    }

    /// M5′: 渲染引用选择器浮动面板（定位在屏幕底部输入区上方）。
    fn show_ref_chooser_popup(&mut self, ctx: &egui::Context) {
        let screen_rect = ctx.screen_rect();
        // 估算 chips 行高（有 chips 时加一行）
        let chips_extra = if self.ref_chips.is_empty() { 0.0 } else { 36.0 };
        // 输入面板高度 ≈ 70px（含内边距），insights_bar ≈ 30px
        let bottom_offset = 70.0 + 30.0 + chips_extra + 8.0;
        let popup_height = 180.0f32;
        let popup_width = (screen_rect.width() - 160.0).clamp(280.0, 600.0);
        let pos = egui::pos2(16.0, screen_rect.bottom() - bottom_offset - popup_height);

        // 收集鼠标点击事件（避免在 Area show 闭包中同时借用 self.ref_chooser 和 self.ref_chips）
        let mut click_confirm_idx: Option<usize> = None;

        egui::Area::new(egui::Id::new("ref_chooser_popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(popup_width);
                    ui.set_max_width(popup_width);

                    // 标题栏
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("# 引用会话消息").strong().size(12.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new("↑↓ 选择  Enter/Tab 确认  Esc 关闭")
                                    .weak()
                                    .size(10.0),
                            );
                        });
                    });
                    ui.separator();

                    if self.ref_chooser.filtered.is_empty() {
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("暂无引用 — 本会话尚无可引用的消息")
                                .weak()
                                .italics()
                                .size(12.0),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("提示：开始对话后，历史消息将出现在此列表")
                                .weak()
                                .size(11.0),
                        );
                        ui.add_space(6.0);
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(popup_height - 55.0)
                            .show(ui, |ui| {
                                let cursor = self.ref_chooser.cursor;
                                for (i, candidate) in self.ref_chooser.filtered.iter().enumerate() {
                                    let is_selected = i == cursor;
                                    let text = egui::RichText::new(&candidate.display).size(12.5);
                                    let text = if is_selected { text.strong() } else { text };
                                    let resp = ui.selectable_label(is_selected, text);
                                    if resp.clicked() {
                                        click_confirm_idx = Some(i);
                                    }
                                    if is_selected {
                                        resp.scroll_to_me(None);
                                    }
                                }
                            });
                    }
                });
            });

        // 处理鼠标点击确认
        if let Some(idx) = click_confirm_idx {
            self.ref_chooser.cursor = idx;
            let hash_pos = self.ref_chooser.hash_byte_offset;
            if let Some(chip) = self.ref_chooser.confirm() {
                self.ref_chips.push(chip);
                session_ref::remove_hash_trigger(
                    &mut self.sessions.active_chat_mut().input,
                    hash_pos,
                );
            }
        }
    }

    fn show_chat_welcome(&self, ui: &mut egui::Ui) {
        ui.add_space(72.0);
        centered_column(ui, 460.0, |ui| {
            soft_card(ui, |ui| {
                ui.label(
                    egui::RichText::new("可以开始了")
                        .strong()
                        .size(22.0)
                        .color(amber()),
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
                    egui::RichText::new("输入 # 可引用本会话或历史消息。")
                        .weak()
                        .size(13.0),
                );
            });
        });
    }

    fn show_footer(&mut self, ctx: &egui::Context) {
        let mut toggle_settings = false;
        let mut reprobe = false;
        egui::TopBottomPanel::bottom("app_footer")
            .exact_height(40.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    let gear = egui::Button::new(egui::RichText::new("⚙  设置").color(
                        if self.admin.open {
                            ink()
                        } else {
                            egui::Color32::from_rgb(230, 232, 238)
                        },
                    ))
                    .fill(if self.admin.open {
                        amber()
                    } else {
                        egui::Color32::from_rgb(36, 40, 54)
                    })
                    .min_size(egui::vec2(96.0, 28.0));
                    if ui
                        .add(gear)
                        .on_hover_text(
                            "设置抽屉：模型 / Skills / MCP / 扩展市场 / 记忆 / 远程访问 / 通用",
                        )
                        .clicked()
                    {
                        toggle_settings = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("重新探测")
                            .on_hover_text("重新扫描 Pi 程序并重连")
                            .clicked()
                        {
                            reprobe = true;
                        }
                    });
                });
            });
        if toggle_settings {
            self.admin.open = !self.admin.open;
            if self.admin.open {
                self.admin.opened_pass = Some(ctx.cumulative_pass_nr());
            } else {
                self.admin.opened_pass = None;
            }
        }
        if reprobe {
            self.reprobe();
        }
    }

    fn show_model_onboarding(&mut self, ctx: &egui::Context) {
        let spawn_note = self.spawn_error.clone();
        let mut dismiss = false;
        let mut open_models = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            paint_lightning_backdrop(ui);
            ui.add_space(48.0);
            centered_column(ui, 480.0, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("⚡").size(28.0).color(amber()));
                    ui.heading("pi-light");
                });
                ui.label(
                    egui::RichText::new("欢迎使用 pi-light")
                        .strong()
                        .size(26.0)
                        .color(egui::Color32::WHITE),
                );
                ui.label(
                    egui::RichText::new("配置模型后即可开始对话")
                        .weak()
                        .size(14.0),
                );
                ui.add_space(16.0);
                soft_card(ui, |ui| {
                    ui.label(
                        egui::RichText::new("配置模型")
                            .strong()
                            .size(16.0)
                            .color(amber()),
                    );
                    ui.label(
                        egui::RichText::new("选择或添加模型提供商，填入 API 密钥后即可使用。")
                            .size(13.0),
                    );
                    ui.add_space(10.0);
                    egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
                        .rounding(egui::Rounding::same(8.0))
                        .stroke(egui::Stroke::new(
                            1.0_f32,
                            egui::Color32::from_rgb(60, 64, 80),
                        ))
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("暂无可用模型").strong().size(14.0));
                            ui.label(
                                egui::RichText::new(
                                    "添加模型提供商以开始对话。密钥保存后只显示掩码。",
                                )
                                .weak()
                                .size(12.0),
                            );
                        });
                    if let Some(note) = &spawn_note {
                        ui.add_space(8.0);
                        ui.colored_label(
                            egui::Color32::from_rgb(210, 150, 70),
                            format!("程序已找到，但这次没有启动成功：{note}"),
                        );
                    }
                    ui.add_space(14.0);
                    if primary_button(ui, "+ 配置模型").clicked() {
                        open_models = true;
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("密钥仅写入本机，不会上传到云端。")
                                .weak()
                                .size(11.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("进入对话").clicked() {
                                dismiss = true;
                            }
                        });
                    });
                });
            });
        });

        if open_models {
            self.admin.open = true;
            self.admin.active_tab = AdminTab::Models;
            self.admin.opened_pass = Some(ctx.cumulative_pass_nr());
            self.admin.add_provider.open_builtin(None);
        }
        if dismiss {
            self.onboarding_dismissed = true;
            self.admin.open = false;
            self.admin.opened_pass = None;
            self.admin.add_provider.close();
        }
    }

    fn show_pi_missing(&mut self, ctx: &egui::Context) {
        let mut reprobe = false;
        let mut open_models = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            paint_lightning_backdrop(ui);
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.add_space(36.0);
                    centered_column(ui, 560.0, |ui| {
                        soft_card(ui, |ui| {
                            ui.label(
                                egui::RichText::new("还没有找到 Pi")
                                    .strong()
                                    .size(22.0)
                                    .color(amber()),
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
                                if primary_button(ui, "重新探测").clicked() {
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
            paint_lightning_backdrop(ui);
            ui.add_space(72.0);
            centered_column(ui, 480.0, |ui| {
                soft_card(ui, |ui| {
                    ui.label(
                        egui::RichText::new("连接断开了")
                            .strong()
                            .size(20.0)
                            .color(amber()),
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
                    if primary_button(ui, "重新探测").clicked() {
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
            paint_lightning_backdrop(ui);
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.add_space(48.0);
                    centered_column(ui, 520.0, |ui| {
                        soft_card(ui, |ui| {
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

// ── 外观：深色底 + 自绘闪电（不引用外部图片）────────────────────────────

fn amber() -> egui::Color32 {
    egui::Color32::from_rgb(244, 198, 96)
}

fn ink() -> egui::Color32 {
    egui::Color32::from_rgb(28, 24, 16)
}

fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = egui::Color32::from_rgb(17, 19, 27);
    visuals.panel_fill = egui::Color32::from_rgb(22, 25, 34);
    visuals.extreme_bg_color = egui::Color32::from_rgb(30, 34, 46);
    visuals.faint_bg_color = egui::Color32::from_rgb(36, 40, 54);
    visuals.code_bg_color = egui::Color32::from_rgb(28, 32, 44);
    visuals.hyperlink_color = amber();
    visuals.selection.bg_fill = egui::Color32::from_rgb(72, 58, 28);
    visuals.selection.stroke.color = amber();
    visuals.window_rounding = egui::Rounding::same(8.0);
    visuals.menu_rounding = egui::Rounding::same(6.0);
    for widgets in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widgets.rounding = egui::Rounding::same(6.0);
    }
    ctx.set_visuals(visuals);
}

fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(ink()).strong())
            .fill(amber())
            .min_size(egui::vec2(132.0, 34.0)),
    )
}

fn soft_card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(22.0, 18.0))
        .rounding(egui::Rounding::same(12.0))
        .fill(egui::Color32::from_rgba_unmultiplied(28, 32, 44, 236))
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgba_unmultiplied(244, 198, 96, 60),
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui)
        })
        .inner
}

fn centered_column(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    let full = ui.available_width();
    let width = width.min(full);
    let side = ((full - width) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        ui.add_space(side);
        ui.vertical(|ui| {
            ui.set_width(width);
            add(ui);
        });
    });
}

/// 折线闪电。顶点是自写的，不是外部素材。
fn bolt_polyline(origin: egui::Pos2, scale: f32) -> Vec<egui::Pos2> {
    const LINE: [[f32; 2]; 4] = [[0.72, 0.00], [0.26, 0.46], [0.54, 0.50], [0.12, 1.00]];
    LINE.into_iter()
        .map(|[x, y]| origin + egui::vec2(x * scale, y * scale))
        .collect()
}

fn paint_lightning_backdrop(ui: &egui::Ui) {
    let rect = ui.max_rect();
    if rect.width() < 80.0 || rect.height() < 80.0 {
        return;
    }
    let painter = ui.painter();
    let scale = (rect.height() * 0.46).clamp(140.0, 280.0);
    let origin = egui::pos2(
        rect.right() - 28.0 - scale * 0.74,
        rect.bottom() - 36.0 - scale,
    );
    painter.add(egui::Shape::line(
        bolt_polyline(origin, scale),
        egui::Stroke::new(
            (scale * 0.055).clamp(8.0, 16.0),
            egui::Color32::from_rgba_unmultiplied(244, 198, 96, 32),
        ),
    ));

    let small = 34.0;
    let small_origin = egui::pos2(rect.right() - 28.0 - small * 0.74, rect.top() + 16.0);
    painter.add(egui::Shape::line(
        bolt_polyline(small_origin, small),
        egui::Stroke::new(
            2.2_f32,
            egui::Color32::from_rgba_unmultiplied(255, 214, 130, 150),
        ),
    ));
}

// ── M3′ Trace / Token UI ──────────────────────────────────────────────────

fn show_session_insights_bar(ui: &mut egui::Ui, chat: &ChatState) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Token").strong().size(12.0));
        match chat.insights.usage_display_state() {
            UsageDisplayState::Available => {
                if let Some(line) = chat.insights.usage_summary_line() {
                    ui.label(egui::RichText::new(line).monospace().size(11.0));
                } else {
                    ui.weak("（字段不完整）");
                }
            }
            UsageDisplayState::Unknown => {
                if chat.insights.stats_fetch == StatsFetchState::Pending {
                    ui.weak("用量统计中…");
                } else {
                    ui.weak("用量未知 — Pi 尚未上报 usage / stats 字段");
                }
            }
            UsageDisplayState::Unsupported => {
                ui.weak("不支持 — 当前 Pi 无法提供 get_session_stats");
            }
        }
        let running = chat
            .insights
            .trace_steps
            .iter()
            .filter(|s| s.running)
            .count();
        if running > 0 {
            ui.separator();
            ui.colored_label(
                egui::Color32::from_rgb(210, 160, 50),
                format!("🛠 {running} 个工具运行中"),
            );
        }
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
        ("运行中…", egui::Color32::from_rgb(210, 160, 50))
    } else if step.is_error {
        ("失败", egui::Color32::from_rgb(210, 80, 60))
    } else {
        ("完成", egui::Color32::from_rgb(80, 160, 100))
    };
    let dur = step
        .duration_ms
        .map(|ms| format!(" · {ms} ms"))
        .unwrap_or_default();
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
        .rounding(egui::Rounding::same(6.0))
        .fill(ui.visuals().extreme_bg_color)
        .stroke(egui::Stroke::new(1.0_f32, color))
        .show(ui, |ui| {
            ui.set_max_width(ui.available_width() * 0.85);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🛠").size(12.0));
                ui.label(
                    egui::RichText::new(&step.tool_name)
                        .strong()
                        .monospace()
                        .size(12.0),
                );
                ui.colored_label(color, status);
                if !dur.is_empty() {
                    ui.weak(dur);
                }
            });
            if let Some(args) = &step.args_preview {
                ui.label(egui::RichText::new(args).monospace().weak().size(10.0));
            }
        });
}

fn show_trace_section(ui: &mut egui::Ui, chat: &ChatState) {
    ui.add_space(10.0);
    ui.separator();
    egui::CollapsingHeader::new("Trace / 工具调用")
        .default_open(true)
        .id_salt("pi_light_trace")
        .show(ui, |ui| match chat.insights.trace_display_state() {
            TraceDisplayState::Empty => {
                ui.weak("暂无 Trace — 本会话尚未收到 tool_execution_* 事件");
            }
            TraceDisplayState::Available => {
                show_tool_cards(ui, chat);
            }
        });
}

// ── Chat message renderers ────────────────────────────────────────────────

fn show_user_message(ui: &mut egui::Ui, text: &str) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .rounding(egui::Rounding::same(8.0))
            .fill(egui::Color32::from_rgb(40, 100, 180))
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width() * 0.75);
                ui.label(
                    egui::RichText::new(text)
                        .color(egui::Color32::WHITE)
                        .size(14.0),
                );
            });
    });
}

fn show_assistant_message(ui: &mut egui::Ui, text: &str, streaming: bool) {
    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .rounding(egui::Rounding::same(8.0))
            .fill(ui.visuals().code_bg_color)
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width() * 0.75);
                if streaming {
                    ui.label(egui::RichText::new(format!("{text}▌")).size(14.0));
                } else {
                    ui.label(egui::RichText::new(text).size(14.0));
                }
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
