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

pub(super) struct SkillsPageUi {
    pub(super) toggle: Option<String>,
    pub(super) view: Option<PathBuf>,
    pub(super) pkg_action: InstalledAction,
}

pub(super) fn render_skills_page(
    ui: &mut egui::Ui,
    state: &mut AdminPanelState,
    body_h: f32,
) -> SkillsPageUi {
    let mut toggle = None;
    let mut view = None;
    let enabled = state
        .skill_items
        .iter()
        .filter(|i| i.state == ItemState::Enabled)
        .count();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "{enabled} 个启用 · 共 {} 个",
                state.skill_items.len()
            ))
            .size(12.0)
            .color(crate::theme::text_muted()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.skill_query)
                    .hint_text("搜索技能或描述…")
                    .desired_width(220.0),
            );
        });
    });
    ui.add_space(8.0);

    let pane_h = (body_h * 0.62).clamp(260.0, 540.0);
    let list_w = (ui.available_width() * 0.46).clamp(260.0, 380.0);
    let needle = state.skill_query.trim().to_lowercase();
    let ws = state.workdir.clone();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(list_w, pane_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(list_w);
                egui::ScrollArea::vertical()
                    .id_salt("skills_list")
                    .auto_shrink([false; 2])
                    .max_height(pane_h)
                    .show(ui, |ui| {
                        if state.skill_items.is_empty() {
                            show_empty(ui, SKILLS_EMPTY_MSG);
                            return;
                        }
                        for item in &state.skill_items {
                            let desc = state
                                .skill_descriptions
                                .get(&item.id)
                                .map(String::as_str)
                                .unwrap_or("");
                            if !needle.is_empty()
                                && !item.id.to_lowercase().contains(&needle)
                                && !desc.to_lowercase().contains(&needle)
                            {
                                continue;
                            }
                            let selected = matches!(
                                (&state.skill_view, &item.config_path),
                                (Some((v, _)), Some(p)) if v == p
                            );
                            let project = matches!(
                                (&ws, &item.config_path),
                                (Some(w), Some(p)) if p.starts_with(w)
                            );
                            let (fill, stroke) = if selected {
                                (
                                    crate::theme::selected_fill(),
                                    crate::theme::accent_stroke_soft(),
                                )
                            } else {
                                (crate::theme::card_bg(), crate::theme::hairline())
                            };
                            egui::Frame::none()
                                .fill(fill)
                                .stroke(egui::Stroke::new(1.0_f32, stroke))
                                .rounding(egui::Rounding::same(crate::theme::radius::MD))
                                .inner_margin(egui::Margin::symmetric(12.0, 9.0))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        let name = egui::RichText::new(&item.display_name)
                                            .strong()
                                            .size(13.0)
                                            .color(crate::theme::text_primary());
                                        if ui
                                            .add(egui::Label::new(name).sense(egui::Sense::click()))
                                            .clicked()
                                        {
                                            view = item.config_path.clone();
                                        }
                                        let (tag, color) = if project {
                                            ("项目", crate::theme::accent_cyan())
                                        } else {
                                            ("用户", crate::theme::text_muted())
                                        };
                                        crate::theme::status_pill(ui, tag, color);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let on = item.state == ItemState::Enabled;
                                                if item.capability == ItemCapability::ReadWrite
                                                    && crate::theme::toggle_switch(ui, on).clicked()
                                                {
                                                    toggle = Some(item.id.clone());
                                                }
                                            },
                                        );
                                    });
                                    let text = if desc.is_empty() {
                                        "（无描述）"
                                    } else {
                                        desc
                                    };
                                    let label = egui::RichText::new(truncate(text, 140))
                                        .size(11.5)
                                        .color(crate::theme::text_muted());
                                    if ui
                                        .add(egui::Label::new(label).sense(egui::Sense::click()))
                                        .clicked()
                                    {
                                        view = item.config_path.clone();
                                    }
                                });
                            ui.add_space(6.0);
                        }
                    });
            },
        );
        ui.add_space(8.0);
        ui.vertical(|ui| {
            egui::Frame::none()
                .fill(crate::theme::card_bg())
                .rounding(egui::Rounding::same(crate::theme::radius::MD))
                .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                .show(ui, |ui| {
                    ui.set_min_height(pane_h - 20.0);
                    ui.set_width(ui.available_width());
                    match &state.skill_view {
                        Some((path, text)) => {
                            ui.label(
                                egui::RichText::new(path.to_string_lossy())
                                    .monospace()
                                    .size(10.5)
                                    .color(crate::theme::text_muted()),
                            );
                            ui.add_space(6.0);
                            egui::ScrollArea::vertical()
                                .id_salt("skill_view")
                                .auto_shrink([false; 2])
                                .max_height(pane_h - 50.0)
                                .show(ui, |ui| {
                                    crate::markdown_ui::show_markdown(
                                        ui,
                                        strip_frontmatter(text),
                                        crate::markdown_ui::MarkdownView {
                                            streaming: false,
                                            show_role: false,
                                        },
                                    );
                                });
                        }
                        None => {
                            ui.label(
                                egui::RichText::new("点击左侧技能名称或描述，查看 SKILL.md 全文。")
                                    .color(crate::theme::text_muted()),
                            );
                        }
                    }
                });
        });
    });
    ui.add_space(14.0);
    let pkg_action = ext_market::render_installed_table(ui, &state.installed_packages, "", true);
    SkillsPageUi {
        toggle,
        view,
        pkg_action,
    }
}

pub(super) fn render_mcp_page(ui: &mut egui::Ui, state: &mut AdminPanelState) -> McpPageUi {
    let mut out = McpPageUi::default();
    let enabled = state
        .mcp_items
        .iter()
        .filter(|i| i.state == ItemState::Enabled)
        .count();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "{enabled} 个启用 · 共 {} 个",
                state.mcp_items.len()
            ))
            .size(12.0)
            .color(crate::theme::text_muted()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.mcp_query)
                    .hint_text("搜索 MCP 服务…")
                    .desired_width(220.0),
            );
        });
    });
    ui.add_space(8.0);
    if state.mcp_items.is_empty() {
        show_empty(ui, MCP_EMPTY_MSG);
    }
    let needle = state.mcp_query.trim().to_lowercase();
    for item in &state.mcp_items {
        if !needle.is_empty() && !item.id.to_lowercase().contains(&needle) {
            continue;
        }
        let detail = state.mcp_details.get(&item.id);
        let on = item.state == ItemState::Enabled;
        egui::Frame::none()
            .fill(crate::theme::card_bg())
            .stroke(egui::Stroke::new(1.0_f32, crate::theme::hairline()))
            .rounding(egui::Rounding::same(crate::theme::radius::MD))
            .inner_margin(egui::Margin::symmetric(12.0, 9.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&item.display_name)
                            .strong()
                            .size(13.0)
                            .color(crate::theme::text_primary()),
                    );
                    let (tag, color) = if on {
                        ("已启用", crate::theme::ok())
                    } else {
                        ("已关闭", crate::theme::text_muted())
                    };
                    crate::theme::status_pill(ui, tag, color);
                    let editable = item.capability == ItemCapability::ReadWrite
                        && item.id != "__parse_error__";
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !editable {
                            return;
                        }
                        if crate::theme::toggle_switch(ui, on).clicked() {
                            out.toggle = Some(item.id.clone());
                        }
                        if state.mcp_confirm_remove.as_deref() == Some(item.id.as_str()) {
                            if ui.button("取消").clicked() {
                                out.cancel_remove = true;
                            }
                            let confirm =
                                egui::RichText::new("确认删除").color(crate::theme::danger());
                            if ui.button(confirm).clicked() {
                                out.remove = Some(item.id.clone());
                            }
                        } else {
                            if crate::theme::ghost_button(ui, "删除").clicked() {
                                out.remove_request = Some(item.id.clone());
                            }
                            if crate::theme::ghost_button(ui, "编辑").clicked() {
                                out.edit = Some(item.id.clone());
                            }
                        }
                    });
                });
                if let Some(d) = detail {
                    let line = match &d.url {
                        Some(url) if d.command.is_empty() => url.clone(),
                        _ => std::iter::once(d.command.as_str())
                            .chain(d.args.iter().map(String::as_str))
                            .collect::<Vec<_>>()
                            .join(" "),
                    };
                    if !line.trim().is_empty() {
                        ui.label(
                            egui::RichText::new(truncate(&line, 160))
                                .monospace()
                                .size(11.0)
                                .color(crate::theme::text_muted()),
                        );
                    }
                    if !d.env.is_empty() {
                        let keys: Vec<&str> = d.env.keys().map(String::as_str).collect();
                        ui.label(
                            egui::RichText::new(format!("env: {}", keys.join(", ")))
                                .size(11.0)
                                .color(crate::theme::text_dim()),
                        );
                    }
                }
            });
        ui.add_space(6.0);
    }

    ui.add_space(8.0);
    egui::Frame::none()
        .fill(crate::theme::card_raised())
        .stroke(egui::Stroke::new(1.0_f32, crate::theme::hairline()))
        .rounding(egui::Rounding::same(crate::theme::radius::MD))
        .inner_margin(egui::Margin::symmetric(14.0, 12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let title = match &state.mcp_editing {
                Some(id) => format!("编辑服务：{id}"),
                None => "添加服务".to_owned(),
            };
            ui.label(egui::RichText::new(title).strong().size(13.0));
            ui.add_space(6.0);
            let editing = state.mcp_editing.is_some();
            egui::Grid::new("mcp_form")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label("名称");
                    ui.add_enabled(
                        !editing,
                        egui::TextEdit::singleline(&mut state.mcp_new_id)
                            .hint_text("例如 filesystem")
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                    ui.label("命令");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.mcp_new_command)
                            .hint_text("例如 npx")
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                    ui.label("参数");
                    ui.add(
                        egui::TextEdit::multiline(&mut state.mcp_new_args)
                            .hint_text(
                                "每行一个，例如\n-y\n@modelcontextprotocol/server-filesystem",
                            )
                            .desired_rows(3)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace),
                    );
                    ui.end_row();
                    ui.label("环境变量");
                    ui.add(
                        egui::TextEdit::multiline(&mut state.mcp_new_env)
                            .hint_text("每行 KEY=VALUE")
                            .desired_rows(2)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace),
                    );
                    ui.end_row();
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if crate::theme::primary_button(ui, "保存到 mcp.json").clicked() {
                    out.save = true;
                }
                if editing && crate::theme::ghost_button(ui, "取消编辑").clicked() {
                    out.cancel_edit = true;
                }
            });
        });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if let Some(path) = mcp_config_hint() {
            ui.label(
                egui::RichText::new(format!("配置文件：{}", path.display()))
                    .size(11.0)
                    .color(crate::theme::text_muted()),
            );
        }
        let label = if state.mcp_show_json {
            "收起 mcp.json"
        } else {
            "查看 mcp.json"
        };
        if crate::theme::ghost_button(ui, label).clicked() {
            out.toggle_json = true;
        }
    });
    if state.mcp_show_json {
        egui::Frame::none()
            .fill(crate::theme::input_bg())
            .rounding(egui::Rounding::same(crate::theme::radius::MD))
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .id_salt("mcp_json")
                    .max_height(260.0)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(&state.mcp_json_text)
                                .monospace()
                                .size(11.0),
                        );
                    });
            });
    }
    out
}

pub(super) fn mcp_config_hint() -> Option<PathBuf> {
    pi_agent_dir().map(|dir| dir.join("mcp.json"))
}

pub(super) fn render_memory_workspace(
    ui: &mut egui::Ui,
    state: &mut AdminPanelState,
    body_h: f32,
) -> (bool, bool, Option<PathBuf>) {
    use crate::memory_store::MemoryGroup;

    let mut save = false;
    let mut cancel = false;
    let mut open_path = None;
    let mut pick: Option<PathBuf> = None;
    let mut reveal: Option<PathBuf> = None;
    let pane_h = (body_h - 12.0).max(280.0);
    let list_w = 236.0;

    if state.memory_selected.is_none() {
        if let Some(first) = state.memory_files.first() {
            pick = Some(first.path.clone());
        }
    }

    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(list_w, pane_h),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(list_w);
                ui.add(
                    egui::TextEdit::singleline(&mut state.memory_query)
                        .hint_text("搜索记忆文件…")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);
                let needle = state.memory_query.trim().to_lowercase();
                egui::ScrollArea::vertical()
                    .id_salt("mem_tree")
                    .auto_shrink([false; 2])
                    .max_height(pane_h - 34.0)
                    .show(ui, |ui| {
                        for group in MemoryGroup::ALL {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(group.label())
                                    .strong()
                                    .size(12.0)
                                    .color(crate::theme::text_primary()),
                            )
                            .on_hover_text(group.hint());
                            ui.label(
                                egui::RichText::new(group.hint())
                                    .size(10.5)
                                    .color(crate::theme::text_muted()),
                            );
                            ui.add_space(2.0);
                            let mut any = false;
                            for file in state.memory_files.iter().filter(|f| {
                                f.group == group
                                    && (needle.is_empty()
                                        || f.label.to_lowercase().contains(&needle))
                            }) {
                                any = true;
                                let selected =
                                    state.memory_selected.as_deref() == Some(file.path.as_path());
                                let resp = ui
                                    .with_layout(
                                        egui::Layout::top_down_justified(egui::Align::Min),
                                        |ui| {
                                            ui.selectable_label(
                                                selected,
                                                egui::RichText::new(&file.label).size(12.0),
                                            )
                                        },
                                    )
                                    .inner
                                    .on_hover_text(file.path.display().to_string());
                                if resp.clicked() {
                                    pick = Some(file.path.clone());
                                }
                            }
                            if !any {
                                ui.label(
                                    egui::RichText::new("暂无文件")
                                        .size(11.0)
                                        .italics()
                                        .color(crate::theme::text_muted()),
                                );
                            }
                        }
                    });
            },
        );
        ui.separator();
        ui.vertical(|ui| {
            let Some(path) = state.memory_selected.clone() else {
                show_empty(
                    ui,
                    "还没有记忆文件。Pi 写入长期记忆、或每轮回复结束后，这里会自动出现。",
                );
                return;
            };
            let editing = state
                .memory_editor
                .as_ref()
                .is_some_and(|e| e.source_path == path);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(
                        path.file_name().and_then(|s| s.to_str()).unwrap_or("文件"),
                    )
                    .strong()
                    .size(14.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::icons::icon_button(
                        ui,
                        crate::icons::Icon::FolderOpen,
                        "在文件管理器中显示",
                    )
                    .clicked()
                    {
                        reveal = Some(path.clone());
                    }
                    if !editing
                        && crate::icons::icon_button(ui, crate::icons::Icon::Edit, "编辑").clicked()
                    {
                        open_path = Some(path.clone());
                    }
                });
            });
            ui.label(
                egui::RichText::new(path.to_string_lossy())
                    .monospace()
                    .size(10.5)
                    .color(crate::theme::text_muted()),
            );
            ui.add_space(6.0);
            if editing {
                if let Some(editor) = &mut state.memory_editor {
                    let (s, c) = render_memory_editor(ui, editor, pane_h - 110.0);
                    save = s;
                    cancel = c;
                }
                return;
            }
            egui::Frame::none()
                .fill(crate::theme::card_bg())
                .rounding(egui::Rounding::same(crate::theme::radius::MD))
                .inner_margin(egui::Margin::symmetric(14.0, 10.0))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("mem_view")
                        .auto_shrink([false; 2])
                        .max_height(pane_h - 80.0)
                        .show(ui, |ui| match &state.memory_view {
                            Some((p, text)) if *p == path && !text.trim().is_empty() => {
                                crate::markdown_ui::show_markdown(
                                    ui,
                                    text,
                                    crate::markdown_ui::MarkdownView {
                                        streaming: false,
                                        show_role: false,
                                    },
                                );
                            }
                            Some((p, _)) if *p == path => {
                                ui.label(egui::RichText::new("（空文件）").italics());
                            }
                            _ => {
                                ui.label("…");
                            }
                        });
                });
        });
    });

    if let Some(path) = pick {
        if state
            .memory_editor
            .as_ref()
            .is_some_and(|e| e.source_path != path)
        {
            state.memory_editor = None;
        }
        state.load_memory_view(path);
    }
    if let Some(path) = reveal {
        if let Err(e) = crate::workspace_git::reveal_in_file_manager(&path) {
            state.last_error = Some(e);
        }
    }
    (save, cancel, open_path)
}

/// 渲染记忆内联编辑器，返回 (save_clicked, cancel_clicked)。
pub(super) fn render_memory_editor(
    ui: &mut egui::Ui,
    editor: &mut MemoryEditorState,
    max_height: f32,
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
        .max_height(max_height.max(160.0))
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

pub(super) fn show_empty(ui: &mut egui::Ui, msg: &str) {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .rounding(egui::Rounding::same(6.0))
        .fill(ui.visuals().faint_bg_color)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(msg).weak().italics().size(12.0));
        });
}
