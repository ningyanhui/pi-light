// SPDX-License-Identifier: Apache-2.0
//
// 主区域走哪条路径。
//
// 默认不自动弹出首次引导。模型与 API 密钥始终可在设置抽屉「模型」页配置。
// Pi 程序位置（环境变量、sidecar 目录、PATH）只在确实没找到程序时，
// 作为折叠起来的高级说明，不占据第一屏。

/// eframe 持久化键：是否显示首次模型引导。缺省或 `"0"` 表示不显示。
pub const SHOW_ONBOARDING_KEY: &str = "pi_light.show_onboarding";

/// 主区域该画什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstRunSurface {
    /// Pi 已就绪，且用户在设置中打开了首次引导。
    ModelSetup,
    /// 没找到 Pi：友好说明。安装步骤不在这一层展开。
    PiMissing,
    /// 进入对话。
    Chat,
}

/// 从 eframe 存储读取开关。只有显式 `"1"` 才打开；缺省为关。
pub fn show_onboarding_from_storage(value: Option<&str>) -> bool {
    value == Some("1")
}

/// 写入 eframe 存储的开关值。
pub fn show_onboarding_storage_value(show: bool) -> &'static str {
    if show {
        "1"
    } else {
        "0"
    }
}

/// `pi_ready` 为真表示探测到了捆绑 sidecar 或本机程序。
///
/// 未就绪 → 缺程序说明。已就绪且 `show_onboarding` → 模型引导。其余进对话。
pub fn first_run_surface(pi_ready: bool, show_onboarding: bool) -> FirstRunSurface {
    if !pi_ready {
        FirstRunSurface::PiMissing
    } else if show_onboarding {
        FirstRunSurface::ModelSetup
    } else {
        FirstRunSurface::Chat
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pi_path::PiProbe;
    use std::path::PathBuf;

    #[test]
    fn storage_default_is_off() {
        assert!(!show_onboarding_from_storage(None));
        assert!(!show_onboarding_from_storage(Some("0")));
        assert!(!show_onboarding_from_storage(Some("")));
        assert!(show_onboarding_from_storage(Some("1")));
        assert_eq!(show_onboarding_storage_value(false), "0");
        assert_eq!(show_onboarding_storage_value(true), "1");
    }

    #[test]
    fn ready_default_is_chat() {
        assert_eq!(first_run_surface(true, false), FirstRunSurface::Chat);
    }

    #[test]
    fn ready_when_opted_in_is_model_setup() {
        assert_eq!(first_run_surface(true, true), FirstRunSurface::ModelSetup);
    }

    #[test]
    fn missing_pi_stays_missing_even_when_guide_opted_in() {
        assert_eq!(first_run_surface(false, false), FirstRunSurface::PiMissing);
        assert_eq!(first_run_surface(false, true), FirstRunSurface::PiMissing);
    }

    #[test]
    fn bundled_sidecar_default_is_chat() {
        let probe = PiProbe::BundledReady(PathBuf::from("sidecar/pi/pi.exe"));
        assert!(probe.is_ready());
        assert_eq!(
            first_run_surface(probe.is_ready(), false),
            FirstRunSurface::Chat
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
    fn first_run_default_skips_model_setup() {
        // 启动默认进对话，不再强制模型引导；未找到 Pi 仍说明缺程序。
        assert_eq!(first_run_surface(true, false), FirstRunSurface::Chat);
        assert_ne!(first_run_surface(true, false), FirstRunSurface::ModelSetup);
        assert_eq!(first_run_surface(false, false), FirstRunSurface::PiMissing);
    }
}
