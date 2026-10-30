// SPDX-License-Identifier: Apache-2.0
//
// Pi 包管理（M1）：经 sidecar 执行 `pi install` / `pi list` / `pi remove`，
// 并从 settings.json 的 `packages` 数组列出已安装项。
//
// 安装必须显式确认（`confirmed == true`），避免列表按钮直接落盘。

use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// 包声明所在作用域。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkgScope {
    User,
    Project,
}

impl PkgScope {
    pub fn label(self) -> &'static str {
        match self {
            PkgScope::User => "用户",
            PkgScope::Project => "项目",
        }
    }
}

/// 包来源：发行树预置、操控预置或用户自行安装。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PkgOrigin {
    Slim,
    Control,
    #[default]
    User,
}

impl PkgOrigin {
    pub fn badge(self) -> Option<&'static str> {
        match self {
            PkgOrigin::Slim => Some("预置"),
            PkgOrigin::Control => Some("操控"),
            PkgOrigin::User => None,
        }
    }
}

/// settings.json / `pi list` 中的一条已安装包。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPackage {
    pub source: String,
    pub name: String,
    pub version: Option<String>,
    pub scope: PkgScope,
    pub filtered: bool,
    pub installed_path: Option<String>,
    pub origin: PkgOrigin,
}

/// 规范化 `npm:<name>` / `npm:<name>@<ver>` 规格。
pub fn npm_spec(name: &str, version: Option<&str>) -> Result<String, String> {
    let name = name.trim();
    if !is_valid_npm_name(name) {
        return Err(format!("非法 npm 包名：{name}"));
    }
    match version.map(str::trim).filter(|v| !v.is_empty()) {
        Some(ver) => {
            if !is_valid_npm_version(ver) {
                return Err(format!("非法版本：{ver}"));
            }
            Ok(format!("npm:{name}@{ver}"))
        }
        None => Ok(format!("npm:{name}")),
    }
}

/// 接受 `npm:…` 或裸包名，返回可交给 `pi install`/`pi remove` 的 source。
pub fn normalize_source(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("包 source 不能为空".to_owned());
    }
    let rest = raw.strip_prefix("npm:").unwrap_or(raw);
    let (name, ver) = split_name_version(rest);
    npm_spec(name, ver)
}

/// 未确认则拒绝执行，避免 UI 误触直接安装。
pub fn install_npm(bin: &Path, source: &str, confirmed: bool) -> Result<String, String> {
    if !confirmed {
        return Err("安装需要确认。第三方 Pi 包可执行代码。".to_owned());
    }
    let source = normalize_source(source)?;
    run_pi_cli(bin, &["install", &source], Duration::from_secs(180))
}

pub fn remove_npm(bin: &Path, source: &str, confirmed: bool) -> Result<String, String> {
    if !confirmed {
        return Err("卸载需要确认。".to_owned());
    }
    let source = normalize_source(source)?;
    run_pi_cli(bin, &["remove", &source], Duration::from_secs(60))
}

pub fn list_cli(bin: &Path) -> Result<Vec<InstalledPackage>, String> {
    let out = run_pi_cli(bin, &["list"], Duration::from_secs(30))?;
    Ok(parse_list_output(&out))
}

/// 从用户级 / 项目级 settings.json 读取 `packages`。
pub fn load_installed_packages(
    user_settings: Option<&Path>,
    project_settings: Option<&Path>,
) -> Vec<InstalledPackage> {
    let mut out = Vec::new();
    if let Some(path) = user_settings {
        out.extend(parse_settings_packages(path, PkgScope::User));
    }
    if let Some(path) = project_settings {
        for pkg in parse_settings_packages(path, PkgScope::Project) {
            if !out.iter().any(|existing| existing.source == pkg.source) {
                out.push(pkg);
            }
        }
    }
    out
}

pub fn parse_settings_packages(path: &Path, scope: PkgScope) -> Vec<InstalledPackage> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    parse_packages_json(&text, scope)
}

pub fn parse_packages_json(text: &str, scope: PkgScope) -> Vec<InstalledPackage> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(arr) = val.get("packages").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|entry| package_from_value(entry, scope))
        .collect()
}

fn package_from_value(entry: &serde_json::Value, scope: PkgScope) -> Option<InstalledPackage> {
    match entry {
        serde_json::Value::String(source) => Some(installed_from_source(source, scope, false)),
        serde_json::Value::Object(obj) => {
            let source = obj.get("source").and_then(|v| v.as_str())?;
            let filtered = ["extensions", "skills", "prompts", "themes"]
                .iter()
                .any(|key| obj.get(*key).is_some());
            Some(installed_from_source(source, scope, filtered))
        }
        _ => None,
    }
}

fn installed_from_source(source: &str, scope: PkgScope, filtered: bool) -> InstalledPackage {
    let (mut name, mut version) = display_name_version(source);
    if looks_like_path(source) {
        if let Some((pkg_name, pkg_ver)) = read_package_json_identity(Path::new(source.trim())) {
            if !pkg_name.is_empty() {
                name = pkg_name;
            }
            if version.is_none() {
                version = pkg_ver;
            }
        }
    }
    let origin = crate::preset_packages::origin_of(&name);
    InstalledPackage {
        source: source.to_owned(),
        name,
        version,
        scope,
        filtered,
        installed_path: None,
        origin,
    }
}

fn read_package_json_identity(dir: &Path) -> Option<(String, Option<String>)> {
    let text = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let val: serde_json::Value = serde_json::from_str(&text).ok()?;
    let name = val
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let version = val
        .get("version")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    Some((name, version))
}

/// 解析 `pi list` 的人类可读输出（chalk 无色时的纯文本）。
pub fn parse_list_output(text: &str) -> Vec<InstalledPackage> {
    let mut out: Vec<InstalledPackage> = Vec::new();
    let mut scope = PkgScope::User;
    let mut last: Option<usize> = None;
    for raw in text.lines() {
        let line = strip_ansi(raw).trim_end().to_owned();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("No packages installed.") {
            continue;
        }
        if trimmed.eq_ignore_ascii_case("User packages:") {
            scope = PkgScope::User;
            last = None;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("Project packages:") {
            scope = PkgScope::Project;
            last = None;
            continue;
        }
        if line.starts_with("    ") || line.starts_with('\t') {
            if let Some(idx) = last {
                let path = trimmed.to_owned();
                if !path.is_empty() {
                    out[idx].installed_path = Some(path);
                }
            }
            continue;
        }
        let mut display = trimmed.trim_start_matches(['-', '*', ' ']).to_owned();
        let filtered = display.ends_with("(filtered)");
        if filtered {
            display = display.trim_end_matches("(filtered)").trim().to_owned();
        }
        if display.is_empty() {
            continue;
        }
        out.push(installed_from_source(&display, scope, filtered));
        last = Some(out.len() - 1);
    }
    out
}

pub fn looks_like_path(source: &str) -> bool {
    let s = source.trim();
    if s.starts_with("npm:") || s.starts_with("git:") || s.starts_with("github:") {
        return false;
    }
    s.starts_with('.')
        || s.starts_with('/')
        || s.starts_with('\\')
        || s.contains('/')
        || s.contains('\\')
        || (s.len() >= 2 && s.as_bytes().get(1) == Some(&b':'))
}

pub fn display_name_version(source: &str) -> (String, Option<String>) {
    let trimmed = source.trim();
    if looks_like_path(trimmed) {
        return path_display_name(trimmed);
    }
    let rest = trimmed
        .strip_prefix("npm:")
        .or_else(|| trimmed.strip_prefix("git:"))
        .unwrap_or(trimmed);
    let (name, ver) = split_name_version(rest);
    (name.to_owned(), ver.map(str::to_owned))
}

fn path_display_name(source: &str) -> (String, Option<String>) {
    let normalized = source.replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');
    let mut parts: Vec<&str> = trimmed.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return (source.to_owned(), None);
    }
    let last = parts.pop().unwrap_or("");
    let name = if last.starts_with('@') {
        last.to_owned()
    } else if let Some(scope) = parts.last().copied().filter(|p| p.starts_with('@')) {
        format!("{scope}/{last}")
    } else {
        last.to_owned()
    };
    (name, None)
}

fn split_name_version(spec: &str) -> (&str, Option<&str>) {
    if let Some(rest) = spec.strip_prefix('@') {
        if let Some(slash) = rest.find('/') {
            let after = &rest[slash + 1..];
            if let Some(at) = after.rfind('@') {
                let name_end = slash + 1 + at;
                return (&spec[..=name_end], Some(&after[at + 1..]));
            }
        }
        return (spec, None);
    }
    if let Some(at) = spec.rfind('@') {
        if at > 0 {
            return (&spec[..at], Some(&spec[at + 1..]));
        }
    }
    (spec, None)
}

fn is_valid_npm_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 214 {
        return false;
    }
    if let Some(rest) = name.strip_prefix('@') {
        let Some((scope, pkg)) = rest.split_once('/') else {
            return false;
        };
        return is_valid_unscoped(scope) && is_valid_unscoped(pkg);
    }
    is_valid_unscoped(name)
}

fn is_valid_unscoped(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.starts_with('_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn is_valid_npm_version(ver: &str) -> bool {
    !ver.is_empty()
        && ver.len() <= 64
        && ver
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'))
}

fn strip_ansi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            i += 2;
            while i < bytes.len() && !(bytes[i] >= 0x40 && bytes[i] <= 0x7e) {
                i += 1;
            }
            i += 1;
            continue;
        }
        let ch = s[i..].chars().next().unwrap_or('\u{fffd}');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn run_pi_cli(bin: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    if !bin.is_file() {
        return Err(format!("未找到 Pi 程序：{}", bin.display()));
    }
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| format!("无法启动 Pi：{e}"))?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                let mut err = String::new();
                if let Some(mut s) = stdout.take() {
                    let _ = s.read_to_string(&mut out);
                }
                if let Some(mut s) = stderr.take() {
                    let _ = s.read_to_string(&mut err);
                }
                let combined = join_out_err(&out, &err);
                if status.success() {
                    return Ok(combined);
                }
                let msg = if combined.trim().is_empty() {
                    format!("pi {} 失败（退出码 {status}）", args.join(" "))
                } else {
                    truncate(&combined, 800)
                };
                return Err(msg);
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("pi {} 超时", args.join(" ")));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => return Err(format!("等待 Pi 退出失败：{e}")),
        }
    }
}

fn join_out_err(out: &str, err: &str) -> String {
    match (out.trim().is_empty(), err.trim().is_empty()) {
        (false, false) => format!("{}\n{}", out.trim(), err.trim()),
        (false, true) => out.trim().to_owned(),
        (true, false) => err.trim().to_owned(),
        (true, true) => String::new(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_owned();
    }
    let boundary = s
        .char_indices()
        .take_while(|(i, _)| *i < max)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(max);
    format!("{}…", &s[..boundary])
}

/// 测试辅助：把 source 写进 settings.json 的 packages 数组（不调用 sidecar）。
#[cfg(test)]
fn write_packages_for_test(path: &Path, sources: &[&str]) {
    let arr: Vec<serde_json::Value> = sources
        .iter()
        .map(|s| serde_json::Value::String((*s).to_owned()))
        .collect();
    let val = serde_json::json!({ "packages": arr });
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, serde_json::to_vec_pretty(&val).unwrap()).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn npm_spec_plain_and_versioned() {
        assert_eq!(
            npm_spec("pi-web-access", None).unwrap(),
            "npm:pi-web-access"
        );
        assert_eq!(
            npm_spec("pi-web-access", Some("0.46.0")).unwrap(),
            "npm:pi-web-access@0.46.0"
        );
        assert_eq!(
            npm_spec("@injaneity/pi-computer-use", Some("0.5.1")).unwrap(),
            "npm:@injaneity/pi-computer-use@0.5.1"
        );
    }

    #[test]
    fn npm_spec_rejects_shell_metacharacters() {
        assert!(npm_spec("foo;rm", None).is_err());
        assert!(npm_spec("../evil", None).is_err());
        assert!(npm_spec("foo bar", None).is_err());
        assert!(npm_spec("", None).is_err());
        assert!(npm_spec("ok", Some("1.0.0;id")).is_err());
    }

    #[test]
    fn normalize_accepts_bare_or_prefixed() {
        assert_eq!(normalize_source("pi-memory").unwrap(), "npm:pi-memory");
        assert_eq!(
            normalize_source("npm:pi-memory@0.4.2").unwrap(),
            "npm:pi-memory@0.4.2"
        );
    }

    #[test]
    fn install_refuses_without_confirm() {
        let err = install_npm(Path::new("/definitely-missing-pi"), "npm:foo", false).unwrap_err();
        assert!(err.contains("确认"), "{err}");
    }

    #[test]
    fn remove_refuses_without_confirm() {
        let err = remove_npm(Path::new("/definitely-missing-pi"), "npm:foo", false).unwrap_err();
        assert!(err.contains("确认"), "{err}");
    }

    #[test]
    fn parse_packages_string_and_object() {
        let json = r#"{
            "packages": [
                "npm:pi-memory@0.4.2",
                {"source": "npm:pi-web-access", "skills": []},
                {"source": "npm:@scope/pkg"}
            ]
        }"#;
        let pkgs = parse_packages_json(json, PkgScope::User);
        assert_eq!(pkgs.len(), 3);
        assert_eq!(pkgs[0].name, "pi-memory");
        assert_eq!(pkgs[0].version.as_deref(), Some("0.4.2"));
        assert!(!pkgs[0].filtered);
        assert_eq!(pkgs[1].source, "npm:pi-web-access");
        assert!(pkgs[1].filtered);
        assert_eq!(pkgs[2].name, "@scope/pkg");
    }

    #[test]
    fn load_merges_user_then_project() {
        let dir = TempDir::new().unwrap();
        let user = dir.path().join("user.json");
        let project = dir.path().join("project.json");
        write_packages_for_test(&user, &["npm:pi-memory"]);
        write_packages_for_test(&project, &["npm:pi-memory", "npm:pi-lens"]);
        let pkgs = load_installed_packages(Some(&user), Some(&project));
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].scope, PkgScope::User);
        assert_eq!(pkgs[1].name, "pi-lens");
        assert_eq!(pkgs[1].scope, PkgScope::Project);
    }

    #[test]
    fn path_source_uses_package_folder_name() {
        let (name, ver) = display_name_version(r"C:\app\sidecar\pi-packages\pi-marketplace");
        assert_eq!(name, "pi-marketplace");
        assert!(ver.is_none());
        let (scoped, _) =
            display_name_version("/opt/pi-light/sidecar/pi-packages/@injaneity/pi-computer-use");
        assert_eq!(scoped, "@injaneity/pi-computer-use");
        assert!(!looks_like_path("npm:pi-memory@0.4.2"));
        assert!(looks_like_path("/tmp/pi-packages/pi-memory"));
    }

    #[test]
    fn parse_list_user_and_project() {
        let text = "\
User packages:
  npm:pi-marketplace@0.1.3
    /home/u/.pi/agent/npm/pi-marketplace
  npm:pi-web-access (filtered)

Project packages:
  npm:local-tools
";
        let pkgs = parse_list_output(text);
        assert_eq!(pkgs.len(), 3);
        assert_eq!(pkgs[0].name, "pi-marketplace");
        assert_eq!(pkgs[0].version.as_deref(), Some("0.1.3"));
        assert_eq!(
            pkgs[0].installed_path.as_deref(),
            Some("/home/u/.pi/agent/npm/pi-marketplace")
        );
        assert!(pkgs[1].filtered);
        assert_eq!(pkgs[2].scope, PkgScope::Project);
    }

    #[test]
    fn parse_list_empty() {
        assert!(parse_list_output("No packages installed.\n").is_empty());
    }

    #[test]
    fn parse_list_strips_ansi() {
        let text = "User packages:\n  \u{1b}[1mnpm:foo\u{1b}[0m\n";
        let pkgs = parse_list_output(text);
        assert_eq!(pkgs[0].source, "npm:foo");
    }

    #[test]
    fn missing_settings_is_empty() {
        let dir = TempDir::new().unwrap();
        assert!(parse_settings_packages(&dir.path().join("nope.json"), PkgScope::User).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn install_confirmed_runs_sidecar_cli() {
        let dir = TempDir::new().unwrap();
        let bin = dir.path().join("pi");
        fs::write(&bin, "#!/bin/sh\nprintf 'ok %s %s\\n' \"$1\" \"$2\"\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        let out = install_npm(&bin, "pi-memory", true).unwrap();
        assert!(out.contains("install"), "{out}");
        assert!(out.contains("npm:pi-memory"), "{out}");
    }

    #[cfg(unix)]
    #[test]
    fn remove_and_list_cli_with_fake_bin() {
        let dir = TempDir::new().unwrap();
        let bin = dir.path().join("pi");
        fs::write(
            &bin,
            "#!/bin/sh\ncase \"$1\" in\n  list) printf 'User packages:\\n  npm:pi-memory\\n';;\n  remove) printf 'removed %s\\n' \"$2\";;\n  *) exit 2;;\nesac\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        let listed = list_cli(&bin).unwrap();
        assert_eq!(listed[0].name, "pi-memory");
        let removed = remove_npm(&bin, "npm:pi-memory", true).unwrap();
        assert!(removed.contains("removed"), "{removed}");
    }
}
