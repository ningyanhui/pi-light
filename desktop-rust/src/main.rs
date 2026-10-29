// SPDX-License-Identifier: Apache-2.0
// pi-light — 内嵌 Pi 的轻量 Rust Shell

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod pi_path;

use eframe::egui;
use pi_path::PiProbe;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("pi-light")
            .with_inner_size([900.0, 600.0])
            .with_min_inner_size([480.0, 320.0]),
        ..Default::default()
    };

    eframe::run_native(
        "pi-light",
        options,
        Box::new(|_cc| Ok(Box::new(PiLightApp::new()))),
    )
}

// ── App state ─────────────────────────────────────────────────────────────

struct PiLightApp {
    /// 当前 Pi 就绪状态（三态）
    pi_probe: PiProbe,
}

impl PiLightApp {
    fn new() -> Self {
        Self {
            pi_probe: pi_path::probe(),
        }
    }
}

// ── eframe::App ───────────────────────────────────────────────────────────

impl eframe::App for PiLightApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── 顶部标题栏 ─────────────────────────────────────────────────────
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("pi-light");
                ui.weak("— 内嵌 Pi 的轻量 Rust Shell");

                // Pi 就绪状态指示灯（使用 is_ready() 助手）
                let dot_color = if self.pi_probe.is_ready() {
                    egui::Color32::from_rgb(80, 200, 100)
                } else {
                    egui::Color32::from_rgb(200, 90, 70)
                };
                ui.colored_label(dot_color, "●");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("⟳ 重新探测")
                        .on_hover_text("重新扫描 Pi 二进制路径，无需重启")
                        .clicked()
                    {
                        self.pi_probe = pi_path::probe();
                    }
                });
            });
        });

        // ── 主区 ────────────────────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                show_pi_status(ui, &self.pi_probe);
                ui.add_space(28.0);
                ui.label(
                    egui::RichText::new("会话聊天功能将在 M2′ 推出")
                        .weak()
                        .italics()
                        .size(12.0),
                );
            });
        });
    }
}

// ── 三态 UI 渲染 ──────────────────────────────────────────────────────────

fn show_pi_status(ui: &mut egui::Ui, probe: &PiProbe) {
    match probe {
        // ── 状态 1：内嵌 sidecar 就绪 ────────────────────────────────────
        PiProbe::BundledReady(_) => {
            show_status_badge(
                ui,
                "✓ Pi 已就绪（内嵌 sidecar / Bundled Ready）",
                egui::Color32::from_rgb(80, 180, 100),
            );
            ui.add_space(10.0);
            ui.label(egui::RichText::new("已从捆绑路径找到 Pi 二进制，可直接使用。").size(14.0));
        }

        // ── 状态 2：探测到本机安装 ────────────────────────────────────────
        PiProbe::DetectedLocal { .. } => {
            // source_label() 在此处使用，避免 dead_code 警告
            let title = format!(
                "~ Pi 就绪（本机 / Detected Local — {}）",
                probe.source_label()
            );
            show_status_badge(ui, &title, egui::Color32::from_rgb(80, 140, 210));
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(format!(
                    "通过「{}」找到 Pi，非捆绑版本。\n建议正式 Release 使用内嵌 sidecar。",
                    probe.source_label()
                ))
                .size(14.0),
            );
        }

        // ── 状态 3：未就绪 ────────────────────────────────────────────────
        PiProbe::NotReady => {
            show_not_ready(ui);
        }
    }

    // 就绪时显示路径框（path() 在此处使用，避免 dead_code 警告）
    if let Some(path) = probe.path() {
        ui.add_space(8.0);
        show_path_box(ui, &path.to_string_lossy());
    }
}

/// 渲染彩色状态徽标
fn show_status_badge(ui: &mut egui::Ui, label: &str, color: egui::Color32) {
    ui.label(egui::RichText::new(label).color(color).strong().size(17.0));
}

/// 渲染路径展示框
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

/// 渲染「未就绪（Not Ready）」指引面板
fn show_not_ready(ui: &mut egui::Ui) {
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
            // 方式 1 — 环境变量
            ui.label(egui::RichText::new("方式 1 — 环境变量（开发 / 临时优先）").strong());
            ui.code("PI_LIGHT_PI_BIN=<pi 或 pi.exe 的完整路径>");
            ui.add_space(10.0);

            // 方式 2 — 捆绑 sidecar
            ui.label(egui::RichText::new("方式 2 — 捆绑 sidecar（推荐，解压到以下目录）").strong());
            ui.code("{exe_dir}/sidecar/pi/pi.exe    (Windows)");
            ui.code("{exe_dir}/sidecar/pi/pi        (Linux / macOS)");
            ui.add_space(10.0);

            // 方式 3 — PATH
            ui.label(egui::RichText::new("方式 3 — 本机 PATH（可选探测）").strong());
            ui.label(
                egui::RichText::new(
                    "将 pi / pi.exe 加入系统 PATH，点击「⟳ 重新探测」即可自动识别。",
                )
                .weak()
                .size(12.0),
            );
        });

    ui.add_space(14.0);

    // 下载链接 + SHA256
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
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(
            "解压后将 pi.exe 放入 {exe_dir}/sidecar/pi/，或通过 PI_LIGHT_PI_BIN 指定路径。",
        )
        .weak()
        .size(12.0),
    );
}
