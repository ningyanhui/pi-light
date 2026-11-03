// SPDX-License-Identifier: Apache-2.0
//
// 三栏工作区：左 dock（会话 / 文件 / Git）+ 无感知 cwd 切换。
// 产品叙事是「换工作区」。Pi v1.4.0 无 set_cwd，异路径时允许静默 respawn。

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use eframe::egui::{self, Color32, Rounding, Stroke};

use crate::assets::ShellTextures;
use crate::chat::SessionHub;
use crate::theme;
use crate::workspace_git::{self, GitKind, GitState};
use crate::workspace_tree::{EntryKind, FileTree, TreeStatus};

pub const SWITCH_FAIL_COPY: &str = "切换工作区失败";
pub const SWITCHING_COPY: &str = "正在切换工作区…";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockTab {
    Sessions,
    Files,
    Git,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitPage {
    Changes,
    History,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CwdSwitchPlan {
    Reuse,
    Respawn,
}

#[derive(Debug, Clone)]
pub enum DockAction {
    NewSession,
    SwitchSession(usize),
    ForkSession(usize),
    QuoteSession(usize),
    DeleteSession(usize),
    StartRename(usize),
    ConfirmRename,
    CancelRename,
    OpenSettings,
    OpenMarketplace,
    OpenRemote,
    InsertPath(String),
    RequestWorkdir(String),
    OpenCwdPopover,
    CloseCwdPopover,
    GitStage,
    GitUnstage,
    GitCommit,
    GitReveal,
    SelectGitPath(PathBuf),
    RetrySwitch,
    DismissError,
}

pub struct Toast {
    pub text: String,
    pub at: Instant,
}

pub struct WorkspaceUi {
    pub dock: DockTab,
    pub git_page: GitPage,
    pub cwd_popover: bool,
    pub switching: bool,
    pub switch_error: Option<String>,
    pub retry_path: Option<PathBuf>,
    pub toast: Option<Toast>,
    pub tree: FileTree,
    pub git: GitState,
    pub commit_draft: String,
    pub commit_confirm: bool,
    pub selected_git: Option<PathBuf>,
    pub git_action_error: Option<String>,
    pub rename_index: Option<usize>,
    pub rename_draft: String,
}

impl Default for WorkspaceUi {
    fn default() -> Self {
        Self {
            dock: DockTab::Sessions,
            git_page: GitPage::Changes,
            cwd_popover: false,
            switching: false,
            switch_error: None,
            retry_path: None,
            toast: None,
            tree: FileTree::default(),
            git: GitState::default(),
            commit_draft: String::new(),
            commit_confirm: false,
            selected_git: None,
            git_action_error: None,
            rename_index: None,
            rename_draft: String::new(),
        }
    }
}

impl WorkspaceUi {
    pub fn bind_root(&mut self, root: PathBuf) {
        self.tree = FileTree::load(root.clone());
        self.git = GitState::load(root);
        self.selected_git = None;
        self.git_action_error = None;
        self.commit_confirm = false;
    }

    pub fn show_toast(&mut self, text: impl Into<String>) {
        self.toast = Some(Toast {
            text: text.into(),
            at: Instant::now(),
        });
    }

    pub fn poll_toast(&mut self) {
        if let Some(toast) = &self.toast {
            if toast.at.elapsed() > Duration::from_secs(4) {
                self.toast = None;
            }
        }
    }
}

/// 同路径复用 sidecar；规范化后不同才允许静默 respawn。
pub fn plan_cwd_switch(current: &Path, next: &Path) -> CwdSwitchPlan {
    if same_cwd(current, next) {
        CwdSwitchPlan::Reuse
    } else {
        CwdSwitchPlan::Respawn
    }
}

pub fn same_cwd(a: &Path, b: &Path) -> bool {
    let ca = normalize_cwd(a);
    let cb = normalize_cwd(b);
    #[cfg(windows)]
    {
        ca.eq_ignore_ascii_case(&cb)
    }
    #[cfg(not(windows))]
    {
        ca == cb
    }
}

pub fn normalize_cwd(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .trim_end_matches(['/', '\\'])
        .to_string()
}

pub fn display_workdir(path: &Path) -> String {
    if let Some(home) = home_dir() {
        if let Ok(rel) = path.strip_prefix(&home) {
            let rest = rel.to_string_lossy();
            if rest.is_empty() {
                return "~".to_owned();
            }
            return format!("~/{}", rest.replace('\\', "/"));
        }
    }
    path.to_string_lossy().replace('\\', "/")
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub fn recent_workdirs(hub: &SessionHub, current: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    push_unique(&mut out, current.to_path_buf());
    for session in hub.list() {
        if let Some(dir) = &session.workdir {
            push_unique(&mut out, dir.clone());
        }
    }
    out
}

fn push_unique(out: &mut Vec<PathBuf>, path: PathBuf) {
    if out.iter().any(|p| same_cwd(p, &path)) {
        return;
    }
    out.push(path);
}

pub fn session_time_label(updated_at: u64) -> String {
    let secs = if updated_at > 10_000_000_000 {
        updated_at / 1000
    } else {
        updated_at
    };
    workspace_git::relative_from_now(secs as i64, now_secs())
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn show_topbar_cwd(
    ui: &mut egui::Ui,
    workdir: &Path,
    draft: &mut String,
    switching: bool,
    popover: bool,
    recent: &[PathBuf],
    error: Option<&str>,
) -> Option<DockAction> {
    let mut action = None;
    ui.label(
        egui::RichText::new("π")
            .strong()
            .size(16.0)
            .color(theme::accent()),
    );
    ui.label(
        egui::RichText::new("pi-light")
            .strong()
            .size(14.0)
            .color(theme::text_primary()),
    );
    ui.add_space(10.0);
    let badge = format!("当前工作区  {}", display_workdir(workdir));
    let badge_resp = ui.add(
        egui::Button::new(
            egui::RichText::new(badge)
                .size(12.0)
                .color(theme::text_primary()),
        )
        .fill(theme::card_bg())
        .stroke(Stroke::new(1.0_f32, theme::glass_stroke()))
        .rounding(Rounding::same(14.0)),
    );
    if badge_resp.on_hover_text("点击徽章即可切换工作区").clicked() {
        action = Some(if popover {
            DockAction::CloseCwdPopover
        } else {
            DockAction::OpenCwdPopover
        });
    }
    if switching {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(format!("◌  {SWITCHING_COPY}"))
                .size(12.0)
                .color(theme::accent_soft()),
        );
    } else {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("点击徽章即可切换工作区")
                .size(11.0)
                .color(theme::text_dim()),
        );
    }

    if popover {
        let mut picked: Option<String> = None;
        egui::Window::new("切换工作区")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 64.0])
            .frame(
                egui::Frame::none()
                    .fill(theme::sidebar_bg())
                    .stroke(Stroke::new(1.0_f32, theme::glass_stroke_strong()))
                    .rounding(Rounding::same(12.0))
                    .inner_margin(egui::Margin::same(14.0)),
            )
            .show(ui.ctx(), |ui| {
                ui.set_min_width(420.0);
                ui.label(
                    egui::RichText::new("当前活动工作区唯一")
                        .strong()
                        .size(13.0)
                        .color(theme::text_primary()),
                );
                ui.label(
                    egui::RichText::new("切换时会短暂同步环境。树与 Git 会跟随新路径。")
                        .size(11.0)
                        .color(theme::text_muted()),
                );
                ui.add_space(8.0);
                ui.add(
                    egui::TextEdit::singleline(draft)
                        .desired_width(380.0)
                        .hint_text("绝对路径"),
                );
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "切换").clicked() {
                        picked = Some(draft.clone());
                    }
                    if theme::ghost_button(ui, "取消").clicked() {
                        action = Some(DockAction::CloseCwdPopover);
                    }
                });
                if !recent.is_empty() {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("最近使用")
                            .size(11.0)
                            .color(theme::text_muted()),
                    );
                    for path in recent {
                        let label = display_workdir(path);
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(label)
                                        .size(12.0)
                                        .color(theme::text_primary()),
                                )
                                .fill(theme::card_bg())
                                .min_size(egui::vec2(ui.available_width(), 26.0)),
                            )
                            .clicked()
                        {
                            picked = Some(path.to_string_lossy().into_owned());
                        }
                    }
                }
                if let Some(err) = error {
                    ui.colored_label(theme::danger(), err);
                }
            });
        if let Some(raw) = picked {
            action = Some(DockAction::RequestWorkdir(raw));
        }
    }
    action
}

pub fn show_switch_error_banner(ui: &mut egui::Ui, error: &str) -> Option<DockAction> {
    let mut action = None;
    egui::Frame::none()
        .fill(Color32::from_rgba_unmultiplied(48, 18, 24, 230))
        .stroke(Stroke::new(1.0_f32, theme::danger()))
        .rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(10.0, 6.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(theme::danger(), error);
                if ui.small_button("重试").clicked() {
                    action = Some(DockAction::RetrySwitch);
                }
                if ui.small_button("关闭").clicked() {
                    action = Some(DockAction::DismissError);
                }
            });
        });
    action
}

pub fn show_toast_overlay(ctx: &egui::Context, text: &str) {
    let screen = ctx.screen_rect();
    let pos = egui::pos2(screen.center().x, screen.bottom() - 92.0);
    egui::Area::new(egui::Id::new("workspace_toast"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -88.0])
        .show(ctx, |ui| {
            let _ = pos;
            egui::Frame::none()
                .fill(Color32::from_rgba_unmultiplied(20, 28, 24, 240))
                .stroke(Stroke::new(1.0_f32, theme::ok()))
                .rounding(Rounding::same(16.0))
                .inner_margin(egui::Margin::symmetric(14.0, 8.0))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!("✓  {text}"))
                            .size(12.0)
                            .color(theme::text_primary()),
                    );
                });
        });
}

pub fn show_dock(
    ui: &mut egui::Ui,
    workspace: &mut WorkspaceUi,
    hub: &SessionHub,
    workdir: &Path,
    streaming: bool,
    switching: bool,
    textures: Option<&ShellTextures>,
) -> Option<DockAction> {
    ui.horizontal(|ui| {
        for (tab, label, icon) in [
            (
                DockTab::Sessions,
                "会话",
                textures.and_then(|t| t.icon_sessions.as_ref()),
            ),
            (
                DockTab::Files,
                "文件",
                textures.and_then(|t| t.icon_files.as_ref()),
            ),
            (
                DockTab::Git,
                "Git",
                textures.and_then(|t| t.icon_git.as_ref()),
            ),
        ] {
            let selected = workspace.dock == tab;
            let text = egui::RichText::new(label).size(12.0).color(if selected {
                theme::text_primary()
            } else {
                theme::text_muted()
            });
            let button = if let Some(tex) = icon {
                egui::Button::image_and_text((tex.id(), egui::vec2(16.0, 16.0)), text)
            } else {
                egui::Button::new(text)
            };
            let resp = ui.add(
                button
                    .fill(if selected {
                        Color32::from_rgb(35, 51, 73)
                    } else {
                        theme::card_bg()
                    })
                    .stroke(if selected {
                        Stroke::new(1.0_f32, theme::accent_stroke_soft())
                    } else {
                        Stroke::NONE
                    })
                    .rounding(Rounding::same(10.0))
                    .min_size(egui::vec2(72.0, 36.0)),
            );
            if selected {
                theme::paint_purple_glow(ui, resp.rect, 4.0);
            }
            if resp.clicked() {
                workspace.dock = tab;
            }
        }
    });
    ui.add_space(10.0);

    let mut action = match workspace.dock {
        DockTab::Sessions => {
            show_sessions_panel(ui, workspace, hub, workdir, streaming || switching)
        }
        DockTab::Files => show_files_panel(ui, workspace, workdir, textures),
        DockTab::Git => show_git_panel(ui, workspace, switching, textures),
    };

    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if dock_link(ui, "⚙ 设置").clicked() {
                action = Some(DockAction::OpenSettings);
            }
            if dock_link(ui, "市场").clicked() {
                action = Some(DockAction::OpenMarketplace);
            }
            if dock_link(ui, "远程").clicked() {
                action = Some(DockAction::OpenRemote);
            }
        });
        ui.label(
            egui::RichText::new("当前活动工作区唯一")
                .size(10.0)
                .color(theme::text_dim()),
        );
    });
    action
}

fn dock_link(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .size(12.0)
                .color(theme::text_muted()),
        )
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE),
    )
}

fn show_sessions_panel(
    ui: &mut egui::Ui,
    workspace: &mut WorkspaceUi,
    hub: &SessionHub,
    workdir: &Path,
    locked: bool,
) -> Option<DockAction> {
    let mut action = None;
    let new_resp = ui.add_enabled(
        !locked,
        egui::Button::new(
            egui::RichText::new("+ 新建会话")
                .color(Color32::WHITE)
                .size(13.0),
        )
        .fill(theme::accent())
        .rounding(Rounding::same(10.0))
        .min_size(egui::vec2(ui.available_width(), 34.0)),
    );
    theme::paint_purple_glow(ui, new_resp.rect, 8.0);
    if new_resp.clicked() {
        action = Some(DockAction::NewSession);
    }
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(format!("按更新时间 · 共 {} 条", hub.list().len()))
            .size(11.0)
            .color(theme::text_muted()),
    );
    ui.add_space(6.0);
    if let Some(rename_action) = show_rename_dialog(ui, workspace, hub) {
        action = Some(rename_action);
    }
    let indices = hub.indices_by_updated();
    let active = hub.active_index();
    let list_height = (ui.available_height() - 28.0).max(40.0);
    egui::ScrollArea::vertical()
        .id_salt("dock_session_scroll")
        .max_height(list_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for i in indices {
                let Some(session) = hub.list().get(i) else {
                    continue;
                };
                let selected = i == active;
                let wd = session.workdir.as_deref().unwrap_or(workdir);
                let time = session_time_label(session.updated_at);
                let (resp, menu) =
                    session_row(ui, i, &session.title, &display_workdir(wd), &time, selected);
                ui.add_space(4.0);
                if let Some(next) = menu {
                    action = Some(next);
                } else if resp.clicked() && !locked {
                    action = Some(DockAction::SwitchSession(i));
                }
            }
        });
    action
}

fn show_rename_dialog(
    ui: &mut egui::Ui,
    workspace: &mut WorkspaceUi,
    hub: &SessionHub,
) -> Option<DockAction> {
    let index = workspace.rename_index?;
    if index >= hub.list().len() {
        workspace.rename_index = None;
        return None;
    }
    let mut action = None;
    egui::Window::new("重命名会话")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, [0.0, 80.0])
        .frame(theme::glass_frame().inner_margin(egui::Margin::same(12.0)))
        .show(ui.ctx(), |ui| {
            ui.set_min_width(280.0);
            ui.add(
                egui::TextEdit::singleline(&mut workspace.rename_draft)
                    .desired_width(260.0)
                    .hint_text("会话标题"),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, "保存").clicked() {
                    action = Some(DockAction::ConfirmRename);
                }
                if theme::ghost_button(ui, "取消").clicked() {
                    action = Some(DockAction::CancelRename);
                }
            });
        });
    action
}

fn session_row(
    ui: &mut egui::Ui,
    index: usize,
    title: &str,
    workdir: &str,
    time: &str,
    selected: bool,
) -> (egui::Response, Option<DockAction>) {
    let mut action = None;
    let fill = if selected {
        Color32::from_rgb(35, 51, 73)
    } else {
        Color32::from_rgba_unmultiplied(16, 14, 28, 70)
    };
    let inner = egui::Frame::none()
        .fill(fill)
        .stroke(theme::session_row_stroke(selected))
        .rounding(Rounding::same(12.0))
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(title)
                            .strong()
                            .size(13.0)
                            .color(theme::text_primary()),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(format!("{workdir}  ·  {time}"))
                            .size(11.0)
                            .color(theme::text_muted()),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let hover = ui.rect_contains_pointer(ui.max_rect()) || selected;
                    if hover {
                        ui.menu_button(
                            egui::RichText::new("⋯")
                                .size(16.0)
                                .color(theme::text_muted()),
                            |ui| {
                                action = session_context_menu(ui, index);
                            },
                        );
                    }
                });
            });
        });
    let resp = inner.response.interact(egui::Sense::click());
    resp.context_menu(|ui| {
        if let Some(next) = session_context_menu(ui, index) {
            action = Some(next);
        }
    });
    (resp, action)
}

fn session_context_menu(ui: &mut egui::Ui, index: usize) -> Option<DockAction> {
    let mut action = None;
    if ui.button("打开").clicked() {
        action = Some(DockAction::SwitchSession(index));
        ui.close_menu();
    }
    if ui.button("Fork").clicked() {
        action = Some(DockAction::ForkSession(index));
        ui.close_menu();
    }
    if ui.button("引用到输入框").clicked() {
        action = Some(DockAction::QuoteSession(index));
        ui.close_menu();
    }
    if ui.button("重命名").clicked() {
        action = Some(DockAction::StartRename(index));
        ui.close_menu();
    }
    if ui.button("删除").clicked() {
        action = Some(DockAction::DeleteSession(index));
        ui.close_menu();
    }
    action
}

fn show_files_panel(
    ui: &mut egui::Ui,
    workspace: &mut WorkspaceUi,
    workdir: &Path,
    textures: Option<&ShellTextures>,
) -> Option<DockAction> {
    let mut action = None;
    ui.label(
        egui::RichText::new(format!("根 = {}", display_workdir(workdir)))
            .size(11.0)
            .color(theme::text_muted()),
    );
    ui.add_space(6.0);
    match workspace.tree.status() {
        TreeStatus::Missing => {
            empty_card(
                ui,
                "无法访问路径",
                "无法读取该路径。请检查权限或更换工作区。界面不崩溃，仅显示空态。",
                textures.and_then(|t| t.empty_noperm.as_ref()),
            );
        }
        TreeStatus::NoPermission => {
            empty_card(
                ui,
                "无访问权限",
                "无法读取该路径。请检查权限或更换工作区。界面不崩溃，仅显示空态。",
                textures.and_then(|t| t.empty_noperm.as_ref()),
            );
        }
        TreeStatus::Empty => {
            empty_card(
                ui,
                "空目录",
                "此文件夹没有可见文件。（常见大目录默认已忽略）",
                textures.and_then(|t| t.empty_folder.as_ref()),
            );
        }
        TreeStatus::Ready => {
            let root = workspace.tree.root().to_path_buf();
            let height = (ui.available_height() - 8.0).max(40.0);
            egui::ScrollArea::vertical()
                .id_salt("dock_file_tree")
                .max_height(height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    action = render_tree_level(ui, workspace, &root, 0);
                });
        }
    }
    action
}

fn render_tree_level(
    ui: &mut egui::Ui,
    workspace: &mut WorkspaceUi,
    path: &Path,
    depth: usize,
) -> Option<DockAction> {
    let mut action = None;
    let entries = workspace.tree.children(path).to_vec();
    for entry in entries {
        match entry.kind {
            EntryKind::IgnoredDir => {
                ui.label(
                    egui::RichText::new(format!(
                        "{}··· {}/（已忽略）",
                        "  ".repeat(depth),
                        entry.name
                    ))
                    .size(12.0)
                    .color(theme::text_dim()),
                );
            }
            EntryKind::Dir => {
                let expanded = workspace.tree.is_expanded(&entry.path);
                let mark = if expanded { "▾" } else { "▸" };
                let resp = ui.selectable_label(
                    false,
                    egui::RichText::new(format!("{}{mark} {}/", "  ".repeat(depth), entry.name))
                        .size(12.0)
                        .color(theme::text_primary()),
                );
                if resp.clicked() {
                    workspace.tree.toggle(&entry.path);
                }
                if workspace.tree.is_expanded(&entry.path) {
                    if let Some(next) = render_tree_level(ui, workspace, &entry.path, depth + 1) {
                        action = Some(next);
                    }
                }
            }
            EntryKind::File => {
                let rel = FileTree::relative_display(workspace.tree.root(), &entry.path);
                let resp = ui.selectable_label(
                    false,
                    egui::RichText::new(format!("{}{}", "  ".repeat(depth), entry.name))
                        .size(12.0)
                        .color(theme::text_muted()),
                );
                if resp.clicked() {
                    action = Some(DockAction::InsertPath(rel));
                }
            }
        }
    }
    action
}

fn show_git_panel(
    ui: &mut egui::Ui,
    workspace: &mut WorkspaceUi,
    locked: bool,
    textures: Option<&ShellTextures>,
) -> Option<DockAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        for (page, label) in [(GitPage::Changes, "Changes"), (GitPage::History, "History")] {
            let selected = workspace.git_page == page;
            if theme::filter_chip(ui, label, selected).clicked() {
                workspace.git_page = page;
            }
        }
    });
    ui.add_space(6.0);

    match workspace.git.kind {
        GitKind::GitMissing => {
            empty_card(
                ui,
                "未找到 git",
                "本机或便携包里没有可用的 git。Changes / History 不可用，界面不崩溃。",
                textures.and_then(|t| t.empty_nongit.as_ref()),
            );
            return action;
        }
        GitKind::NotARepo => {
            empty_card(
                ui,
                "非 Git 仓库",
                "当前工作区不是 git 仓。Changes / History 不可用。可切换仓库或其他目录。",
                textures.and_then(|t| t.empty_nongit.as_ref()),
            );
            return action;
        }
        GitKind::Error(ref err) => {
            empty_card(
                ui,
                "无法读取 Git",
                err,
                textures.and_then(|t| t.empty_noperm.as_ref()),
            );
            return action;
        }
        GitKind::Ready => {}
    }

    if let Some(err) = &workspace.git_action_error {
        ui.colored_label(theme::danger(), err);
        ui.add_space(4.0);
    }

    match workspace.git_page {
        GitPage::Changes => {
            ui.label(
                egui::RichText::new(format!("{} 个变更", workspace.git.changes.len()))
                    .size(11.0)
                    .color(theme::text_muted()),
            );
            let height = (ui.available_height() - 86.0).max(40.0);
            egui::ScrollArea::vertical()
                .id_salt("dock_git_changes")
                .max_height(height)
                .show(ui, |ui| {
                    if workspace.git.changes.is_empty() {
                        ui.label(
                            egui::RichText::new("工作区干净，没有未提交变更。")
                                .size(12.0)
                                .color(theme::text_muted()),
                        );
                    }
                    for change in &workspace.git.changes {
                        let selected = workspace
                            .selected_git
                            .as_deref()
                            .is_some_and(|p| p == change.path.as_path());
                        let label = format!("{}  {}", change.label(), change.display_path());
                        if ui.selectable_label(selected, label).clicked() {
                            action = Some(DockAction::SelectGitPath(change.path.clone()));
                        }
                    }
                });
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("选中后可在资源管理器中显示")
                    .size(10.0)
                    .color(theme::text_dim()),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !locked && workspace.selected_git.is_some(),
                        egui::Button::new("Stage"),
                    )
                    .clicked()
                {
                    action = Some(DockAction::GitStage);
                }
                if ui
                    .add_enabled(
                        !locked && workspace.selected_git.is_some(),
                        egui::Button::new("Unstage"),
                    )
                    .clicked()
                {
                    action = Some(DockAction::GitUnstage);
                }
                if ui
                    .add_enabled(workspace.selected_git.is_some(), egui::Button::new("揭示"))
                    .clicked()
                {
                    action = Some(DockAction::GitReveal);
                }
            });
            ui.add(
                egui::TextEdit::singleline(&mut workspace.commit_draft)
                    .desired_width(ui.available_width())
                    .hint_text("提交说明"),
            );
            ui.horizontal(|ui| {
                if workspace.commit_confirm {
                    ui.label("确认提交？");
                    if ui.add_enabled(!locked, egui::Button::new("确认")).clicked() {
                        action = Some(DockAction::GitCommit);
                    }
                    if ui.button("取消").clicked() {
                        workspace.commit_confirm = false;
                    }
                } else if ui
                    .add_enabled(!locked, egui::Button::new("Commit"))
                    .clicked()
                {
                    workspace.commit_confirm = true;
                }
            });
        }
        GitPage::History => {
            ui.label(
                egui::RichText::new(format!("最近 {} 条 · 只读", workspace_git::HISTORY_LIMIT))
                    .size(11.0)
                    .color(theme::text_muted()),
            );
            let height = (ui.available_height() - 8.0).max(40.0);
            egui::ScrollArea::vertical()
                .id_salt("dock_git_history")
                .max_height(height)
                .show(ui, |ui| {
                    if workspace.git.history.is_empty() {
                        ui.label(
                            egui::RichText::new("还没有提交记录。")
                                .size(12.0)
                                .color(theme::text_muted()),
                        );
                    }
                    for commit in &workspace.git.history {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(format!("{}  {}", commit.hash, commit.subject))
                                .size(12.0)
                                .color(theme::text_primary()),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "{}  ·  {}",
                                commit.author,
                                workspace_git::format_commit_time(commit.timestamp)
                            ))
                            .size(11.0)
                            .color(theme::text_muted()),
                        );
                    }
                });
        }
    }
    action
}

fn empty_card(
    ui: &mut egui::Ui,
    title: &str,
    body: &str,
    illustration: Option<&egui::TextureHandle>,
) {
    theme::glass_frame().show(ui, |ui| {
        if let Some(tex) = illustration {
            ui.vertical_centered(|ui| {
                ui.add(egui::Image::new((tex.id(), egui::vec2(96.0, 96.0))).rounding(8.0));
            });
            ui.add_space(8.0);
        }
        ui.label(
            egui::RichText::new(title)
                .strong()
                .size(13.0)
                .color(theme::text_primary()),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(body)
                .size(12.0)
                .color(theme::text_muted()),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_path_reuses_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path();
        let b = dir.path().join(".").canonicalize().unwrap();
        assert_eq!(plan_cwd_switch(a, &b), CwdSwitchPlan::Reuse);
    }

    #[test]
    fn different_path_plans_respawn() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        assert_eq!(plan_cwd_switch(a.path(), b.path()), CwdSwitchPlan::Respawn);
    }

    #[test]
    fn display_home_uses_tilde() {
        if let Some(home) = home_dir() {
            let nested = home.join("work").join("demo");
            let shown = display_workdir(&nested);
            assert!(shown.starts_with("~/"), "{shown}");
        }
    }

    #[test]
    fn fail_copy_has_no_engine_restart() {
        assert_eq!(SWITCH_FAIL_COPY, "切换工作区失败");
        assert!(!SWITCH_FAIL_COPY.contains("引擎"));
        assert!(!SWITCH_FAIL_COPY.contains("重启"));
        assert!(!SWITCHING_COPY.contains("引擎"));
        assert!(!SWITCHING_COPY.contains("重启"));
    }
}
