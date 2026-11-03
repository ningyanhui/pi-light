// SPDX-License-Identifier: Apache-2.0
//
// 当前活动工作区的轻量 Git：status 列表、最近历史、最小 stage/commit。
// 不提供 merge / rebase / 冲突 UI。

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

pub const HISTORY_LIMIT: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitKind {
    Ready,
    NotARepo,
    GitMissing,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitChange {
    pub path: PathBuf,
    pub index: char,
    pub worktree: char,
}

impl GitChange {
    pub fn label(&self) -> char {
        if self.index != ' ' && self.index != '?' {
            self.index
        } else if self.worktree != ' ' {
            self.worktree
        } else {
            '?'
        }
    }

    pub fn is_staged(&self) -> bool {
        self.index != ' ' && self.index != '?'
    }

    pub fn display_path(&self) -> String {
        self.path.to_string_lossy().replace('\\', "/")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommit {
    pub hash: String,
    pub subject: String,
    pub author: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone)]
pub struct GitState {
    pub kind: GitKind,
    pub changes: Vec<GitChange>,
    pub history: Vec<GitCommit>,
    pub last_error: Option<String>,
    git_bin: Option<PathBuf>,
    root: PathBuf,
}

impl Default for GitState {
    fn default() -> Self {
        Self {
            kind: GitKind::GitMissing,
            changes: Vec::new(),
            history: Vec::new(),
            last_error: None,
            git_bin: None,
            root: PathBuf::new(),
        }
    }
}

impl GitState {
    pub fn load(root: PathBuf) -> Self {
        let mut state = Self {
            git_bin: probe_git(),
            root,
            ..Self::default()
        };
        state.reload();
        state
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn reload(&mut self) {
        self.last_error = None;
        let Some(git) = self.git_bin.clone().or_else(probe_git) else {
            self.kind = GitKind::GitMissing;
            self.changes.clear();
            self.history.clear();
            return;
        };
        self.git_bin = Some(git.clone());
        if !self.root.is_dir() {
            self.kind = GitKind::Error("无法读取工作区路径".to_owned());
            self.changes.clear();
            self.history.clear();
            return;
        }
        match rev_parse_inside(&git, &self.root) {
            Ok(true) => {
                self.kind = GitKind::Ready;
                match status_porcelain(&git, &self.root) {
                    Ok(changes) => self.changes = changes,
                    Err(err) => {
                        self.last_error = Some(err);
                        self.changes.clear();
                    }
                }
                match log_recent(&git, &self.root, HISTORY_LIMIT) {
                    Ok(history) => self.history = history,
                    Err(err) => {
                        self.last_error = Some(err);
                        self.history.clear();
                    }
                }
            }
            Ok(false) => {
                self.kind = GitKind::NotARepo;
                self.changes.clear();
                self.history.clear();
            }
            Err(err) => {
                if looks_like_missing_git(&err) {
                    self.kind = GitKind::GitMissing;
                } else if looks_like_not_a_repo(&err) {
                    self.kind = GitKind::NotARepo;
                } else {
                    self.kind = GitKind::Error(err);
                }
                self.changes.clear();
                self.history.clear();
            }
        }
    }

    pub fn stage(&mut self, rel: &Path) -> Result<(), String> {
        let git = self.git()?;
        run_git(git, &self.root, &["add", "--", &rel.to_string_lossy()])?;
        self.reload();
        Ok(())
    }

    pub fn unstage(&mut self, rel: &Path) -> Result<(), String> {
        let git = self.git()?;
        run_git(
            git,
            &self.root,
            &["restore", "--staged", "--", &rel.to_string_lossy()],
        )?;
        self.reload();
        Ok(())
    }

    pub fn commit(&mut self, message: &str) -> Result<(), String> {
        let message = message.trim();
        if message.is_empty() {
            return Err("提交说明不能为空".to_owned());
        }
        let git = self.git()?;
        run_git(git, &self.root, &["commit", "-m", message])?;
        self.reload();
        Ok(())
    }

    fn git(&self) -> Result<&Path, String> {
        self.git_bin
            .as_deref()
            .ok_or_else(|| "未找到 git 可执行文件".to_owned())
    }
}

pub fn probe_git() -> Option<PathBuf> {
    if let Some(raw) = std::env::var_os("PI_LIGHT_GIT_BIN") {
        let path = PathBuf::from(raw);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        for rel in [
            "sidecar/bash/cmd/git.exe",
            "sidecar/bash/mingw64/bin/git.exe",
            "sidecar/bash/cmd/git",
        ] {
            let candidate = exe_dir.join(rel);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    which_git()
}

fn which_git() -> Option<PathBuf> {
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let path_dirs = std::env::var_os("PATH")
        .map(|v| std::env::split_paths(&v).collect::<Vec<_>>())
        .unwrap_or_default();
    for dir in path_dirs {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn parse_porcelain_line(line: &str) -> Option<GitChange> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.len() < 4 {
        return None;
    }
    let bytes = line.as_bytes();
    let index = bytes[0] as char;
    let worktree = bytes[1] as char;
    if bytes[2] != b' ' {
        return None;
    }
    let raw = &line[3..];
    let path = if let Some((_, renamed)) = raw.split_once(" -> ") {
        PathBuf::from(renamed)
    } else {
        PathBuf::from(raw)
    };
    if path.as_os_str().is_empty() {
        return None;
    }
    Some(GitChange {
        path,
        index,
        worktree,
    })
}

pub fn parse_log_line(line: &str) -> Option<GitCommit> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut parts = line.splitn(4, '\t');
    let hash = parts.next()?.trim();
    let subject = parts.next()?.trim();
    let author = parts.next()?.trim();
    let ts = parts.next()?.trim();
    if hash.is_empty() || subject.is_empty() {
        return None;
    }
    let timestamp = ts.parse::<i64>().unwrap_or(0);
    Some(GitCommit {
        hash: hash.to_owned(),
        subject: subject.to_owned(),
        author: author.to_owned(),
        timestamp,
    })
}

pub fn format_commit_time(timestamp: i64) -> String {
    relative_from_now(timestamp, now_secs())
}

pub fn relative_from_now(timestamp: i64, now: i64) -> String {
    let delta = now.saturating_sub(timestamp).max(0);
    if delta < 60 {
        "刚刚".to_owned()
    } else if delta < 3600 {
        format!("{} 分钟前", delta / 60)
    } else if delta < 86400 {
        format!("{} 小时前", delta / 3600)
    } else {
        format!("{} 天前", delta / 86400)
    }
}

pub fn reveal_in_file_manager(path: &Path) -> Result<(), String> {
    let target = if path.exists() {
        path.to_path_buf()
    } else if let Some(parent) = path.parent() {
        parent.to_path_buf()
    } else {
        return Err("路径不存在".to_owned());
    };

    #[cfg(windows)]
    {
        let mut cmd = Command::new("explorer");
        if target.is_file() {
            cmd.arg(format!("/select,{}", target.display()));
        } else {
            cmd.arg(target.as_os_str());
        }
        apply_no_window(&mut cmd);
        cmd.spawn()
            .map(|_| ())
            .map_err(|e| format!("无法打开资源管理器：{e}"))
    }

    #[cfg(target_os = "macos")]
    {
        let mut cmd = Command::new("open");
        if target.is_file() {
            cmd.args(["-R", &target.to_string_lossy()]);
        } else {
            cmd.arg(target.as_os_str());
        }
        cmd.spawn()
            .map(|_| ())
            .map_err(|e| format!("无法打开访达：{e}"))
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let dir = if target.is_file() {
            target.parent().unwrap_or(target.as_path()).to_path_buf()
        } else {
            target
        };
        Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("无法打开文件管理器：{e}"))
    }
}

fn rev_parse_inside(git: &Path, cwd: &Path) -> Result<bool, String> {
    let out = run_git_output(git, cwd, &["rev-parse", "--is-inside-work-tree"])?;
    Ok(out.trim().eq_ignore_ascii_case("true"))
}

fn status_porcelain(git: &Path, cwd: &Path) -> Result<Vec<GitChange>, String> {
    let out = run_git_output(git, cwd, &["status", "--porcelain=v1", "-u"])?;
    Ok(out.lines().filter_map(parse_porcelain_line).collect())
}

fn log_recent(git: &Path, cwd: &Path, limit: usize) -> Result<Vec<GitCommit>, String> {
    let n = limit.to_string();
    let out = run_git_output(git, cwd, &["log", "-n", &n, "--format=%h\t%s\t%an\t%at"])?;
    Ok(out.lines().filter_map(parse_log_line).collect())
}

fn run_git(git: &Path, cwd: &Path, args: &[&str]) -> Result<(), String> {
    run_git_output(git, cwd, args).map(|_| ())
}

fn run_git_output(git: &Path, cwd: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new(git);
    cmd.args(args).current_dir(cwd);
    apply_no_window(&mut cmd);
    let output = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "未找到 git 可执行文件".to_owned()
        } else {
            format!("无法运行 git：{e}")
        }
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    if looks_like_not_a_repo(&combined) {
        return Err(combined);
    }
    if looks_like_missing_git(&combined) {
        return Err(combined);
    }
    Err(if combined.trim().is_empty() {
        format!("git 退出码 {}", output.status.code().unwrap_or(-1))
    } else {
        combined.trim().to_owned()
    })
}

fn looks_like_not_a_repo(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("not a git repository") || lower.contains("不是 git 仓库")
}

fn looks_like_missing_git(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("未找到 git") || lower.contains("not found")
}

fn apply_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd;
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_status_letters() {
        let m = parse_porcelain_line(" M src/ui/dock.rs").unwrap();
        assert_eq!(m.label(), 'M');
        assert!(!m.is_staged());
        let a = parse_porcelain_line("A  src/ui/git_panel.rs").unwrap();
        assert_eq!(a.label(), 'A');
        assert!(a.is_staged());
        let d = parse_porcelain_line(" D src/legacy.rs").unwrap();
        assert_eq!(d.label(), 'D');
        let untracked = parse_porcelain_line("?? new.rs").unwrap();
        assert_eq!(untracked.label(), '?');
        let renamed = parse_porcelain_line("R  old.rs -> src/new.rs").unwrap();
        assert_eq!(renamed.display_path(), "src/new.rs");
    }

    #[test]
    fn parse_log_fields() {
        let c = parse_log_line("a1b2c3d\tfeat: dock\tyanhui\t1710000000").unwrap();
        assert_eq!(c.hash, "a1b2c3d");
        assert_eq!(c.subject, "feat: dock");
        assert_eq!(c.author, "yanhui");
        assert_eq!(c.timestamp, 1_710_000_000);
    }

    #[test]
    fn relative_time_buckets() {
        assert_eq!(relative_from_now(100, 110), "刚刚");
        assert_eq!(relative_from_now(0, 180), "3 分钟前");
        assert_eq!(relative_from_now(0, 7200), "2 小时前");
        assert_eq!(relative_from_now(0, 86400 * 3), "3 天前");
    }

    #[test]
    fn load_non_git_dir_is_honest() {
        let dir = tempfile::tempdir().unwrap();
        let state = GitState::load(dir.path().to_path_buf());
        match state.kind {
            GitKind::NotARepo | GitKind::GitMissing => {}
            other => panic!("expected empty-honest kind, got {other:?}"),
        }
        assert!(state.changes.is_empty());
        assert!(state.history.is_empty());
    }

    #[test]
    fn real_repo_status_and_commit_if_git_available() {
        let Some(git) = probe_git() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        run_git(&git, dir.path(), &["init"]).unwrap();
        run_git(
            &git,
            dir.path(),
            &["config", "user.email", "dev@example.com"],
        )
        .unwrap();
        run_git(&git, dir.path(), &["config", "user.name", "dev"]).unwrap();
        fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        let mut state = GitState::load(dir.path().to_path_buf());
        assert_eq!(state.kind, GitKind::Ready);
        assert!(state.changes.iter().any(|c| c.display_path() == "a.txt"));
        state.stage(Path::new("a.txt")).unwrap();
        assert!(state
            .changes
            .iter()
            .any(|c| c.display_path() == "a.txt" && c.is_staged()));
        state.commit("feat: add a").unwrap();
        assert!(state.changes.is_empty());
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].subject, "feat: add a");
        fs::write(dir.path().join("a.txt"), "two\n").unwrap();
        state.reload();
        state.stage(Path::new("a.txt")).unwrap();
        state.unstage(Path::new("a.txt")).unwrap();
        assert!(state
            .changes
            .iter()
            .any(|c| c.display_path() == "a.txt" && !c.is_staged()));
    }
}
