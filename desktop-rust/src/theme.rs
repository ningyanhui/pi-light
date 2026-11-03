// SPDX-License-Identifier: Apache-2.0
//
// pi-light UI system: matte graphite surfaces, restrained cobalt accent.

use eframe::egui::{self, Color32, Rounding, Stroke};

pub fn amber() -> Color32 {
    Color32::from_rgb(226, 174, 92)
}

pub fn accent() -> Color32 {
    Color32::from_rgb(91, 137, 255)
}

pub fn accent_bright() -> Color32 {
    Color32::from_rgb(126, 164, 255)
}

pub fn accent_soft() -> Color32 {
    Color32::from_rgb(184, 204, 255)
}

pub fn ink() -> Color32 {
    Color32::from_rgb(11, 15, 21)
}

pub fn chrome_bg() -> Color32 {
    Color32::from_rgb(15, 19, 26)
}

pub fn sidebar_bg() -> Color32 {
    Color32::from_rgb(18, 23, 31)
}

pub fn card_bg() -> Color32 {
    Color32::from_rgb(27, 34, 45)
}

pub fn hairline() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, 20)
}

pub fn text_primary() -> Color32 {
    Color32::from_rgb(231, 236, 244)
}

pub fn text_muted() -> Color32 {
    Color32::from_rgb(157, 168, 183)
}

pub fn text_dim() -> Color32 {
    Color32::from_rgb(112, 125, 143)
}

pub fn ok() -> Color32 {
    Color32::from_rgb(62, 185, 139)
}

pub fn warn() -> Color32 {
    Color32::from_rgb(220, 169, 83)
}

pub fn danger() -> Color32 {
    Color32::from_rgb(225, 105, 105)
}

pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = chrome_bg();
    visuals.panel_fill = chrome_bg();
    visuals.extreme_bg_color = card_bg();
    visuals.faint_bg_color = Color32::from_rgb(22, 29, 39);
    visuals.code_bg_color = Color32::from_rgb(17, 23, 31);
    visuals.hyperlink_color = accent_soft();
    visuals.selection.bg_fill = Color32::from_rgb(37, 56, 83);
    visuals.selection.stroke.color = accent_soft();
    visuals.window_rounding = Rounding::same(10.0);
    visuals.menu_rounding = Rounding::same(8.0);
    visuals.window_stroke = Stroke::new(1.0_f32, glass_stroke());
    visuals.widgets.noninteractive.bg_fill = card_bg();
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(29, 37, 49);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(39, 50, 66);
    visuals.widgets.active.bg_fill = Color32::from_rgb(47, 61, 80);
    visuals.widgets.open.bg_fill = Color32::from_rgb(34, 44, 59);
    for widgets in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widgets.rounding = Rounding::same(7.0);
        widgets.fg_stroke.color = Color32::from_rgb(231, 236, 244);
        widgets.bg_stroke = Stroke::new(1.0_f32, glass_stroke());
    }
    ctx.set_visuals(visuals);
}

pub fn glass_fill() -> Color32 {
    card_bg()
}

pub fn glass_strong() -> Color32 {
    sidebar_bg()
}

/// Matte neutral border for panels and controls.
pub fn glass_stroke() -> Color32 {
    hairline()
}

pub fn glass_stroke_strong() -> Color32 {
    Color32::from_rgb(58, 71, 89)
}

pub fn accent_stroke_soft() -> Color32 {
    Color32::from_rgba_unmultiplied(91, 137, 255, 92)
}

pub fn accent_stroke_think() -> Color32 {
    Color32::from_rgba_unmultiplied(91, 137, 255, 58)
}

pub fn accent_stroke_composer() -> Color32 {
    Color32::from_rgba_unmultiplied(91, 137, 255, 92)
}

pub fn glass_shadow() -> egui::Shadow {
    egui::Shadow {
        offset: egui::vec2(0.0, 3.0),
        blur: 14.0,
        spread: 0.0,
        color: Color32::from_rgba_unmultiplied(0, 0, 0, 28),
    }
}

pub fn glass_frame() -> egui::Frame {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(14.0, 11.0))
        .rounding(Rounding::same(10.0))
        .fill(glass_fill())
        .stroke(Stroke::new(1.0_f32, glass_stroke()))
}

pub fn glass_bar() -> egui::Frame {
    glass_frame().fill(glass_strong()).stroke(Stroke::NONE)
}

pub fn emphasis_frame() -> egui::Frame {
    glass_frame().stroke(Stroke::new(1.0_f32, accent_stroke_composer()))
}

pub fn think_frame() -> egui::Frame {
    glass_frame()
        .inner_margin(egui::Margin::symmetric(12.0, 9.0))
        .stroke(Stroke::new(1.0_f32, accent_stroke_think()))
}

pub fn card_frame() -> egui::Frame {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .rounding(Rounding::same(9.0))
        .fill(card_bg())
        .stroke(Stroke::NONE)
}

pub fn session_row_stroke(selected: bool) -> Stroke {
    if selected {
        Stroke::new(1.0_f32, accent_stroke_soft())
    } else {
        Stroke::NONE
    }
}

pub fn soft_card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(18.0, 16.0))
        .rounding(Rounding::same(12.0))
        .fill(card_bg())
        .stroke(Stroke::new(1.0_f32, glass_stroke()))
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
            .rounding(Rounding::same(8.0))
            .min_size(egui::vec2(120.0, 36.0)),
    )
}

pub fn ghost_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(text_primary()))
            .fill(Color32::from_rgb(32, 42, 56))
            .stroke(Stroke::new(1.0_f32, hairline()))
            .rounding(Rounding::same(8.0))
            .min_size(egui::vec2(84.0, 32.0)),
    )
}

pub fn icon_button(ui: &mut egui::Ui, icon: &str, tip: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(icon).size(14.0).color(text_muted()))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::NONE)
            .min_size(egui::vec2(30.0, 30.0)),
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
            Color32::from_rgb(39, 47, 60)
        })
        .rounding(Rounding::same(9.0))
        .min_size(egui::vec2(38.0, 36.0)),
    )
}

pub fn paint_purple_glow(_ui: &egui::Ui, _rect: egui::Rect, _expand: f32) {}

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
                    Color32::from_rgb(32, 42, 56)
                } else {
                    Color32::from_rgb(29, 37, 49)
                })
                .stroke(Stroke::new(
                    1.0_f32,
                    if selected {
                        Color32::from_rgb(58, 71, 89)
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
            Color32::from_rgb(34, 46, 63)
        } else {
            Color32::TRANSPARENT
        })
        .stroke(Stroke::new(
            1.0_f32,
            if selected {
                Color32::from_rgb(58, 71, 89)
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
                Color32::from_rgb(91, 137, 255),
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
        Color32::from_rgb(43, 53, 68)
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
            .rect_filled(rect, Rounding::same(4.0), Color32::from_rgb(32, 42, 56));
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

/// 折线几何仍保留给测试；琥珀/金 bolt 已退役，不得再作为主装饰。
pub fn bolt_polyline(origin: egui::Pos2, scale: f32) -> Vec<egui::Pos2> {
    const LINE: [[f32; 2]; 4] = [[0.72, 0.00], [0.26, 0.46], [0.54, 0.50], [0.12, 1.00]];
    LINE.into_iter()
        .map(|[x, y]| origin + egui::vec2(x * scale, y * scale))
        .collect()
}

/// 旧琥珀折线闪电：退役为空操作。主路径请用 [`paint_atmosphere`]。
pub fn paint_lightning_backdrop(_ui: &egui::Ui) {}

pub fn paint_atmosphere(_ui: &egui::Ui, _texture: &egui::TextureHandle) {}

pub fn cover_uv(tex: egui::Vec2, dest: egui::Vec2) -> egui::Rect {
    if tex.x <= 0.0 || tex.y <= 0.0 || dest.x <= 0.0 || dest.y <= 0.0 {
        return egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    }
    let tex_aspect = tex.x / tex.y;
    let dest_aspect = dest.x / dest.y;
    if tex_aspect > dest_aspect {
        let vis = dest_aspect / tex_aspect;
        let x0 = (1.0 - vis) * 0.5;
        egui::Rect::from_min_max(egui::pos2(x0, 0.0), egui::pos2(x0 + vis, 1.0))
    } else {
        let vis = tex_aspect / dest_aspect;
        let y0 = (1.0 - vis) * 0.5;
        egui::Rect::from_min_max(egui::pos2(0.0, y0), egui::pos2(1.0, y0 + vis))
    }
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

    #[test]
    fn accent_uses_cobalt_palette() {
        assert_eq!(accent(), Color32::from_rgb(91, 137, 255));
        assert_eq!(accent_bright(), Color32::from_rgb(126, 164, 255));
    }

    #[test]
    fn cover_uv_crops_wider_texture() {
        let uv = cover_uv(egui::vec2(16.0, 9.0), egui::vec2(8.0, 9.0));
        assert!(uv.min.x > 0.0);
        assert!((uv.max.x - uv.min.x - 0.5).abs() < 0.02);
    }

    #[test]
    fn default_glass_stroke_is_neutral_and_composer_uses_cobalt() {
        assert_eq!(glass_stroke(), Color32::from_rgb(43, 53, 68));
        assert_eq!(
            accent_stroke_composer(),
            Color32::from_rgba_unmultiplied(91, 137, 255, 92)
        );
    }

    #[test]
    fn unselected_session_row_has_no_stroke() {
        assert_eq!(session_row_stroke(false), Stroke::NONE);
    }

    #[test]
    fn selected_session_row_uses_soft_accent_not_solid() {
        let stroke = session_row_stroke(true);
        assert_ne!(stroke.color, accent());
        let [r, g, b, a] = stroke.color.to_srgba_unmultiplied();
        assert!((80..=110).contains(&a), "alpha={a}");
        assert!((80..=105).contains(&r), "r={r}");
        assert!((120..=150).contains(&g), "g={g}");
        assert!(b >= 240, "b={b}");
    }
}
