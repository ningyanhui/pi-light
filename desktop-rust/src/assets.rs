// SPDX-License-Identifier: Apache-2.0
//
// 运行时 UI kit：dock 图标与空态插画。洁净室自制，不引用外部壳主题。

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

const ICON_SESSIONS: &[u8] = include_bytes!("../assets/icon-sessions.png");
const ICON_FILES: &[u8] = include_bytes!("../assets/icon-files.png");
const ICON_GIT: &[u8] = include_bytes!("../assets/icon-git.png");
const EMPTY_FOLDER: &[u8] = include_bytes!("../assets/empty-folder.png");
const EMPTY_NONGIT: &[u8] = include_bytes!("../assets/empty-nongit.png");
const EMPTY_NOPERM: &[u8] = include_bytes!("../assets/empty-noperm.png");

#[derive(Clone)]
pub struct ShellTextures {
    pub icon_sessions: Option<TextureHandle>,
    pub icon_files: Option<TextureHandle>,
    pub icon_git: Option<TextureHandle>,
    pub empty_folder: Option<TextureHandle>,
    pub empty_nongit: Option<TextureHandle>,
    pub empty_noperm: Option<TextureHandle>,
}

impl ShellTextures {
    pub fn load(ctx: &egui::Context) -> Self {
        Self {
            icon_sessions: load_texture(ctx, "icon_sessions", ICON_SESSIONS),
            icon_files: load_texture(ctx, "icon_files", ICON_FILES),
            icon_git: load_texture(ctx, "icon_git", ICON_GIT),
            empty_folder: load_texture(ctx, "empty_folder", EMPTY_FOLDER),
            empty_nongit: load_texture(ctx, "empty_nongit", EMPTY_NONGIT),
            empty_noperm: load_texture(ctx, "empty_noperm", EMPTY_NOPERM),
        }
    }
}

fn load_texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let image = decode_rgba(bytes)?;
    Some(ctx.load_texture(name, image, TextureOptions::LINEAR))
}

fn decode_rgba(bytes: &[u8]) -> Option<ColorImage> {
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kit_bytes_decode() {
        assert!(decode_rgba(ICON_SESSIONS).is_some());
        assert!(decode_rgba(ICON_FILES).is_some());
        assert!(decode_rgba(ICON_GIT).is_some());
        assert!(decode_rgba(EMPTY_FOLDER).is_some());
        assert!(decode_rgba(EMPTY_NONGIT).is_some());
        assert!(decode_rgba(EMPTY_NOPERM).is_some());
    }
}
