// SPDX-License-Identifier: Apache-2.0
//! 全屏状态页：模型引导、Pi 缺失、连接断开、启动失败。

use super::*;

impl PiLightApp {
    pub(crate) fn show_model_onboarding(&mut self, ctx: &egui::Context) {
        let spawn_note = self.spawn_error.clone();
        let mut dismiss = false;
        let mut focus_form = false;
        let mut form_result = provider_catalog::ModelsTabResult::default();
        let configured = self
            .admin
            .api_keys
            .rows
            .iter()
            .any(|row| !matches!(row.presence, pi_light::api_keys::KeyPresence::Absent));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::chrome_bg()))
            .show(ctx, |ui| {
                self.paint_main_backdrop(ui);
                let half = (ui.available_width() * 0.5).max(320.0);
                ui.columns(2, |cols| {
                    cols[0].vertical_centered(|ui| {
                        ui.set_min_width((half - 24.0).max(280.0));
                        ui.add_space(48.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("⚡").size(22.0).color(theme::accent()));
                            ui.label(
                                egui::RichText::new("PI-LIGHT")
                                    .strong()
                                    .size(13.0)
                                    .color(theme::text_muted()),
                            );
                        });
                        ui.label(
                            egui::RichText::new("欢迎使用 Pi-Light")
                                .strong()
                                .size(28.0)
                                .color(egui::Color32::WHITE),
                        );
                        ui.label(
                            egui::RichText::new("配置模型后，即可开始智能开发之旅")
                                .weak()
                                .size(14.0),
                        );
                        ui.add_space(20.0);
                        theme::soft_card(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("⚡")
                                        .size(16.0)
                                        .color(theme::accent()),
                                );
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new("配置模型").strong().size(15.0),
                                    );
                                    ui.label(
                                        egui::RichText::new(
                                            "选择或添加 AI 模型提供商，开始使用 Pi-Light",
                                        )
                                        .weak()
                                        .size(12.0),
                                    );
                                });
                            });
                        });
                        ui.add_space(10.0);
                        theme::soft_card(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("▢").size(16.0).color(theme::text_muted()));
                                ui.vertical(|ui| {
                                    ui.label(
                                        egui::RichText::new(if configured {
                                            "已有可用提供商"
                                        } else {
                                            "暂无可用模型"
                                        })
                                        .strong()
                                        .size(14.0),
                                    );
                                    ui.label(
                                        egui::RichText::new(
                                            "添加模型提供商以开始对话与开发任务。密钥保存后只显示掩码。",
                                        )
                                        .weak()
                                        .size(12.0),
                                    );
                                });
                            });
                        });
                        if let Some(note) = &spawn_note {
                            ui.add_space(8.0);
                            ui.colored_label(
                                theme::warn(),
                                format!("程序已找到，但这次没有启动成功：{note}"),
                            );
                        }
                        ui.add_space(16.0);
                        if theme::primary_button(ui, "+  配置模型").clicked() {
                            focus_form = true;
                        }
                        ui.add_space(16.0);
                        ui.label(
                            egui::RichText::new("企业级安全 · 本地优先 · 完全掌控")
                                .weak()
                                .size(11.0),
                        );
                        ui.add_space(8.0);
                        if ui.small_button("进入对话").clicked() {
                            dismiss = true;
                        }
                    });

                    cols[1].add_space(36.0);
                    theme::soft_card(&mut cols[1], |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("⚡")
                                    .size(16.0)
                                    .color(theme::accent()),
                            );
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new("添加 Provider").strong().size(18.0),
                                );
                                ui.label(
                                    egui::RichText::new("选择第三方服务或自定义 API 接入")
                                        .weak()
                                        .size(12.0),
                                );
                            });
                        });
                        ui.add_space(10.0);
                        let auth_path = admin_panel::pi_agent_dir().map(|dir| dir.join("auth.json"));
                        let models_path =
                            admin_panel::pi_agent_dir().map(|dir| dir.join("models.json"));
                        form_result = provider_catalog::render_add_provider_inline(
                            ui,
                            &mut self.admin.add_provider,
                            &mut self.admin.api_keys,
                            auth_path.as_deref(),
                            models_path.as_deref(),
                            true,
                        );
                    });
                });
            });

        if focus_form {
            self.admin.add_provider.open_builtin(None);
        }
        if form_result.restart {
            self.restart_sidecar();
        }
        if let Some(err) = form_result.error {
            self.admin.last_error = Some(err);
        }
        if dismiss {
            self.admin.show_onboarding = false;
            self.primed_model_tab = false;
            self.admin.open = false;
            self.admin.opened_pass = None;
            self.admin.add_provider.close();
            if let Some(req) = self.admin.persist_inferred_default() {
                self.handle_set_model_request(req);
            }
        }
    }

    pub(crate) fn show_pi_missing(&mut self, ctx: &egui::Context) {
        let mut reprobe = false;
        let mut open_models = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            self.paint_main_backdrop(ui);
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.add_space(36.0);
                    theme::centered_column(ui, 560.0, |ui| {
                        theme::soft_card(ui, |ui| {
                            ui.label(
                                egui::RichText::new("还没有找到 Pi")
                                    .strong()
                                    .size(22.0)
                                    .color(theme::amber()),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(
                                    "平时只需要配置模型和 API 密钥。当前还没找到 Pi，所以暂时不能对话。",
                                )
                                .size(14.0),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(
                                    "官方 Windows 便携包已经把 Pi 放在程序旁边。请完整解压 pi-light-windows-x64.zip，直接运行 pi-light.exe，然后点「重新探测」。一般不用自己设环境变量或 PATH。",
                                )
                                .size(13.0),
                            );
                            ui.add_space(14.0);
                            ui.horizontal(|ui| {
                                if theme::primary_button(ui, "重新探测").clicked() {
                                    reprobe = true;
                                }
                                ui.add_space(8.0);
                                if ui.button("先填写模型配置").clicked() {
                                    open_models = true;
                                }
                            });
                            ui.add_space(16.0);
                            egui::CollapsingHeader::new("高级：自己指定 Pi 程序")
                                .default_open(false)
                                .id_salt("pi_recovery_center")
                                .show(ui, |ui| {
                                    admin_panel::show_pi_binary_recovery(ui);
                                });
                        });
                    });
                    ui.add_space(24.0);
                });
        });

        if open_models {
            self.admin.open = true;
            self.admin.active_tab = AdminTab::Models;
            self.admin.opened_pass = Some(ctx.cumulative_pass_nr());
        }
        if reprobe {
            self.reprobe();
        }
    }

    pub(crate) fn show_pi_disconnected(&mut self, ctx: &egui::Context) {
        let path = self
            .pi_probe
            .path()
            .map(|p| p.to_string_lossy().into_owned());
        let mut reprobe = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            self.paint_main_backdrop(ui);
            ui.add_space(72.0);
            theme::centered_column(ui, 480.0, |ui| {
                theme::soft_card(ui, |ui| {
                    ui.label(
                        egui::RichText::new("连接断开了")
                            .strong()
                            .size(20.0)
                            .color(theme::amber()),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Pi 已经找到，不用重新安装。再探测一次即可。")
                            .size(14.0),
                    );
                    if let Some(path) = &path {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(path).monospace().weak().size(11.0));
                    }
                    ui.add_space(14.0);
                    if theme::primary_button(ui, "重新探测").clicked() {
                        reprobe = true;
                    }
                });
            });
        });

        if reprobe {
            self.reprobe();
        }
    }

    pub(crate) fn show_spawn_error(&self, ctx: &egui::Context, error: &str) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.paint_main_backdrop(ui);
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.add_space(48.0);
                    theme::centered_column(ui, 520.0, |ui| {
                        theme::soft_card(ui, |ui| {
                            ui.label(
                                egui::RichText::new("Pi 没有启动")
                                    .color(theme::warn())
                                    .strong()
                                    .size(20.0),
                            );
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new(error).size(13.0));
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(
                                    "确认程序可以运行后，点窗口底部的「重新探测」。",
                                )
                                .weak()
                                .size(13.0),
                            );
                            ui.add_space(12.0);
                            egui::CollapsingHeader::new("高级：换一个 Pi 程序")
                                .default_open(false)
                                .id_salt("pi_recovery_spawn")
                                .show(ui, |ui| {
                                    admin_panel::show_pi_binary_recovery(ui);
                                });
                        });
                    });
                });
        });
    }
}
