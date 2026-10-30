// SPDX-License-Identifier: Apache-2.0
//
// 首次打开走哪条主路径。
//
// 普通用户先配置模型和 API 密钥。Pi 程序位置（环境变量、sidecar 目录、PATH）
// 只在确实没找到程序时，作为折叠起来的高级说明，不占据第一屏。

/// eframe 持久化键：用户点过「进入对话」。
pub const ONBOARDING_DISMISSED_KEY: &str = "pi_light.onboarding_dismissed";

/// 主区域该画什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstRunSurface {
    /// Pi 已就绪，且用户还没离开引导：主界面是模型与密钥。
    ModelSetup,
    /// 没找到 Pi：友好说明。安装步骤不在这一层展开。
    PiMissing,
    /// 进入对话。
    Chat,
}

/// `pi_ready` 为真表示探测到了捆绑 sidecar 或本机程序。
///
/// 已就绪且未跳过引导 → 模型配置。未就绪 → 缺程序说明，与是否跳过引导无关。
pub fn first_run_surface(pi_ready: bool, onboarding_dismissed: bool) -> FirstRunSurface {
    if !pi_ready {
        FirstRunSurface::PiMissing
    } else if onboarding_dismissed {
        FirstRunSurface::Chat
    } else {
        FirstRunSurface::ModelSetup
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pi_path::PiProbe;
    use std::path::PathBuf;

    #[test]
    fn ready_first_open_is_model_setup() {
        assert_eq!(first_run_surface(true, false), FirstRunSurface::ModelSetup);
    }

    #[test]
    fn ready_after_dismiss_is_chat() {
        assert_eq!(first_run_surface(true, true), FirstRunSurface::Chat);
    }

    #[test]
    fn missing_pi_stays_missing_even_after_dismiss() {
        assert_eq!(first_run_surface(false, false), FirstRunSurface::PiMissing);
        assert_eq!(first_run_surface(false, true), FirstRunSurface::PiMissing);
    }

    #[test]
    fn bundled_sidecar_opens_model_setup() {
        let probe = PiProbe::BundledReady(PathBuf::from("sidecar/pi/pi.exe"));
        assert!(probe.is_ready());
        assert_eq!(
            first_run_surface(probe.is_ready(), false),
            FirstRunSurface::ModelSetup
        );
    }

    #[test]
    fn not_ready_does_not_open_model_setup() {
        assert_eq!(
            first_run_surface(PiProbe::NotReady.is_ready(), false),
            FirstRunSurface::PiMissing
        );
    }

    #[test]
    fn first_run_default_path_is_model_or_missing_not_install_wizard() {
        // 首次引导只走模型配置或「未找到 Pi」；安装方式不是默认主路径。
        assert_ne!(first_run_surface(true, false), FirstRunSurface::Chat);
        assert_eq!(first_run_surface(true, false), FirstRunSurface::ModelSetup);
        assert_eq!(first_run_surface(false, false), FirstRunSurface::PiMissing);
    }
}
