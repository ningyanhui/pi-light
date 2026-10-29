// SPDX-License-Identifier: Apache-2.0
//
// M4′ 管理面 — Skills / MCP / 插件 / 记忆 / 模型 五类 MVP
//
// 数据来源：
//   - Pi agent 目录（默认 ~/.pi/agent/，可被 PI_CODING_AGENT_DIR 覆盖）
//   - 项目 .pi/ 目录
//   - ~/.agents/skills/ 与 .agents/skills/（Agent Skills 规范路径）
//   - 模型/API 密钥状态：settings.json + auth.json + 环境变量
//
// Pi v1.4.0 无管理类 RPC；模型切换 via set_model RPC 在 M4′ 标注为「需连接 Pi」；
// 配置文件读写作为离线 fallback。

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use eframe::egui;

// ── Tabs ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AdminTab {
    #[default]
    Skills,
    Mcp,
    Plugins,
    Memory,
    Models,
}

impl AdminTab {
    pub fn label(self) -> &'static str {
        match self {
            AdminTab::Skills => "Skills",
            AdminTab::Mcp => "MCP",
            AdminTab::Plugins => "插件",
            AdminTab::Memory => "记忆",
            AdminTab::Models => "模型",
        }
    }
}

// ── Item types ────────────────────────────────────────────────────────────

/// 条目是否支持 UI 级开关。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemCapability {
    ReadWrite,
    /// 仅只读展示；Pi 无对应 RPC 或配置路径不明。
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    Enabled,
    Disabled,
    Unknown,
}

/// Skills / MCP / 插件列表中的一行条目。
#[derive(Debug, Clone)]
pub struct ManagedItem {
    pub id: String,
    pub display_name: String,
    /// 分类标签（skill category）或命令名（mcp command）。
    pub tag: Option<String>,
    pub state: ItemState,
    pub capability: ItemCapability,
    /// 配置文件路径（用于写回）。
    pub config_path: Option<PathBuf>,
}

// ── Memory ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryLayer {
    Project,
    User,
    Session,
}

impl MemoryLayer {
    pub fn label(self) -> &'static str {
        match self {
            MemoryLayer::Project => "Project",
            MemoryLayer::User => "User",
            MemoryLayer::Session => "Session",
        }
    }

    fn description(self) -> &'static str {
        match self {
            MemoryLayer::Project => "当前项目指令（CLAUDE.md / AGENTS.md / .pi/APPEND_SYSTEM.md）",
            MemoryLayer::User => "用户级跨项目指令（~/.pi/agent/AGENTS.md 或 CLAUDE.md）",
            MemoryLayer::Session => "本次 RPC 会话临时上下文（进程重启后不保留）",
        }
    }

    fn empty_label(self) -> &'static str {
        match self {
            MemoryLayer::Project => {
                "未发现项目指令文件（CLAUDE.md / AGENTS.md / .pi/APPEND_SYSTEM.md）"
            }
            MemoryLayer::User => "未发现用户指令文件（~/.pi/agent/AGENTS.md）",
            MemoryLayer::Session => "当前会话无注入上下文",
        }
    }
}

/// 单条记忆摘要（只读展示）。
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub layer: MemoryLayer,
    pub key: String,
    pub excerpt: String,
    pub source_path: Option<PathBuf>,
}

// ── Model config ──────────────────────────────────────────────────────────

/// 从 settings.json 读取的模型配置快照。
#[derive(Debug, Clone, Default)]
pub struct ModelConfig {
    /// 当前默认提供商（来自 settings.json `defaultProvider`）。
    pub default_provider: Option<String>,
    /// 当前默认模型 ID（来自 settings.json `defaultModel`）。
    pub default_model: Option<String>,
    /// settings.json 路径（用于写回）。
    pub settings_path: Option<PathBuf>,
    /// 自定义端点列表（来自 models.json）。
    pub custom_endpoints: Vec<CustomEndpoint>,
    /// API 密钥状态。
    pub auth_status: AuthStatus,
    /// 来自 RPC `get_state` 的实时模型名称（可选）。
    pub live_model: Option<String>,
    /// 来自 RPC `get_available_models` 的模型列表（可选）。
    pub available_models: Vec<String>,
    /// 用户在 UI 输入的"切换目标"（未提交）。
    pub switch_input: String,
}

/// models.json 中的一个自定义端点条目。
#[derive(Debug, Clone)]
pub struct CustomEndpoint {
    pub name: String,
    pub base_url: String,
}

/// API 密钥配置状态（只展示状态，不展示明文）。
#[derive(Debug, Clone, Default)]
pub enum AuthStatus {
    #[default]
    Unknown,
    /// 通过环境变量配置（显示变量名，不显示值）。
    EnvVar(String),
    /// auth.json 文件存在（显示"已配置"，不读内容）。
    FileExists(PathBuf),
    /// 未检测到任何凭据配置。
    NotConfigured,
}

// ── Discovery state ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DiscoveryState {
    #[default]
    NotStarted,
    Ready,
    Failed(String),
}

// ── AdminPanelState ───────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct AdminPanelState {
    pub open: bool,
    pub active_tab: AdminTab,

    pub skills_state: DiscoveryState,
    pub skill_items: Vec<ManagedItem>,

    pub mcp_state: DiscoveryState,
    pub mcp_items: Vec<ManagedItem>,

    pub plugins_state: DiscoveryState,

    pub memory_state: DiscoveryState,
    pub memory_entries: Vec<MemoryEntry>,
    /// 来自聊天层的会话上下文摘要。
    pub session_memory_text: Option<String>,

    pub model_state: DiscoveryState,
    pub model_config: ModelConfig,

    pub last_error: Option<String>,
}

impl AdminPanelState {
    // ── Discovery ─────────────────────────────────────────────────────────

    pub fn discover_all(&mut self) {
        self.skill_items = discover_skills();
        self.skills_state = DiscoveryState::Ready;

        self.mcp_items = discover_mcp();
        self.mcp_state = DiscoveryState::Ready;

        self.plugins_state = DiscoveryState::Ready;

        self.memory_entries = discover_memory();
        self.memory_state = DiscoveryState::Ready;

        self.model_config = discover_model_config();
        self.model_state = DiscoveryState::Ready;
    }

    pub fn rediscover(&mut self) {
        self.skills_state = DiscoveryState::NotStarted;
        self.mcp_state = DiscoveryState::NotStarted;
        self.plugins_state = DiscoveryState::NotStarted;
        self.memory_state = DiscoveryState::NotStarted;
        self.model_state = DiscoveryState::NotStarted;
        self.discover_all();
    }

    /// 将 RPC 获得的实时模型信息更新到 model_config（由 main.rs 调用）。
    pub fn update_live_model(&mut self, model_name: Option<String>, models: Vec<String>) {
        self.model_config.live_model = model_name;
        if !models.is_empty() {
            self.model_config.available_models = models;
        }
    }

    // ── Toggle ────────────────────────────────────────────────────────────

    pub fn toggle_item(&mut self, tab: AdminTab, id: &str) -> Result<(), String> {
        let items: &mut [ManagedItem] = match tab {
            AdminTab::Mcp => &mut self.mcp_items,
            AdminTab::Skills => &mut self.skill_items,
            AdminTab::Plugins => return Err("插件条目仅只读展示。".to_owned()),
            AdminTab::Memory => return Err("记忆条目仅只读展示。".to_owned()),
            AdminTab::Models => return Err("请使用切换按钮修改默认模型。".to_owned()),
        };

        let item = items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| format!("条目不存在：{id}"))?;

        if item.capability == ItemCapability::ReadOnly {
            return Err(format!(
                "「{}」暂不支持 UI 开关，请手动编辑配置文件。",
                item.display_name
            ));
        }

        let new_state = match item.state {
            ItemState::Enabled => ItemState::Disabled,
            ItemState::Disabled | ItemState::Unknown => ItemState::Enabled,
        };

        if let Some(path) = item.config_path.clone() {
            write_mcp_disabled(&path, id, new_state == ItemState::Disabled)
                .map_err(|e| format!("写入配置失败：{e}"))?;
        }

        item.state = new_state;
        Ok(())
    }

    // ── UI ────────────────────────────────────────────────────────────────

    /// 渲染右侧管理面板。
    ///
    /// `session_memory_hint`：来自聊天层的会话上下文摘要（可选）。
    /// 返回 `Some(set_model_request)` 当用户请求通过 RPC 切换模型。
    pub fn show_panel(
        &mut self,
        ctx: &egui::Context,
        session_memory_hint: Option<&str>,
    ) -> Option<SetModelRequest> {
        if !self.open {
            return None;
        }

        if self.skills_state == DiscoveryState::NotStarted {
            self.discover_all();
        }

        if let Some(hint) = session_memory_hint {
            self.session_memory_text = Some(hint.to_owned());
        }

        let mut model_request: Option<SetModelRequest> = None;

        egui::SidePanel::right("admin_panel")
            .resizable(true)
            .default_width(340.0)
            .min_width(220.0)
            .max_width(520.0)
            .show(ctx, |ui| {
                // 标题栏
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("⚙ 管理面").strong().size(15.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").on_hover_text("关闭面板").clicked() {
                            self.open = false;
                        }
                        if ui
                            .small_button("⟳")
                            .on_hover_text("重新扫描本机配置")
                            .clicked()
                        {
                            self.rediscover();
                        }
                    });
                });
                ui.separator();

                // 标签栏
                ui.horizontal_wrapped(|ui| {
                    for tab in [
                        AdminTab::Skills,
                        AdminTab::Mcp,
                        AdminTab::Plugins,
                        AdminTab::Memory,
                        AdminTab::Models,
                    ] {
                        if ui
                            .selectable_label(self.active_tab == tab, tab.label())
                            .clicked()
                        {
                            self.active_tab = tab;
                        }
                    }
                });
                ui.separator();

                // 错误横幅
                if let Some(err) = self.last_error.clone() {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(220, 120, 80), format!("⚠ {err}"));
                        if ui.small_button("×").clicked() {
                            self.last_error = None;
                        }
                    });
                    ui.separator();
                }

                // 内容区
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        let tab = self.active_tab;
                        let toggle_req = match tab {
                            AdminTab::Skills => render_item_list(
                                ui,
                                "Skills（本机已发现）",
                                &mut self.skill_items,
                                SKILLS_EMPTY_MSG,
                            ),
                            AdminTab::Mcp => {
                                render_item_list(ui, "MCP 服务", &mut self.mcp_items, MCP_EMPTY_MSG)
                            }
                            AdminTab::Plugins => {
                                render_plugins(ui);
                                None
                            }
                            AdminTab::Memory => {
                                render_memory(
                                    ui,
                                    &self.memory_entries,
                                    self.session_memory_text.as_deref(),
                                );
                                None
                            }
                            AdminTab::Models => {
                                model_request = render_models(ui, &mut self.model_config);
                                None
                            }
                        };

                        if let Some(id) = toggle_req {
                            if let Err(e) = self.toggle_item(tab, &id) {
                                self.last_error = Some(e);
                            }
                        }
                    });
            });

        model_request
    }
}

/// 来自 UI 的 set_model RPC 请求。
#[derive(Debug, Clone)]
pub struct SetModelRequest {
    pub provider: String,
    pub model_id: String,
}

// ── Tab renderers ─────────────────────────────────────────────────────────

const SKILLS_EMPTY_MSG: &str = "未在本机发现 Pi skills 文件\n\
    （已查：~/.pi/agent/skills/、.pi/skills/、~/.agents/skills/、.agents/skills/）\n\
    在上述目录中创建 <name>/SKILL.md 后点击「⟳ 刷新」。";

const MCP_EMPTY_MSG: &str = "未找到 MCP 配置文件\n\
    （已查：~/.pi/agent/mcp.json、.pi/mcp.json）";

fn render_item_list(
    ui: &mut egui::Ui,
    section: &str,
    items: &mut [ManagedItem],
    empty_msg: &str,
) -> Option<String> {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(section).strong().size(13.0));
    ui.add_space(6.0);

    if items.is_empty() {
        show_empty(ui, empty_msg);
        return None;
    }

    let mut toggle_id: Option<String> = None;

    for item in items.iter_mut() {
        let is_enabled = item.state == ItemState::Enabled;
        let can_toggle = item.capability == ItemCapability::ReadWrite;

        ui.horizontal(|ui| {
            let mut checked = is_enabled;
            let resp = ui.add_enabled(can_toggle, egui::Checkbox::new(&mut checked, ""));
            if resp.changed() {
                toggle_id = Some(item.id.clone());
            }

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&item.display_name).strong());
                    if let Some(tag) = &item.tag {
                        ui.label(
                            egui::RichText::new(format!("[{tag}]"))
                                .weak()
                                .monospace()
                                .size(10.0),
                        );
                    }
                });
                let (state_text, state_color) = match item.state {
                    ItemState::Enabled => ("已启用", egui::Color32::from_rgb(60, 160, 80)),
                    ItemState::Disabled => ("已禁用", egui::Color32::from_rgb(160, 80, 60)),
                    ItemState::Unknown => ("未知", egui::Color32::from_rgb(140, 130, 60)),
                };
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(state_text)
                            .color(state_color)
                            .size(11.0),
                    );
                    if item.capability == ItemCapability::ReadOnly {
                        ui.label(
                            egui::RichText::new("（只读 / 暂不支持开关）")
                                .weak()
                                .size(10.0),
                        )
                        .on_hover_text("Pi v1.4.0 无对应管理 RPC；请手动编辑配置文件。");
                    }
                });
                if let Some(path) = &item.config_path {
                    ui.label(
                        egui::RichText::new(path.to_string_lossy())
                            .weak()
                            .monospace()
                            .size(10.0),
                    );
                }
            });
        });
        ui.separator();
    }

    toggle_id
}

fn render_plugins(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new("插件 / Extensions").strong().size(13.0));
    ui.add_space(10.0);
    show_empty(
        ui,
        "Pi v1.4.0 暂无独立插件扩展体系。\n此标签保留，待后续版本接入。",
    );
}

fn render_memory(ui: &mut egui::Ui, entries: &[MemoryEntry], session_text: Option<&str>) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new("记忆（只读展示）").strong().size(13.0));
    ui.add_space(6.0);
    for layer in [
        MemoryLayer::Project,
        MemoryLayer::User,
        MemoryLayer::Session,
    ] {
        render_memory_layer(ui, layer, entries, session_text);
        ui.add_space(4.0);
    }
}

fn render_memory_layer(
    ui: &mut egui::Ui,
    layer: MemoryLayer,
    entries: &[MemoryEntry],
    session_text: Option<&str>,
) {
    let layer_entries: Vec<&MemoryEntry> = entries.iter().filter(|e| e.layer == layer).collect();
    let has_session = layer == MemoryLayer::Session && session_text.is_some();
    let has_content = !layer_entries.is_empty() || has_session;

    egui::CollapsingHeader::new(
        egui::RichText::new(format!("▸ {} 层", layer.label()))
            .strong()
            .size(13.0),
    )
    .id_salt(format!("mem_{}", layer.label()))
    .default_open(true)
    .show(ui, |ui| {
        ui.label(
            egui::RichText::new(layer.description())
                .weak()
                .italics()
                .size(11.0),
        );
        ui.add_space(4.0);

        if !has_content {
            show_empty(ui, layer.empty_label());
            return;
        }

        if has_session {
            let text = session_text.unwrap_or_default();
            show_excerpt_frame(ui, "会话上下文", &truncate(text, 240), None);
        }
        for entry in layer_entries {
            show_excerpt_frame(ui, &entry.key, &entry.excerpt, entry.source_path.as_ref());
            ui.add_space(3.0);
        }
    });
}

/// 模型配置 tab。
///
/// 返回 `Some(SetModelRequest)` 若用户点击「通过 RPC 切换」。
fn render_models(ui: &mut egui::Ui, cfg: &mut ModelConfig) -> Option<SetModelRequest> {
    ui.add_space(4.0);
    ui.label(egui::RichText::new("模型配置").strong().size(13.0));
    ui.add_space(6.0);

    let mut rpc_request: Option<SetModelRequest> = None;

    // ── 当前模型 ──────────────────────────────────────────────────────────
    ui.label(egui::RichText::new("当前模型").strong().size(12.0));
    let current = cfg
        .live_model
        .as_deref()
        .or(cfg.default_model.as_deref())
        .unwrap_or("（未知 / Pi 未就绪）");
    let provider = cfg.default_provider.as_deref().unwrap_or("（未知）");
    ui.label(
        egui::RichText::new(format!("  模型：{current}"))
            .monospace()
            .size(12.0),
    );
    ui.label(
        egui::RichText::new(format!("  提供商：{provider}"))
            .monospace()
            .size(12.0),
    );
    if cfg.live_model.is_some() {
        ui.label(
            egui::RichText::new("  ● 来自实时 RPC get_state")
                .color(egui::Color32::from_rgb(60, 160, 80))
                .size(11.0),
        );
    } else if cfg.settings_path.is_some() {
        ui.label(
            egui::RichText::new("  ○ 来自 settings.json（Pi 未运行）")
                .weak()
                .size(11.0),
        );
    } else {
        ui.label(
            egui::RichText::new("  ○ 未找到 settings.json")
                .weak()
                .size(11.0),
        );
    }
    ui.add_space(8.0);

    // ── 可用模型列表 ──────────────────────────────────────────────────────
    ui.label(egui::RichText::new("可用模型").strong().size(12.0));
    if cfg.available_models.is_empty() {
        show_empty(
            ui,
            "Pi 未就绪时无法获取实时模型列表（需 RPC get_available_models）。\n\
             可通过 settings.json 的 enabledModels 字段筛选。",
        );
    } else {
        for m in &cfg.available_models {
            ui.label(
                egui::RichText::new(format!("  • {m}"))
                    .monospace()
                    .size(11.0),
            );
        }
    }
    ui.add_space(8.0);

    // ── 切换默认模型 ──────────────────────────────────────────────────────
    ui.label(egui::RichText::new("切换默认模型").strong().size(12.0));
    ui.horizontal(|ui| {
        ui.label("provider/modelId：");
        ui.add(
            egui::TextEdit::singleline(&mut cfg.switch_input)
                .hint_text("anthropic/claude-sonnet-4-20250514")
                .desired_width(200.0),
        );
    });
    ui.horizontal(|ui| {
        // RPC 切换（需要 Pi 在线）
        let parts: Vec<&str> = cfg.switch_input.splitn(2, '/').collect();
        let valid = parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty();
        if ui
            .add_enabled(valid, egui::Button::new("● 通过 RPC 切换（需 Pi 就绪）"))
            .on_hover_text("向运行中的 Pi 发送 set_model 命令；Pi 未就绪时灰显。")
            .clicked()
            && valid
        {
            rpc_request = Some(SetModelRequest {
                provider: parts[0].to_owned(),
                model_id: parts[1].to_owned(),
            });
        }

        // 写入 settings.json
        if ui
            .add_enabled(
                valid && cfg.settings_path.is_some(),
                egui::Button::new("○ 写入 settings.json"),
            )
            .on_hover_text(
                "将 defaultProvider/defaultModel 写入 settings.json；下次启动 Pi 时生效。",
            )
            .clicked()
            && valid
            && cfg.settings_path.is_some()
        {
            let path = cfg.settings_path.clone().unwrap();
            match write_settings_model(&path, parts[0], parts[1]) {
                Ok(()) => {
                    cfg.default_provider = Some(parts[0].to_owned());
                    cfg.default_model = Some(parts[1].to_owned());
                }
                Err(e) => {
                    // 错误由调用方通过返回值或 last_error 展示；这里先忽略
                    let _ = e;
                }
            }
        }
    });
    ui.label(
        egui::RichText::new("格式：<provider>/<modelId>，例如 anthropic/claude-sonnet-4-20250514")
            .weak()
            .size(10.0),
    );
    ui.add_space(8.0);

    // ── API 密钥状态 ──────────────────────────────────────────────────────
    ui.label(egui::RichText::new("API 密钥状态").strong().size(12.0));
    match &cfg.auth_status {
        AuthStatus::EnvVar(var) => {
            ui.label(
                egui::RichText::new(format!("  ✓ 通过环境变量配置：{var}=***"))
                    .color(egui::Color32::from_rgb(60, 160, 80))
                    .size(12.0),
            );
        }
        AuthStatus::FileExists(path) => {
            ui.label(
                egui::RichText::new(format!("  ✓ auth.json 存在：{}", path.display()))
                    .color(egui::Color32::from_rgb(60, 160, 80))
                    .size(12.0),
            );
            ui.label(
                egui::RichText::new("  （内容不展示；如需重新登录，在 Pi 中运行 /login）")
                    .weak()
                    .size(10.0),
            );
        }
        AuthStatus::NotConfigured => {
            ui.label(
                egui::RichText::new("  ✗ 未检测到 API 密钥（环境变量或 auth.json）")
                    .color(egui::Color32::from_rgb(210, 120, 60))
                    .size(12.0),
            );
            ui.label(
                egui::RichText::new("  在 Pi 中运行 /login 进行认证，或设置对应环境变量。")
                    .weak()
                    .size(11.0),
            );
        }
        AuthStatus::Unknown => {
            ui.label(egui::RichText::new("  ○ 状态未知").weak().size(12.0));
        }
    }
    ui.add_space(8.0);

    // ── 自定义端点 ────────────────────────────────────────────────────────
    if !cfg.custom_endpoints.is_empty() {
        ui.label(
            egui::RichText::new("自定义端点（models.json）")
                .strong()
                .size(12.0),
        );
        for ep in &cfg.custom_endpoints {
            ui.label(
                egui::RichText::new(format!("  {} — {}", ep.name, ep.base_url))
                    .monospace()
                    .size(11.0),
            );
        }
        ui.add_space(4.0);
    }

    // ── settings.json 路径提示 ────────────────────────────────────────────
    if let Some(p) = &cfg.settings_path {
        ui.label(
            egui::RichText::new(format!("settings.json：{}", p.display()))
                .weak()
                .monospace()
                .size(10.0),
        );
    } else {
        let agent_dir = pi_agent_dir();
        if let Some(d) = agent_dir {
            ui.label(
                egui::RichText::new(format!(
                    "settings.json 不存在（期望路径：{}/settings.json）",
                    d.display()
                ))
                .weak()
                .size(10.0),
            );
        }
    }

    rpc_request
}

fn show_excerpt_frame(ui: &mut egui::Ui, title: &str, excerpt: &str, path: Option<&PathBuf>) {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
        .rounding(egui::Rounding::same(4.0))
        .fill(ui.visuals().extreme_bg_color)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).strong().size(12.0));
                if let Some(p) = path {
                    ui.label(
                        egui::RichText::new(format!("({})", p.to_string_lossy()))
                            .weak()
                            .monospace()
                            .size(10.0),
                    );
                }
            });
            ui.label(egui::RichText::new(excerpt).weak().size(11.0));
        });
}

fn show_empty(ui: &mut egui::Ui, msg: &str) {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .rounding(egui::Rounding::same(6.0))
        .fill(ui.visuals().faint_bg_color)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(msg).weak().italics().size(12.0));
        });
}

// ── Pi path resolution ────────────────────────────────────────────────────

/// Pi agent 目录（默认 ~/.pi/agent，可被 PI_CODING_AGENT_DIR 覆盖）。
pub fn pi_agent_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("PI_CODING_AGENT_DIR") {
        return Some(PathBuf::from(dir));
    }
    home_dir().map(|h| h.join(".pi").join("agent"))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn pi_project_dir() -> Option<PathBuf> {
    std::env::current_dir().ok().map(|d| d.join(".pi"))
}

// ── Skills discovery ──────────────────────────────────────────────────────

/// 发现本机 Pi skills 文件。
///
/// 查找路径（官方约定，优先级从高到低）：
///
/// 1. `.pi/skills/`（项目级）
/// 2. `.agents/skills/`（项目级 Agent Skills 规范路径）
/// 3. `~/.pi/agent/skills/`（用户级，Pi agent 目录）
/// 4. `~/.agents/skills/`（用户级 Agent Skills 规范路径）
pub fn discover_skills() -> Vec<ManagedItem> {
    let dirs = [
        (
            pi_project_dir().map(|d| d.join("skills")),
            ItemCapability::ReadOnly,
        ),
        (
            std::env::current_dir()
                .ok()
                .map(|d| d.join(".agents").join("skills")),
            ItemCapability::ReadOnly,
        ),
        (
            pi_agent_dir().map(|d| d.join("skills")),
            ItemCapability::ReadOnly,
        ),
        (
            home_dir().map(|h| h.join(".agents").join("skills")),
            ItemCapability::ReadOnly,
        ),
    ];

    let mut items: Vec<ManagedItem> = Vec::new();
    for (dir_opt, cap) in dirs {
        let Some(dir) = dir_opt else { continue };
        if dir.is_dir() {
            collect_skill_files(&dir, cap, &mut items);
        }
    }
    items
}

/// 递归扫描 `dir` 中的 skill 文件。
///
/// 支持两种约定：
///
/// - `<name>/SKILL.md`（目录含 SKILL.md，[Agent Skills 规范](https://agentskills.io)）
/// - `<name>.md`（扁平 markdown 文件）
fn collect_skill_files(dir: &Path, cap: ItemCapability, out: &mut Vec<ManagedItem>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let skill_md = path.join("SKILL.md");
            if skill_md.is_file() {
                let name = dir_stem(&path);
                let category = dir.file_name().and_then(|s| s.to_str()).map(str::to_owned);
                push_skill(out, name, category, cap, skill_md);
                continue;
            }
        }
        if path.is_file()
            && path.extension().and_then(|e| e.to_str()) == Some("md")
            && path.file_name().and_then(|s| s.to_str()) != Some("SKILL.md")
        {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_owned();
            push_skill(out, name, None, cap, path);
        }
    }
}

fn push_skill(
    out: &mut Vec<ManagedItem>,
    name: String,
    tag: Option<String>,
    cap: ItemCapability,
    path: PathBuf,
) {
    if out.iter().any(|i| i.id == name) {
        return;
    }
    out.push(ManagedItem {
        id: name.clone(),
        display_name: name,
        tag,
        state: ItemState::Unknown,
        capability: cap,
        config_path: Some(path),
    });
}

// ── MCP discovery ─────────────────────────────────────────────────────────

/// 发现本机 MCP 配置（`mcpServers` 格式）。
pub fn discover_mcp() -> Vec<ManagedItem> {
    let candidates = [
        pi_project_dir().map(|d| d.join("mcp.json")),
        pi_agent_dir().map(|d| d.join("mcp.json")),
    ];

    let mut items: Vec<ManagedItem> = Vec::new();
    for path_opt in candidates {
        let Some(path) = path_opt else { continue };
        if !path.is_file() {
            continue;
        }
        match parse_mcp_json(&path) {
            Ok(mut parsed) => {
                let seen: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
                parsed.retain(|i| !seen.contains(&i.id));
                items.append(&mut parsed);
            }
            Err(_) => {
                items.push(ManagedItem {
                    id: "__parse_error__".to_owned(),
                    display_name: format!("（解析失败：{}）", path.display()),
                    tag: None,
                    state: ItemState::Unknown,
                    capability: ItemCapability::ReadOnly,
                    config_path: Some(path),
                });
            }
        }
    }
    items
}

/// 解析 `mcp.json` 的 `mcpServers` 对象格式。
pub fn parse_mcp_json(path: &Path) -> io::Result<Vec<ManagedItem>> {
    let content = fs::read_to_string(path)?;
    let val: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let Some(servers) = val.get("mcpServers").and_then(|v| v.as_object()) else {
        return Ok(Vec::new());
    };

    let items = servers
        .iter()
        .map(|(name, cfg)| {
            let disabled = cfg
                .get("disabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let cmd = cfg
                .get("command")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            ManagedItem {
                id: name.clone(),
                display_name: name.clone(),
                tag: cmd,
                state: if disabled {
                    ItemState::Disabled
                } else {
                    ItemState::Enabled
                },
                capability: ItemCapability::ReadWrite,
                config_path: Some(path.to_path_buf()),
            }
        })
        .collect();
    Ok(items)
}

// ── Memory discovery ──────────────────────────────────────────────────────

/// 发现本机三层记忆文件。
pub fn discover_memory() -> Vec<MemoryEntry> {
    let mut entries = Vec::new();

    // Project 层：CLAUDE.md / AGENTS.md / .pi/APPEND_SYSTEM.md
    let project_candidates = project_memory_paths();
    for path in project_candidates {
        if let Ok(text) = fs::read_to_string(&path) {
            entries.push(MemoryEntry {
                layer: MemoryLayer::Project,
                key: file_name(&path),
                excerpt: truncate(text.trim(), 200),
                source_path: Some(path),
            });
        }
    }

    // User 层：~/.pi/agent/AGENTS.md 或 CLAUDE.md
    let agent_dir = pi_agent_dir();
    for name in &["AGENTS.md", "CLAUDE.md", "AGENTS.override.md"] {
        if let Some(path) = agent_dir.as_ref().map(|d| d.join(name)) {
            if let Ok(text) = fs::read_to_string(&path) {
                entries.push(MemoryEntry {
                    layer: MemoryLayer::User,
                    key: (*name).to_owned(),
                    excerpt: truncate(text.trim(), 200),
                    source_path: Some(path),
                });
            }
        }
    }

    entries
}

fn project_memory_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        for name in &["CLAUDE.md", "AGENTS.md", "AGENTS.override.md"] {
            paths.push(cwd.join(name));
        }
        paths.push(cwd.join(".pi").join("APPEND_SYSTEM.md"));
    }
    paths
}

// ── Model config discovery ────────────────────────────────────────────────

/// 读取 Pi agent 目录中的 settings.json 与 auth.json，合成 ModelConfig。
pub fn discover_model_config() -> ModelConfig {
    let agent_dir = pi_agent_dir();
    let settings_path = agent_dir.as_ref().map(|d| d.join("settings.json"));
    let auth_path = agent_dir.as_ref().map(|d| d.join("auth.json"));
    let models_path = agent_dir.as_ref().map(|d| d.join("models.json"));

    let (default_provider, default_model, found_settings) = settings_path
        .as_ref()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| {
            let prov = v
                .get("defaultProvider")
                .and_then(|x| x.as_str())
                .map(str::to_owned);
            let model = v
                .get("defaultModel")
                .and_then(|x| x.as_str())
                .map(str::to_owned);
            (prov, model, true)
        })
        .unwrap_or((None, None, false));

    let custom_endpoints = models_path
        .as_ref()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| parse_custom_endpoints(&v))
        .unwrap_or_default();

    let auth_status = detect_auth_status(auth_path.as_deref());

    ModelConfig {
        default_provider,
        default_model,
        settings_path: if found_settings { settings_path } else { None },
        custom_endpoints,
        auth_status,
        live_model: None,
        available_models: Vec::new(),
        switch_input: String::new(),
    }
}

/// 从 env 或 auth.json 存在性判断 API 密钥状态（不读密钥内容）。
fn detect_auth_status(auth_path: Option<&Path>) -> AuthStatus {
    // 环境变量优先（安全推荐方式）
    for var in &[
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "DEEPSEEK_API_KEY",
        "GEMINI_API_KEY",
        "GROQ_API_KEY",
        "PI_API_KEY",
    ] {
        if std::env::var(var).is_ok() {
            return AuthStatus::EnvVar((*var).to_owned());
        }
    }
    // auth.json 文件存在即视为"已配置"（不读内容，不展示明文）
    if let Some(path) = auth_path {
        if path.is_file() {
            return AuthStatus::FileExists(path.to_path_buf());
        }
    }
    AuthStatus::NotConfigured
}

fn parse_custom_endpoints(val: &serde_json::Value) -> Option<Vec<CustomEndpoint>> {
    let arr = val.as_array()?;
    let eps = arr
        .iter()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.to_owned();
            let base_url = item.get("baseUrl")?.as_str()?.to_owned();
            Some(CustomEndpoint { name, base_url })
        })
        .collect();
    Some(eps)
}

// ── Config write-back ─────────────────────────────────────────────────────

/// 写回 mcp.json 中 `mcpServers.<id>.disabled` 字段。
pub fn write_mcp_disabled(path: &Path, id: &str, disabled: bool) -> io::Result<()> {
    let content = fs::read_to_string(path)?;
    let mut val: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let entry = val
        .get_mut("mcpServers")
        .and_then(|s| s.get_mut(id))
        .and_then(|e| e.as_object_mut())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("mcpServers.{id} 不存在或格式不对"),
            )
        })?;

    entry.insert("disabled".to_owned(), serde_json::Value::Bool(disabled));

    let out = serde_json::to_string_pretty(&val)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, out.as_bytes())
}

/// 写回 settings.json 的 defaultProvider / defaultModel 字段。
pub fn write_settings_model(path: &Path, provider: &str, model_id: &str) -> io::Result<()> {
    let content = fs::read_to_string(path).unwrap_or_else(|_| "{}".to_owned());
    let mut val: serde_json::Value =
        serde_json::from_str(&content).unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    if let Some(obj) = val.as_object_mut() {
        obj.insert(
            "defaultProvider".to_owned(),
            serde_json::Value::String(provider.to_owned()),
        );
        obj.insert(
            "defaultModel".to_owned(),
            serde_json::Value::String(model_id.to_owned()),
        );
    }

    let out = serde_json::to_string_pretty(&val)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, out.as_bytes())
}

// ── String helpers ────────────────────────────────────────────────────────

/// 截取最多 `max` 字节（UTF-8 字符边界）并附加省略号。
pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let boundary = s
        .char_indices()
        .take_while(|(i, _)| *i < max)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(max);
    format!("{}…", &s[..boundary])
}

fn dir_stem(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_owned()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_owned()
}

// ── Unit tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let p = dir.path().join(name);
        fs::write(&p, content).unwrap();
        p
    }

    fn mcp_json(dir: &TempDir, content: &str) -> PathBuf {
        write(dir, "mcp.json", content)
    }

    // ── parse_mcp_json ────────────────────────────────────────────────────

    #[test]
    fn mcp_parse_enabled_disabled() {
        let dir = TempDir::new().unwrap();
        let path = mcp_json(
            &dir,
            r#"{"mcpServers":{"fs":{"command":"npx","disabled":false},"gh":{"command":"npx","disabled":true}}}"#,
        );
        let items = parse_mcp_json(&path).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(
            items.iter().find(|i| i.id == "fs").unwrap().state,
            ItemState::Enabled
        );
        assert_eq!(
            items.iter().find(|i| i.id == "gh").unwrap().state,
            ItemState::Disabled
        );
    }

    #[test]
    fn mcp_parse_no_mcpservers_empty() {
        let dir = TempDir::new().unwrap();
        let path = mcp_json(&dir, r#"{"servers":[]}"#);
        assert!(parse_mcp_json(&path).unwrap().is_empty());
    }

    #[test]
    fn mcp_parse_bad_json_errors() {
        let dir = TempDir::new().unwrap();
        let path = mcp_json(&dir, "not json");
        assert!(parse_mcp_json(&path).is_err());
    }

    // ── write_mcp_disabled ────────────────────────────────────────────────

    #[test]
    fn mcp_write_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = mcp_json(
            &dir,
            r#"{"mcpServers":{"s":{"command":"n","disabled":false}}}"#,
        );

        write_mcp_disabled(&path, "s", true).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["s"]["disabled"].as_bool(), Some(true));

        write_mcp_disabled(&path, "s", false).unwrap();
        let v2: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v2["mcpServers"]["s"]["disabled"].as_bool(), Some(false));
    }

    #[test]
    fn mcp_write_missing_key_errors() {
        let dir = TempDir::new().unwrap();
        let path = mcp_json(&dir, r#"{"mcpServers":{}}"#);
        assert!(write_mcp_disabled(&path, "nope", true).is_err());
    }

    // ── write_settings_model ──────────────────────────────────────────────

    #[test]
    fn settings_write_creates_fields() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{}").unwrap();

        write_settings_model(&path, "anthropic", "claude-sonnet-4-20250514").unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["defaultProvider"].as_str(), Some("anthropic"));
        assert_eq!(v["defaultModel"].as_str(), Some("claude-sonnet-4-20250514"));
    }

    #[test]
    fn settings_write_nonexistent_file_creates() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        // File doesn't exist → write_settings_model should create it
        // (read_to_string fails → defaults to "{}", then writes)
        write_settings_model(&path, "openai", "gpt-4o").unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["defaultModel"].as_str(), Some("gpt-4o"));
    }

    // ── AdminPanelState ───────────────────────────────────────────────────

    #[test]
    fn panel_defaults() {
        let panel = AdminPanelState::default();
        assert!(!panel.open);
        assert_eq!(panel.active_tab, AdminTab::Skills);
    }

    #[test]
    fn toggle_readonly_err() {
        let mut panel = AdminPanelState::default();
        panel.skill_items.push(ManagedItem {
            id: "bash".to_owned(),
            display_name: "bash".to_owned(),
            tag: None,
            state: ItemState::Enabled,
            capability: ItemCapability::ReadOnly,
            config_path: None,
        });
        assert!(panel.toggle_item(AdminTab::Skills, "bash").is_err());
    }

    #[test]
    fn toggle_readwrite_updates_state() {
        let mut panel = AdminPanelState::default();
        panel.mcp_items.push(ManagedItem {
            id: "srv".to_owned(),
            display_name: "srv".to_owned(),
            tag: None,
            state: ItemState::Enabled,
            capability: ItemCapability::ReadWrite,
            config_path: None,
        });
        panel.toggle_item(AdminTab::Mcp, "srv").unwrap();
        assert_eq!(panel.mcp_items[0].state, ItemState::Disabled);
    }

    #[test]
    fn toggle_with_file() {
        let dir = TempDir::new().unwrap();
        let path = mcp_json(
            &dir,
            r#"{"mcpServers":{"s":{"command":"n","disabled":false}}}"#,
        );
        let mut panel = AdminPanelState::default();
        panel.mcp_items.push(ManagedItem {
            id: "s".to_owned(),
            display_name: "s".to_owned(),
            tag: None,
            state: ItemState::Enabled,
            capability: ItemCapability::ReadWrite,
            config_path: Some(path.clone()),
        });
        panel.toggle_item(AdminTab::Mcp, "s").unwrap();
        assert_eq!(panel.mcp_items[0].state, ItemState::Disabled);
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["s"]["disabled"].as_bool(), Some(true));
    }

    #[test]
    fn toggle_memory_always_err() {
        let mut panel = AdminPanelState::default();
        assert!(panel.toggle_item(AdminTab::Memory, "x").is_err());
    }

    #[test]
    fn discover_all_sets_ready() {
        let mut panel = AdminPanelState::default();
        panel.discover_all();
        assert_eq!(panel.skills_state, DiscoveryState::Ready);
        assert_eq!(panel.mcp_state, DiscoveryState::Ready);
        assert_eq!(panel.plugins_state, DiscoveryState::Ready);
        assert_eq!(panel.memory_state, DiscoveryState::Ready);
        assert_eq!(panel.model_state, DiscoveryState::Ready);
        // Pi v1.4.0 无插件体系；插件条目始终为空（在 toggle_item 中直接报错）
    }

    // ── Skills collection ─────────────────────────────────────────────────

    #[test]
    fn skill_subdir_skill_md() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: my-skill\n---").unwrap();

        let mut items = Vec::new();
        collect_skill_files(dir.path(), ItemCapability::ReadOnly, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "my-skill");
    }

    #[test]
    fn skill_flat_md() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("tdd.md"), "# TDD").unwrap();
        fs::write(dir.path().join("SKILL.md"), "# root").unwrap(); // ignored

        let mut items = Vec::new();
        collect_skill_files(dir.path(), ItemCapability::ReadOnly, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "tdd");
    }

    #[test]
    fn skill_dedup() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("bash.md"), "# bash").unwrap();
        let mut items = Vec::new();
        collect_skill_files(dir.path(), ItemCapability::ReadOnly, &mut items);
        collect_skill_files(dir.path(), ItemCapability::ReadOnly, &mut items);
        assert_eq!(items.len(), 1);
    }

    // ── truncate ──────────────────────────────────────────────────────────

    #[test]
    fn truncate_short() {
        assert_eq!(truncate("hello", 100), "hello");
    }

    #[test]
    fn truncate_long() {
        let s = "a".repeat(300);
        let r = truncate(&s, 200);
        assert!(r.ends_with('…'));
        assert!(r.len() <= 205);
    }

    // ── Auth detection ────────────────────────────────────────────────────

    #[test]
    fn auth_file_exists_detected() {
        let dir = TempDir::new().unwrap();
        let auth = dir.path().join("auth.json");
        fs::write(&auth, r#"{"tokens":{}}"#).unwrap();
        // Clear env vars to avoid false positive from CI environment
        let saved: Vec<(String, String)> = [
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "DEEPSEEK_API_KEY",
            "GEMINI_API_KEY",
            "GROQ_API_KEY",
            "PI_API_KEY",
        ]
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
        .collect();
        for (k, _) in &saved {
            std::env::remove_var(k);
        }

        let status = detect_auth_status(Some(&auth));
        // Restore env vars
        for (k, v) in saved {
            std::env::set_var(k, v);
        }

        assert!(matches!(status, AuthStatus::FileExists(_)));
    }

    #[test]
    fn auth_missing_is_not_configured() {
        let dir = TempDir::new().unwrap();
        let auth = dir.path().join("auth.json"); // does not exist
        let saved: Vec<(String, String)> = [
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "DEEPSEEK_API_KEY",
            "GEMINI_API_KEY",
            "GROQ_API_KEY",
            "PI_API_KEY",
        ]
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
        .collect();
        for (k, _) in &saved {
            std::env::remove_var(k);
        }

        let status = detect_auth_status(Some(&auth));
        for (k, v) in saved {
            std::env::set_var(k, v);
        }
        assert!(matches!(status, AuthStatus::NotConfigured));
    }

    // ── Tab / layer labels ────────────────────────────────────────────────

    #[test]
    fn tab_labels() {
        assert_eq!(AdminTab::Skills.label(), "Skills");
        assert_eq!(AdminTab::Mcp.label(), "MCP");
        assert_eq!(AdminTab::Plugins.label(), "插件");
        assert_eq!(AdminTab::Memory.label(), "记忆");
        assert_eq!(AdminTab::Models.label(), "模型");
    }

    #[test]
    fn memory_layer_labels() {
        assert_eq!(MemoryLayer::Project.label(), "Project");
        assert_eq!(MemoryLayer::User.label(), "User");
        assert_eq!(MemoryLayer::Session.label(), "Session");
    }
}
