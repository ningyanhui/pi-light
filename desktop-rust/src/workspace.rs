// SPDX-License-Identifier: Apache-2.0
//
// 三栏工作区：左 dock（会话 / 文件 / Git）+ 无感知 cwd 切换。
// 产品叙事是「换工作区」。Pi v1.4.0 无 set_cwd，异路径时允许静默 respawn。

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use eframe::egui::{self, Color32, Rounding, Stroke};

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
        .fill(Color32::from_rgba_unmultiplied(28, 24, 48, 220))
        .stroke(Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(124, 77, 255, 90),
        ))
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
                    .fill(Color32::from_rgba_unmultiplied(16, 14, 28, 245))
                    .stroke(Stroke::new(
                        1.0_f32,
                        Color32::from_rgba_unmultiplied(124, 77, 255, 70),
                    ))
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
                                .fill(Color32::from_rgb(22, 20, 36))
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
) -> Option<DockAction> {
    ui.horizontal(|ui| {
        for (tab, icon, label) in [
            (DockTab::Sessions, "◌", "会话"),
            (DockTab::Files, "▤", "文件"),
            (DockTab::Git, "⌥", "Git"),
        ] {
            let selected = workspace.dock == tab;
            let resp = ui.add(
                egui::Button::new(
                    egui::RichText::new(format!("{icon}\n{label}"))
                        .size(11.0)
                        .color(if selected {
                            Color32::WHITE
                        } else {
                            theme::text_muted()
                        }),
                )
                .fill(if selected {
                    theme::accent()
                } else {
                    Color32::from_rgb(16, 14, 26)
                })
                .rounding(Rounding::same(10.0))
                .min_size(egui::vec2(64.0, 44.0)),
            );
            if resp.clicked() {
                workspace.dock = tab;
            }
        }
    });
    ui.add_space(10.0);

    let mut action = match workspace.dock {
        DockTab::Sessions => show_sessions_panel(ui, hub, workdir, streaming || switching),
        DockTab::Files => show_files_panel(ui, workspace, workdir),
        DockTab::Git => show_git_panel(ui, workspace, switching),
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
    hub: &SessionHub,
    workdir: &Path,
    locked: bool,
) -> Option<DockAction> {
    let mut action = None;
    if ui
        .add_enabled(
            !locked,
            egui::Button::new(
                egui::RichText::new("+ 新建会话")
                    .color(Color32::WHITE)
                    .size(13.0),
            )
            .fill(theme::accent())
            .rounding(Rounding::same(10.0))
            .min_size(egui::vec2(ui.available_width(), 32.0)),
        )
        .clicked()
    {
        action = Some(DockAction::NewSession);
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(format!("按更新时间 · 共 {} 条", hub.list().len()))
            .size(11.0)
            .color(theme::text_muted()),
    );
    ui.add_space(4.0);
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
                let resp = session_row(ui, &session.title, &display_workdir(wd), &time, selected);
                if resp.clicked() && !locked {
                    action = Some(DockAction::SwitchSession(i));
                }
            }
        });
    action
}

fn session_row(
    ui: &mut egui::Ui,
    title: &str,
    workdir: &str,
    time: &str,
    selected: bool,
) -> egui::Response {
    let fill = if selected {
        Color32::from_rgba_unmultiplied(40, 28, 72, 230)
    } else {
        Color32::TRANSPARENT
    };
    let stroke = if selected {
        Stroke::new(1.0_f32, theme::accent())
    } else {
        Stroke::NONE
    };
    egui::Frame::none()
        .fill(fill)
        .stroke(stroke)
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(title)
                        .strong()
                        .size(13.0)
                        .color(theme::text_primary()),
                );
                ui.label(
                    egui::RichText::new(format!("{workdir}  ·  {time}"))
                        .size(11.0)
                        .color(theme::text_muted()),
                );
            });
        })
        .response
        .interact(egui::Sense::click())
}

fn show_files_panel(
    ui: &mut egui::Ui,
    workspace: &mut WorkspaceUi,
    workdir: &Path,
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
            );
        }
        TreeStatus::NoPermission => {
            empty_card(
                ui,
                "无访问权限",
                "无法读取该路径。请检查权限或更换工作区。界面不崩溃，仅显示空态。",
            );
        }
        TreeStatus::Empty => {
            empty_card(
                ui,
                "空目录",
                "此文件夹没有可见文件。（常见大目录默认已忽略）",
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
            );
            return action;
        }
        GitKind::NotARepo => {
            empty_card(
                ui,
                "非 Git 仓库",
                "当前工作区不是 git 仓。Changes / History 不可用。可切换仓库或其他目录。",
            );
            return action;
        }
        GitKind::Error(ref err) => {
            empty_card(ui, "无法读取 Git", err);
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

fn empty_card(ui: &mut egui::Ui, title: &str, body: &str) {
    egui::Frame::none()
        .fill(Color32::from_rgba_unmultiplied(18, 16, 30, 230))
        .stroke(Stroke::new(1.0_f32, theme::hairline()))
        .rounding(Rounding::same(12.0))
        .inner_margin(egui::Margin::same(12.0))
        .show(ui, |ui| {
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
