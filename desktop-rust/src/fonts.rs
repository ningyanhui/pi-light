// SPDX-License-Identifier: Apache-2.0
//
// 系统 CJK 字体：作为 egui 回退，避免中文显示成方框。
// 不捆绑字体文件。Windows 优先微软雅黑（msyh.ttc / msyh.ttf）。

use std::path::{Path, PathBuf};

use eframe::egui;

const FONT_ID: &str = "pi-light-cjk";

/// 在默认拉丁字体之后挂上系统 CJK 字体。找不到字体时保持 egui 默认。
pub fn install_system_cjk(ctx: &egui::Context) {
    let Some(bytes) = load_first_font(&candidate_paths()) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    // index 0：ttc 里的常规体（微软雅黑 msyh.ttc 的 Regular）。
    let mut data = egui::FontData::from_owned(bytes);
    data.index = 0;
    fonts.font_data.insert(FONT_ID.to_owned(), data);
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        if let Some(list) = fonts.families.get_mut(&family) {
            list.push(FONT_ID.to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

/// 候选路径。`PI_LIGHT_CJK_FONT` 优先，其次是当前系统的常见 CJK 字体。
pub fn candidate_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(raw) = std::env::var_os("PI_LIGHT_CJK_FONT") {
        let path = PathBuf::from(raw);
        if !path.as_os_str().is_empty() {
            paths.push(path);
        }
    }
    let windir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    paths.extend(system_cjk_candidates(std::env::consts::OS, &windir));
    paths
}

/// 按操作系统给出系统 CJK 字体路径，不含环境变量覆盖。
pub fn system_cjk_candidates(os_family: &str, windir: &Path) -> Vec<PathBuf> {
    match os_family {
        "windows" => {
            let fonts = windir.join("Fonts");
            [
                "msyh.ttc",
                "msyh.ttf",
                "msyhl.ttc",
                "simhei.ttf",
                "simsun.ttc",
                "msjh.ttc",
            ]
            .into_iter()
            .map(|name| fonts.join(name))
            .collect()
        }
        "macos" => vec![
            PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
            PathBuf::from("/System/Library/Fonts/Hiragino Sans GB.ttc"),
            PathBuf::from("/System/Library/Fonts/STHeiti Light.ttc"),
        ],
        _ => vec![
            PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
            PathBuf::from("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc"),
            PathBuf::from("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc"),
            PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf"),
            PathBuf::from("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc"),
        ],
    }
}

fn load_first_font(paths: &[PathBuf]) -> Option<Vec<u8>> {
    for path in paths {
        if let Ok(bytes) = std::fs::read(path) {
            if looks_like_font(&bytes) {
                return Some(bytes);
            }
        }
    }
    None
}

fn looks_like_font(bytes: &[u8]) -> bool {
    bytes.starts_with(b"ttcf")
        || bytes.starts_with(b"OTTO")
        || bytes.starts_with(b"true")
        || bytes.starts_with(&[0x00, 0x01, 0x00, 0x00])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_prefers_microsoft_yahei() {
        let windir = Path::new(r"C:\Windows");
        let paths = system_cjk_candidates("windows", windir);
        assert!(paths[0].ends_with("msyh.ttc"));
        assert!(paths[1].ends_with("msyh.ttf"));
        assert!(paths.iter().any(|p| p.ends_with("simsun.ttc")));
    }

    #[test]
    fn non_windows_does_not_lead_with_yahei() {
        let paths = system_cjk_candidates("linux", Path::new("/usr"));
        assert!(!paths[0].ends_with("msyh.ttc"));
        assert!(!paths.is_empty());
    }

    #[test]
    fn font_magic_accepts_ttc_and_ttf() {
        assert!(looks_like_font(b"ttcfxxxx"));
        assert!(looks_like_font(b"OTTOxxxx"));
        assert!(looks_like_font(&[0x00, 0x01, 0x00, 0x00, 0x11]));
        assert!(!looks_like_font(b"not a font"));
        assert!(!looks_like_font(b""));
    }

    #[test]
    fn skips_non_font_and_reads_next() {
        let dir = tempfile::tempdir().unwrap();
        let bogus = dir.path().join("notes.txt");
        let font = dir.path().join("YaHei.ttc");
        std::fs::write(&bogus, b"hello").unwrap();
        std::fs::write(&font, b"ttcf-fake-font").unwrap();
        let loaded = load_first_font(&[bogus, font]).unwrap();
        assert!(loaded.starts_with(b"ttcf"));
    }
}
