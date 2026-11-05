// SPDX-License-Identifier: Apache-2.0
//! 设置面板各分类页的渲染函数。

use super::*;

// ── Tab renderers ─────────────────────────────────────────────────────────

pub(super) const SKILLS_EMPTY_MSG: &str = "未在本机发现 Pi skills 文件\n\
    （已查：~/.pi/agent/skills/、.pi/skills/、~/.agents/skills/、.agents/skills/）\n\
    在上述目录中创建 <name>/SKILL.md 后点击「⟳ 刷新」。";

pub(super) const MCP_EMPTY_MSG: &str = "未找到 MCP 配置文件\n\
    （已查：~/.pi/agent/mcp.json、.pi/mcp.json）\n\
    创建后点击「⟳ 刷新」。";

pub(super) struct SkillsMcpUi {
    pub(super) skill_toggle: Option<String>,
    pub(super) mcp_toggle: Option<String>,
    pub(super) mcp_add: Option<(String, String)>,
    pub(super) pkg_action: InstalledAction,
}

pub(super) fn render_skills_mcp_workspace(
    ui: &mut egui::Ui,
    state: &mut AdminPanelState,
) -> SkillsMcpUi {
    ui.label(egui::RichText::new("技能开关与配置").strong().size(14.0));
    ui.label(
        egui::RichText::new("已从 mcp.json / settings.json 加载。启用的功能将自动连接。")
            .weak()
            .size(11.0),
    );
    ui.add_space(8.0);

    let mut skill_toggle = None;
    let mut mcp_toggle = None;
    let mut mcp_add = None;
    ui.columns(2, |cols| {
        cols[0].label(egui::RichText::new("技能 (Skills)").strong().size(13.0));
        cols[0].add(
            egui::TextEdit::singleline(&mut state.skill_query)
                .hint_text("搜索技能…")
                .desired_width(f32::INFINITY),
        );
        let query = state.skill_query.clone();
        skill_toggle = render_item_list(
            &mut cols[0],
            "Skills",
            &mut state.skill_items,
            SKILLS_EMPTY_MSG,
            &query,
        );

        cols[1].label(
            egui::RichText::new("MCP 服务器 (Servers)")
                .strong()
                .size(13.0),
        );
        cols[1].add(
            egui::TextEdit::singleline(&mut state.mcp_query)
                .hint_text("搜索 MCP 服务器…")
                .desired_width(f32::INFINITY),
        );
        let query = state.mcp_query.clone();
        mcp_toggle = render_item_list(
            &mut cols[1],
            "MCP 服务",
            &mut state.mcp_items,
            MCP_EMPTY_MSG,
            &query,
        );
        cols[1].add_space(8.0);
        cols[1].label(egui::RichText::new("添加服务").strong().size(12.0));
        cols[1].horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.mcp_new_id)
                    .hint_text("名称")
                    .desired_width(100.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut state.mcp_new_command)
                    .hint_text("命令，例如 npx")
                    .desired_width(ui.available_width().max(80.0)),
            );
        });
        if cols[1].button("写入 mcp.json").clicked() {
            mcp_add = Some((state.mcp_new_id.clone(), state.mcp_new_command.clone()));
        }
    });

    ui.add_space(6.0);
    if let Some(path) = mcp_config_hint() {
        ui.label(
            egui::RichText::new(format!("配置文件：{}", path.display()))
                .weak()
                .size(11.0),
        );
    }
    ui.add_space(10.0);
    let pkg_action = ext_market::render_installed_table(ui, &state.installed_packages, "", true);
    SkillsMcpUi {
        skill_toggle,
        mcp_toggle,
        mcp_add,
        pkg_action,
    }
}

pub(super) fn mcp_config_hint() -> Option<PathBuf> {
    pi_agent_dir().map(|dir| dir.join("mcp.json"))
}

pub(super) fn render_memory_workspace(
    ui: &mut egui::Ui,
    state: &mut AdminPanelState,
) -> (bool, bool, Option<PathBuf>) {
    ui.label(egui::RichText::new("记忆设置").strong().size(14.0));
    ui.label(
        egui::RichText::new("管理应用内的记忆文件、记忆片段与长期上下文存储。")
            .weak()
            .size(11.0),
    );
    ui.add_space(8.0);
    ui.add(
        egui::TextEdit::singleline(&mut state.memory_query)
            .hint_text("搜索记忆文件…")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(8.0);

    let mut save = false;
    let mut cancel = false;
    let mut open_path = None;
    let query = state.memory_query.clone();
    ui.columns(2, |cols| {
        if let Some(editor) = &mut state.memory_editor {
            let (s, c) = render_memory_editor(&mut cols[0], editor);
            save = s;
            cancel = c;
        }
        open_path = render_memory_list(
            &mut cols[0],
            &state.memory_entries,
            state.session_memory_text.as_deref(),
            state
                .memory_editor
                .as_ref()
                .map(|editor| editor.source_path.as_path()),
            &query,
        );

        render_trace_token_panel(&mut cols[1], state.memory_insights.as_ref());
    });
    (save, cancel, open_path)
}

pub(super) fn render_trace_token_panel(
    ui: &mut egui::Ui,
    insights: Option<&crate::session_insights::SessionInsights>,
) {
    crate::theme::card_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Trace / Token 面板")
                    .strong()
                    .size(13.0),
            );
            ui.label(egui::RichText::new("Beta").weak().size(10.0));
        });
        let Some(insights) = insights else {
            ui.label(
                egui::RichText::new("进入对话后，这里会显示本轮 Token 与工具耗时。")
                    .weak()
                    .size(11.0),
            );
            return;
        };
        let (tokens, trace, tps) = insights.footer_metric_parts();
        ui.label(
            egui::RichText::new(format!("{tokens}  ·  {trace}  ·  {tps}"))
                .weak()
                .size(11.0),
        );
        ui.add_space(8.0);
        let slices = insights.token_slices();
        let total: u64 = slices.iter().map(|(_, v)| *v).sum();
        let colors = [
            crate::theme::accent(),
            crate::theme::ok(),
            crate::theme::warn(),
            crate::theme::accent_soft(),
        ];
        let chart: Vec<(egui::Color32, f32)> = slices
            .iter()
            .enumerate()
            .map(|(i, (_, v))| (colors[i % colors.len()], *v as f32))
            .collect();
        ui.horizontal(|ui| {
            crate::theme::donut_chart(
                ui,
                &chart,
                if total == 0 {
                    "—".to_owned()
                } else {
                    format!("{total}")
                }
                .as_str(),
            );
            ui.vertical(|ui| {
                for (i, (label, value)) in slices.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.colored_label(colors[i % colors.len()], "●");
                        ui.label(
                            egui::RichText::new(format!("{label}  {value}"))
                                .size(11.0)
                                .color(crate::theme::text_muted()),
                        );
                    });
                }
                if slices.is_empty() {
                    ui.label(egui::RichText::new("暂无 Token 明细").weak().size(11.0));
                }
            });
        });
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("工具调用时间（本轮）")
                .strong()
                .size(12.0),
        );
        let max_ms = insights
            .trace_steps
            .iter()
            .filter_map(|s| s.duration_ms)
            .max()
            .unwrap_or(0);
        if insights.trace_steps.is_empty() {
            ui.label(egui::RichText::new("暂无工具调用").weak().size(11.0));
        } else {
            for (i, step) in insights.trace_steps.iter().enumerate() {
                crate::theme::bar_row(
                    ui,
                    &step.tool_name,
                    step.duration_ms.unwrap_or(0),
                    max_ms,
                    colors[i % colors.len()],
                );
            }
        }
    });
}

/// 渲染条目列表，返回被点击的开关 id。`query` 只过滤显示，不改数据。
pub(super) fn render_item_list(
    ui: &mut egui::Ui,
    section: &str,
    items: &mut [ManagedItem],
    empty_msg: &str,
    query: &str,
) -> Option<String> {
    ui.add_space(4.0);
    let enabled = items
        .iter()
        .filter(|item| item.state == ItemState::Enabled)
        .count();
    ui.label(
        egui::RichText::new(format!(
            "{section}    {enabled} 个启用 / {} 个",
            items.len()
        ))
        .strong()
        .size(13.0),
    );
    ui.add_space(6.0);

    if items.is_empty() {
        show_empty(ui, empty_msg);
        return None;
    }

    let needle = query.trim().to_lowercase();
    let shown: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            if needle.is_empty() {
                return true;
            }
            let tag = item.tag.as_deref().unwrap_or("");
            item.display_name.to_lowercase().contains(&needle)
                || item.id.to_lowercase().contains(&needle)
                || tag.to_lowercase().contains(&needle)
        })
        .map(|(index, _)| index)
        .collect();

    if shown.is_empty() {
        ui.label(
            egui::RichText::new("没有匹配的条目")
                .weak()
                .italics()
                .size(12.0),
        );
        return None;
    }

    let mut toggle_id: Option<String> = None;
    for index in shown {
        let item = &mut items[index];
        let is_enabled = item.state == ItemState::Enabled;
        let can_toggle = item.capability == ItemCapability::ReadWrite;
        let (state_text, state_color) = match item.state {
            ItemState::Enabled => ("已启用", crate::theme::ok()),
            ItemState::Disabled => ("已关闭", crate::theme::danger()),
            ItemState::Unknown => ("未知", crate::theme::warn()),
        };

        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .rounding(egui::Rounding::same(8.0))
            .fill(ui.visuals().extreme_bg_color)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(&item.display_name).strong().size(13.0));
                        if let Some(tag) = &item.tag {
                            ui.label(egui::RichText::new(tag).weak().monospace().size(11.0));
                        }
                        if item.capability == ItemCapability::ReadOnly {
                            ui.label(
                                egui::RichText::new("只读，没有可写的配置路径")
                                    .weak()
                                    .size(10.0),
                            );
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if can_toggle && crate::theme::toggle_switch(ui, is_enabled).clicked() {
                            toggle_id = Some(item.id.clone());
                        }
                        ui.label(
                            egui::RichText::new(state_text)
                                .color(state_color)
                                .size(12.0),
                        );
                    });
                });
            });
        ui.add_space(6.0);
    }

    toggle_id
}

/// 渲染记忆内联编辑器，返回 (save_clicked, cancel_clicked)。
pub(super) fn render_memory_editor(
    ui: &mut egui::Ui,
    editor: &mut MemoryEditorState,
) -> (bool, bool) {
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
                    .color(crate::theme::warn())
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
        ui.colored_label(crate::theme::danger(), format!("✗ {err}"));
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
pub(super) fn render_memory_list(
    ui: &mut egui::Ui,
    entries: &[MemoryEntry],
    session_text: Option<&str>,
    active_edit_path: Option<&Path>,
    query: &str,
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
        let needle = query.trim().to_ascii_lowercase();
        let layer_entries: Vec<&MemoryEntry> = entries
            .iter()
            .filter(|e| e.layer == layer)
            .filter(|e| {
                needle.is_empty()
                    || e.key.to_ascii_lowercase().contains(&needle)
                    || e.excerpt.to_ascii_lowercase().contains(&needle)
                    || e.source_path
                        .as_ref()
                        .and_then(|p| p.to_str())
                        .is_some_and(|p| p.to_ascii_lowercase().contains(&needle))
            })
            .collect();
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

/// 首次引导 / 通用设置：当前 bash 来源（内嵌 MinGit 或系统路径）。
pub fn show_shell_source(ui: &mut egui::Ui) {
    let probe = crate::bash_path::probe();
    ui.label(egui::RichText::new("当前 Shell").strong().size(13.0));
    ui.label(egui::RichText::new(format!("来源：{}", probe.source_label())).size(12.0));
    if let Some(path) = probe.bash_exe() {
        ui.label(
            egui::RichText::new(path.display().to_string())
                .monospace()
                .size(11.0),
        );
        if crate::bash_path::is_wsl_launcher(path) {
            ui.colored_label(
                crate::theme::warn(),
                "这是 WSL 启动器，不会把它当作默认 shellPath。",
            );
        }
    } else {
        ui.label(
            egui::RichText::new("未找到可用 bash。便携包应带 sidecar/bash；系统 Git 也可。")
                .weak()
                .size(12.0),
        );
    }
    ui.label(
        egui::RichText::new("不会使用 C:\\Windows\\System32\\bash.exe（WSL）。")
            .weak()
            .size(11.0),
    );
}

/// 自行指定 Pi 程序的三种方式。
///
/// 只放在折叠的高级区里，不作为首次打开的主界面。
pub fn show_pi_binary_recovery(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new("改完任意一项后，点窗口底部的「重新探测」。")
            .weak()
            .size(12.0),
    );
    ui.add_space(8.0);

    ui.label(egui::RichText::new("方式 1 — 环境变量").strong().size(12.0));
    ui.label(
        egui::RichText::new("适合临时指定。设置 PI_LIGHT_PI_BIN 为 pi 或 pi.exe 的完整路径。")
            .size(12.0),
    );
    ui.label(
        egui::RichText::new("PI_LIGHT_PI_BIN=<完整路径>")
            .monospace()
            .size(11.0),
    );
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new("方式 2 — 放在程序旁边")
            .strong()
            .size(12.0),
    );
    ui.label(egui::RichText::new("把官方 Pi 解压到与 pi-light 可执行文件同级的目录：").size(12.0));
    ui.label(
        egui::RichText::new("{程序目录}/sidecar/pi/pi.exe    （Windows）")
            .monospace()
            .size(11.0),
    );
    ui.label(
        egui::RichText::new("{程序目录}/sidecar/pi/pi        （Linux / macOS）")
            .monospace()
            .size(11.0),
    );
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new("方式 3 — 系统 PATH")
            .strong()
            .size(12.0),
    );
    ui.label(egui::RichText::new("把 pi 或 pi.exe 加进系统 PATH，再点「重新探测」。").size(12.0));
    ui.add_space(10.0);

    ui.label(egui::RichText::new("Pi v1.4.0 官方压缩包：").size(12.0));
    ui.hyperlink_to(
        "pi-windows-x64.zip",
        "https://github.com/earendil-works/pi/releases/download/v1.4.0/pi-windows-x64.zip",
    );
    ui.label(
        egui::RichText::new(
            "SHA-256: 0f7256ea8bdac3af20012b94fdc1252a31cc6dc0d2f9b9eb539555d22e3e321b",
        )
        .weak()
        .monospace()
        .size(10.0),
    );
}

/// 模型配置 tab。
///
/// 返回 `(即时切换请求, 是否重启 sidecar, 写入设置的错误)`。
pub(super) fn render_models(
    ui: &mut egui::Ui,
    cfg: &mut ModelConfig,
    pi_connected: bool,
) -> (Option<SetModelRequest>, bool, Option<String>) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new("模型配置").strong().size(13.0));
    ui.add_space(6.0);

    let mut rpc_request: Option<SetModelRequest> = None;
    let mut want_restart = false;
    let mut write_error: Option<String> = None;

    // ── 当前模型 ──────────────────────────────────────────────────────────
    ui.label(egui::RichText::new("当前模型").strong().size(12.0));
    let current = {
        let spec = provider_catalog::display_model_spec(
            cfg.live_model.as_deref(),
            cfg.default_provider.as_deref(),
            cfg.default_model.as_deref(),
            &cfg.available_models,
            &cfg.custom_providers,
        );
        if spec.is_empty() {
            "尚未选择".to_owned()
        } else {
            spec
        }
    };
    let provider = cfg.default_provider.as_deref().unwrap_or("尚未设置");
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
                .color(crate::theme::ok())
                .size(11.0),
        );
    } else if cfg.settings_path.is_some() {
        ui.label(
            egui::RichText::new("  ○ 来自已保存的设置（当前未连接）")
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
            "连上 Pi 之后，这里会列出可用模型。\n也可以直接在下面填写 provider/modelId。",
        );
    } else {
        ui.label(
            egui::RichText::new("点一个模型，会填进下面的输入框。")
                .weak()
                .size(11.0),
        );
        let models = cfg.available_models.clone();
        for model in models {
            let selected = cfg.switch_input == model;
            if ui
                .selectable_label(selected, egui::RichText::new(&model).monospace().size(12.0))
                .clicked()
            {
                cfg.switch_input = model;
            }
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
                .desired_width(ui.available_width().max(120.0)),
        );
    });

    let parts: Vec<&str> = cfg.switch_input.splitn(2, '/').collect();
    let valid = parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty();
    let has_settings_path = cfg.settings_path.is_some();

    ui.horizontal(|ui| {
        // RPC 切换（需要 Pi 在线）
        if ui
            .add_enabled(valid && pi_connected, egui::Button::new("立即切换"))
            .on_hover_text("立刻让当前这次对话改用该模型。还没连上 Pi 时按钮是灰的。")
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
            "保存为默认并重启"
        } else {
            "保存为默认"
        };
        if ui
            .add_enabled(valid && has_settings_path, egui::Button::new(write_label))
            .on_hover_text(if pi_connected {
                "写入默认模型后马上重启，让新配置生效。"
            } else {
                "写入默认模型。下次启动 Pi 时生效。"
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
                Err(e) => {
                    write_error = Some(format!("保存设置失败：{e}"));
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

    // ── 自定义端点 ────────────────────────────────────────────────────────
    if !cfg.custom_providers.is_empty() {
        ui.label(
            egui::RichText::new("自定义端点（models.json）")
                .strong()
                .size(12.0),
        );
        for ep in &cfg.custom_providers {
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

    (rpc_request, want_restart, write_error)
}

pub(super) fn show_excerpt_frame(
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

pub(super) fn show_empty(ui: &mut egui::Ui, msg: &str) {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .rounding(egui::Rounding::same(6.0))
        .fill(ui.visuals().faint_bg_color)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(msg).weak().italics().size(12.0));
        });
}
