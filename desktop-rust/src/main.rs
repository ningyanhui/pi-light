// SPDX-License-Identifier: Apache-2.0
// pi-light — 内嵌 Pi 的轻量 Rust Shell
// M2′ 聊天 + M3′ Trace/Token + M4′+ 管理面 + M5′ # 会话引用

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use pi_light::{
    admin_panel::{AdminPanelState, SetModelRequest},
    chat::{ChatMessage, ChatState},
    pi_path::{self, PiProbe},
    pi_rpc::{PiRpcClient, PiRpcEvent},
    session_insights::{StatsFetchState, TraceDisplayState, UsageDisplayState},
    session_ref::{self, RefCandidate, RefChip, RefChooserState},
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
    chat: ChatState,
    spawn_error: Option<String>,
    egui_ctx: egui::Context,
    /// M4′ 管理面状态。
    admin: AdminPanelState,
    /// M5′ `#` 引用选择器状态。
    ref_chooser: RefChooserState,
    /// M5′ 已确认待附带发送的引用 chips。
    ref_chips: Vec<RefChip>,
    /// M5′ 候选缓存（在 # 触发时构建，每次触发刷新）。
    ref_candidates_cache: Vec<RefCandidate>,
}

impl PiLightApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let probe = pi_path::probe();
        let egui_ctx = cc.egui_ctx.clone();
        let mut app = Self {
            pi_probe: probe,
            rpc_client: None,
            chat: ChatState::default(),
            spawn_error: None,
            egui_ctx,
            admin: AdminPanelState::default(),
            ref_chooser: RefChooserState::default(),
            ref_chips: Vec::new(),
            ref_candidates_cache: Vec::new(),
        };
        app.try_spawn_rpc();
        app
    }

    fn try_spawn_rpc(&mut self) {
        self.rpc_client = None;
        self.spawn_error = None;
        self.chat = ChatState::default();

        if let Some(path) = self.pi_probe.path() {
            let ctx = self.egui_ctx.clone();
            match PiRpcClient::spawn(path, move || ctx.request_repaint()) {
                Ok(mut client) => {
                    let _ = client.request_get_state();
                    let _ = client.request_available_models();
                    self.rpc_client = Some(client);
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
        self.chat = ChatState::default();
        self.try_spawn_rpc();
        // 重启后刷新管理面发现结果
        self.admin.rediscover();
    }

    /// 发送当前输入（含 chips 引用上下文）给 Pi。
    fn do_send(&mut self) {
        let user_text = self.chat.input.trim().to_owned();
        if user_text.is_empty() && self.ref_chips.is_empty() {
            return;
        }
        if let Some(client) = &mut self.rpc_client {
            let chips = std::mem::take(&mut self.ref_chips);
            let effective_prompt = session_ref::build_prompt_text(&chips, &user_text);

            // 聊天历史中显示：引用标签 + 用户文字
            let display_text = if chips.is_empty() {
                user_text.clone()
            } else {
                let labels: Vec<&str> = chips.iter().map(|c| c.display.as_str()).collect();
                format!("[引用: {}]\n{}", labels.join(", "), user_text)
            };

            self.chat.push_user(display_text);
            self.chat.input.clear();
            self.ref_chooser.close();
            self.chat.begin_streaming();
            if let Err(e) = client.send_prompt(&effective_prompt) {
                self.chat.set_error(format!("发送失败：{e}"));
            }
        }
    }

    fn do_abort(&mut self) {
        if let Some(client) = &mut self.rpc_client {
            let _ = client.send_abort();
        }
    }

    fn drain_rpc_events(&mut self) {
        if let Some(client) = &mut self.rpc_client {
            let events = client.drain_events();
            for event in events {
                let is_exit = matches!(&event, PiRpcEvent::ProcessExited { .. });
                let request_stats = matches!(&event, PiRpcEvent::AgentSettled { aborted: false });

                match &event {
                    PiRpcEvent::StateInfo { model } => {
                        self.admin.update_live_model(model.clone(), Vec::new());
                    }
                    PiRpcEvent::AvailableModels(models) => {
                        self.admin.update_live_model(None, models.clone());
                    }
                    PiRpcEvent::ModelChanged(model) => {
                        self.admin
                            .update_live_model(Some(model.clone()), Vec::new());
                    }
                    _ => {}
                }

                self.chat.handle_rpc_event(event);
                if request_stats {
                    self.request_session_stats();
                }
                if is_exit {
                    self.rpc_client = None;
                    break;
                }
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
                Some("Pi 未就绪，无法通过 RPC 切换模型。请使用「写入 settings.json」。".to_owned());
        }
    }

    fn request_session_stats(&mut self) {
        if let Some(client) = &mut self.rpc_client {
            self.chat.insights.on_stats_request_sent();
            if client.request_session_stats().is_err() {
                self.chat.insights.stats_fetch = StatsFetchState::Failed;
            }
        }
    }

    /// M5′: 根据当前输入检测 `#` 触发词，刷新选择器候选。
    /// 每帧调用一次（非常廉价：仅字符串扫描 + 列表过滤）。
    fn update_ref_chooser(&mut self) {
        let trigger = session_ref::find_hash_trigger(&self.chat.input);
        let was_active = self.ref_chooser.active;

        match trigger {
            Some(hash_pos) => {
                // 刚刚激活：刷新候选缓存（含当前会话消息 + Pi 本地会话文件）
                if !was_active {
                    let mut cands = session_ref::candidates_from_messages(&self.chat.messages);
                    cands.extend(session_ref::candidates_from_pi_sessions());
                    self.ref_candidates_cache = cands;
                }
                let query = session_ref::extract_query(&self.chat.input, hash_pos).to_owned();
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_rpc_events();
        self.update_ref_chooser();

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
                    session_ref::remove_hash_trigger(&mut self.chat.input, hash_pos);
                }
            } else if closed {
                self.ref_chooser.close();
            }
        }

        // ── Header ────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("pi-light");
                ui.weak("— 内嵌 Pi 的轻量 Rust Shell");

                let (dot_color, dot_tip) = pi_dot_color(&self.pi_probe, self.rpc_client.is_some());
                ui.colored_label(dot_color, "●").on_hover_text(dot_tip);

                if self.chat.is_streaming {
                    ui.weak("● 生成中…");
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("⟳ 重新探测")
                        .on_hover_text("重新扫描 Pi 二进制并重启 RPC 连接")
                        .clicked()
                    {
                        self.reprobe();
                    }

                    let admin_label = if self.admin.open {
                        "⚙ 关闭管理面"
                    } else {
                        "⚙ 管理面"
                    };
                    if ui
                        .selectable_label(self.admin.open, admin_label)
                        .on_hover_text("打开 / 关闭 Skills / MCP / 插件 / 记忆管理面板")
                        .clicked()
                    {
                        self.admin.open = !self.admin.open;
                    }
                });
            });
        });

        // ── M4′+ 管理面（右侧面板，始终可见，无需 Pi 就绪）────────────────
        let panel_ret = self.admin.show_panel(ctx, None, self.rpc_client.is_some());
        if let Some(req) = panel_ret.set_model_request {
            self.handle_set_model_request(req);
        }
        if panel_ret.restart_sidecar {
            self.restart_sidecar();
        }

        // ── Dispatch: chat UI vs. not-ready info screen ───────────────────
        if self.pi_probe.is_ready() && self.rpc_client.is_some() {
            self.show_chat_ui(ctx);
        } else if let Some(err) = &self.spawn_error.clone() {
            self.show_spawn_error(ctx, &err.clone());
        } else {
            show_not_ready_screen(ctx, &self.pi_probe);
        }
    }
}

impl PiLightApp {
    // ── Chat UI ───────────────────────────────────────────────────────────

    fn show_chat_ui(&mut self, ctx: &egui::Context) {
        // Token / trace summary above the input area
        egui::TopBottomPanel::bottom("insights_bar")
            .min_height(28.0)
            .show(ctx, |ui| {
                show_session_insights_bar(ui, &self.chat);
            });

        // Bottom input panel
        egui::TopBottomPanel::bottom("chat_input")
            .min_height(56.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);

                // ── M5′ Chips row ─────────────────────────────────────────
                let mut chip_to_remove: Option<usize> = None;
                if !self.ref_chips.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for (i, chip) in self.ref_chips.iter().enumerate() {
                            // chip 显示为高亮小按钮，点击可移除
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

                // ── Input row ─────────────────────────────────────────────
                ui.horizontal(|ui| {
                    let can_send = !self.chat.is_streaming
                        && (!self.chat.input.trim().is_empty() || !self.ref_chips.is_empty());
                    let can_stop = self.chat.is_streaming;

                    let input_response = ui.add_sized(
                        [ui.available_width() - 140.0, 32.0],
                        egui::TextEdit::singleline(&mut self.chat.input)
                            .hint_text("输入消息，按 Enter 发送；输入 # 引用会话…")
                            .interactive(!self.chat.is_streaming),
                    );

                    // Enter 发送（选择器未激活时）
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

                    let stop_btn = ui.add_enabled(
                        can_stop,
                        egui::Button::new("■ 停止").min_size([60.0, 32.0].into()),
                    );
                    if stop_btn.clicked() {
                        self.do_abort();
                    }
                    stop_btn.on_hover_text("中止当前生成（发送 abort 命令）");
                });
                ui.add_space(4.0);
            });

        // ── M5′ 引用选择器 popup ──────────────────────────────────────────
        if self.ref_chooser.active {
            self.show_ref_chooser_popup(ctx);
        }

        // Central message list
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.chat.messages.is_empty() && !self.chat.is_streaming {
                self.show_chat_welcome(ui);
                return;
            }

            if let Some(err) = &self.chat.error.clone() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(210, 80, 60), format!("⚠ {err}"));
                    if ui.small_button("关闭").clicked() {
                        self.chat.error = None;
                    }
                });
                ui.separator();
            }

            let scroll_to_bottom = self.chat.scroll_to_bottom;
            self.chat.scroll_to_bottom = false;

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.add_space(8.0);

                    for msg in &self.chat.messages {
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

                    if self.chat.is_streaming {
                        show_assistant_message(ui, &self.chat.streaming_buf, true);
                        ui.add_space(6.0);
                    }

                    if scroll_to_bottom {
                        ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                    }

                    show_trace_section(ui, &self.chat);
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
                session_ref::remove_hash_trigger(&mut self.chat.input, hash_pos);
            }
        }
    }

    fn show_chat_welcome(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.label(
                egui::RichText::new("Pi 已就绪")
                    .color(egui::Color32::from_rgb(80, 180, 100))
                    .strong()
                    .size(18.0),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(format!(
                    "二进制：{}",
                    self.pi_probe
                        .path()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default()
                ))
                .monospace()
                .weak()
                .size(11.0),
            );
            ui.add_space(20.0);
            ui.label(
                egui::RichText::new("在底部输入框中输入消息开始对话")
                    .weak()
                    .italics()
                    .size(13.0),
            );
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("输入 # 可引用历史会话消息，附带上下文发送")
                    .weak()
                    .size(12.0),
            );
        });
    }

    fn show_spawn_error(&self, ctx: &egui::Context, error: &str) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.label(
                    egui::RichText::new("⚠ Pi 启动失败")
                        .color(egui::Color32::from_rgb(210, 80, 60))
                        .strong()
                        .size(17.0),
                );
                ui.add_space(12.0);
                ui.label(egui::RichText::new(error).monospace().size(12.0));
                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new("请检查 Pi 二进制是否可执行，然后点击「⟳ 重新探测」。")
                        .weak()
                        .size(13.0),
                );
            });
        });
    }
}

// ── Not-ready info screen (M1′ three-state, preserved) ────────────────────

fn show_not_ready_screen(ctx: &egui::Context, probe: &PiProbe) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            show_pi_status(ui, probe);
        });
    });
}

fn show_pi_status(ui: &mut egui::Ui, probe: &PiProbe) {
    match probe {
        PiProbe::BundledReady(_) => {
            show_status_badge(
                ui,
                "✓ Pi 已就绪（内嵌 sidecar / Bundled Ready）",
                egui::Color32::from_rgb(80, 180, 100),
            );
            ui.add_space(10.0);
            ui.label(egui::RichText::new("已从捆绑路径找到 Pi 二进制。").size(14.0));
        }
        PiProbe::DetectedLocal { .. } => {
            let title = format!(
                "~ Pi 就绪（本机 / Detected Local — {}）",
                probe.source_label()
            );
            show_status_badge(ui, &title, egui::Color32::from_rgb(80, 140, 210));
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(format!("通过「{}」找到 Pi。", probe.source_label()))
                    .size(14.0),
            );
        }
        PiProbe::NotReady => {
            show_not_ready_panel(ui);
        }
    }

    if let Some(path) = probe.path() {
        ui.add_space(8.0);
        show_path_box(ui, &path.to_string_lossy());
    }

    ui.add_space(28.0);
    ui.label(
        egui::RichText::new("会话聊天需要 Pi 就绪后自动启动，当前尚未找到 Pi")
            .weak()
            .italics()
            .size(12.0),
    );
}

fn show_status_badge(ui: &mut egui::Ui, label: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(label).color(color).strong().size(17.0));
}

fn show_path_box(ui: &mut egui::Ui, path: &str) {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(14.0, 8.0))
        .rounding(egui::Rounding::same(4.0))
        .fill(ui.visuals().extreme_bg_color)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Pi 路径：").strong().size(12.0));
            ui.label(egui::RichText::new(path).monospace().size(12.0));
        });
}

fn show_not_ready_panel(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("⚠ Pi 尚未就绪（Not Ready）")
            .color(egui::Color32::from_rgb(210, 150, 50))
            .strong()
            .size(17.0),
    );
    ui.add_space(12.0);
    ui.label(
        egui::RichText::new("请通过以下方式之一提供 Pi 二进制，然后点击「⟳ 重新探测」：")
            .size(14.0),
    );
    ui.add_space(10.0);

    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
        .rounding(egui::Rounding::same(4.0))
        .fill(ui.visuals().extreme_bg_color)
        .show(ui, |ui| {
            ui.label(egui::RichText::new("方式 1 — 环境变量（开发 / 临时优先）").strong());
            ui.code("PI_LIGHT_PI_BIN=<pi 或 pi.exe 的完整路径>");
            ui.add_space(10.0);

            ui.label(egui::RichText::new("方式 2 — 捆绑 sidecar（推荐，解压到以下目录）").strong());
            ui.code("{exe_dir}/sidecar/pi/pi.exe    (Windows)");
            ui.code("{exe_dir}/sidecar/pi/pi        (Linux / macOS)");
            ui.add_space(10.0);

            ui.label(egui::RichText::new("方式 3 — 本机 PATH（可选探测）").strong());
            ui.label(
                egui::RichText::new("将 pi / pi.exe 加入系统 PATH，点击「⟳ 重新探测」即可识别。")
                    .weak()
                    .size(12.0),
            );
        });

    ui.add_space(14.0);
    ui.label(egui::RichText::new("下载 Pi v1.4.0 官方 Release：").size(13.0));
    ui.hyperlink_to(
        "pi-windows-x64.zip（GitHub Releases）",
        "https://github.com/earendil-works/pi/releases/download/v1.4.0/pi-windows-x64.zip",
    );
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "SHA-256: 0f7256ea8bdac3af20012b94fdc1252a31cc6dc0d2f9b9eb539555d22e3e321b",
        )
        .weak()
        .monospace()
        .size(11.0),
    );
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
    });
}

fn show_trace_section(ui: &mut egui::Ui, chat: &ChatState) {
    ui.add_space(10.0);
    ui.separator();
    egui::CollapsingHeader::new("Trace / 工具步骤")
        .default_open(true)
        .id_salt("pi_light_trace")
        .show(ui, |ui| match chat.insights.trace_display_state() {
            TraceDisplayState::Empty => {
                ui.weak("暂无 Trace — 本会话尚未收到 tool_execution_* 事件");
            }
            TraceDisplayState::Available => {
                for step in &chat.insights.trace_steps {
                    let status = if step.running {
                        "运行中…"
                    } else if step.is_error {
                        "失败"
                    } else {
                        "完成"
                    };
                    let dur = step
                        .duration_ms
                        .map(|ms| format!(" · {ms} ms"))
                        .unwrap_or_default();
                    ui.label(
                        egui::RichText::new(format!(
                            "• {} ({}) — {}{}",
                            step.tool_name, step.tool_call_id, status, dur
                        ))
                        .monospace()
                        .size(11.0),
                    );
                }
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
            "Pi 未就绪 — 请提供 Pi 二进制",
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
