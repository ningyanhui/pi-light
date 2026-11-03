// SPDX-License-Identifier: Apache-2.0
// Clean-room brand mark for the native viewport (not eframe's default "E").
// Geometry matches desktop-rust/assets/pi-light.svg: violet disc + π.

use egui::IconData;

const VIOLET: [u8; 4] = [0x7C, 0x3A, 0xED, 0xFF];
const VIOLET_RING: [u8; 4] = [0x5B, 0x21, 0xB6, 0xFF];
const WHITE: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

/// 256×256 RGBA brand icon used by the native window / taskbar.
pub fn icon_data() -> IconData {
    rasterize(256)
}

fn rasterize(size: u32) -> IconData {
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let s = size as f32;
    let cx = (s - 1.0) * 0.5;
    let cy = cx;
    let outer = s * 0.46;
    let inner = s * 0.40;
    let aa = 1.15_f32;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if d > outer + aa {
                continue;
            }
            let cover = if d <= outer {
                1.0
            } else {
                ((outer + aa - d) / aa).clamp(0.0, 1.0)
            };
            let col = if d > inner { VIOLET_RING } else { VIOLET };
            put(&mut rgba, size, x, y, col, cover);
        }
    }

    fill_round_rect(
        &mut rgba,
        size,
        s * 0.26,
        s * 0.30,
        s * 0.74,
        s * 0.40,
        s * 0.035,
    );
    fill_round_rect(
        &mut rgba,
        size,
        s * 0.33,
        s * 0.32,
        s * 0.43,
        s * 0.72,
        s * 0.035,
    );
    fill_round_rect(
        &mut rgba,
        size,
        s * 0.57,
        s * 0.32,
        s * 0.67,
        s * 0.72,
        s * 0.035,
    );

    IconData {
        rgba,
        width: size,
        height: size,
    }
}

fn fill_round_rect(buf: &mut [u8], size: u32, x0: f32, y0: f32, x1: f32, y1: f32, radius: f32) {
    let r = radius.max(0.5);
    for y in 0..size {
        for x in 0..size {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let cx = px.clamp(x0 + r, x1 - r);
            let cy = py.clamp(y0 + r, y1 - r);
            let dist = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            if dist <= r + 0.55 {
                let i = ((y * size + x) * 4) as usize;
                if buf[i + 3] > 80 {
                    buf[i..i + 4].copy_from_slice(&WHITE);
                }
            }
        }
    }
}

fn put(buf: &mut [u8], size: u32, x: u32, y: u32, color: [u8; 4], cover: f32) {
    let i = ((y * size + x) * 4) as usize;
    let a = (f32::from(color[3]) * cover) as u8;
    buf[i] = color[0];
    buf[i + 1] = color[1];
    buf[i + 2] = color[2];
    buf[i + 3] = a;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_icon_is_256_rgba_and_not_blank() {
        let icon = icon_data();
        assert_eq!(icon.width, 256);
        assert_eq!(icon.height, 256);
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        let opaque = icon.rgba.chunks(4).filter(|p| p[3] > 200).count();
        assert!(opaque > 1000, "icon should have a filled disc");
        let i = (128 * 256 + 128) * 4;
        let (r, g, b, a) = (
            icon.rgba[i],
            icon.rgba[i + 1],
            icon.rgba[i + 2],
            icon.rgba[i + 3],
        );
        assert!(a > 200);
        assert!(
            r > 80 && b > 180,
            "centre should be violet, got {r},{g},{b}"
        );
    }
}
