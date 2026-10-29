// SPDX-License-Identifier: Apache-2.0
//
// Pi sidecar 路径解析模块（M0′ stub）
//
// 路径探测顺序（AGENTS.md 中的 M1′ 契约）：
//   1. 环境变量 PI_LIGHT_PI_BIN
//   2. 捆绑相对路径 {exe_dir}/sidecar/pi/pi.exe（Windows）
//      或 {exe_dir}/sidecar/pi/pi（其他平台）
//   3. （M1′+）可选：探测本机 PATH 中的 pi
//
// M0′ 只完成路径解析，不实际 spawn 进程。

use std::path::PathBuf;

/// 按探测顺序返回可用的 Pi 二进制路径，若均未找到则返回 None。
///
/// 此函数在 M0′ 阶段仅用于 UI 状态展示。
/// 实际进程管理在 M1′（路径约定完整）和 M2′（RPC 连接）阶段实现。
pub fn resolve_pi_path() -> Option<PathBuf> {
    // 1. 环境变量覆盖（开发优先）
    if let Ok(env_path) = std::env::var("PI_LIGHT_PI_BIN") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return Some(p);
        }
    }

    // 2. 捆绑 sidecar 相对路径
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let sidecar = exe_dir
                .join("sidecar")
                .join("pi")
                .join(pi_binary_name());
            if sidecar.is_file() {
                return Some(sidecar);
            }
        }
    }

    // 3. （M1′+）PATH 探测占位 — 尚未实现
    // TODO(M1′): which::which("pi").ok()

    None
}

/// 按平台返回 Pi 二进制名称。
fn pi_binary_name() -> &'static str {
    if cfg!(windows) {
        "pi.exe"
    } else {
        "pi"
    }
}
