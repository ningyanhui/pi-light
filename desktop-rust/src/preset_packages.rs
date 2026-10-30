// SPDX-License-Identifier: Apache-2.0
//
// M2 预置 Pi 包：lockfile、发行树路径、把 Slim 本地 path 写入 settings.json。

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde_json::Value;

use crate::pi_pkg::{display_name_version, looks_like_path, PkgOrigin};

const LOCK_JSON: &str = include_str!("../../scripts/preset-packages.lock.json");

const FORBIDDEN_DEFAULT: &[&str] = &["pi-lens", "billion-context"];

/// lockfile 中的一条预置包。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetSpec {
    pub name: String,
    pub version: String,
    pub license: String,
    pub tarball: String,
    pub sha256: String,
    pub dir: String,
    pub role: PresetRole,
    pub default_enabled: bool,
    pub needs_permission_wizard: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetRole {
    Slim,
    Control,
}

impl PresetRole {
    pub fn label(self) -> &'static str {
        match self {
            PresetRole::Slim => "预置",
            PresetRole::Control => "操控",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LockFile {
    packages: Vec<PresetSpec>,
}

fn parse_lock(text: &str) -> Result<LockFile, String> {
    let val: Value =
        serde_json::from_str(text).map_err(|e| format!("preset lock JSON 无法解析：{e}"))?;
    let arr = val
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| "preset lock 缺少 packages".to_owned())?;
    let mut packages = Vec::new();
    for entry in arr {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "preset lock 条目缺 name".to_owned())?
            .to_owned();
        let version = entry
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let role = match entry.get("role").and_then(Value::as_str) {
            Some("slim") => PresetRole::Slim,
            Some("control") => PresetRole::Control,
            other => {
                return Err(format!(
                    "{name}: 未知 role {}",
                    other.unwrap_or("(missing)")
                ))
            }
        };
        packages.push(PresetSpec {
            version,
            license: entry
                .get("license")
                .and_then(Value::as_str)
                .unwrap_or("MIT")
                .to_owned(),
            tarball: entry
                .get("tarball")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            sha256: entry
                .get("sha256")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase(),
            dir: entry
                .get("dir")
                .and_then(Value::as_str)
                .unwrap_or(&name)
                .to_owned(),
            default_enabled: entry
                .get("default_enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            needs_permission_wizard: entry
                .get("needs_permission_wizard")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            name,
            role,
        });
    }
    Ok(LockFile { packages })
}

fn catalog() -> &'static [PresetSpec] {
    use std::sync::OnceLock;
    static CATALOG: OnceLock<Vec<PresetSpec>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            parse_lock(LOCK_JSON)
                .expect("embedded preset lock must parse")
                .packages
        })
        .as_slice()
}

pub fn all_specs() -> &'static [PresetSpec] {
    catalog()
}

pub fn spec_by_name(name: &str) -> Option<&'static PresetSpec> {
    catalog().iter().find(|p| p.name == name)
}

pub fn version_of(name: &str) -> Option<&'static str> {
    spec_by_name(name).map(|p| p.version.as_str())
}

pub fn origin_of(name: &str) -> PkgOrigin {
    match spec_by_name(name).map(|p| p.role) {
        Some(PresetRole::Slim) => PkgOrigin::Slim,
        Some(PresetRole::Control) => PkgOrigin::Control,
        None => PkgOrigin::User,
    }
}

pub fn is_forbidden_default(name: &str) -> bool {
    FORBIDDEN_DEFAULT.contains(&name)
}

/// `{exe_dir}/sidecar/pi-packages`，或 `PI_LIGHT_PACKAGES_DIR`。
pub fn bundled_packages_dir() -> Option<PathBuf> {
    if let Some(raw) = std::env::var_os("PI_LIGHT_PACKAGES_DIR") {
        let path = PathBuf::from(raw);
        if path.is_dir() {
            return Some(path);
        }
        return None;
    }
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))?;
    let dir = exe_dir.join("sidecar").join("pi-packages");
    dir.is_dir().then_some(dir)
}

pub fn package_dir(root: &Path, spec: &PresetSpec) -> PathBuf {
    let mut out = root.to_path_buf();
    for part in spec.dir.split(['/', '\\']) {
        if !part.is_empty() && part != "." && part != ".." {
            out.push(part);
        }
    }
    out
}

pub fn package_present(root: &Path, spec: &PresetSpec) -> bool {
    package_dir(root, spec).join("package.json").is_file()
}

/// 比较键：去掉 Windows 扩展前缀 `\\?\` / `\\?\UNC\`，统一分隔符。
/// Windows 上忽略大小写。8.3 与长路径先 canonicalize，再走此键。
fn path_cmp_key(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('/', "\\");
    let stripped = if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        raw
    };
    if cfg!(windows) {
        stripped.to_ascii_lowercase()
    } else {
        stripped
    }
}

/// 是否指向同一位置。Windows GHA 上 tempfile 可能是 `RUNNER~1` 短路径，
/// `canonicalize` 则给出带 `\\?\` 的长路径；两边都规范化后再比，不假设长路径。
fn paths_equivalent(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    let a_n = fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let b_n = fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    a_n == b_n || path_cmp_key(&a_n) == path_cmp_key(&b_n)
}

fn source_name(source: &str) -> String {
    display_name_version(source).0
}

fn packages_array(val: &mut Value) -> io::Result<&mut Vec<Value>> {
    let obj = val
        .as_object_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "settings.json 非对象"))?;
    let entry = obj
        .entry("packages")
        .or_insert_with(|| Value::Array(Vec::new()));
    entry
        .as_array_mut()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "packages 字段非数组"))
}

fn entry_source(entry: &Value) -> Option<&str> {
    match entry {
        Value::String(s) => Some(s.as_str()),
        Value::Object(obj) => obj.get("source").and_then(Value::as_str),
        _ => None,
    }
}

fn set_entry_source(entry: &mut Value, source: String) {
    match entry {
        Value::String(s) => *s = source,
        Value::Object(obj) => {
            obj.insert("source".to_owned(), Value::String(source));
        }
        _ => *entry = Value::String(source),
    }
}

fn read_settings(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|_| "{}".to_owned());
    serde_json::from_str(&text).unwrap_or(Value::Object(serde_json::Map::new()))
}

fn write_settings(path: &Path, val: &Value) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let out = serde_json::to_string_pretty(val)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(path, out.as_bytes())
}

/// 若 Slim 包已解压且 settings 中没有同名声明，则写入绝对本地 path。
/// 已有 `npm:<name>` 不覆盖。过期的本地 path 会改到当前发行树。
pub fn sync_slim_into_settings(settings_path: &Path, packages_root: &Path) -> io::Result<bool> {
    if !packages_root.is_dir() {
        return Ok(false);
    }
    let mut val = read_settings(settings_path);
    let mut changed = false;
    {
        let arr = packages_array(&mut val)?;
        for spec in catalog().iter().filter(|p| p.role == PresetRole::Slim) {
            if !package_present(packages_root, spec) {
                continue;
            }
            let dest = package_dir(packages_root, spec);
            let dest_str = dest.to_string_lossy().into_owned();
            match arr
                .iter_mut()
                .find(|entry| entry_source(entry).is_some_and(|src| source_name(src) == spec.name))
            {
                Some(entry) => {
                    let src = entry_source(entry).unwrap_or("");
                    if looks_like_path(src)
                        && !paths_equivalent(Path::new(src), &dest)
                        && !Path::new(src).is_dir()
                    {
                        set_entry_source(entry, dest_str);
                        changed = true;
                    }
                }
                None => {
                    arr.push(Value::String(dest_str));
                    changed = true;
                }
            }
        }
    }
    if changed {
        write_settings(settings_path, &val)?;
    }
    Ok(changed)
}

/// 把操控包的本地 path 写入 settings。computer-use 必须 `wizard_ok`。
pub fn enable_control(
    settings_path: &Path,
    packages_root: &Path,
    name: &str,
    wizard_ok: bool,
) -> Result<(), String> {
    let spec = spec_by_name(name).ok_or_else(|| format!("不是预置操控包：{name}"))?;
    if spec.role != PresetRole::Control {
        return Err(format!("{name} 不是默认关闭的操控包"));
    }
    if spec.needs_permission_wizard && !wizard_ok {
        return Err("启用电脑操控前需要完成权限向导。".to_owned());
    }
    if !package_present(packages_root, spec) {
        return Err(format!(
            "未找到预置文件：{}",
            package_dir(packages_root, spec).display()
        ));
    }
    let dest_str = package_dir(packages_root, spec)
        .to_string_lossy()
        .into_owned();
    let mut val = read_settings(settings_path);
    {
        let arr = packages_array(&mut val).map_err(|e| e.to_string())?;
        if arr
            .iter()
            .any(|entry| entry_source(entry).is_some_and(|src| source_name(src) == spec.name))
        {
            return Ok(());
        }
        arr.push(Value::String(dest_str));
    }
    write_settings(settings_path, &val).map_err(|e| e.to_string())
}

pub fn disable_named(settings_path: &Path, name: &str) -> Result<bool, String> {
    let mut val = read_settings(settings_path);
    let changed = {
        let arr = packages_array(&mut val).map_err(|e| e.to_string())?;
        let before = arr.len();
        arr.retain(|entry| entry_source(entry).is_none_or(|src| source_name(src) != name));
        arr.len() != before
    };
    if changed {
        write_settings(settings_path, &val).map_err(|e| e.to_string())?;
    }
    Ok(changed)
}

pub fn computer_use_name() -> &'static str {
    "@injaneity/pi-computer-use"
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_pkg(root: &Path, spec: &PresetSpec, version: &str) {
        let dir = package_dir(root, spec);
        fs::create_dir_all(&dir).unwrap();
        let body = serde_json::json!({
            "name": spec.name,
            "version": version,
        });
        fs::write(
            dir.join("package.json"),
            serde_json::to_vec_pretty(&body).unwrap(),
        )
        .unwrap();
    }

    fn package_sources(settings: &Path) -> Vec<String> {
        let val: Value = serde_json::from_str(&fs::read_to_string(settings).unwrap()).unwrap();
        match val.get("packages").and_then(Value::as_array) {
            Some(arr) => arr
                .iter()
                .filter_map(entry_source)
                .map(str::to_owned)
                .collect(),
            None => Vec::new(),
        }
    }

    #[test]
    fn lock_has_slim_five_and_control_three() {
        let slim: Vec<_> = catalog()
            .iter()
            .filter(|p| p.role == PresetRole::Slim)
            .map(|p| (p.name.as_str(), p.version.as_str()))
            .collect();
        assert_eq!(
            slim,
            [
                ("pi-marketplace", "0.1.3"),
                ("pi-mcp-adapter", "7.0.0"),
                ("pi-web-access", "0.46.0"),
                ("pi-memory", "0.4.2"),
                ("pi-subagents", "0.84.0"),
            ]
        );
        let control: Vec<_> = catalog()
            .iter()
            .filter(|p| p.role == PresetRole::Control)
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(
            control,
            [
                "pi-browser-use",
                "pi-chrome-connector",
                "@injaneity/pi-computer-use",
            ]
        );
        for spec in catalog() {
            assert_eq!(spec.sha256.len(), 64, "{}", spec.name);
            assert!(
                spec.tarball.starts_with("https://registry.npmjs.org/"),
                "{}",
                spec.name
            );
            assert!(!spec.default_enabled || spec.role == PresetRole::Slim);
            assert!(spec.default_enabled || spec.role == PresetRole::Control);
        }
        assert!(
            spec_by_name("@injaneity/pi-computer-use")
                .unwrap()
                .needs_permission_wizard
        );
        assert!(!is_forbidden_default("pi-memory"));
        assert!(is_forbidden_default("pi-lens"));
        assert!(is_forbidden_default("billion-context"));
        assert_eq!(origin_of("pi-marketplace"), PkgOrigin::Slim);
        assert_eq!(origin_of("pi-browser-use"), PkgOrigin::Control);
        assert_eq!(origin_of("pi-lens"), PkgOrigin::User);
    }

    #[test]
    fn sync_writes_slim_local_paths() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("pkgs");
        let settings = tmp.path().join("agent").join("settings.json");
        for spec in catalog().iter().filter(|p| p.role == PresetRole::Slim) {
            write_pkg(&root, spec, &spec.version);
        }
        assert!(sync_slim_into_settings(&settings, &root).unwrap());
        let sources = package_sources(&settings);
        for spec in catalog().iter().filter(|p| p.role == PresetRole::Slim) {
            let dest = package_dir(&root, spec);
            assert!(
                sources
                    .iter()
                    .any(|s| paths_equivalent(Path::new(s), &dest)),
                "missing equivalent path for {} dest={dest:?} sources={sources:?}",
                spec.name
            );
        }
        assert!(!sync_slim_into_settings(&settings, &root).unwrap());
    }

    #[test]
    fn sync_skips_existing_npm_and_repairs_stale_path() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("pkgs");
        let settings = tmp.path().join("settings.json");
        let memory = spec_by_name("pi-memory").unwrap();
        write_pkg(&root, memory, "0.4.2");
        let market = spec_by_name("pi-marketplace").unwrap();
        write_pkg(&root, market, "0.1.3");
        let stale = tmp.path().join("old").join("pi-marketplace");
        fs::write(
            &settings,
            serde_json::to_vec_pretty(&serde_json::json!({
                "packages": [
                    "npm:pi-memory@0.4.2",
                    stale.to_string_lossy().as_ref()
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(sync_slim_into_settings(&settings, &root).unwrap());
        let val: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        let arr = val.get("packages").and_then(Value::as_array).unwrap();
        let joined: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        assert!(joined.iter().any(|s| s == "npm:pi-memory@0.4.2"));
        let market_dir = package_dir(&root, market);
        assert!(
            joined
                .iter()
                .any(|s| paths_equivalent(Path::new(s), &market_dir)),
            "repaired path not equivalent to {market_dir:?}: {joined:?}"
        );
        assert!(!joined.iter().any(|s| s.contains("old")));
    }

    #[test]
    fn path_cmp_key_strips_windows_verbatim_prefix() {
        let verbatim = Path::new(r"\\?\C:\Users\runneradmin\AppData\Local\Temp\pkgs\pi-memory");
        let plain = Path::new(r"C:\Users\runneradmin\AppData\Local\Temp\pkgs\pi-memory");
        assert_eq!(path_cmp_key(verbatim), path_cmp_key(plain));
        let unc = Path::new(r"\\?\UNC\server\share\pkgs\pi-memory");
        let unc_plain = Path::new(r"\\server\share\pkgs\pi-memory");
        assert_eq!(path_cmp_key(unc), path_cmp_key(unc_plain));
    }

    #[test]
    fn paths_equivalent_after_json_backslash_escape() {
        // Windows 上路径写入 pretty JSON 会把 `\` 编成 `\\`，对原文做 contains 必失败。
        let win = r"C:\Users\RUNNER~1\AppData\Local\Temp\.tmpmfaGR3\pkgs\pi-memory";
        let json = serde_json::to_string_pretty(&serde_json::json!({ "packages": [win] })).unwrap();
        assert!(
            !json.contains(win),
            "raw JSON must escape backslashes: {json}"
        );
        let val: Value = serde_json::from_str(&json).unwrap();
        let src = val["packages"][0].as_str().unwrap();
        assert_eq!(src, win);
        assert_eq!(path_cmp_key(Path::new(src)), path_cmp_key(Path::new(win)));
    }

    #[test]
    fn paths_equivalent_matches_canonical_form() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("pkgs").join("pi-memory");
        fs::create_dir_all(&dest).unwrap();
        assert!(paths_equivalent(&dest, &dest));
        let canon = fs::canonicalize(&dest).unwrap();
        assert!(
            paths_equivalent(&dest, &canon),
            "raw={dest:?} canon={canon:?}"
        );
    }

    #[test]
    fn enable_control_requires_wizard_for_computer_use() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("pkgs");
        let settings = tmp.path().join("settings.json");
        let spec = spec_by_name(computer_use_name()).unwrap();
        write_pkg(&root, spec, &spec.version);
        let err = enable_control(&settings, &root, computer_use_name(), false).unwrap_err();
        assert!(err.contains("权限"), "{err}");
        enable_control(&settings, &root, computer_use_name(), true).unwrap();
        let text = fs::read_to_string(&settings).unwrap();
        assert!(text.contains("pi-computer-use"), "{text}");
        assert!(disable_named(&settings, computer_use_name()).unwrap());
        let text = fs::read_to_string(&settings).unwrap();
        assert!(!text.contains("pi-computer-use"), "{text}");
    }

    #[test]
    fn enable_rejects_slim_name() {
        let tmp = TempDir::new().unwrap();
        let err =
            enable_control(&tmp.path().join("s.json"), tmp.path(), "pi-memory", false).unwrap_err();
        assert!(err.contains("操控"), "{err}");
    }
}
