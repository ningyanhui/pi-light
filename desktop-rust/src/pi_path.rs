// SPDX-License-Identifier: Apache-2.0
//
// Pi sidecar 路径解析模块（M1′）
//
// 路径探测顺序（三态契约）：
//   1. 环境变量 PI_LIGHT_PI_BIN              → DetectedLocal(EnvVar)
//   2. {exe_dir}/sidecar/pi/pi.exe (Win)     → BundledReady
//      {exe_dir}/sidecar/pi/pi     (Unix)    → BundledReady
//   3. 探测本机 PATH 中的 pi / pi.exe         → DetectedLocal(PathProbe)
//   (无结果)                                  → NotReady

use std::path::{Path, PathBuf};

// ── 公共类型 ───────────────────────────────────────────────────────────────

/// Pi 二进制就绪状态（M1′ 三态）
#[derive(Debug, Clone, PartialEq)]
pub enum PiProbe {
    /// 在 `{exe_dir}/sidecar/pi/` 下找到捆绑二进制（正式 Release 默认路径）
    BundledReady(PathBuf),
    /// 通过环境变量或 PATH 探测到本机安装
    DetectedLocal { path: PathBuf, source: LocalSource },
    /// 未找到，需用户手动提供
    NotReady,
}

/// DetectedLocal 的来源标记
#[derive(Debug, Clone, PartialEq)]
pub enum LocalSource {
    /// 通过 `PI_LIGHT_PI_BIN` 环境变量指定
    EnvVar,
    /// 通过系统 PATH 探测到
    PathProbe,
}

impl PiProbe {
    /// 返回探测到的路径（NotReady 时为 None）
    pub fn path(&self) -> Option<&Path> {
        match self {
            PiProbe::BundledReady(p) => Some(p.as_path()),
            PiProbe::DetectedLocal { path, .. } => Some(path.as_path()),
            PiProbe::NotReady => None,
        }
    }

    /// 是否就绪（BundledReady 或 DetectedLocal）
    pub fn is_ready(&self) -> bool {
        !matches!(self, PiProbe::NotReady)
    }

    /// 人类可读的来源描述（中文）
    pub fn source_label(&self) -> &'static str {
        match self {
            PiProbe::BundledReady(_) => "内嵌 sidecar",
            PiProbe::DetectedLocal {
                source: LocalSource::EnvVar,
                ..
            } => "环境变量 PI_LIGHT_PI_BIN",
            PiProbe::DetectedLocal {
                source: LocalSource::PathProbe,
                ..
            } => "本机 PATH",
            PiProbe::NotReady => "未就绪",
        }
    }
}

// ── 公开探测入口 ────────────────────────────────────────────────────────────

/// 按探测顺序返回 Pi 就绪状态。
///
/// 每次调用均重新探测，供 UI「重新探测」按钮调用。
pub fn probe() -> PiProbe {
    let env_bin = std::env::var_os("PI_LIGHT_PI_BIN").map(PathBuf::from);
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();

    probe_impl(env_bin.as_deref(), exe_dir.as_deref(), &path_dirs)
}

// ── 可测试的内部实现 ────────────────────────────────────────────────────────

/// 依赖注入版本：接受显式参数，方便单元测试。
///
/// - `env_bin`：`PI_LIGHT_PI_BIN` 变量的值（None 表示未设置）
/// - `exe_dir`：可执行文件所在目录（None 表示无法获取）
/// - `path_dirs`：PATH 中的目录列表
pub(crate) fn probe_impl(
    env_bin: Option<&Path>,
    exe_dir: Option<&Path>,
    path_dirs: &[PathBuf],
) -> PiProbe {
    let bin_name = pi_binary_name();

    // 1. 环境变量覆盖（开发优先）
    if let Some(p) = env_bin {
        if p.is_file() {
            return PiProbe::DetectedLocal {
                path: p.to_path_buf(),
                source: LocalSource::EnvVar,
            };
        }
    }

    // 2. 捆绑 sidecar 相对路径
    if let Some(dir) = exe_dir {
        let sidecar = dir.join("sidecar").join("pi").join(bin_name);
        if sidecar.is_file() {
            return PiProbe::BundledReady(sidecar);
        }
    }

    // 3. 探测本机 PATH
    for dir in path_dirs {
        let candidate = dir.join(bin_name);
        if candidate.is_file() {
            return PiProbe::DetectedLocal {
                path: candidate,
                source: LocalSource::PathProbe,
            };
        }
    }

    PiProbe::NotReady
}

/// 按当前平台返回 Pi 二进制文件名。
pub fn pi_binary_name() -> &'static str {
    if cfg!(windows) {
        "pi.exe"
    } else {
        "pi"
    }
}

// ── 单元测试 ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// 在给定目录下创建一个空的可执行文件占位（仅测试用）
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
    fn not_ready_when_nothing_found() {
        let result = probe_impl(None, None, &[]);
        assert_eq!(result, PiProbe::NotReady);
    }

    #[test]
    fn env_var_takes_priority_over_sidecar() {
        let tmp = TempDir::new().unwrap();
        let bin_name = pi_binary_name();

        // 环境变量指向的二进制
        let env_bin = touch(tmp.path(), bin_name);

        // sidecar 目录中也放一个（应被 env 覆盖）
        let sidecar_dir = tmp.path().join("sidecar").join("pi");
        fs::create_dir_all(&sidecar_dir).unwrap();
        touch(&sidecar_dir, bin_name);

        let result = probe_impl(Some(env_bin.as_path()), Some(tmp.path()), &[]);
        assert!(
            matches!(
                result,
                PiProbe::DetectedLocal {
                    source: LocalSource::EnvVar,
                    ..
                }
            ),
            "expected DetectedLocal(EnvVar), got {:?}",
            result
        );
        assert_eq!(result.path(), Some(env_bin.as_path()));
    }

    #[test]
    fn sidecar_bundled_ready() {
        let tmp = TempDir::new().unwrap();
        let bin_name = pi_binary_name();

        let sidecar_dir = tmp.path().join("sidecar").join("pi");
        fs::create_dir_all(&sidecar_dir).unwrap();
        let sidecar_bin = touch(&sidecar_dir, bin_name);

        let result = probe_impl(None, Some(tmp.path()), &[]);
        assert!(
            matches!(result, PiProbe::BundledReady(_)),
            "expected BundledReady, got {:?}",
            result
        );
        assert_eq!(result.path(), Some(sidecar_bin.as_path()));
    }

    #[test]
    fn path_probe_detected_local() {
        let tmp = TempDir::new().unwrap();
        let bin_name = pi_binary_name();
        let path_bin = touch(tmp.path(), bin_name);

        let result = probe_impl(None, None, &[tmp.path().to_path_buf()]);
        assert!(
            matches!(
                result,
                PiProbe::DetectedLocal {
                    source: LocalSource::PathProbe,
                    ..
                }
            ),
            "expected DetectedLocal(PathProbe), got {:?}",
            result
        );
        assert_eq!(result.path(), Some(path_bin.as_path()));
    }

    #[test]
    fn env_var_missing_file_falls_through_to_sidecar() {
        let tmp = TempDir::new().unwrap();
        let bin_name = pi_binary_name();

        // env_bin 路径不存在（is_file() 为 false）
        let nonexistent = tmp.path().join("nonexistent_pi");

        let sidecar_dir = tmp.path().join("sidecar").join("pi");
        fs::create_dir_all(&sidecar_dir).unwrap();
        let sidecar_bin = touch(&sidecar_dir, bin_name);

        let result = probe_impl(Some(nonexistent.as_path()), Some(tmp.path()), &[]);
        assert!(
            matches!(result, PiProbe::BundledReady(_)),
            "expected BundledReady after env_bin miss, got {:?}",
            result
        );
        assert_eq!(result.path(), Some(sidecar_bin.as_path()));
    }

    #[test]
    fn sidecar_takes_priority_over_path() {
        let tmp = TempDir::new().unwrap();
        let bin_name = pi_binary_name();

        // sidecar
        let sidecar_dir = tmp.path().join("sidecar").join("pi");
        fs::create_dir_all(&sidecar_dir).unwrap();
        let sidecar_bin = touch(&sidecar_dir, bin_name);

        // PATH 中也有
        let path_dir = TempDir::new().unwrap();
        touch(path_dir.path(), bin_name);

        let result = probe_impl(None, Some(tmp.path()), &[path_dir.path().to_path_buf()]);
        assert!(
            matches!(result, PiProbe::BundledReady(_)),
            "expected BundledReady (sidecar > PATH), got {:?}",
            result
        );
        assert_eq!(result.path(), Some(sidecar_bin.as_path()));
    }

    #[test]
    fn probe_helpers_work() {
        let probe_ready = PiProbe::BundledReady(PathBuf::from("/fake/pi.exe"));
        assert!(probe_ready.is_ready());
        assert_eq!(probe_ready.source_label(), "内嵌 sidecar");

        let probe_env = PiProbe::DetectedLocal {
            path: PathBuf::from("/env/pi.exe"),
            source: LocalSource::EnvVar,
        };
        assert!(probe_env.is_ready());
        assert_eq!(probe_env.source_label(), "环境变量 PI_LIGHT_PI_BIN");

        let probe_path = PiProbe::DetectedLocal {
            path: PathBuf::from("/usr/local/bin/pi"),
            source: LocalSource::PathProbe,
        };
        assert!(probe_path.is_ready());
        assert_eq!(probe_path.source_label(), "本机 PATH");

        let probe_not = PiProbe::NotReady;
        assert!(!probe_not.is_ready());
        assert!(probe_not.path().is_none());
    }
}
