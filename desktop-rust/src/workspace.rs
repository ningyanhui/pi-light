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
use crate::icons::{self, Icon};
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
    /// Open the native folder picker and switch to the chosen folder.
    PickFolder,
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
    /// Side panel visibility; the icon rail toggles it.
    pub dock_open: bool,
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
            dock_open: true,
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

fn is_recent(updated_at: u64) -> bool {
    let secs = if updated_at > 10_000_000_000 {
        updated_at / 1000
    } else {
        updated_at
    };
    now_secs() - (secs as i64) < 24 * 3600
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Title-bar workspace selector: current folder chip plus a dropdown with
/// the native "open folder" action and recently used folders.
pub fn show_workspace_switcher(
    ui: &mut egui::Ui,
    workdir: &Path,
    switching: bool,
    popover: bool,
    recent: &[PathBuf],
    error: Option<&str>,
) -> Option<DockAction> {
    let mut action = None;
    let name = workdir_name(workdir);
    let chip = egui::Frame::none()
        .fill(if popover {
            theme::card_bg()
        } else {
            theme::sidebar_bg()
        })
        .stroke(Stroke::new(1.0_f32, theme::hairline()))
        .rounding(Rounding::same(theme::radius::MD))
        .inner_margin(egui::Margin::symmetric(10.0, 5.0))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 7.0;
            ui.horizontal(|ui| {
                icons::icon(ui, Icon::Folder, 15.0, theme::accent_bright());
                ui.label(
                    egui::RichText::new(name)
                        .size(13.0)
                        .color(theme::text_primary()),
                );
                let (dot, _) = ui.allocate_exact_size(egui::vec2(6.0, 6.0), egui::Sense::hover());
                let live = if switching {
                    theme::warn()
                } else {
                    theme::ok()
                };
                ui.painter().circle_filled(dot.center(), 3.0, live);
                icons::icon(ui, Icon::ChevronDown, 13.0, theme::text_dim());
            });
        });
    let chip_rect = chip.response.rect;
    let resp = ui.interact(chip_rect, ui.id().with("ws_switcher"), egui::Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.on_hover_text(display_workdir(workdir)).clicked() {
        action = Some(if popover {
            DockAction::CloseCwdPopover
        } else {
            DockAction::OpenCwdPopover
        });
    }
    if switching {
        ui.label(
            egui::RichText::new(SWITCHING_COPY)
                .size(12.0)
                .color(theme::accent_soft()),
        );
    }
    if !popover {
        return action;
    }

    let area = egui::Area::new(ui.id().with("ws_popover"))
        .order(egui::Order::Foreground)
        .fixed_pos(chip_rect.left_bottom() + egui::vec2(0.0, 8.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::none()
                .fill(theme::sidebar_bg())
                .stroke(Stroke::new(1.0_f32, theme::hairline_strong()))
                .rounding(Rounding::same(theme::radius::LG))
                .shadow(theme::glass_shadow())
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    ui.set_width(380.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if theme::menu_row(
                        ui,
                        Icon::FolderOpen,
                        "打开文件夹…",
                        None,
                        Some("Ctrl+O"),
                        true,
                    )
                    .clicked()
                    {
                        action = Some(DockAction::PickFolder);
                    }
                    let others: Vec<&PathBuf> = recent
                        .iter()
                        .filter(|p| !same_cwd(p, workdir))
                        .take(8)
                        .collect();
                    if !others.is_empty() {
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.add_space(10.0);
                            ui.label(
                                egui::RichText::new("最近打开")
                                    .size(11.0)
                                    .color(theme::text_dim()),
                            );
                        });
                        for path in others {
                            let detail = display_workdir(path);
                            if theme::menu_row(
                                ui,
                                Icon::Folder,
                                &workdir_name(path),
                                Some(&detail),
                                None,
                                false,
                            )
                            .clicked()
                            {
                                action = Some(DockAction::RequestWorkdir(
                                    path.to_string_lossy().into_owned(),
                                ));
                            }
                        }
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("也可以把文件夹直接拖进窗口")
                                .size(11.0)
                                .color(theme::text_dim()),
                        );
                    });
                    if let Some(err) = error {
                        ui.add_space(4.0);
                        ui.colored_label(theme::danger(), err);
                    }
                });
        });
    if action.is_none() {
        let dismiss = ui.input(|i| {
            i.key_pressed(egui::Key::Escape)
                || (i.pointer.any_click()
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|p| !area.response.rect.contains(p) && !chip_rect.contains(p)))
        });
        if dismiss {
            action = Some(DockAction::CloseCwdPopover);
        }
    }
    action
}

/// Last path component (or the full path for roots like `C:\`).
pub fn workdir_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| display_workdir(path))
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
    let title = match workspace.dock {
        DockTab::Sessions => "会话",
        DockTab::Files => "文件",
        DockTab::Git => "Git",
    };
    let mut header_action = None;
    ui.horizontal(|ui| {
        ui.set_min_height(28.0);
        ui.label(
            egui::RichText::new(title)
                .strong()
                .size(13.5)
                .color(theme::text_primary()),
        );
        if workspace.dock == DockTab::Sessions {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled = !(streaming || switching);
                let resp = ui
                    .add_enabled_ui(enabled, |ui| {
                        crate::icons::icon_button(ui, crate::icons::Icon::Plus, "新建会话")
                    })
                    .inner;
                if resp.clicked() {
                    header_action = Some(DockAction::NewSession);
                }
            });
        }
    });
    ui.add_space(6.0);

    let action = match workspace.dock {
        DockTab::Sessions => {
            show_sessions_panel(ui, workspace, hub, workdir, streaming || switching)
        }
        DockTab::Files => show_files_panel(ui, workspace, workdir, textures),
        DockTab::Git => show_git_panel(ui, workspace, switching, textures),
    };

    action.or(header_action)
}

fn show_sessions_panel(
    ui: &mut egui::Ui,
    workspace: &mut WorkspaceUi,
    hub: &SessionHub,
    workdir: &Path,
    locked: bool,
) -> Option<DockAction> {
    let mut action = None;
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
            let mut group: Option<bool> = None;
            for i in indices {
                let Some(session) = hub.list().get(i) else {
                    continue;
                };
                let recent = is_recent(session.updated_at);
                if group != Some(recent) {
                    group = Some(recent);
                    ui.add_space(if recent { 2.0 } else { 10.0 });
                    ui.label(
                        egui::RichText::new(if recent { "最近" } else { "更早" })
                            .size(11.0)
                            .color(theme::text_dim()),
                    );
                    ui.add_space(4.0);
                }
                let selected = i == active;
                let wd = session.workdir.as_deref().unwrap_or(workdir);
                let time = session_time_label(session.updated_at);
                let (resp, menu) =
                    session_row(ui, i, &session.title, &workdir_name(wd), &time, selected);
                ui.add_space(2.0);
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
    let hover_id = egui::Id::new(("dock_session_row_hover", index));
    let was_hovered = ui.data(|d| d.get_temp::<bool>(hover_id).unwrap_or(false));
    let fill = if selected {
        theme::selected_fill()
    } else if was_hovered {
        theme::hover_fill()
    } else {
        Color32::TRANSPARENT
    };
    let inner = egui::Frame::none()
        .fill(fill)
        .stroke(theme::session_row_stroke(selected))
        .rounding(Rounding::same(9.0))
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - 28.0).max(0.0));
                    ui.label(egui::RichText::new(title).size(13.0).color(if selected {
                        theme::text_primary()
                    } else {
                        theme::text_muted()
                    }));
                    ui.add_space(1.0);
                    ui.label(
                        egui::RichText::new(format!("{workdir}  ·  {time}"))
                            .size(11.0)
                            .color(theme::text_dim()),
                    );
                });
            });
        });
    let resp = inner.response.interact(egui::Sense::click());
    let menu_id = egui::Id::new(("dock_session_row_menu", index));
    let menu_was_open = ui.data(|d| d.get_temp::<bool>(menu_id).unwrap_or(false));
    let btn = egui::Rect::from_center_size(
        egui::pos2(resp.rect.right() - 22.0, resp.rect.center().y),
        egui::vec2(26.0, 24.0),
    );
    // Always create the child so egui's auto-ids (and the menu state) stay stable per row.
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(btn));
    if selected || menu_was_open || ui.rect_contains_pointer(resp.rect) {
        let open = {
            let ui = &mut child;
            ui.menu_button(
                egui::RichText::new("⋯")
                    .size(16.0)
                    .color(theme::text_muted()),
                |ui| {
                    if let Some(next) = session_context_menu(ui, index) {
                        action = Some(next);
                    }
                },
            )
            .inner
            .is_some()
        };
        if open != menu_was_open {
            ui.data_mut(|d| d.insert_temp(menu_id, open));
        }
    }
    let hovered = resp.hovered();
    if hovered != was_hovered {
        ui.data_mut(|d| d.insert_temp(hover_id, hovered));
        ui.ctx().request_repaint();
    }
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
