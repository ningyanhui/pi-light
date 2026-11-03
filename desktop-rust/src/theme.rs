// SPDX-License-Identifier: Apache-2.0
//
// 桌面壳共用外观。颜色与控件形状按验收原型自写，不引用外部素材。

use eframe::egui::{self, Color32, Rounding, Stroke};

pub fn amber() -> Color32 {
    Color32::from_rgb(232, 196, 112)
}

pub fn accent() -> Color32 {
    Color32::from_rgb(124, 77, 255)
}

pub fn accent_soft() -> Color32 {
    Color32::from_rgb(168, 148, 255)
}

pub fn ink() -> Color32 {
    Color32::from_rgb(16, 14, 22)
}

pub fn chrome_bg() -> Color32 {
    Color32::from_rgb(10, 11, 16)
}

pub fn sidebar_bg() -> Color32 {
    Color32::from_rgb(8, 8, 14)
}

pub fn card_bg() -> Color32 {
    Color32::from_rgb(16, 17, 24)
}

pub fn hairline() -> Color32 {
    Color32::from_rgb(32, 34, 46)
}

pub fn text_primary() -> Color32 {
    Color32::from_rgb(232, 234, 240)
}

pub fn text_muted() -> Color32 {
    Color32::from_rgb(140, 144, 158)
}

pub fn text_dim() -> Color32 {
    Color32::from_rgb(110, 114, 128)
}

pub fn ok() -> Color32 {
    Color32::from_rgb(80, 200, 120)
}

pub fn warn() -> Color32 {
    Color32::from_rgb(220, 160, 70)
}

pub fn danger() -> Color32 {
    Color32::from_rgb(210, 90, 80)
}

pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = chrome_bg();
    visuals.panel_fill = chrome_bg();
    visuals.extreme_bg_color = card_bg();
    visuals.faint_bg_color = Color32::from_rgb(20, 22, 30);
    visuals.code_bg_color = Color32::from_rgb(18, 20, 28);
    visuals.hyperlink_color = accent_soft();
    visuals.selection.bg_fill = Color32::from_rgb(56, 48, 88);
    visuals.selection.stroke.color = accent_soft();
    visuals.window_rounding = Rounding::same(12.0);
    visuals.menu_rounding = Rounding::same(8.0);
    visuals.widgets.noninteractive.bg_fill = card_bg();
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(18, 19, 26);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(26, 28, 38);
    visuals.widgets.active.bg_fill = Color32::from_rgb(32, 34, 46);
    visuals.widgets.open.bg_fill = Color32::from_rgb(24, 26, 36);
    for widgets in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widgets.rounding = Rounding::same(8.0);
        widgets.fg_stroke.color = Color32::from_rgb(214, 216, 224);
    }
    ctx.set_visuals(visuals);
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .rounding(Rounding::same(10.0))
        .fill(card_bg())
        .stroke(Stroke::new(1.0_f32, hairline()))
}

pub fn soft_card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(22.0, 18.0))
        .rounding(Rounding::same(12.0))
        .fill(Color32::from_rgba_unmultiplied(22, 24, 34, 236))
        .stroke(Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(124, 92, 255, 50),
        ))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui)
        })
        .inner
}

pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(Color32::WHITE).strong())
            .fill(accent())
            .rounding(Rounding::same(10.0))
            .min_size(egui::vec2(132.0, 36.0)),
    )
}

pub fn ghost_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(text_primary()))
            .fill(Color32::from_rgb(28, 30, 40))
            .stroke(Stroke::new(1.0_f32, hairline()))
            .rounding(Rounding::same(10.0))
            .min_size(egui::vec2(88.0, 32.0)),
    )
}

pub fn icon_button(ui: &mut egui::Ui, icon: &str, tip: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(icon).size(14.0).color(text_muted()))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .min_size(egui::vec2(28.0, 28.0)),
    )
    .on_hover_text(tip)
}

pub fn send_button(ui: &mut egui::Ui, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            egui::RichText::new("➤")
                .color(if enabled {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(160, 160, 170)
                })
                .size(14.0),
        )
        .fill(if enabled {
            accent()
        } else {
            Color32::from_rgb(40, 42, 54)
        })
        .rounding(Rounding::same(16.0))
        .min_size(egui::vec2(36.0, 36.0)),
    )
}

pub fn underline_tabs<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    tabs: impl IntoIterator<Item = (T, &'static str)>,
    current: &mut T,
) {
    ui.horizontal_wrapped(|ui| {
        for (tab, label) in tabs {
            let selected = *current == tab;
            let text = if selected {
                egui::RichText::new(label)
                    .strong()
                    .color(text_primary())
                    .size(13.0)
            } else {
                egui::RichText::new(label).color(text_muted()).size(13.0)
            };
            let resp = ui.add(
                egui::Button::new(text)
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::NONE),
            );
            if selected {
                let rect = resp.rect;
                ui.painter().line_segment(
                    [
                        egui::pos2(rect.left() + 2.0, rect.bottom() - 1.0),
                        egui::pos2(rect.right() - 2.0, rect.bottom() - 1.0),
                    ],
                    Stroke::new(2.0_f32, accent()),
                );
            }
            if resp.clicked() {
                *current = tab;
            }
        }
    });
}

pub fn segmented_tabs<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    tabs: impl IntoIterator<Item = (T, &'static str)>,
    current: &mut T,
) {
    let tabs: Vec<(T, &'static str)> = tabs.into_iter().collect();
    let n = tabs.len().max(1) as f32;
    let width = (ui.available_width() / n).max(120.0);
    ui.horizontal(|ui| {
        for (tab, label) in tabs {
            let selected = *current == tab;
            let resp = ui.add_sized(
                [width, 32.0],
                egui::Button::new(
                    egui::RichText::new(label)
                        .color(if selected {
                            text_primary()
                        } else {
                            text_muted()
                        })
                        .size(13.0),
                )
                .fill(if selected {
                    Color32::from_rgb(28, 30, 40)
                } else {
                    Color32::from_rgb(18, 19, 26)
                })
                .stroke(Stroke::new(
                    1.0_f32,
                    if selected {
                        Color32::from_rgb(48, 50, 64)
                    } else {
                        hairline()
                    },
                ))
                .rounding(Rounding::same(16.0)),
            );
            if resp.clicked() {
                *current = tab;
            }
        }
    });
}

pub fn filter_chip(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(12.0).color(if selected {
            text_primary()
        } else {
            text_muted()
        }))
        .fill(if selected {
            Color32::from_rgb(28, 30, 42)
        } else {
            Color32::TRANSPARENT
        })
        .stroke(Stroke::new(
            1.0_f32,
            if selected {
                Color32::from_rgb(56, 58, 74)
            } else {
                hairline()
            },
        ))
        .rounding(Rounding::same(14.0)),
    )
}

pub fn glyph_circle(ui: &mut egui::Ui, letter: char, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), 13.0, color.gamma_multiply(0.28));
    ui.painter()
        .circle_stroke(rect.center(), 13.0, Stroke::new(1.0_f32, color));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        letter.to_string(),
        egui::FontId::proportional(13.0),
        color,
    );
}

pub fn provider_glyph_color(provider_id: &str) -> Color32 {
    match provider_id {
        "openai" => Color32::from_rgb(16, 163, 127),
        "anthropic" => Color32::from_rgb(217, 119, 87),
        "google" => Color32::from_rgb(66, 133, 244),
        "deepseek" => Color32::from_rgb(78, 140, 255),
        "groq" => Color32::from_rgb(244, 162, 80),
        _ => accent_soft(),
    }
}

pub fn package_glyph_color(name: &str) -> Color32 {
    match name {
        "pi-marketplace" => Color32::from_rgb(168, 92, 220),
        "pi-mcp-adapter" => Color32::from_rgb(64, 180, 176),
        "pi-web-access" => Color32::from_rgb(80, 160, 230),
        "pi-subagents" => Color32::from_rgb(150, 120, 230),
        "pi-lens" => Color32::from_rgb(220, 180, 80),
        "pi-memory" => Color32::from_rgb(80, 140, 210),
        "billion-context" => Color32::from_rgb(70, 190, 130),
        _ => {
            let hash = name
                .bytes()
                .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
            let palette = [
                Color32::from_rgb(124, 92, 255),
                Color32::from_rgb(80, 160, 230),
                Color32::from_rgb(90, 180, 140),
                Color32::from_rgb(210, 150, 80),
                Color32::from_rgb(200, 110, 160),
            ];
            palette[(hash as usize) % palette.len()]
        }
    }
}

pub fn first_letter(name: &str) -> char {
    name.chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('P')
}

/// 简易开关。返回是否被点击翻转。
pub fn toggle_switch(ui: &mut egui::Ui, on: bool) -> egui::Response {
    let desired = egui::vec2(36.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
    let track = if on {
        accent()
    } else {
        Color32::from_rgb(48, 50, 62)
    };
    ui.painter().rect_filled(rect, Rounding::same(10.0), track);
    let knob_x = if on {
        rect.right() - 10.0
    } else {
        rect.left() + 10.0
    };
    ui.painter()
        .circle_filled(egui::pos2(knob_x, rect.center().y), 7.0, Color32::WHITE);
    resp
}

pub fn donut_chart(ui: &mut egui::Ui, slices: &[(Color32, f32)], center_label: &str) {
    let size = egui::vec2(112.0, 112.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    let center = rect.center();
    let outer = 48.0;
    let inner = 30.0;
    let total: f32 = slices.iter().map(|(_, v)| *v).sum::<f32>().max(0.001);
    let mut start = -std::f32::consts::FRAC_PI_2;
    for (color, value) in slices {
        if *value <= 0.0 {
            continue;
        }
        let sweep = (*value / total) * std::f32::consts::TAU;
        let steps = ((sweep.abs() / 0.12).ceil() as usize).clamp(4, 48);
        let mut points = Vec::with_capacity(steps * 2 + 2);
        for i in 0..=steps {
            let a = start + sweep * (i as f32 / steps as f32);
            points.push(center + egui::vec2(a.cos() * outer, a.sin() * outer));
        }
        for i in (0..=steps).rev() {
            let a = start + sweep * (i as f32 / steps as f32);
            points.push(center + egui::vec2(a.cos() * inner, a.sin() * inner));
        }
        painter.add(egui::Shape::convex_polygon(points, *color, Stroke::NONE));
        start += sweep;
    }
    painter.circle_filled(center, inner - 1.0, card_bg());
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        center_label,
        egui::FontId::proportional(12.0),
        text_primary(),
    );
}

pub fn bar_row(ui: &mut egui::Ui, label: &str, ms: u64, max_ms: u64, color: Color32) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [88.0, 16.0],
            egui::Label::new(egui::RichText::new(label).size(11.0).color(text_muted())),
        );
        let avail = (ui.available_width() - 72.0).max(40.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(avail, 10.0), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, Rounding::same(4.0), Color32::from_rgb(28, 30, 40));
        let frac = if max_ms == 0 {
            0.0
        } else {
            (ms as f32 / max_ms as f32).clamp(0.04, 1.0)
        };
        let fill =
            egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * frac, rect.height()));
        ui.painter().rect_filled(fill, Rounding::same(4.0), color);
        ui.label(
            egui::RichText::new(format!("{ms}ms"))
                .size(11.0)
                .color(text_muted()),
        );
    });
}

pub fn centered_column(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
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
pub fn bolt_polyline(origin: egui::Pos2, scale: f32) -> Vec<egui::Pos2> {
    const LINE: [[f32; 2]; 4] = [[0.72, 0.00], [0.26, 0.46], [0.54, 0.50], [0.12, 1.00]];
    LINE.into_iter()
        .map(|[x, y]| origin + egui::vec2(x * scale, y * scale))
        .collect()
}

pub fn paint_lightning_backdrop(ui: &egui::Ui) {
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
        Stroke::new(
            (scale * 0.055).clamp(8.0, 16.0),
            Color32::from_rgba_unmultiplied(244, 198, 96, 28),
        ),
    ));

    let small = 34.0;
    let small_origin = egui::pos2(rect.right() - 28.0 - small * 0.74, rect.top() + 16.0);
    painter.add(egui::Shape::line(
        bolt_polyline(small_origin, small),
        Stroke::new(2.2_f32, Color32::from_rgba_unmultiplied(255, 214, 130, 140)),
    ));

    painter.circle_filled(
        egui::pos2(rect.left() + rect.width() * 0.18, rect.top() + 40.0),
        rect.height() * 0.42,
        Color32::from_rgba_unmultiplied(124, 77, 255, 18),
    );
    painter.circle_filled(
        egui::pos2(rect.right() - 80.0, rect.center().y),
        rect.height() * 0.55,
        Color32::from_rgba_unmultiplied(80, 40, 160, 16),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_colors_are_stable() {
        assert_eq!(
            provider_glyph_color("openai"),
            Color32::from_rgb(16, 163, 127)
        );
        assert_ne!(
            package_glyph_color("pi-lens"),
            package_glyph_color("pi-memory")
        );
        assert_eq!(first_letter("pi-web-access"), 'P');
    }
}
