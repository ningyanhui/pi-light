// SPDX-License-Identifier: Apache-2.0
//! Agent step timeline (read / run / edit rows with inline diff) and the
//! right-hand context inspector.

use egui::{Color32, Margin, RichText, Rounding, Stroke};

use crate::icons::{self, Icon};
use crate::pi_rpc::TraceStep;
use crate::theme;
use crate::tool_step::{StepDiff, StepKind};

const DIFF_PREVIEW_LINES: usize = 12;

pub fn format_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}m{:02}s", ms / 60_000, (ms % 60_000) / 1000)
    }
}

pub fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

fn step_target(step: &TraceStep) -> String {
    step.summary
        .as_ref()
        .map(|s| s.target.clone())
        .or_else(|| step.args_preview.clone())
        .unwrap_or_default()
}

/// Header line + card listing every tool step of the current turn.
pub fn show_timeline(ui: &mut egui::Ui, steps: &[TraceStep]) {
    if steps.is_empty() {
        return;
    }
    let total_ms: u64 = steps.iter().filter_map(|s| s.duration_ms).sum();
    let running = steps.iter().any(|s| s.running);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            RichText::new(format!("{} 步", steps.len()))
                .size(11.5)
                .color(theme::text_dim()),
        );
        if total_ms > 0 {
            ui.label(
                RichText::new(format!("· {}", format_ms(total_ms)))
                    .size(11.5)
                    .color(theme::text_dim()),
            );
        }
        if running {
            ui.add(egui::Spinner::new().size(11.0).color(theme::accent_cyan()));
        }
    });
    ui.add_space(4.0);
    egui::Frame::none()
        .fill(theme::sidebar_bg())
        .stroke(Stroke::new(1.0_f32, theme::hairline()))
        .rounding(Rounding::same(theme::radius::LG))
        .inner_margin(Margin::symmetric(0.0, 2.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, step) in steps.iter().enumerate() {
                if i > 0 {
                    theme::hairline_rule(ui);
                }
                step_row(ui, step);
            }
        });
}

fn step_row(ui: &mut egui::Ui, step: &TraceStep) {
    let kind = StepKind::from_tool(&step.tool_name);
    let target = step_target(step);
    let diff = step.summary.as_ref().and_then(|s| s.diff.as_ref());
    egui::Frame::none()
        .inner_margin(Margin::symmetric(12.0, 8.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if step.running {
                    ui.add(egui::Spinner::new().size(14.0).color(theme::accent_cyan()));
                } else if step.is_error {
                    icons::icon(ui, Icon::Close, 14.0, theme::danger());
                } else {
                    icons::icon(ui, Icon::Check, 14.0, theme::ok());
                }
                ui.label(
                    RichText::new(kind.verb())
                        .size(12.5)
                        .color(theme::text_muted()),
                );
                if kind == StepKind::Other || target.is_empty() {
                    ui.label(
                        RichText::new(&step.tool_name)
                            .monospace()
                            .size(12.0)
                            .color(theme::accent_bright()),
                    );
                }
                let target_w = (ui.available_width() - 96.0).max(0.0);
                ui.scope(|ui| {
                    ui.set_max_width(target_w);
                    ui.add(
                        egui::Label::new(
                            RichText::new(&target)
                                .monospace()
                                .strong()
                                .size(12.5)
                                .color(theme::text_primary()),
                        )
                        .truncate(),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if step.running {
                        ui.label(RichText::new("运行中…").size(11.5).color(theme::text_dim()));
                    } else if let Some(d) = diff {
                        diff_stats(ui, d);
                    } else if let Some(ms) = step.duration_ms {
                        ui.label(
                            RichText::new(format_ms(ms))
                                .monospace()
                                .size(11.0)
                                .color(theme::text_dim()),
                        );
                    } else if step.is_error {
                        ui.label(RichText::new("失败").size(11.5).color(theme::danger()));
                    }
                });
            });
            if let Some(d) = diff {
                ui.add_space(6.0);
                show_diff(ui, &step.tool_call_id, &target, d);
            }
        });
}

/// `+a -b` right-to-left (call inside a right_to_left layout).
fn diff_stats(ui: &mut egui::Ui, d: &StepDiff) {
    if d.removed_total > 0 {
        ui.label(
            RichText::new(format!("-{}", d.removed_total))
                .monospace()
                .size(11.0)
                .color(theme::danger()),
        );
    }
    if d.added_total > 0 {
        ui.label(
            RichText::new(format!("+{}", d.added_total))
                .monospace()
                .size(11.0)
                .color(theme::ok()),
        );
    }
}

fn show_diff(ui: &mut egui::Ui, id: &str, path: &str, d: &StepDiff) {
    let expand_id = egui::Id::new(("step_diff_expand", id));
    let expanded = ui.data(|m| m.get_temp::<bool>(expand_id).unwrap_or(false));
    egui::Frame::none()
        .fill(theme::ink())
        .stroke(Stroke::new(1.0_f32, theme::hairline()))
        .rounding(Rounding::same(theme::radius::MD))
        .inner_margin(Margin::same(0.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::none()
                .inner_margin(Margin::symmetric(10.0, 6.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(path)
                            .monospace()
                            .size(11.5)
                            .color(theme::text_muted()),
                    );
                });
            let lines = d
                .removed
                .iter()
                .map(|l| (false, l))
                .chain(d.added.iter().map(|l| (true, l)));
            let shown: Vec<(bool, &String)> = if expanded {
                lines.collect()
            } else {
                lines.take(DIFF_PREVIEW_LINES).collect()
            };
            let total_listed = d.removed.len() + d.added.len();
            for (added, line) in &shown {
                let (sign, color) = if *added {
                    ("+", theme::ok())
                } else {
                    ("-", theme::danger())
                };
                egui::Frame::none()
                    .fill(color.gamma_multiply(0.10))
                    .inner_margin(Margin::symmetric(10.0, 1.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("{sign} {line}"))
                                    .monospace()
                                    .size(12.0)
                                    .color(if *added {
                                        Color32::from_rgb(190, 240, 214)
                                    } else {
                                        Color32::from_rgb(250, 200, 200)
                                    }),
                            )
                            .truncate(),
                        );
                    });
            }
            let hidden = total_listed - shown.len() + d.hidden_lines();
            if hidden > 0 || expanded {
                egui::Frame::none()
                    .inner_margin(Margin::symmetric(10.0, 5.0))
                    .show(ui, |ui| {
                        let label = if expanded {
                            "收起".to_owned()
                        } else {
                            format!("展开 · 还有 {hidden} 行")
                        };
                        let resp = ui.add(
                            egui::Label::new(
                                RichText::new(label)
                                    .size(11.5)
                                    .color(theme::accent_bright()),
                            )
                            .sense(egui::Sense::click()),
                        );
                        if resp.clicked() && (expanded || total_listed > shown.len()) {
                            ui.data_mut(|m| m.insert_temp(expand_id, !expanded));
                        }
                    });
            }
        });
}

/// Data shown in the right-hand inspector.
pub struct InspectorData<'a> {
    pub context_used: Option<u64>,
    pub context_window: Option<u64>,
    pub model: String,
    pub thinking: String,
    pub steps: &'a [TraceStep],
    pub tps: Option<f64>,
    pub cost: Option<f64>,
}

pub fn show_inspector(ui: &mut egui::Ui, data: &InspectorData<'_>) {
    card(ui, "上下文", |ui| {
        let used = data.context_used;
        let value = match (used, data.context_window) {
            (Some(u), Some(w)) => format!("{} / {}", format_tokens(u), format_tokens(w)),
            (Some(u), None) => format_tokens(u),
            _ => "—".to_owned(),
        };
        kv_row(ui, "Tokens", &value, theme::text_primary());
        let frac = match (used, data.context_window) {
            (Some(u), Some(w)) if w > 0 => (u as f32 / w as f32).clamp(0.0, 1.0),
            _ => 0.0,
        };
        ui.add_space(4.0);
        progress(ui, frac);
        ui.add_space(6.0);
        let model = if data.model.is_empty() {
            "未选择"
        } else {
            data.model.rsplit('/').next().unwrap_or(&data.model)
        };
        kv_row(ui, "模型", model, theme::text_primary());
        kv_row(ui, "思考强度", &data.thinking, theme::warn());
    });
    ui.add_space(10.0);
    card(ui, "本次改动", |ui| {
        let mut files: Vec<(StepKind, String, Option<&StepDiff>)> = Vec::new();
        for step in data.steps {
            let kind = StepKind::from_tool(&step.tool_name);
            if !kind.touches_file() {
                continue;
            }
            let path = step_target(step);
            if path.is_empty() {
                continue;
            }
            let diff = step.summary.as_ref().and_then(|s| s.diff.as_ref());
            if let Some(existing) = files.iter_mut().find(|f| f.1 == path) {
                if kind != StepKind::Read {
                    existing.0 = kind;
                    existing.2 = diff.or(existing.2);
                }
            } else {
                files.push((kind, path, diff));
            }
        }
        if files.is_empty() {
            ui.label(
                RichText::new("本轮还没有读写文件")
                    .size(12.0)
                    .color(theme::text_dim()),
            );
        }
        for (kind, path, diff) in files {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let icon = if kind == StepKind::Read {
                    Icon::File
                } else {
                    Icon::Edit
                };
                icons::icon(ui, icon, 13.0, theme::text_muted());
                let name = path
                    .rsplit(['/', std::path::MAIN_SEPARATOR])
                    .next()
                    .unwrap_or(&path)
                    .to_owned();
                let w = (ui.available_width() - 64.0).max(0.0);
                ui.scope(|ui| {
                    ui.set_max_width(w);
                    ui.add(
                        egui::Label::new(
                            RichText::new(name)
                                .monospace()
                                .size(12.0)
                                .color(theme::text_primary()),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&path);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    match (kind, diff) {
                        (StepKind::Read, _) => {
                            ui.label(RichText::new("已读").size(11.0).color(theme::text_dim()));
                        }
                        (_, Some(d)) => diff_stats(ui, d),
                        _ => {
                            ui.label(RichText::new("已改").size(11.0).color(theme::text_dim()));
                        }
                    }
                });
            });
        }
    });
    ui.add_space(10.0);
    card(ui, "本轮", |ui| {
        let total_ms: u64 = data.steps.iter().filter_map(|s| s.duration_ms).sum();
        kv_row(
            ui,
            "工具调用",
            &data.steps.len().to_string(),
            theme::text_primary(),
        );
        let time = if total_ms > 0 {
            format_ms(total_ms)
        } else {
            "—".to_owned()
        };
        kv_row(ui, "工具耗时", &time, theme::text_primary());
        let tps = data
            .tps
            .map(|v| format!("{v:.0} tok/s"))
            .unwrap_or_else(|| "—".to_owned());
        kv_row(ui, "生成速度", &tps, theme::text_primary());
        if let Some(cost) = data.cost {
            kv_row(ui, "费用", &format!("${cost:.4}"), theme::text_primary());
        }
    });
}

fn card(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(theme::card_bg())
        .stroke(Stroke::new(1.0_f32, theme::hairline()))
        .rounding(Rounding::same(theme::radius::LG))
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(11.5).color(theme::text_dim()));
            ui.add_space(6.0);
            add(ui);
        });
}

fn kv_row(ui: &mut egui::Ui, key: &str, value: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).size(12.0).color(theme::text_muted()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).size(12.0).strong().color(color));
        });
    });
}

fn progress(ui: &mut egui::Ui, frac: f32) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width().max(0.0), 4.0),
        egui::Sense::hover(),
    );
    let painter = ui.painter();
    painter.rect_filled(rect, Rounding::same(2.0), theme::hairline());
    if frac > 0.0 {
        let mut fill = rect;
        fill.set_width(rect.width() * frac);
        painter.rect_filled(fill, Rounding::same(2.0), theme::accent_cyan());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_durations_and_tokens() {
        assert_eq!(format_ms(850), "850ms");
        assert_eq!(format_ms(2100), "2.1s");
        assert_eq!(format_ms(125_000), "2m05s");
        assert_eq!(format_tokens(950), "950");
        assert_eq!(format_tokens(68_200), "68.2k");
        assert_eq!(format_tokens(2_000_000), "2.0M");
    }
}
