// SPDX-License-Identifier: Apache-2.0
//! 桌面端消息列表渲染（用户/助手消息、分支导航、模型 Chip）。仅供 `main.rs` 使用。

use super::*;

pub(crate) fn show_model_chip(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    current: &str,
    models: &[String],
    width: f32,
) -> Option<String> {
    let mut selected = None;
    let chip = if current.is_empty() {
        "选择模型".to_owned()
    } else {
        current.rsplit('/').next().unwrap_or(current).to_owned()
    };
    let mut choices: Vec<String> = models.to_vec();
    if !current.is_empty() && !choices.iter().any(|m| m == current) {
        choices.insert(0, current.to_owned());
    }
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.inactive.bg_stroke = egui::Stroke::new(1.0_f32, theme::hairline());
        for v in [&mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            v.rounding = egui::Rounding::same(13.0);
        }
        {
            egui::ComboBox::from_id_salt(id_salt)
                .width(width)
                .selected_text(
                    egui::RichText::new(chip)
                        .size(12.0)
                        .color(theme::text_muted()),
                )
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
        }
    });
    selected.filter(|m| m != current)
}

// ── M3′ Trace / Token UI ──────────────────────────────────────────────────

pub(crate) fn show_thinking_block(ui: &mut egui::Ui, chat: &ChatState) {
    timeline_ui::show_timeline(ui, &chat.insights.trace_steps);
}

// ── Chat message renderers ────────────────────────────────────────────────

pub(crate) enum ChatUiAction {
    Prev(NodeId),
    Next(NodeId),
    EditStart(NodeId),
    EditConfirm,
    EditCancel,
    Regen(NodeId),
    Navigate(NodeId),
}

pub(crate) fn show_branch_nav(
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
                egui::Button::new(
                    egui::RichText::new("‹")
                        .size(14.0)
                        .color(theme::text_muted()),
                )
                .frame(false)
                .min_size([20.0, 20.0].into()),
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
                egui::Button::new(
                    egui::RichText::new("›")
                        .size(14.0)
                        .color(theme::text_muted()),
                )
                .frame(false)
                .min_size([20.0, 20.0].into()),
            )
            .on_hover_text("下一条兄弟分支")
            .clicked()
        {
            action = Some(ChatUiAction::Next(id));
        }
    });
    action
}

pub(crate) fn show_user_message(
    ui: &mut egui::Ui,
    row: &pi_light::message_tree::PathRow,
    editing: bool,
    draft: &mut String,
    enabled: bool,
) -> Option<ChatUiAction> {
    let mut action = None;
    let bubble_max = (ui.available_width() * 0.78).min(720.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(14.0, 10.0))
            .rounding(egui::Rounding {
                nw: 14.0,
                ne: 14.0,
                sw: 14.0,
                se: 4.0,
            })
            .fill(theme::selected_fill())
            .show(ui, |ui| {
                ui.set_max_width(bubble_max);
                if editing {
                    ui.vertical(|ui| {
                        ui.set_width(bubble_max);
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
                                    egui::Button::new(
                                        egui::RichText::new("发送分支").color(egui::Color32::WHITE),
                                    )
                                    .fill(theme::accent()),
                                )
                                .clicked()
                            {
                                action = Some(ChatUiAction::EditConfirm);
                            }
                            if ui.button("取消").clicked() {
                                action = Some(ChatUiAction::EditCancel);
                            }
                        });
                    });
                } else {
                    ui.label(
                        egui::RichText::new(&row.text)
                            .color(theme::text_primary())
                            .size(14.0),
                    );
                }
            });
    });
    if !editing {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(
                        egui::RichText::new("编辑")
                            .size(11.5)
                            .color(theme::text_dim()),
                    )
                    .frame(false),
                )
                .on_hover_text("改写这条用户消息并分出同会话分支")
                .clicked()
            {
                action = Some(ChatUiAction::EditStart(row.id));
            }
            if let Some(nav) =
                show_branch_nav(ui, row.id, row.sibling_index, row.sibling_count, enabled)
            {
                action = Some(nav);
            }
        });
    }
    action
}

pub(crate) fn show_assistant_message(
    ui: &mut egui::Ui,
    row: &pi_light::message_tree::PathRow,
    enabled: bool,
) -> Option<ChatUiAction> {
    let mut action = None;
    // 纵向列，左对齐。left_to_right 会让 Frame 继承横向流，多块 Markdown 被排成一行。
    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        let column = ui.available_width().min(920.0);
        if column.is_finite() {
            ui.set_max_width(column);
        }
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(4.0, 6.0))
            .fill(egui::Color32::TRANSPARENT)
            .show(ui, |ui| {
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
                        ui.spacing_mut().item_spacing.x = 10.0;
                        if quiet_action(ui, "复制", true).clicked() {
                            ui.ctx().copy_text(row.text.clone());
                        }
                        if quiet_action(ui, "重新生成", enabled).clicked() {
                            action = Some(ChatUiAction::Regen(row.id));
                        }
                    });
                }
            });
    });
    action
}

pub(crate) fn quiet_action(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            egui::RichText::new(label)
                .size(11.5)
                .color(theme::text_dim()),
        )
        .frame(false),
    )
}

pub(crate) fn show_streaming_assistant(ui: &mut egui::Ui, text: &str) {
    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
        let column = ui.available_width() * 0.92;
        if column.is_finite() {
            ui.set_max_width(column);
        }
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(4.0, 6.0))
            .fill(egui::Color32::TRANSPARENT)
            .show(ui, |ui| {
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
