// SPDX-License-Identifier: Apache-2.0
//
// M4′+ 管理面 — Skills / MCP / 记忆 / 模型 四类，壳侧全量落地
//
// 数据来源：
//   - Pi agent 目录（默认 ~/.pi/agent/，可被 PI_CODING_AGENT_DIR 覆盖）
//   - 项目 .pi/ 目录
//   - ~/.agents/skills/ 与 .agents/skills/（Agent Skills 规范路径）
//   - 模型/API 密钥状态：settings.json + auth.json + 环境变量
//
// 落地项：
//   - Skills 开关：读写 settings.json `skills` 数组（-id 排除 / +id 加回）
//   - Skills 状态：由排除列表推导 Enabled/Disabled，无 Unknown
//   - MCP 字段对齐：官方 `enabled: false` 主字段，兼容旧 `disabled` 读取，写回删 `disabled`
//   - 记忆内联编辑：TextEdit + 保存写回 + 取消
//   - 模型切换：Pi 就绪则 set_model RPC；不就绪则写 settings.json；写后可重启 sidecar

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
    Memory,
    Models,
}

impl AdminTab {
    pub fn label(self) -> &'static str {
        match self {
            AdminTab::Skills => "Skills",
            AdminTab::Mcp => "MCP",
            AdminTab::Memory => "记忆",
            AdminTab::Models => "模型",
        }
    }
}

// ── Item types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemCapability {
    ReadWrite,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemState {
    Enabled,
    Disabled,
    /// 仅在文件损坏等真实无法推断时使用；正常路径不出现。
    Unknown,
}

/// Skills / MCP 列表中的一行条目。
#[derive(Debug, Clone)]
pub struct ManagedItem {
    pub id: String,
    pub display_name: String,
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

/// 单条记忆摘要条目。
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub layer: MemoryLayer,
    pub key: String,
    pub excerpt: String,
    pub source_path: Option<PathBuf>,
}

/// 记忆内联编辑器状态（Memory tab 中的文本编辑器）。
#[derive(Debug, Clone)]
pub struct MemoryEditorState {
    pub source_path: PathBuf,
    pub content: String,
    /// 打开时的原始内容，用于「取消」恢复和 dirty 检测。
    original_content: String,
    pub save_error: Option<String>,
}

impl MemoryEditorState {
    pub fn open(path: PathBuf, content: String) -> Self {
        Self {
            original_content: content.clone(),
            content,
            source_path: path,
            save_error: None,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.content != self.original_content
    }
}

// ── Model config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ModelConfig {
    pub default_provider: Option<String>,
    pub default_model: Option<String>,
    /// settings.json 路径（用于写回）。若 Pi agent 目录已知则始终 Some，无论文件是否存在。
    pub settings_path: Option<PathBuf>,
    pub custom_endpoints: Vec<CustomEndpoint>,
    pub auth_status: AuthStatus,
    pub live_model: Option<String>,
    pub available_models: Vec<String>,
    pub switch_input: String,
}

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
    EnvVar(String),
    FileExists(PathBuf),
    NotConfigured,
}

// ── Skills settings ───────────────────────────────────────────────────────

/// 从 settings.json 读取的 skills 排除配置。
#[derive(Debug)]
pub struct SkillsSettings {
    /// settings.json 中 `skills` 数组的所有条目（原始字符串）。
    pub exclusions: Vec<String>,
    /// 可写回的 settings.json 路径（pi_agent_dir 存在则 Some）。
    pub settings_path: Option<PathBuf>,
}

impl SkillsSettings {
    /// 判断 skill id 是否被排除（禁用）。
    pub fn is_excluded(&self, id: &str) -> bool {
        let excluded = self.exclusions.iter().any(|e| {
            if let Some(key) = e.strip_prefix('-') {
                // 匹配 id 本身，或以 /id 、\id 结尾的路径段
                key == id || key.ends_with(&format!("/{id}")) || key.ends_with(&format!("\\{id}"))
            } else {
                false
            }
        });
        if !excluded {
            return false;
        }
        // 被 +id 加回则视为启用
        !self.exclusions.iter().any(|e| {
            if let Some(key) = e.strip_prefix('+') {
                key == id || key.ends_with(&format!("/{id}")) || key.ends_with(&format!("\\{id}"))
            } else {
                false
            }
        })
    }
}

// ── Discovery state ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DiscoveryState {
    #[default]
    NotStarted,
    Ready,
    Failed(String),
}

// ── Panel return type ─────────────────────────────────────────────────────

/// `show_panel()` 的返回值，汇总所有需要 main.rs 响应的操作请求。
#[derive(Debug, Default)]
pub struct AdminPanelReturn {
    pub set_model_request: Option<SetModelRequest>,
    /// 若为 true，main.rs 应调用 `restart_sidecar()` 使配置生效。
    pub restart_sidecar: bool,
}

/// 来自 UI 的 set_model RPC 请求。
#[derive(Debug, Clone)]
pub struct SetModelRequest {
    pub provider: String,
    pub model_id: String,
}

// ── AdminPanelState ───────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct AdminPanelState {
    pub open: bool,
    pub active_tab: AdminTab,

    pub skills_state: DiscoveryState,
    pub skill_items: Vec<ManagedItem>,
    /// 当前 skills 排除配置（用于 toggle 写回）。
    pub skills_settings: Option<SkillsSettings>,

    pub mcp_state: DiscoveryState,
    pub mcp_items: Vec<ManagedItem>,

    pub memory_state: DiscoveryState,
    pub memory_entries: Vec<MemoryEntry>,
    pub session_memory_text: Option<String>,
    /// 内联记忆编辑器（None = 未打开）。
    pub memory_editor: Option<MemoryEditorState>,

    pub model_state: DiscoveryState,
    pub model_config: ModelConfig,

    pub last_error: Option<String>,
    /// 由 toggle 或模型写入触发；main.rs 消费后重启 sidecar。
    pub sidecar_restart_needed: bool,
}

impl AdminPanelState {
    // ── Discovery ─────────────────────────────────────────────────────────

    pub fn discover_all(&mut self) {
        let (items, settings) = discover_skills_with_settings();
        self.skill_items = items;
        self.skills_settings = Some(settings);
        self.skills_state = DiscoveryState::Ready;

        self.mcp_items = discover_mcp();
        self.mcp_state = DiscoveryState::Ready;

        self.memory_entries = discover_memory();
        self.memory_state = DiscoveryState::Ready;

        self.model_config = discover_model_config();
        self.model_state = DiscoveryState::Ready;
    }

    pub fn rediscover(&mut self) {
        self.skills_state = DiscoveryState::NotStarted;
        self.mcp_state = DiscoveryState::NotStarted;
        self.memory_state = DiscoveryState::NotStarted;
        self.model_state = DiscoveryState::NotStarted;
        self.memory_editor = None;
        self.discover_all();
    }

    /// 将 RPC 获得的实时模型信息更新到 model_config。
    pub fn update_live_model(&mut self, model_name: Option<String>, models: Vec<String>) {
        self.model_config.live_model = model_name;
        if !models.is_empty() {
            self.model_config.available_models = models;
        }
    }

    // ── Toggle ────────────────────────────────────────────────────────────

    pub fn toggle_item(&mut self, tab: AdminTab, id: &str) -> Result<(), String> {
        match tab {
            AdminTab::Skills => self.toggle_skill(id),
            AdminTab::Mcp => self.toggle_mcp(id),
            AdminTab::Memory | AdminTab::Models => Err("此 tab 条目不支持直接开关。".to_owned()),
        }
    }

    fn toggle_skill(&mut self, id: &str) -> Result<(), String> {
        let item = self
            .skill_items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| format!("skill 不存在：{id}"))?;

        if item.capability == ItemCapability::ReadOnly {
            return Err(format!(
                "「{}」无可写 settings.json 路径，请手动编辑配置文件。",
                item.display_name
            ));
        }

        let new_state = match item.state {
            ItemState::Enabled | ItemState::Unknown => ItemState::Disabled,
            ItemState::Disabled => ItemState::Enabled,
        };

        let settings_path = self
            .skills_settings
            .as_ref()
            .and_then(|s| s.settings_path.clone())
            .ok_or_else(|| "未找到 settings.json 写入路径".to_owned())?;

        write_skills_exclusion(&settings_path, id, new_state == ItemState::Disabled)
            .map_err(|e| format!("写入 settings.json 失败：{e}"))?;

        item.state = new_state;

        // 同步更新内存中的排除列表
        if let Some(ss) = &mut self.skills_settings {
            let excl_key = format!("-{id}");
            let incl_key = format!("+{id}");
            if new_state == ItemState::Disabled {
                ss.exclusions.retain(|e| e != &incl_key);
                if !ss.exclusions.contains(&excl_key) {
                    ss.exclusions.push(excl_key);
                }
            } else {
                ss.exclusions.retain(|e| e != &excl_key && e != &incl_key);
            }
        }

        self.sidecar_restart_needed = true;
        Ok(())
    }

    fn toggle_mcp(&mut self, id: &str) -> Result<(), String> {
        let item = self
            .mcp_items
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| format!("MCP 条目不存在：{id}"))?;

        if item.capability == ItemCapability::ReadOnly {
            return Err(format!("「{}」暂不支持 UI 开关。", item.display_name));
        }

        let new_enabled = match item.state {
            ItemState::Enabled | ItemState::Unknown => false,
            ItemState::Disabled => true,
        };

        let path = item
            .config_path
            .clone()
            .ok_or_else(|| "MCP 配置路径未知".to_owned())?;

        write_mcp_enabled(&path, id, new_enabled)
            .map_err(|e| format!("写入 mcp.json 失败：{e}"))?;

        item.state = if new_enabled {
            ItemState::Enabled
        } else {
            ItemState::Disabled
        };

        self.sidecar_restart_needed = true;
        Ok(())
    }

    // ── UI ────────────────────────────────────────────────────────────────

    /// 渲染右侧管理面板。
    ///
    /// `pi_connected`：当前是否有存活的 Pi RPC 连接（影响模型切换按钮文案）。
    pub fn show_panel(
        &mut self,
        ctx: &egui::Context,
        session_memory_hint: Option<&str>,
        pi_connected: bool,
    ) -> AdminPanelReturn {
        if !self.open {
            return AdminPanelReturn::default();
        }

        if self.skills_state == DiscoveryState::NotStarted {
            self.discover_all();
        }

        if let Some(hint) = session_memory_hint {
            self.session_memory_text = Some(hint.to_owned());
        }

        // 用局部变量收集面板内产生的操作请求，避免在 egui 闭包中同时借用 self。
        let mut model_request: Option<SetModelRequest> = None;
        let mut want_restart_from_model = false;
        // 记忆编辑器动作
        let mut memory_want_save = false;
        let mut memory_want_cancel = false;
        let mut memory_open_path: Option<PathBuf> = None;

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
                        match tab {
                            AdminTab::Skills => {
                                let toggle_id = render_item_list(
                                    ui,
                                    "Skills（本机已发现）",
                                    &mut self.skill_items,
                                    SKILLS_EMPTY_MSG,
                                );
                                if let Some(id) = toggle_id {
                                    if let Err(e) = self.toggle_skill(&id) {
                                        self.last_error = Some(e);
                                    }
                                }
                            }
                            AdminTab::Mcp => {
                                let toggle_id = render_item_list(
                                    ui,
                                    "MCP 服务",
                                    &mut self.mcp_items,
                                    MCP_EMPTY_MSG,
                                );
                                if let Some(id) = toggle_id {
                                    if let Err(e) = self.toggle_mcp(&id) {
                                        self.last_error = Some(e);
                                    }
                                }
                            }
                            AdminTab::Memory => {
                                // 编辑器区（顶部）
                                if let Some(editor) = &mut self.memory_editor {
                                    let (save, cancel) = render_memory_editor(ui, editor);
                                    memory_want_save = save;
                                    memory_want_cancel = cancel;
                                    ui.separator();
                                }

                                // 列表区
                                let open_path = render_memory_list(
                                    ui,
                                    &self.memory_entries,
                                    self.session_memory_text.as_deref(),
                                    self.memory_editor.as_ref().map(|e| e.source_path.as_path()),
                                );
                                memory_open_path = open_path;
                            }
                            AdminTab::Models => {
                                let (req, want_restart) =
                                    render_models(ui, &mut self.model_config, pi_connected);
                                model_request = req;
                                want_restart_from_model = want_restart;
                            }
                        }
                    });
            });

        // ── 应用记忆编辑器动作（在 egui 闭包外，避免借用冲突）──────────
        if memory_want_save {
            let write_result = self.memory_editor.as_ref().map(|ed| {
                (
                    ed.source_path.clone(),
                    fs::write(&ed.source_path, ed.content.as_bytes()),
                )
            });
            match write_result {
                Some((_, Ok(()))) => {
                    self.memory_entries = discover_memory();
                    self.memory_editor = None;
                }
                Some((_, Err(e))) => {
                    if let Some(ed) = &mut self.memory_editor {
                        ed.save_error = Some(format!("保存失败：{e}"));
                    }
                }
                None => {}
            }
        } else if memory_want_cancel {
            self.memory_editor = None;
        }

        if let Some(path) = memory_open_path {
            match fs::read_to_string(&path) {
                Ok(content) => {
                    self.memory_editor = Some(MemoryEditorState::open(path, content));
                }
                Err(e) => {
                    self.last_error = Some(format!("读取文件失败：{e}"));
                }
            }
        }

        let restart_sidecar =
            std::mem::take(&mut self.sidecar_restart_needed) || want_restart_from_model;

        AdminPanelReturn {
            set_model_request: model_request,
            restart_sidecar,
        }
    }
}

// ── Tab renderers ─────────────────────────────────────────────────────────

const SKILLS_EMPTY_MSG: &str = "未在本机发现 Pi skills 文件\n\
    （已查：~/.pi/agent/skills/、.pi/skills/、~/.agents/skills/、.agents/skills/）\n\
    在上述目录中创建 <name>/SKILL.md 后点击「⟳ 刷新」。";

const MCP_EMPTY_MSG: &str = "未找到 MCP 配置文件\n\
    （已查：~/.pi/agent/mcp.json、.pi/mcp.json）\n\
    创建后点击「⟳ 刷新」。";

/// 渲染条目列表，返回被点击的 toggle id。
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
                            egui::RichText::new("（只读 / 无可写路径）")
                                .weak()
                                .size(10.0),
                        )
                        .on_hover_text(
                            "未找到可写的 settings.json 路径；请手动编辑配置文件或确认 Pi agent 目录。",
                        );
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

/// 渲染记忆内联编辑器，返回 (save_clicked, cancel_clicked)。
fn render_memory_editor(ui: &mut egui::Ui, editor: &mut MemoryEditorState) -> (bool, bool) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "✏ 编辑：{}",
                editor
                    .source_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("文件")
            ))
            .strong()
            .size(13.0),
        );
        if editor.is_dirty() {
            ui.label(
                egui::RichText::new("● 已修改")
                    .color(egui::Color32::from_rgb(220, 160, 40))
                    .size(11.0),
            );
        }
    });
    ui.label(
        egui::RichText::new(editor.source_path.to_string_lossy())
            .weak()
            .monospace()
            .size(10.0),
    );

    if let Some(err) = &editor.save_error {
        ui.colored_label(egui::Color32::from_rgb(210, 80, 60), format!("✗ {err}"));
    }

    ui.add_space(4.0);
    egui::ScrollArea::vertical()
        .id_salt("mem_editor_scroll")
        .max_height(200.0)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut editor.content)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
        });

    ui.add_space(6.0);
    let mut save_clicked = false;
    let mut cancel_clicked = false;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                editor.is_dirty(),
                egui::Button::new("💾 保存").min_size([80.0, 26.0].into()),
            )
            .on_hover_text("将编辑内容写回文件")
            .clicked()
        {
            save_clicked = true;
        }
        if ui
            .button("取消")
            .on_hover_text("关闭编辑器，不保存")
            .clicked()
        {
            cancel_clicked = true;
        }
    });

    (save_clicked, cancel_clicked)
}

/// 渲染记忆列表，返回用户点击「编辑」的条目路径（None = 未点击）。
fn render_memory_list(
    ui: &mut egui::Ui,
    entries: &[MemoryEntry],
    session_text: Option<&str>,
    active_edit_path: Option<&Path>,
) -> Option<PathBuf> {
    ui.add_space(4.0);
    ui.label(egui::RichText::new("记忆文件").strong().size(13.0));
    ui.add_space(6.0);

    let mut open_path: Option<PathBuf> = None;

    for layer in [
        MemoryLayer::Project,
        MemoryLayer::User,
        MemoryLayer::Session,
    ] {
        let layer_entries: Vec<&MemoryEntry> =
            entries.iter().filter(|e| e.layer == layer).collect();
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
                show_excerpt_frame(
                    ui,
                    "会话上下文",
                    &truncate(text, 240),
                    None,
                    None,
                    &mut open_path,
                );
            }

            for entry in layer_entries {
                let is_active = entry
                    .source_path
                    .as_deref()
                    .zip(active_edit_path)
                    .map(|(a, b)| a == b)
                    .unwrap_or(false);
                show_excerpt_frame(
                    ui,
                    &entry.key,
                    &entry.excerpt,
                    entry.source_path.as_ref(),
                    if is_active {
                        None
                    } else {
                        entry.source_path.as_ref()
                    },
                    &mut open_path,
                );
                ui.add_space(3.0);
            }
        });
        ui.add_space(4.0);
    }

    open_path
}

/// 模型配置 tab。
///
/// 返回 `(Option<SetModelRequest>, restart_sidecar)`.
fn render_models(
    ui: &mut egui::Ui,
    cfg: &mut ModelConfig,
    pi_connected: bool,
) -> (Option<SetModelRequest>, bool) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new("模型配置").strong().size(13.0));
    ui.add_space(6.0);

    let mut rpc_request: Option<SetModelRequest> = None;
    let mut want_restart = false;

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

    let parts: Vec<&str> = cfg.switch_input.splitn(2, '/').collect();
    let valid = parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty();
    let has_settings_path = cfg.settings_path.is_some();

    ui.horizontal(|ui| {
        // RPC 切换（需要 Pi 在线）
        if ui
            .add_enabled(
                valid && pi_connected,
                egui::Button::new("● 通过 RPC 切换（Pi 就绪）"),
            )
            .on_hover_text("向运行中的 Pi 发送 set_model 命令；Pi 未就绪时灰显。")
            .clicked()
            && valid
            && pi_connected
        {
            rpc_request = Some(SetModelRequest {
                provider: parts[0].to_owned(),
                model_id: parts[1].to_owned(),
            });
        }
    });
    ui.horizontal(|ui| {
        // 写入 settings.json（+ 可选重启）
        let write_label = if pi_connected {
            "○ 写入 settings.json 并重启 Pi"
        } else {
            "○ 写入 settings.json（下次启动生效）"
        };
        if ui
            .add_enabled(valid && has_settings_path, egui::Button::new(write_label))
            .on_hover_text(if pi_connected {
                "写入 defaultProvider/defaultModel 后立即重启 Pi sidecar 使其生效。"
            } else {
                "写入 defaultProvider/defaultModel 到 settings.json；下次启动 Pi 时生效。"
            })
            .clicked()
            && valid
            && has_settings_path
        {
            let path = cfg.settings_path.clone().unwrap();
            match write_settings_model(&path, parts[0], parts[1]) {
                Ok(()) => {
                    cfg.default_provider = Some(parts[0].to_owned());
                    cfg.default_model = Some(parts[1].to_owned());
                    if pi_connected {
                        want_restart = true;
                    }
                }
                Err(_e) => {
                    // 错误由 show_panel 的 last_error 展示；渲染层不直接持有 self
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
                    "settings.json 期望路径：{}/settings.json",
                    d.display()
                ))
                .weak()
                .size(10.0),
            );
        }
    }

    (rpc_request, want_restart)
}

fn show_excerpt_frame(
    ui: &mut egui::Ui,
    title: &str,
    excerpt: &str,
    display_path: Option<&PathBuf>,
    edit_path: Option<&PathBuf>,
    open_path_out: &mut Option<PathBuf>,
) {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
        .rounding(egui::Rounding::same(4.0))
        .fill(ui.visuals().extreme_bg_color)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).strong().size(12.0));
                if let Some(p) = display_path {
                    ui.label(
                        egui::RichText::new(format!("({})", p.to_string_lossy()))
                            .weak()
                            .monospace()
                            .size(10.0),
                    );
                }
                if let Some(p) = edit_path {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("✏")
                            .on_hover_text("在面板内编辑此文件")
                            .clicked()
                        {
                            *open_path_out = Some(p.clone());
                        }
                    });
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

// ── Skills settings ───────────────────────────────────────────────────────

/// 加载 skills 排除配置，优先用户级 settings.json，其次项目级。
pub fn load_skills_settings() -> SkillsSettings {
    let candidates = [
        pi_agent_dir().map(|d| d.join("settings.json")),
        pi_project_dir().map(|d| d.join("settings.json")),
    ];

    for path_opt in candidates.iter().flatten() {
        if let Ok(content) = fs::read_to_string(path_opt) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                let exclusions = val
                    .get("skills")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                return SkillsSettings {
                    exclusions,
                    settings_path: Some(path_opt.clone()),
                };
            }
        }
    }

    // 未找到 settings.json；仍提供写入路径以便 toggle 时创建文件
    SkillsSettings {
        exclusions: Vec::new(),
        settings_path: pi_agent_dir().map(|d| d.join("settings.json")),
    }
}

/// 写回 settings.json 的 `skills` 数组（`-id` 排除 / 移除 `-id` 启用）。
pub fn write_skills_exclusion(path: &Path, skill_id: &str, exclude: bool) -> io::Result<()> {
    let content = fs::read_to_string(path).unwrap_or_else(|_| "{}".to_owned());
    let mut val: serde_json::Value =
        serde_json::from_str(&content).unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    let obj = val
        .as_object_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "settings.json 非对象"))?;

    let skills = obj
        .entry("skills")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));

    let arr = skills
        .as_array_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "skills 字段非数组"))?;

    let exclude_key = format!("-{skill_id}");
    let include_key = format!("+{skill_id}");

    if exclude {
        // 移除 +id（若有），添加 -id（若不存在）
        arr.retain(|v| v.as_str() != Some(&include_key));
        if !arr.iter().any(|v| v.as_str() == Some(&exclude_key)) {
            arr.push(serde_json::Value::String(exclude_key));
        }
    } else {
        // 移除 -id / +id 条目（回到默认启用）
        arr.retain(|v| v.as_str() != Some(&exclude_key) && v.as_str() != Some(&include_key));
    }

    // 确保父目录存在
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let out = serde_json::to_string_pretty(&val)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, out.as_bytes())
}

// ── Skills discovery ──────────────────────────────────────────────────────

/// 发现本机 Pi skills 文件，同时从 settings.json 推导启用状态。
///
/// 返回 `(items, SkillsSettings)`.
pub fn discover_skills_with_settings() -> (Vec<ManagedItem>, SkillsSettings) {
    let settings = load_skills_settings();
    let has_write_path = settings.settings_path.is_some();

    let dirs: &[(Option<PathBuf>, _)] = &[
        (pi_project_dir().map(|d| d.join("skills")), ()),
        (
            std::env::current_dir()
                .ok()
                .map(|d| d.join(".agents").join("skills")),
            (),
        ),
        (pi_agent_dir().map(|d| d.join("skills")), ()),
        (home_dir().map(|h| h.join(".agents").join("skills")), ()),
    ];

    let mut items: Vec<ManagedItem> = Vec::new();
    for (dir_opt, _) in dirs {
        let Some(dir) = dir_opt else { continue };
        if dir.is_dir() {
            collect_skill_files(dir, has_write_path, &settings, &mut items);
        }
    }
    (items, settings)
}

/// 内部辅助：保持向后兼容（测试用）。
pub fn discover_skills() -> Vec<ManagedItem> {
    discover_skills_with_settings().0
}

fn collect_skill_files(
    dir: &Path,
    has_write_path: bool,
    settings: &SkillsSettings,
    out: &mut Vec<ManagedItem>,
) {
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
                push_skill_with_settings(out, name, category, has_write_path, settings, skill_md);
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
            push_skill_with_settings(out, name, None, has_write_path, settings, path);
        }
    }
}

fn push_skill_with_settings(
    out: &mut Vec<ManagedItem>,
    name: String,
    tag: Option<String>,
    has_write_path: bool,
    settings: &SkillsSettings,
    path: PathBuf,
) {
    if out.iter().any(|i| i.id == name) {
        return;
    }
    let state = if settings.is_excluded(&name) {
        ItemState::Disabled
    } else {
        ItemState::Enabled
    };
    let capability = if has_write_path {
        ItemCapability::ReadWrite
    } else {
        ItemCapability::ReadOnly
    };
    out.push(ManagedItem {
        id: name.clone(),
        display_name: name,
        tag,
        state,
        capability,
        config_path: Some(path),
    });
}

// ── MCP discovery ─────────────────────────────────────────────────────────

pub fn discover_mcp() -> Vec<ManagedItem> {
    let candidates = [
        pi_project_dir().map(|d| d.join("mcp.json")),
        pi_agent_dir().map(|d| d.join("mcp.json")),
    ];

    let mut items: Vec<ManagedItem> = Vec::new();
    for path_opt in candidates.iter().flatten() {
        if !path_opt.is_file() {
            continue;
        }
        match parse_mcp_json(path_opt) {
            Ok(mut parsed) => {
                let seen: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
                parsed.retain(|i| !seen.contains(&i.id));
                items.append(&mut parsed);
            }
            Err(_) => {
                items.push(ManagedItem {
                    id: "__parse_error__".to_owned(),
                    display_name: format!("（解析失败：{}）", path_opt.display()),
                    tag: None,
                    state: ItemState::Unknown,
                    capability: ItemCapability::ReadOnly,
                    config_path: Some(path_opt.clone()),
                });
            }
        }
    }
    items
}

/// 解析 mcp.json 的 `mcpServers` 格式。
///
/// 字段优先级：`enabled`（官方）> `!disabled`（旧版兼容）；默认视为 enabled=true。
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
            // 官方字段 `enabled`（absent = true）；旧版兼容 `disabled`
            let is_enabled = if let Some(enabled_val) = cfg.get("enabled") {
                enabled_val.as_bool().unwrap_or(true)
            } else {
                !cfg.get("disabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            };
            let cmd = cfg
                .get("command")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            ManagedItem {
                id: name.clone(),
                display_name: name.clone(),
                tag: cmd,
                state: if is_enabled {
                    ItemState::Enabled
                } else {
                    ItemState::Disabled
                },
                capability: ItemCapability::ReadWrite,
                config_path: Some(path.to_path_buf()),
            }
        })
        .collect();
    Ok(items)
}

/// 写回 mcp.json 中指定服务的 `enabled` 字段（官方）并移除旧 `disabled` 字段。
pub fn write_mcp_enabled(path: &Path, id: &str, enabled: bool) -> io::Result<()> {
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

    entry.insert("enabled".to_owned(), serde_json::Value::Bool(enabled));
    // 移除旧版 disabled 字段，保持 mcp.json 干净
    entry.remove("disabled");

    let out = serde_json::to_string_pretty(&val)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, out.as_bytes())
}

// 保持向后兼容的别名（旧测试可用 write_mcp_disabled，新代码用 write_mcp_enabled）
#[allow(dead_code)]
pub fn write_mcp_disabled(path: &Path, id: &str, disabled: bool) -> io::Result<()> {
    write_mcp_enabled(path, id, !disabled)
}

// ── Memory discovery ──────────────────────────────────────────────────────

pub fn discover_memory() -> Vec<MemoryEntry> {
    let mut entries = Vec::new();

    // Project 层
    for path in project_memory_paths() {
        if let Ok(text) = fs::read_to_string(&path) {
            entries.push(MemoryEntry {
                layer: MemoryLayer::Project,
                key: file_name(&path),
                excerpt: truncate(text.trim(), 200),
                source_path: Some(path),
            });
        }
    }

    // User 层
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

pub fn discover_model_config() -> ModelConfig {
    let agent_dir = pi_agent_dir();
    let settings_path = agent_dir.as_ref().map(|d| d.join("settings.json"));
    let auth_path = agent_dir.as_ref().map(|d| d.join("auth.json"));
    let models_path = agent_dir.as_ref().map(|d| d.join("models.json"));

    let (default_provider, default_model) = settings_path
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
            (prov, model)
        })
        .unwrap_or((None, None));

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
        // 始终提供写入路径（即使文件不存在；write_settings_model 会创建它）
        settings_path,
        custom_endpoints,
        auth_status,
        live_model: None,
        available_models: Vec::new(),
        switch_input: String::new(),
    }
}

fn detect_auth_status(auth_path: Option<&Path>) -> AuthStatus {
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

/// 写回 settings.json 的 defaultProvider / defaultModel 字段（文件不存在则创建）。
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

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let out = serde_json::to_string_pretty(&val)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, out.as_bytes())
}

// ── String helpers ────────────────────────────────────────────────────────

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

    fn write_file(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let p = dir.path().join(name);
        fs::write(&p, content).unwrap();
        p
    }

    fn mcp_json(dir: &TempDir, content: &str) -> PathBuf {
        write_file(dir, "mcp.json", content)
    }

    // ── parse_mcp_json ────────────────────────────────────────────────────

    #[test]
    fn mcp_parse_enabled_field() {
        let dir = TempDir::new().unwrap();
        let path = mcp_json(
            &dir,
            r#"{"mcpServers":{"fs":{"command":"npx","enabled":true},"gh":{"command":"npx","enabled":false}}}"#,
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
    fn mcp_parse_disabled_compat() {
        // 旧版 disabled 字段兼容读取
        let dir = TempDir::new().unwrap();
        let path = mcp_json(
            &dir,
            r#"{"mcpServers":{"fs":{"command":"npx","disabled":false},"gh":{"command":"npx","disabled":true}}}"#,
        );
        let items = parse_mcp_json(&path).unwrap();
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
    fn mcp_parse_absent_defaults_enabled() {
        // 无 enabled/disabled 字段 → 默认 Enabled
        let dir = TempDir::new().unwrap();
        let path = mcp_json(&dir, r#"{"mcpServers":{"fs":{"command":"npx"}}}"#);
        let items = parse_mcp_json(&path).unwrap();
        assert_eq!(items[0].state, ItemState::Enabled);
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

    // ── write_mcp_enabled ─────────────────────────────────────────────────

    #[test]
    fn mcp_write_enabled_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = mcp_json(
            &dir,
            r#"{"mcpServers":{"s":{"command":"n","enabled":true}}}"#,
        );

        write_mcp_enabled(&path, "s", false).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["s"]["enabled"].as_bool(), Some(false));
        // disabled 字段不应存在
        assert!(v["mcpServers"]["s"].get("disabled").is_none());

        write_mcp_enabled(&path, "s", true).unwrap();
        let v2: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v2["mcpServers"]["s"]["enabled"].as_bool(), Some(true));
    }

    #[test]
    fn mcp_write_removes_old_disabled_field() {
        let dir = TempDir::new().unwrap();
        // 文件中有旧版 disabled 字段
        let path = mcp_json(
            &dir,
            r#"{"mcpServers":{"s":{"command":"n","disabled":true}}}"#,
        );
        write_mcp_enabled(&path, "s", true).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["s"]["enabled"].as_bool(), Some(true));
        assert!(v["mcpServers"]["s"].get("disabled").is_none());
    }

    #[test]
    fn mcp_write_missing_key_errors() {
        let dir = TempDir::new().unwrap();
        let path = mcp_json(&dir, r#"{"mcpServers":{}}"#);
        assert!(write_mcp_enabled(&path, "nope", true).is_err());
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
        write_settings_model(&path, "openai", "gpt-4o").unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["defaultModel"].as_str(), Some("gpt-4o"));
    }

    // ── write_skills_exclusion ────────────────────────────────────────────

    #[test]
    fn skills_exclude_add_entry() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{}").unwrap();

        write_skills_exclusion(&path, "bash", true).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let arr = v["skills"].as_array().unwrap();
        assert!(arr.iter().any(|e| e.as_str() == Some("-bash")));
    }

    #[test]
    fn skills_enable_removes_exclusion() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"skills":["-bash"]}"#).unwrap();

        write_skills_exclusion(&path, "bash", false).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let arr = v["skills"].as_array().unwrap();
        assert!(!arr.iter().any(|e| e.as_str() == Some("-bash")));
    }

    #[test]
    fn skills_exclude_idempotent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"skills":["-bash"]}"#).unwrap();

        write_skills_exclusion(&path, "bash", true).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let arr = v["skills"].as_array().unwrap();
        assert_eq!(
            arr.iter().filter(|e| e.as_str() == Some("-bash")).count(),
            1
        );
    }

    #[test]
    fn skills_exclude_removes_include_entry() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"skills":["+bash"]}"#).unwrap();

        write_skills_exclusion(&path, "bash", true).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let arr = v["skills"].as_array().unwrap();
        assert!(!arr.iter().any(|e| e.as_str() == Some("+bash")));
        assert!(arr.iter().any(|e| e.as_str() == Some("-bash")));
    }

    // ── SkillsSettings::is_excluded ───────────────────────────────────────

    #[test]
    fn skills_settings_is_excluded() {
        let ss = SkillsSettings {
            exclusions: vec!["-bash".to_owned()],
            settings_path: None,
        };
        assert!(ss.is_excluded("bash"));
        assert!(!ss.is_excluded("python"));
    }

    #[test]
    fn skills_settings_reinclude_overrides_exclude() {
        let ss = SkillsSettings {
            exclusions: vec!["-bash".to_owned(), "+bash".to_owned()],
            settings_path: None,
        };
        assert!(!ss.is_excluded("bash"));
    }

    #[test]
    fn skills_settings_path_suffix_match() {
        let ss = SkillsSettings {
            exclusions: vec!["-~/.agents/skills/bash".to_owned()],
            settings_path: None,
        };
        assert!(ss.is_excluded("bash"));
    }

    // ── AdminPanelState ───────────────────────────────────────────────────

    #[test]
    fn panel_defaults() {
        let panel = AdminPanelState::default();
        assert!(!panel.open);
        assert_eq!(panel.active_tab, AdminTab::Skills);
    }

    #[test]
    fn toggle_readonly_skill_err() {
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
    fn toggle_skill_readwrite_updates_state() {
        let dir = TempDir::new().unwrap();
        let settings_path = dir.path().join("settings.json");
        fs::write(&settings_path, "{}").unwrap();

        let mut panel = AdminPanelState {
            skills_settings: Some(SkillsSettings {
                exclusions: vec![],
                settings_path: Some(settings_path.clone()),
            }),
            ..Default::default()
        };
        panel.skill_items.push(ManagedItem {
            id: "bash".to_owned(),
            display_name: "bash".to_owned(),
            tag: None,
            state: ItemState::Enabled,
            capability: ItemCapability::ReadWrite,
            config_path: None,
        });

        panel.toggle_item(AdminTab::Skills, "bash").unwrap();
        assert_eq!(panel.skill_items[0].state, ItemState::Disabled);
        assert!(panel.sidecar_restart_needed);

        // 验证写回
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert!(v["skills"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.as_str() == Some("-bash")));
    }

    #[test]
    fn toggle_mcp_with_file() {
        let dir = TempDir::new().unwrap();
        let path = mcp_json(
            &dir,
            r#"{"mcpServers":{"s":{"command":"n","enabled":true}}}"#,
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
        assert!(panel.sidecar_restart_needed);

        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["s"]["enabled"].as_bool(), Some(false));
        assert!(v["mcpServers"]["s"].get("disabled").is_none());
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
        assert_eq!(panel.memory_state, DiscoveryState::Ready);
        assert_eq!(panel.model_state, DiscoveryState::Ready);
    }

    // ── Skills collection ─────────────────────────────────────────────────

    #[test]
    fn skill_subdir_skill_md() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: my-skill\n---").unwrap();

        let settings = SkillsSettings {
            exclusions: vec![],
            settings_path: None,
        };
        let mut items = Vec::new();
        collect_skill_files(dir.path(), false, &settings, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "my-skill");
        assert_eq!(items[0].state, ItemState::Enabled);
    }

    #[test]
    fn skill_flat_md() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("tdd.md"), "# TDD").unwrap();
        fs::write(dir.path().join("SKILL.md"), "# root").unwrap(); // ignored

        let settings = SkillsSettings {
            exclusions: vec![],
            settings_path: None,
        };
        let mut items = Vec::new();
        collect_skill_files(dir.path(), false, &settings, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "tdd");
    }

    #[test]
    fn skill_excluded_is_disabled() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("bash.md"), "# bash").unwrap();

        let settings = SkillsSettings {
            exclusions: vec!["-bash".to_owned()],
            settings_path: None,
        };
        let mut items = Vec::new();
        collect_skill_files(dir.path(), true, &settings, &mut items);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].state, ItemState::Disabled);
        assert_eq!(items[0].capability, ItemCapability::ReadWrite);
    }

    #[test]
    fn skill_dedup() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("bash.md"), "# bash").unwrap();
        let settings = SkillsSettings {
            exclusions: vec![],
            settings_path: None,
        };
        let mut items = Vec::new();
        collect_skill_files(dir.path(), false, &settings, &mut items);
        collect_skill_files(dir.path(), false, &settings, &mut items);
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
            #[allow(deprecated)]
            std::env::remove_var(k);
        }
        let status = detect_auth_status(Some(&auth));
        for (k, v) in saved {
            #[allow(unsafe_code)]
            unsafe {
                std::env::set_var(k, v);
            }
        }
        assert!(matches!(status, AuthStatus::FileExists(_)));
    }

    #[test]
    fn auth_missing_is_not_configured() {
        let dir = TempDir::new().unwrap();
        let auth = dir.path().join("auth.json");
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
            #[allow(deprecated)]
            std::env::remove_var(k);
        }
        let status = detect_auth_status(Some(&auth));
        for (k, v) in saved {
            #[allow(unsafe_code)]
            unsafe {
                std::env::set_var(k, v);
            }
        }
        assert!(matches!(status, AuthStatus::NotConfigured));
    }

    // ── Tab / layer labels ────────────────────────────────────────────────

    #[test]
    fn tab_labels() {
        assert_eq!(AdminTab::Skills.label(), "Skills");
        assert_eq!(AdminTab::Mcp.label(), "MCP");
        assert_eq!(AdminTab::Memory.label(), "记忆");
        assert_eq!(AdminTab::Models.label(), "模型");
    }

    #[test]
    fn memory_layer_labels() {
        assert_eq!(MemoryLayer::Project.label(), "Project");
        assert_eq!(MemoryLayer::User.label(), "User");
        assert_eq!(MemoryLayer::Session.label(), "Session");
    }

    // ── MemoryEditorState ─────────────────────────────────────────────────

    #[test]
    fn memory_editor_dirty_detection() {
        let path = PathBuf::from("/tmp/test.md");
        let mut editor = MemoryEditorState::open(path, "hello".to_owned());
        assert!(!editor.is_dirty());
        editor.content.push_str(" world");
        assert!(editor.is_dirty());
    }
}
