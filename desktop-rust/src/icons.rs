// SPDX-License-Identifier: Apache-2.0
//
// Line icons painted with egui shapes on a 24×24 grid, so they never depend on
// font glyph coverage.

use eframe::egui::{self, epaint::CubicBezierShape, Color32, Pos2, Rect, Sense, Stroke, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Folder,
    FolderOpen,
    Chat,
    File,
    Git,
    Puzzle,
    Gear,
    Plus,
    Search,
    ChevronDown,
    ChevronRight,
    Check,
    ArrowUp,
    Globe,
    Minimize,
    Maximize,
    Restore,
    Close,
    Refresh,
    Stop,
    Copy,
    Edit,
    User,
    Terminal,
    Clock,
    More,
    PanelRight,
}

struct Pen<'a> {
    painter: &'a egui::Painter,
    origin: Pos2,
    scale: f32,
    stroke: Stroke,
}

impl Pen<'_> {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        self.origin + Vec2::new(x, y) * self.scale
    }
    fn line(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.p(x, y)).collect();
        self.painter.add(egui::Shape::line(pts, self.stroke));
    }
    fn closed(&self, pts: &[(f32, f32)]) {
        let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| self.p(x, y)).collect();
        self.painter.add(egui::Shape::closed_line(pts, self.stroke));
    }
    fn circle(&self, x: f32, y: f32, r: f32) {
        self.painter
            .circle_stroke(self.p(x, y), r * self.scale, self.stroke);
    }
    fn dot(&self, x: f32, y: f32, r: f32) {
        self.painter
            .circle_filled(self.p(x, y), r * self.scale, self.stroke.color);
    }
    fn rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32) {
        let rect = Rect::from_min_size(self.p(x, y), Vec2::new(w, h) * self.scale);
        self.painter.rect_stroke(rect, r * self.scale, self.stroke);
    }
    fn curve(&self, a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) {
        self.painter.add(CubicBezierShape::from_points_stroke(
            [
                self.p(a.0, a.1),
                self.p(b.0, b.1),
                self.p(c.0, c.1),
                self.p(d.0, d.1),
            ],
            false,
            Color32::TRANSPARENT,
            self.stroke,
        ));
    }
}

/// Paint `icon` centered in `rect` (square side = min(w, h)).
pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let side = rect.width().min(rect.height());
    let scale = side / 24.0;
    let origin = rect.center() - Vec2::splat(side / 2.0);
    let pen = Pen {
        painter,
        origin,
        scale,
        stroke: Stroke::new((1.7 * scale).max(1.1), color),
    };
    match icon {
        Icon::Folder => pen.closed(&[
            (3.0, 6.5),
            (9.0, 6.5),
            (11.0, 8.5),
            (21.0, 8.5),
            (21.0, 18.5),
            (3.0, 18.5),
        ]),
        Icon::FolderOpen => {
            pen.line(&[
                (3.0, 18.5),
                (3.0, 6.5),
                (9.0, 6.5),
                (11.0, 8.5),
                (19.0, 8.5),
                (19.0, 11.0),
            ]);
            pen.closed(&[(3.0, 18.5), (6.0, 11.0), (22.0, 11.0), (19.0, 18.5)]);
        }
        Icon::Chat => pen.closed(&[
            (4.0, 5.0),
            (20.0, 5.0),
            (20.0, 16.0),
            (10.0, 16.0),
            (5.5, 20.0),
            (5.5, 16.0),
            (4.0, 16.0),
        ]),
        Icon::File => {
            pen.closed(&[
                (6.0, 3.0),
                (14.0, 3.0),
                (19.0, 8.0),
                (19.0, 21.0),
                (6.0, 21.0),
            ]);
            pen.line(&[(14.0, 3.0), (14.0, 8.0), (19.0, 8.0)]);
        }
        Icon::Git => {
            pen.circle(6.5, 5.5, 2.2);
            pen.circle(6.5, 18.5, 2.2);
            pen.circle(17.5, 8.0, 2.2);
            pen.line(&[(6.5, 7.7), (6.5, 16.3)]);
            pen.curve((17.5, 10.2), (17.5, 14.5), (11.0, 13.0), (7.8, 16.6));
        }
        Icon::Puzzle => {
            pen.rect(4.0, 4.0, 7.0, 7.0, 1.5);
            pen.rect(13.0, 4.0, 7.0, 7.0, 1.5);
            pen.rect(4.0, 13.0, 7.0, 7.0, 1.5);
            pen.line(&[(16.5, 13.0), (16.5, 20.0)]);
            pen.line(&[(13.0, 16.5), (20.0, 16.5)]);
        }
        Icon::Gear => {
            pen.circle(12.0, 12.0, 3.0);
            pen.circle(12.0, 12.0, 7.0);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let (s, c) = a.sin_cos();
                pen.line(&[
                    (12.0 + 7.0 * c, 12.0 + 7.0 * s),
                    (12.0 + 9.5 * c, 12.0 + 9.5 * s),
                ]);
            }
        }
        Icon::Plus => {
            pen.line(&[(12.0, 5.0), (12.0, 19.0)]);
            pen.line(&[(5.0, 12.0), (19.0, 12.0)]);
        }
        Icon::Search => {
            pen.circle(10.5, 10.5, 6.0);
            pen.line(&[(15.0, 15.0), (20.0, 20.0)]);
        }
        Icon::ChevronDown => pen.line(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)]),
        Icon::ChevronRight => pen.line(&[(10.0, 7.0), (15.0, 12.0), (10.0, 17.0)]),
        Icon::Check => pen.line(&[(5.0, 12.5), (10.0, 17.0), (19.0, 7.0)]),
        Icon::ArrowUp => {
            pen.line(&[(12.0, 19.0), (12.0, 5.0)]);
            pen.line(&[(6.0, 11.0), (12.0, 5.0), (18.0, 11.0)]);
        }
        Icon::Globe => {
            pen.circle(12.0, 12.0, 9.0);
            pen.line(&[(3.0, 12.0), (21.0, 12.0)]);
            pen.curve((12.0, 3.0), (16.0, 7.0), (16.0, 17.0), (12.0, 21.0));
            pen.curve((12.0, 3.0), (8.0, 7.0), (8.0, 17.0), (12.0, 21.0));
        }
        Icon::Minimize => pen.line(&[(6.0, 12.0), (18.0, 12.0)]),
        Icon::Maximize => pen.rect(6.0, 6.0, 12.0, 12.0, 1.0),
        Icon::Restore => {
            pen.rect(5.0, 8.5, 10.5, 10.5, 1.0);
            pen.line(&[
                (8.5, 8.5),
                (8.5, 5.0),
                (19.0, 5.0),
                (19.0, 15.5),
                (15.5, 15.5),
            ]);
        }
        Icon::Close => {
            pen.line(&[(6.5, 6.5), (17.5, 17.5)]);
            pen.line(&[(17.5, 6.5), (6.5, 17.5)]);
        }
        Icon::Refresh => {
            pen.curve((19.0, 12.0), (19.0, 5.0), (9.0, 2.5), (5.5, 8.5));
            pen.curve((5.0, 12.0), (5.0, 19.0), (15.0, 21.5), (18.5, 15.5));
            pen.line(&[(5.0, 4.5), (5.5, 8.5), (9.5, 8.0)]);
            pen.line(&[(19.0, 19.5), (18.5, 15.5), (14.5, 16.0)]);
        }
        Icon::Stop => {
            let r = Rect::from_min_size(pen.p(7.0, 7.0), Vec2::splat(10.0 * scale));
            painter.rect_filled(r, 2.0 * scale, color);
        }
        Icon::Copy => {
            pen.rect(8.0, 8.0, 12.0, 12.0, 2.0);
            pen.line(&[
                (16.0, 8.0),
                (16.0, 4.0),
                (4.0, 4.0),
                (4.0, 16.0),
                (8.0, 16.0),
            ]);
        }
        Icon::Edit => {
            pen.closed(&[
                (4.0, 20.0),
                (4.0, 16.0),
                (15.0, 5.0),
                (19.0, 9.0),
                (8.0, 20.0),
            ]);
            pen.line(&[(12.5, 7.5), (16.5, 11.5)]);
        }
        Icon::User => {
            pen.circle(12.0, 8.0, 4.0);
            pen.curve((4.5, 20.5), (5.5, 14.5), (18.5, 14.5), (19.5, 20.5));
        }
        Icon::Terminal => {
            pen.line(&[(5.0, 7.0), (10.0, 12.0), (5.0, 17.0)]);
            pen.line(&[(12.0, 17.0), (19.0, 17.0)]);
        }
        Icon::Clock => {
            pen.circle(12.0, 12.0, 8.5);
            pen.line(&[(12.0, 7.0), (12.0, 12.0), (15.5, 14.0)]);
            pen.dot(12.0, 12.0, 0.6);
        }
        Icon::More => {
            pen.dot(6.0, 12.0, 1.4);
            pen.dot(12.0, 12.0, 1.4);
            pen.dot(18.0, 12.0, 1.4);
        }
        Icon::PanelRight => {
            pen.rect(3.5, 4.5, 17.0, 15.0, 2.5);
            pen.line(&[(14.5, 4.5), (14.5, 19.5)]);
        }
    }
}

/// Allocate `size`×`size` and paint the icon (no interaction).
pub fn icon(ui: &mut egui::Ui, icon: Icon, size: f32, color: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint(ui.painter(), rect, icon, color);
    resp
}

/// Square frameless icon button with hover fill.
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    icon_button_sized(ui, icon, 28.0, 16.0, tip, false)
}

pub fn icon_button_sized(
    ui: &mut egui::Ui,
    icon: Icon,
    box_size: f32,
    icon_size: f32,
    tip: &str,
    active: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(box_size), Sense::click());
    let hovered = resp.hovered();
    if active || hovered {
        let fill = if active {
            crate::theme::card_bg()
        } else {
            crate::theme::hover_fill()
        };
        ui.painter().rect_filled(rect, 8.0, fill);
    }
    let color = if active || hovered {
        crate::theme::text_primary()
    } else {
        crate::theme::text_muted()
    };
    paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(icon_size)),
        icon,
        color,
    );
    if tip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tip)
    }
}
