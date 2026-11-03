// SPDX-License-Identifier: Apache-2.0
//
// Bash/MSYS 环境路径探测模块（便携 bash sidecar）
//
// 探测顺序：
//   1. {exe_dir}/sidecar/bash/usr/bin/{bash.exe,sh.exe}  → Bundled
//      MinGit-2.47.1.2 zip 只有 sh.exe；Release 打包会复制为 bash.exe
//   2. 系统 PATH 中的 bash / bash.exe                      → System
//   (均无)                                                 → NotAvailable
//
// PATH 注入：bundled_path_dirs() 返回需要前置插入子进程 PATH 的目录列表。
// 调用者（pi_rpc::PiRpcClient::spawn）在 spawn 时一次性注入，满足"单次校验入口"约束。

use std::{
    path::{Path, PathBuf},
    process::Command,
};

// ── 公共类型 ───────────────────────────────────────────────────────────────

/// Bash/MSYS 环境就绪状态
#[derive(Debug, Clone, PartialEq)]
pub enum BashProbe {
    /// 捆绑的 MinGit POSIX 壳（打包后为 `usr/bin/bash.exe`，zip 原件为 `sh.exe`）
    Bundled { bash_exe: PathBuf },
    /// 系统 PATH 中探测到 bash
    System { bash_exe: PathBuf },
    /// 均未找到（非 Windows 时不影响 Pi 运行；系统 bash 通常在 PATH 中）
    NotAvailable,
}

impl BashProbe {
    pub fn bash_exe(&self) -> Option<&Path> {
        match self {
            BashProbe::Bundled { bash_exe } | BashProbe::System { bash_exe } => {
                Some(bash_exe.as_path())
            }
            BashProbe::NotAvailable => None,
        }
    }

    pub fn is_available(&self) -> bool {
        !matches!(self, BashProbe::NotAvailable)
    }

    pub fn source_label(&self) -> &'static str {
        match self {
            BashProbe::Bundled { .. } => "内嵌 sidecar",
            BashProbe::System { .. } => "系统 PATH",
            BashProbe::NotAvailable => "不可用",
        }
    }
}

/// `C:\Windows\System32\bash.exe` 是 WSL 启动器，不能当 Git Bash / 默认 shellPath。
pub fn is_wsl_launcher(path: &Path) -> bool {
    let normalized = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    normalized.contains(r"\windows\system32\bash.exe")
        || normalized.contains(r"\windows\sysnative\bash.exe")
}

// ── 公开探测入口 ────────────────────────────────────────────────────────────

/// 按探测顺序返回 bash 就绪状态。
///
/// 每次调用均重新探测；pi_rpc::spawn 负责缓存（一次 spawn = 一次探测）。
pub fn probe() -> BashProbe {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();
    let known = known_system_git_bash();

    probe_impl_full(exe_dir.as_deref(), &path_dirs, &known)
}

/// 给 sidecar / `pi install` 注入 PATH 与 SHELL。内嵌 MinGit 优先，排除 WSL。
pub fn apply_to_command(cmd: &mut Command) {
    let probe = probe();
    let mut extras = bundled_path_dirs();
    if extras.is_empty() {
        if let Some(exe) = probe.bash_exe() {
            if !is_wsl_launcher(exe) {
                if let Some(dir) = exe.parent() {
                    extras.push(dir.to_path_buf());
                }
            }
        }
    }
    if !extras.is_empty() {
        let base = std::env::var_os("PATH").unwrap_or_default();
        let all: Vec<PathBuf> = extras
            .into_iter()
            .chain(std::env::split_paths(&base))
            .collect();
        if let Ok(new_path) = std::env::join_paths(&all) {
            cmd.env("PATH", new_path);
        }
    }
    if let Some(exe) = probe.bash_exe() {
        if !is_wsl_launcher(exe) {
            cmd.env("SHELL", exe);
        }
    }
}

/// 返回需要前置插入子进程 PATH 的捆绑目录列表（有序，优先级由高到低）。
///
/// 仅捆绑 sidecar 存在时返回非空。非 Windows 返回空列表（bash 在 PATH 中自然可用）。
pub fn bundled_path_dirs() -> Vec<PathBuf> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let Some(exe_dir) = exe_dir else {
        return vec![];
    };
    bundled_path_dirs_from(exe_dir.as_path())
}

// ── 可测试的内部实现 ────────────────────────────────────────────────────────

/// 依赖注入版本：接受显式参数，方便单元测试。
#[cfg(test)]
pub(crate) fn probe_impl(exe_dir: Option<&Path>, path_dirs: &[PathBuf]) -> BashProbe {
    probe_impl_full(exe_dir, path_dirs, &[])
}

/// 与 [`probe_impl`] 相同，另加「常见自定义盘 Git」候选（测试可注入）。
pub(crate) fn probe_impl_full(
    exe_dir: Option<&Path>,
    path_dirs: &[PathBuf],
    known_system: &[PathBuf],
) -> BashProbe {
    // 1. 捆绑 sidecar（仅 Windows）：先 bash.exe，再 MinGit 原件 sh.exe
    if let Some(dir) = exe_dir {
        for d in bundled_path_dirs_from(dir) {
            for name in bundled_shell_names() {
                let candidate = d.join(name);
                if candidate.is_file() {
                    return BashProbe::Bundled {
                        bash_exe: candidate,
                    };
                }
            }
        }
    }

    // 2. 系统 PATH 回退（只认正式 bash 名；跳过 WSL 启动器）
    let bash_name = bash_binary_name();
    for dir in path_dirs {
        let candidate = dir.join(bash_name);
        if is_wsl_launcher(&candidate) {
            continue;
        }
        if candidate.is_file() {
            return BashProbe::System {
                bash_exe: candidate,
            };
        }
    }

    // 3. 常见 Git 安装位置（含 D:\programs\Git\bin\bash.exe 等自定义盘）
    for candidate in known_system {
        if is_wsl_launcher(candidate) {
            continue;
        }
        if candidate.is_file() {
            return BashProbe::System {
                bash_exe: candidate.clone(),
            };
        }
    }

    BashProbe::NotAvailable
}

/// 常见 Git Bash 相对盘符路径。含 Program Files 与自定义 `programs\Git`。
pub fn known_git_bash_relatives() -> &'static [&'static str] {
    &[
        "programs/Git/bin/bash.exe",
        "Program Files/Git/bin/bash.exe",
        "Program Files (x86)/Git/bin/bash.exe",
        "Git/bin/bash.exe",
        "dev/Git/bin/bash.exe",
        "tools/Git/bin/bash.exe",
    ]
}

fn known_system_git_bash() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut out = Vec::new();
        if let Ok(root) = std::env::var("ProgramFiles") {
            out.push(PathBuf::from(root).join("Git").join("bin").join("bash.exe"));
        }
        if let Ok(root) = std::env::var("ProgramFiles(x86)") {
            out.push(PathBuf::from(root).join("Git").join("bin").join("bash.exe"));
        }
        if let Ok(root) = std::env::var("LOCALAPPDATA") {
            out.push(
                PathBuf::from(root)
                    .join("Programs")
                    .join("Git")
                    .join("bin")
                    .join("bash.exe"),
            );
        }
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:\\", letter as char);
            for rel in known_git_bash_relatives() {
                out.push(PathBuf::from(&drive).join(rel));
            }
        }
        out
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// 从给定的 exe_dir 计算捆绑 bash 所需的 PATH 目录列表。
///
/// MinGit 解压后的目录结构：
///   sidecar/bash/usr/bin/     ← sh.exe；打包脚本再复制为 bash.exe 供 Pi 查找
///   sidecar/bash/mingw64/bin/ ← 工具链
///   sidecar/bash/cmd/         ← git.exe
///
/// 非 Windows 返回空列表（bash 无需额外注入）。
pub(crate) fn bundled_path_dirs_from(exe_dir: &Path) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let bash_root = exe_dir.join("sidecar").join("bash");
        [
            bash_root.join("usr").join("bin"),
            bash_root.join("mingw64").join("bin"),
            bash_root.join("cmd"),
        ]
        .into_iter()
        .filter(|d| d.is_dir())
        .collect()
    }
    #[cfg(not(windows))]
    {
        let _ = exe_dir;
        vec![]
    }
}

/// 当前平台的 bash 二进制文件名。
pub fn bash_binary_name() -> &'static str {
    if cfg!(windows) {
        "bash.exe"
    } else {
        "bash"
    }
}

/// 捆绑 sidecar 内按优先级查找的壳文件名。
/// Windows MinGit zip 只有 `sh.exe`；Release 会另放 `bash.exe`。
fn bundled_shell_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["bash.exe", "sh.exe"]
    } else {
        &["bash"]
    }
}

// ── 单元测试 ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, b"").expect("touch failed");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).expect("chmod failed");
        }
        p
    }

    #[test]
    fn not_available_when_nothing_found() {
        assert_eq!(probe_impl(None, &[]), BashProbe::NotAvailable);
    }

    #[test]
    fn system_bash_fallback() {
        let tmp = TempDir::new().unwrap();
        let bash_name = bash_binary_name();
        let bash_exe = touch(tmp.path(), bash_name);

        let result = probe_impl(None, &[tmp.path().to_path_buf()]);
        assert!(
            matches!(result, BashProbe::System { .. }),
            "expected System, got {:?}",
            result
        );
        assert_eq!(result.bash_exe(), Some(bash_exe.as_path()));
    }

    #[cfg(windows)]
    #[test]
    fn bundled_detected_in_usr_bin() {
        let tmp = TempDir::new().unwrap();
        let usr_bin = tmp
            .path()
            .join("sidecar")
            .join("bash")
            .join("usr")
            .join("bin");
        fs::create_dir_all(&usr_bin).unwrap();
        let bash_exe = touch(&usr_bin, "bash.exe");

        let result = probe_impl(Some(tmp.path()), &[]);
        assert!(
            matches!(result, BashProbe::Bundled { .. }),
            "expected Bundled, got {:?}",
            result
        );
        assert_eq!(result.bash_exe(), Some(bash_exe.as_path()));
    }

    #[cfg(windows)]
    #[test]
    fn bundled_detected_mingit_sh_exe() {
        // MinGit-2.47.1.2-64-bit.zip ships usr/bin/sh.exe, not bash.exe.
        let tmp = TempDir::new().unwrap();
        let usr_bin = tmp
            .path()
            .join("sidecar")
            .join("bash")
            .join("usr")
            .join("bin");
        fs::create_dir_all(&usr_bin).unwrap();
        let sh_exe = touch(&usr_bin, "sh.exe");

        let result = probe_impl(Some(tmp.path()), &[]);
        assert!(
            matches!(result, BashProbe::Bundled { .. }),
            "expected Bundled from MinGit sh.exe, got {:?}",
            result
        );
        assert_eq!(result.bash_exe(), Some(sh_exe.as_path()));
    }

    #[cfg(windows)]
    #[test]
    fn bundled_takes_priority_over_system() {
        let tmp = TempDir::new().unwrap();
        let usr_bin = tmp
            .path()
            .join("sidecar")
            .join("bash")
            .join("usr")
            .join("bin");
        fs::create_dir_all(&usr_bin).unwrap();
        touch(&usr_bin, "bash.exe");

        let sys_tmp = TempDir::new().unwrap();
        touch(sys_tmp.path(), "bash.exe");

        let result = probe_impl(Some(tmp.path()), &[sys_tmp.path().to_path_buf()]);
        assert!(
            matches!(result, BashProbe::Bundled { .. }),
            "expected Bundled (sidecar > system), got {:?}",
            result
        );
    }

    #[test]
    fn probe_helpers_work() {
        let b = BashProbe::Bundled {
            bash_exe: PathBuf::from("/fake/bash.exe"),
        };
        assert!(b.is_available());
        assert_eq!(b.source_label(), "内嵌 sidecar");

        let s = BashProbe::System {
            bash_exe: PathBuf::from("/usr/bin/bash"),
        };
        assert!(s.is_available());
        assert_eq!(s.source_label(), "系统 PATH");

        let n = BashProbe::NotAvailable;
        assert!(!n.is_available());
        assert!(n.bash_exe().is_none());
    }

    #[test]
    fn wsl_launcher_is_never_default() {
        assert!(is_wsl_launcher(Path::new(r"C:\Windows\System32\bash.exe")));
        assert!(is_wsl_launcher(Path::new(r"C:/Windows/Sysnative/bash.exe")));
        assert!(!is_wsl_launcher(Path::new(r"D:\programs\Git\bin\bash.exe")));
        assert!(!is_wsl_launcher(Path::new(
            r"C:\Program Files\Git\bin\bash.exe"
        )));

        let result = probe_impl(None, &[PathBuf::from(r"C:\Windows\System32")]);
        assert_eq!(result, BashProbe::NotAvailable);
    }

    #[test]
    fn known_custom_drive_git_beats_wsl_path() {
        let tmp = TempDir::new().unwrap();
        let git_bin = tmp.path().join("programs").join("Git").join("bin");
        fs::create_dir_all(&git_bin).unwrap();
        let bash = touch(&git_bin, "bash.exe");

        let result = probe_impl_full(
            None,
            &[PathBuf::from(r"C:\Windows\System32")],
            std::slice::from_ref(&bash),
        );
        assert!(
            matches!(result, BashProbe::System { .. }),
            "expected System Git, got {:?}",
            result
        );
        assert_eq!(result.bash_exe(), Some(bash.as_path()));
    }

    #[test]
    fn known_relatives_include_custom_programs_git() {
        assert!(known_git_bash_relatives()
            .iter()
            .any(|p| p.eq_ignore_ascii_case("programs/Git/bin/bash.exe")));
    }
}
