// SPDX-License-Identifier: Apache-2.0
//
// Cloudflare Tunnel（cloudflared）探测、首次下载与临时 quick tunnel。
// 二进制不进 git；版本与 SHA-256 钉在 scripts/cloudflared.lock.json。

use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use serde::Deserialize;

const LOCK_JSON: &str = include_str!("../../scripts/cloudflared.lock.json");
const USER_AGENT: &str = "pi-light/0.2.2 (+https://github.com/ningyanhui/pi-light)";

// ── Pin ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct CloudflaredLock {
    pub version: String,
    pub license: String,
    pub repository: String,
    pub artifacts: std::collections::BTreeMap<String, LockedArtifact>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LockedArtifact {
    pub file: String,
    pub url: String,
    pub sha256: String,
    pub install_name: String,
}

pub fn load_lock() -> CloudflaredLock {
    serde_json::from_str(LOCK_JSON).expect("cloudflared.lock.json 必须是合法 JSON")
}

pub fn locked_artifact(key: &str) -> Option<LockedArtifact> {
    load_lock().artifacts.get(key).cloned()
}

/// 当前编译目标对应的发行物。macOS 等未钉平台返回 None（只探测 PATH / 环境变量）。
pub fn current_artifact() -> Option<LockedArtifact> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        return locked_artifact("windows-amd64");
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return locked_artifact("linux-amd64");
    }
    #[allow(unreachable_code)]
    None
}

pub fn install_name() -> &'static str {
    if cfg!(windows) {
        "cloudflared.exe"
    } else {
        "cloudflared"
    }
}

// ── Probe ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudflaredSource {
    EnvVar,
    Bundled,
    UserDir,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudflaredProbe {
    Missing,
    Found {
        path: PathBuf,
        source: CloudflaredSource,
    },
}

impl CloudflaredProbe {
    pub fn path(&self) -> Option<&Path> {
        match self {
            CloudflaredProbe::Found { path, .. } => Some(path.as_path()),
            CloudflaredProbe::Missing => None,
        }
    }

    pub fn source_label(&self) -> &'static str {
        match self {
            CloudflaredProbe::Missing => "未找到",
            CloudflaredProbe::Found {
                source: CloudflaredSource::EnvVar,
                ..
            } => "环境变量 PI_LIGHT_CLOUDFLARED",
            CloudflaredProbe::Found {
                source: CloudflaredSource::Bundled,
                ..
            } => "内嵌 sidecar/relay",
            CloudflaredProbe::Found {
                source: CloudflaredSource::UserDir,
                ..
            } => "用户目录 ~/.pi-light/relay",
            CloudflaredProbe::Found {
                source: CloudflaredSource::Path,
                ..
            } => "系统 PATH",
        }
    }
}

pub fn probe() -> CloudflaredProbe {
    let env_bin = std::env::var_os("PI_LIGHT_CLOUDFLARED").map(PathBuf::from);
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let user_dir = user_relay_dir();
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();
    probe_impl(
        env_bin.as_deref(),
        exe_dir.as_deref(),
        user_dir.as_deref(),
        &path_dirs,
    )
}

pub fn user_relay_dir() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".pi-light").join("relay"))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub(crate) fn probe_impl(
    env_bin: Option<&Path>,
    exe_dir: Option<&Path>,
    user_dir: Option<&Path>,
    path_dirs: &[PathBuf],
) -> CloudflaredProbe {
    let name = install_name();
    if let Some(p) = env_bin {
        if p.is_file() {
            return CloudflaredProbe::Found {
                path: p.to_path_buf(),
                source: CloudflaredSource::EnvVar,
            };
        }
    }
    if let Some(dir) = exe_dir {
        let bundled = dir.join("sidecar").join("relay").join(name);
        if bundled.is_file() {
            return CloudflaredProbe::Found {
                path: bundled,
                source: CloudflaredSource::Bundled,
            };
        }
    }
    if let Some(dir) = user_dir {
        let p = dir.join(name);
        if p.is_file() {
            return CloudflaredProbe::Found {
                path: p,
                source: CloudflaredSource::UserDir,
            };
        }
    }
    for dir in path_dirs {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return CloudflaredProbe::Found {
                path: candidate,
                source: CloudflaredSource::Path,
            };
        }
    }
    CloudflaredProbe::Missing
}

// ── Status / process ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunnelStatus {
    Idle,
    BinaryMissing,
    Downloading,
    Starting,
    Connected { public_url: String },
    Error(String),
}

impl TunnelStatus {
    pub fn label_zh(&self) -> String {
        match self {
            TunnelStatus::Idle => "未连接".to_owned(),
            TunnelStatus::BinaryMissing => "未找到 cloudflared".to_owned(),
            TunnelStatus::Downloading => "正在下载 cloudflared…".to_owned(),
            TunnelStatus::Starting => "正在建立中继…".to_owned(),
            TunnelStatus::Connected { .. } => "已连接".to_owned(),
            TunnelStatus::Error(e) => format!("出错：{e}"),
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, TunnelStatus::Connected { .. })
    }

    pub fn public_url(&self) -> Option<&str> {
        match self {
            TunnelStatus::Connected { public_url } => Some(public_url.as_str()),
            _ => None,
        }
    }
}

enum RelayEvent {
    Line(String),
}

enum DownloadEvent {
    Ok(PathBuf),
    Err(String),
}

pub struct RelayManager {
    pub status: TunnelStatus,
    pub probe: CloudflaredProbe,
    child: Option<Child>,
    lines: Option<Receiver<RelayEvent>>,
    download_rx: Option<Receiver<DownloadEvent>>,
    last_lines: Vec<String>,
}

impl std::fmt::Debug for RelayManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelayManager")
            .field("status", &self.status)
            .field("probe", &self.probe)
            .finish_non_exhaustive()
    }
}

impl Default for RelayManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RelayManager {
    fn drop(&mut self) {
        self.stop();
    }
}

impl RelayManager {
    pub fn new() -> Self {
        let probe = probe();
        let status = if probe.path().is_some() {
            TunnelStatus::Idle
        } else {
            TunnelStatus::BinaryMissing
        };
        Self {
            status,
            probe,
            child: None,
            lines: None,
            download_rx: None,
            last_lines: Vec::new(),
        }
    }

    pub fn refresh_probe(&mut self) {
        self.probe = probe();
        if self.child.is_none() && !matches!(self.status, TunnelStatus::Downloading) {
            self.status = if self.probe.path().is_some() {
                TunnelStatus::Idle
            } else {
                TunnelStatus::BinaryMissing
            };
        }
    }

    pub fn wants_repaint(&self) -> bool {
        matches!(
            self.status,
            TunnelStatus::Downloading | TunnelStatus::Starting
        ) || self.download_rx.is_some()
            || self.lines.is_some()
    }

    /// 启动 trycloudflare 临时隧道，把本机 `http://127.0.0.1:{port}` 暴露到公网。
    /// 不需要 Cloudflare 账号；主机名每次启动都会变。
    pub fn start_quick(&mut self, local_port: u16) -> Result<(), String> {
        self.stop();
        let bin = self
            .probe
            .path()
            .ok_or_else(|| {
                "未找到 cloudflared。请先下载，或把可执行文件放到 sidecar/relay/。".to_owned()
            })?
            .to_path_buf();
        let url = format!("http://127.0.0.1:{local_port}");
        let mut cmd = Command::new(&bin);
        cmd.arg("tunnel")
            .arg("--no-autoupdate")
            .arg("--url")
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hide_window(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("启动 cloudflared 失败：{e}"))?;
        let (tx, rx) = mpsc::channel();
        spawn_line_reader(child.stdout.take(), tx.clone());
        spawn_line_reader(child.stderr.take(), tx);
        self.child = Some(child);
        self.lines = Some(rx);
        self.last_lines.clear();
        self.status = TunnelStatus::Starting;
        Ok(())
    }

    /// 使用 Cloudflare 命名隧道 token（`cloudflared tunnel run --token`）。
    /// 公网主机名需用户自行填写。
    pub fn start_named(&mut self, token: &str) -> Result<(), String> {
        let token = token.trim();
        if token.is_empty() {
            return Err("命名隧道需要 Cloudflare Tunnel token。".to_owned());
        }
        self.stop();
        let bin = self
            .probe
            .path()
            .ok_or_else(|| "未找到 cloudflared。".to_owned())?
            .to_path_buf();
        let mut cmd = Command::new(&bin);
        cmd.arg("tunnel")
            .arg("--no-autoupdate")
            .arg("run")
            .arg("--token")
            .arg(token)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hide_window(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("启动 cloudflared 失败：{e}"))?;
        let (tx, rx) = mpsc::channel();
        spawn_line_reader(child.stdout.take(), tx.clone());
        spawn_line_reader(child.stderr.take(), tx);
        self.child = Some(child);
        self.lines = Some(rx);
        self.last_lines.clear();
        self.status = TunnelStatus::Starting;
        Ok(())
    }

    pub fn mark_named_connected(&mut self, public_url: String) {
        if self.child.is_some() {
            self.status = TunnelStatus::Connected { public_url };
        }
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.lines = None;
        if !matches!(self.status, TunnelStatus::Downloading) {
            self.status = if self.probe.path().is_some() {
                TunnelStatus::Idle
            } else {
                TunnelStatus::BinaryMissing
            };
        }
    }

    pub fn start_download(&mut self, dest_dir: PathBuf) -> Result<(), String> {
        if self.download_rx.is_some() {
            return Ok(());
        }
        let art = current_artifact()
            .ok_or_else(|| "当前平台未钉 cloudflared 发行物，请自行安装并加入 PATH。".to_owned())?;
        self.status = TunnelStatus::Downloading;
        let (tx, rx) = mpsc::channel();
        self.download_rx = Some(rx);
        thread::spawn(move || {
            let result = download_verified(&art, &dest_dir);
            let ev = match result {
                Ok(p) => DownloadEvent::Ok(p),
                Err(e) => DownloadEvent::Err(e),
            };
            let _ = tx.send(ev);
        });
        Ok(())
    }

    /// 每帧调用。返回是否建议重绘。
    pub fn poll(&mut self) -> bool {
        let mut dirty = false;
        if let Some(rx) = &self.download_rx {
            match rx.try_recv() {
                Ok(DownloadEvent::Ok(path)) => {
                    self.download_rx = None;
                    let source = if path
                        .components()
                        .any(|c| c.as_os_str() == std::ffi::OsStr::new("sidecar"))
                    {
                        CloudflaredSource::Bundled
                    } else {
                        CloudflaredSource::UserDir
                    };
                    self.probe = CloudflaredProbe::Found { path, source };
                    if !matches!(
                        self.status,
                        TunnelStatus::Starting | TunnelStatus::Connected { .. }
                    ) {
                        self.status = TunnelStatus::Idle;
                    }
                    dirty = true;
                }
                Ok(DownloadEvent::Err(e)) => {
                    self.download_rx = None;
                    self.status = TunnelStatus::Error(e);
                    dirty = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.download_rx = None;
                    if matches!(self.status, TunnelStatus::Downloading) {
                        self.status = TunnelStatus::Error("下载线程已结束。".to_owned());
                    }
                    dirty = true;
                }
            }
        }
        if let Some(rx) = &self.lines {
            while let Ok(ev) = rx.try_recv() {
                dirty = true;
                let RelayEvent::Line(line) = ev;
                if self.last_lines.len() >= 40 {
                    self.last_lines.remove(0);
                }
                self.last_lines.push(line.clone());
                if let Some(url) = parse_quick_tunnel_url(&line) {
                    self.status = TunnelStatus::Connected { public_url: url };
                }
            }
        }
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.child = None;
                    self.lines = None;
                    if !self.status.is_connected() {
                        let hint = self
                            .last_lines
                            .iter()
                            .rev()
                            .find(|l| !l.trim().is_empty())
                            .cloned()
                            .unwrap_or_else(|| format!("退出码 {status}"));
                        self.status = TunnelStatus::Error(truncate_err(&hint));
                    } else {
                        self.status = TunnelStatus::Idle;
                    }
                    dirty = true;
                }
                Ok(None) => {}
                Err(e) => {
                    self.status = TunnelStatus::Error(format!("轮询 cloudflared 失败：{e}"));
                    dirty = true;
                }
            }
        }
        dirty
    }
}

fn spawn_line_reader<R>(pipe: Option<R>, tx: std::sync::mpsc::Sender<RelayEvent>)
where
    R: Read + Send + 'static,
{
    let Some(pipe) = pipe else { return };
    thread::spawn(move || {
        let reader = io::BufReader::new(pipe);
        for line in io::BufRead::lines(reader).map_while(Result::ok) {
            if tx.send(RelayEvent::Line(line)).is_err() {
                break;
            }
        }
    });
}

fn hide_window(_cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        _cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

fn truncate_err(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= 160 {
        s.to_owned()
    } else {
        format!("{}…", s.chars().take(160).collect::<String>())
    }
}

/// 从 cloudflared 日志里抽出 trycloudflare 公网 URL。
pub fn parse_quick_tunnel_url(text: &str) -> Option<String> {
    const PREFIX: &str = "https://";
    const SUFFIX: &str = ".trycloudflare.com";
    let mut rest = text;
    while let Some(start) = rest.find(PREFIX) {
        let slice = &rest[start..];
        if let Some(suf) = slice.find(SUFFIX) {
            let end = suf + SUFFIX.len();
            let url = &slice[..end];
            if is_plausible_https_host(url) {
                return Some(url.to_owned());
            }
            rest = &slice[PREFIX.len()..];
        } else {
            break;
        }
    }
    None
}

fn is_plausible_https_host(url: &str) -> bool {
    let Some(host) = url.strip_prefix("https://") else {
        return false;
    };
    !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn download_verified(art: &LockedArtifact, dest_dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dest_dir).map_err(|e| format!("创建目录失败：{e}"))?;
    let dest = dest_dir.join(&art.install_name);
    let tmp = dest_dir.join(format!("{}.download", art.install_name));
    let resp = ureq::get(&art.url)
        .set("User-Agent", USER_AGENT)
        .timeout(Duration::from_secs(180))
        .call()
        .map_err(|e| format!("下载 cloudflared 失败：{e}"))?;
    if resp.status() != 200 {
        return Err(format!("下载 cloudflared HTTP {}", resp.status()));
    }
    let mut reader = resp.into_reader();
    let mut file = fs::File::create(&tmp).map_err(|e| format!("写入临时文件失败：{e}"))?;
    let mut buf = [0u8; 64 * 1024];
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("读取下载流失败：{e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])
            .map_err(|e| format!("写入下载文件失败：{e}"))?;
    }
    file.flush().ok();
    drop(file);
    let got = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let expected = art.sha256.to_ascii_lowercase();
    if got != expected {
        let _ = fs::remove_file(&tmp);
        return Err(format!(
            "cloudflared SHA-256 不符：期望 {expected}，实际 {got}"
        ));
    }
    if dest.exists() {
        let _ = fs::remove_file(&dest);
    }
    fs::rename(&tmp, &dest).map_err(|e| format!("就位 cloudflared 失败：{e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dest, fs::Permissions::from_mode(0o755));
    }
    Ok(dest)
}

/// 校验已下载文件的 SHA-256。
pub fn verify_file_sha256(path: &Path, expected: &str) -> Result<(), String> {
    let got = sha256_file(path).map_err(|e| format!("读取文件失败：{e}"))?;
    if got != expected.to_ascii_lowercase() {
        return Err(format!(
            "SHA-256 不符：期望 {}，实际 {got}",
            expected.to_ascii_lowercase()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn lock_file_parses_and_shas_are_hex() {
        let lock = load_lock();
        assert_eq!(lock.version, "2026.10.6");
        assert_eq!(lock.license, "Apache-2.0");
        let win = lock.artifacts.get("windows-amd64").expect("windows");
        let lin = lock.artifacts.get("linux-amd64").expect("linux");
        assert_eq!(win.sha256.len(), 64);
        assert_eq!(lin.sha256.len(), 64);
        assert!(win.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(win.url.contains("2026.10.6"));
        assert_eq!(win.install_name, "cloudflared.exe");
        assert_eq!(lin.install_name, "cloudflared");
    }

    #[test]
    fn notice_lists_pinned_windows_sha() {
        let notice = include_str!("../../NOTICE");
        let win = locked_artifact("windows-amd64").unwrap();
        assert!(notice.contains(&win.sha256));
        assert!(notice.contains("2026.10.6"));
        assert!(notice.contains("Apache-2.0"));
        assert!(notice.contains("cloudflared"));
    }

    #[test]
    fn parse_trycloudflare_from_banner() {
        let log = "\
2026-10-30T10:00:00Z INF +--------------------------------------------------------------------------------------------+\n\
2026-10-30T10:00:00Z INF |  Your quick Tunnel has been created! Visit it at (it may take some time to be reachable):  |\n\
2026-10-30T10:00:00Z INF |  https://alpha-beta-1234.trycloudflare.com                                               |\n\
2026-10-30T10:00:00Z INF +--------------------------------------------------------------------------------------------+\n";
        assert_eq!(
            parse_quick_tunnel_url(log).as_deref(),
            Some("https://alpha-beta-1234.trycloudflare.com")
        );
    }

    #[test]
    fn parse_trycloudflare_ignores_unrelated_https() {
        assert!(parse_quick_tunnel_url("see https://example.com/docs").is_none());
        assert!(parse_quick_tunnel_url("").is_none());
    }

    #[test]
    fn probe_order_env_then_bundled_then_user_then_path() {
        let root = TempDir::new().unwrap();
        let env = root.path().join("from-env");
        fs::write(&env, b"x").unwrap();
        let exe = root.path().join("app");
        fs::create_dir_all(exe.join("sidecar").join("relay")).unwrap();
        fs::write(exe.join("sidecar").join("relay").join(install_name()), b"b").unwrap();
        let user = root.path().join("user");
        fs::create_dir_all(&user).unwrap();
        fs::write(user.join(install_name()), b"u").unwrap();
        let path_dir = root.path().join("path");
        fs::create_dir_all(&path_dir).unwrap();
        fs::write(path_dir.join(install_name()), b"p").unwrap();

        let path_dirs = [path_dir];
        let found = probe_impl(Some(&env), Some(&exe), Some(&user), &path_dirs);
        assert!(matches!(
            found,
            CloudflaredProbe::Found {
                source: CloudflaredSource::EnvVar,
                ..
            }
        ));

        let found = probe_impl(None, Some(&exe), Some(&user), &path_dirs);
        assert!(matches!(
            found,
            CloudflaredProbe::Found {
                source: CloudflaredSource::Bundled,
                ..
            }
        ));

        let found = probe_impl(None, None, Some(&user), &path_dirs);
        assert!(matches!(
            found,
            CloudflaredProbe::Found {
                source: CloudflaredSource::UserDir,
                ..
            }
        ));

        let found = probe_impl(None, None, None, &path_dirs);
        assert!(matches!(
            found,
            CloudflaredProbe::Found {
                source: CloudflaredSource::Path,
                ..
            }
        ));

        assert_eq!(probe_impl(None, None, None, &[]), CloudflaredProbe::Missing);
    }

    #[test]
    fn verify_sha_roundtrip_file() {
        let dir = TempDir::new().unwrap();
        let p = dir.path().join("blob");
        fs::write(&p, b"hello-cloudflared").unwrap();
        let hex = sha256_file(&p).unwrap();
        assert_eq!(hex.len(), 64);
        verify_file_sha256(&p, &hex).unwrap();
        assert!(verify_file_sha256(&p, "00".repeat(32).as_str()).is_err());
    }

    #[test]
    fn status_labels_are_honest() {
        let idle = TunnelStatus::Idle;
        assert_eq!(idle.label_zh(), "未连接");
        assert!(!idle.is_connected());
        let connected = TunnelStatus::Connected {
            public_url: "https://x.trycloudflare.com".into(),
        };
        assert!(connected.is_connected());
        assert_eq!(connected.public_url(), Some("https://x.trycloudflare.com"));
    }
}
