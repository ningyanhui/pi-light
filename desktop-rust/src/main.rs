// SPDX-License-Identifier: Apache-2.0
// pi-light — 内嵌 Pi 的轻量 Rust Shell

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod pi_path;

use eframe::egui;

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
        Box::new(|_cc| Ok(Box::new(PiLightApp::default()))),
    )
}

#[derive(Default)]
struct PiLightApp {
    pi_status: PiStatus,
}

#[derive(Default, PartialEq)]
enum PiStatus {
    #[default]
    NotReady,
    // Future states: Ready(PathBuf), BundledReady(PathBuf)
}

impl eframe::App for PiLightApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.heading("pi-light");
                ui.add_space(8.0);
                ui.label("内嵌 Pi 的轻量 Rust Shell");
                ui.add_space(32.0);

                self.show_pi_status(ui);
            });
        });
    }
}

impl PiLightApp {
    fn show_pi_status(&self, ui: &mut egui::Ui) {
        match &self.pi_status {
            PiStatus::NotReady => {
                ui.label(
                    egui::RichText::new("⚠ Pi 尚未就绪")
                        .color(egui::Color32::from_rgb(200, 150, 50))
                        .size(16.0),
                );
                ui.add_space(12.0);
                ui.label("请通过以下方式之一提供 Pi 二进制：");
                ui.add_space(8.0);

                let path = pi_path::resolve_pi_path();
                let searched_path = path
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(未找到)".to_string());

                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(16.0, 8.0))
                    .rounding(4.0)
                    .fill(ui.visuals().extreme_bg_color)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("方式 1 — 环境变量")
                                .strong(),
                        );
                        ui.code("PI_LIGHT_PI_BIN=<pi.exe 完整路径>");
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("方式 2 — 捆绑 sidecar（推荐）")
                                .strong(),
                        );
                        ui.label("将 Pi v1.4.0 解压至：");
                        ui.code("{exe_dir}/sidecar/pi/pi.exe");
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(format!("当前探测路径：{}", searched_path))
                                .weak(),
                        );
                    });

                ui.add_space(12.0);
                ui.hyperlink_to(
                    "下载 Pi v1.4.0 (pi-windows-x64.zip)",
                    "https://github.com/earendil-works/pi/releases/tag/v1.4.0",
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "SHA-256: 0f7256ea8bdac3af20012b94fdc1252a31cc6dc0d2f9b9eb539555d22e3e321b",
                    )
                    .weak()
                    .size(11.0),
                );
            }
        }
    }
}
