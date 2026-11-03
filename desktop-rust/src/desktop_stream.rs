// SPDX-License-Identifier: Apache-2.0
//
// 远程桌面像素流：JPEG 编码、Windows 主屏捕获、键鼠注入。
// 浏览模式不注入；触控 / 全屏才应用输入。非 Windows 主机诚实报告不可捕获。

use std::time::{SystemTime, UNIX_EPOCH};

use jpeg_encoder::{ColorType, Encoder};
use serde::{Deserialize, Serialize};

pub const FRAME_MAX_WIDTH: u32 = 1280;
pub const JPEG_QUALITY: u8 = 58;
pub const CAPTURE_INTERVAL_MS: u64 = 150;

pub const DESKTOP_SESSION_PATH: &str = "/v1/desktop/session";
pub const DESKTOP_FRAME_PATH: &str = "/v1/desktop/frame";
pub const DESKTOP_INPUT_PATH: &str = "/v1/desktop/input";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DesktopViewMode {
    #[default]
    Browse,
    Touch,
    Fullscreen,
}

impl DesktopViewMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Browse => "browse",
            Self::Touch => "touch",
            Self::Fullscreen => "fullscreen",
        }
    }

    pub fn label_zh(self) -> &'static str {
        match self {
            Self::Browse => "浏览",
            Self::Touch => "触控",
            Self::Fullscreen => "全屏",
        }
    }

    pub fn allows_input(self) -> bool {
        matches!(self, Self::Touch | Self::Fullscreen)
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "touch" | "触控" => Self::Touch,
            "fullscreen" | "full" | "全屏" => Self::Fullscreen,
            _ => Self::Browse,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureStatus {
    Ok,
    PermissionRequired,
    Unavailable,
}

impl CaptureStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::PermissionRequired => "permission_required",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputStatus {
    Ok,
    ViewOnly,
    Denied,
    Unavailable,
}

impl InputStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::ViewOnly => "view_only",
            Self::Denied => "denied",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DesktopFrame {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub seq: u64,
    pub captured_unix_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesktopInput {
    pub kind: InputKind,
    pub x: f32,
    pub y: f32,
    pub button: PointerButton,
    pub action: InputAction,
    pub dx: f32,
    pub dy: f32,
    pub key: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Pointer,
    Scroll,
    Key,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PointerButton {
    #[default]
    Left,
    Right,
    Middle,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputAction {
    #[default]
    Click,
    Down,
    Up,
    Move,
}

impl DesktopInput {
    pub fn from_json(v: &serde_json::Value) -> Result<Self, String> {
        let kind = match v.get("type").and_then(|x| x.as_str()).unwrap_or("pointer") {
            "scroll" => InputKind::Scroll,
            "key" => InputKind::Key,
            "text" => InputKind::Text,
            "pointer" | "mouse" => InputKind::Pointer,
            other => return Err(format!("unknown_type:{other}")),
        };
        let button = match v
            .get("button")
            .or_else(|| v.get("buttons"))
            .and_then(|x| x.as_str())
            .unwrap_or("left")
        {
            "right" => PointerButton::Right,
            "middle" => PointerButton::Middle,
            "none" => PointerButton::None,
            _ => PointerButton::Left,
        };
        let action = match v.get("action").and_then(|x| x.as_str()).unwrap_or("") {
            "down" => InputAction::Down,
            "up" => InputAction::Up,
            "move" => InputAction::Move,
            _ => InputAction::Click,
        };
        Ok(Self {
            kind,
            x: num_f32(v, "x"),
            y: num_f32(v, "y"),
            button,
            action,
            dx: num_f32(v, "dx"),
            dy: num_f32(v, "dy"),
            key: v
                .get("key")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_owned(),
            text: v
                .get("text")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_owned(),
        })
    }
}

fn num_f32(v: &serde_json::Value, key: &str) -> f32 {
    v.get(key)
        .and_then(|x| x.as_f64().or_else(|| x.as_i64().map(|n| n as f64)))
        .unwrap_or(0.0) as f32
}

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn encode_rgb_jpeg(
    width: u32,
    height: u32,
    rgb: &[u8],
    quality: u8,
) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 || width > 10_000 || height > 10_000 {
        return Err("invalid_size".into());
    }
    let need = width as usize * height as usize * 3;
    if rgb.len() < need {
        return Err("rgb_short".into());
    }
    let mut out = Vec::new();
    let enc = Encoder::new(&mut out, quality);
    enc.encode(&rgb[..need], width as u16, height as u16, ColorType::Rgb)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn scale_rgb(width: u32, height: u32, rgb: &[u8], max_width: u32) -> (u32, u32, Vec<u8>) {
    if width <= max_width || max_width == 0 {
        let need = width as usize * height as usize * 3;
        return (width, height, rgb[..need.min(rgb.len())].to_vec());
    }
    let new_w = max_width;
    let new_h = ((height as u64 * new_w as u64) / width as u64).max(1) as u32;
    let mut out = vec![0u8; new_w as usize * new_h as usize * 3];
    for y in 0..new_h {
        let src_y = y * height / new_h;
        for x in 0..new_w {
            let src_x = x * width / new_w;
            let si = (src_y * width + src_x) as usize * 3;
            let di = (y * new_w + x) as usize * 3;
            if si + 2 < rgb.len() {
                out[di] = rgb[si];
                out[di + 1] = rgb[si + 1];
                out[di + 2] = rgb[si + 2];
            }
        }
    }
    (new_w, new_h, out)
}

/// 生成一张真实 JPEG（渐变像素），供测试或「无法截屏」占位。
pub fn solid_jpeg(width: u32, height: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut rgb = vec![0u8; width as usize * height as usize * 3];
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) as usize * 3;
            rgb[i] = r.wrapping_add((x % 40) as u8);
            rgb[i + 1] = g.wrapping_add((y % 24) as u8);
            rgb[i + 2] = b;
        }
    }
    encode_rgb_jpeg(width, height, &rgb, JPEG_QUALITY).unwrap_or_else(|_| b"\xff\xd8\xff".to_vec())
}

#[derive(Debug)]
pub enum CaptureError {
    Permission(String),
    Unavailable(String),
}

impl CaptureError {
    pub fn status(&self) -> CaptureStatus {
        match self {
            Self::Permission(_) => CaptureStatus::PermissionRequired,
            Self::Unavailable(_) => CaptureStatus::Unavailable,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Permission(s) | Self::Unavailable(s) => s,
        }
    }
}

/// 捕获主屏 RGB。Windows 用 GDI BitBlt；其它平台返回 Unavailable。
pub fn capture_primary_rgb() -> Result<(u32, u32, Vec<u8>), CaptureError> {
    #[cfg(windows)]
    {
        return windows_capture_primary();
    }
    #[cfg(not(windows))]
    {
        Err(CaptureError::Unavailable(
            "screen capture is implemented for the Windows primary display".into(),
        ))
    }
}

pub fn capture_encoded_frame(seq: u64) -> Result<DesktopFrame, CaptureError> {
    let (w, h, rgb) = capture_primary_rgb()?;
    let (w, h, rgb) = scale_rgb(w, h, &rgb, FRAME_MAX_WIDTH);
    let jpeg = encode_rgb_jpeg(w, h, &rgb, JPEG_QUALITY)
        .map_err(|e| CaptureError::Unavailable(format!("jpeg encode failed: {e}")))?;
    Ok(DesktopFrame {
        jpeg,
        width: w,
        height: h,
        seq,
        captured_unix_ms: now_unix_ms(),
    })
}

pub fn apply_input(ev: &DesktopInput) -> Result<(), String> {
    #[cfg(windows)]
    {
        return windows_apply_input(ev);
    }
    #[cfg(not(windows))]
    {
        let _ = ev;
        Ok(())
    }
}

#[cfg(windows)]
fn windows_capture_primary() -> Result<(u32, u32, Vec<u8>), CaptureError> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);
        if width <= 0 || height <= 0 {
            return Err(CaptureError::Unavailable("primary metrics failed".into()));
        }
        let desktop = HWND::default();
        let hdc_screen = GetDC(desktop);
        if hdc_screen.is_invalid() {
            return Err(CaptureError::Unavailable("GetDC failed".into()));
        }
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        if hdc_mem.is_invalid() {
            ReleaseDC(desktop, hdc_screen);
            return Err(CaptureError::Unavailable(
                "CreateCompatibleDC failed".into(),
            ));
        }
        let hbmp = CreateCompatibleBitmap(hdc_screen, width, height);
        if hbmp.is_invalid() {
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(desktop, hdc_screen);
            return Err(CaptureError::Unavailable(
                "CreateCompatibleBitmap failed".into(),
            ));
        }
        let old = SelectObject(hdc_mem, hbmp);
        if BitBlt(hdc_mem, 0, 0, width, height, hdc_screen, 0, 0, SRCCOPY).is_err() {
            SelectObject(hdc_mem, old);
            let _ = DeleteObject(hbmp);
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(desktop, hdc_screen);
            return Err(CaptureError::Permission(
                "BitBlt denied — Windows may require screen capture permission".into(),
            ));
        }

        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [Default::default()],
        };
        let mut bgra = vec![0u8; (width * height * 4) as usize];
        let lines = GetDIBits(
            hdc_mem,
            hbmp,
            0,
            height as u32,
            Some(bgra.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        SelectObject(hdc_mem, old);
        let _ = DeleteObject(hbmp);
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(desktop, hdc_screen);
        if lines == 0 {
            return Err(CaptureError::Unavailable("GetDIBits failed".into()));
        }
        let mut rgb = vec![0u8; (width * height * 3) as usize];
        for i in 0..(width * height) as usize {
            rgb[i * 3] = bgra[i * 4 + 2];
            rgb[i * 3 + 1] = bgra[i * 4 + 1];
            rgb[i * 3 + 2] = bgra[i * 4];
        }
        Ok((width as u32, height as u32, rgb))
    }
}

#[cfg(windows)]
fn windows_apply_input(ev: &DesktopInput) -> Result<(), String> {
    match ev.kind {
        InputKind::Pointer => windows_pointer(ev),
        InputKind::Scroll => windows_scroll(ev),
        InputKind::Key => windows_key(ev),
        InputKind::Text => windows_text(&ev.text),
    }
}

#[cfg(windows)]
fn windows_pointer(ev: &DesktopInput) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
        MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSE_EVENT_FLAGS,
    };

    windows_move_abs(ev.x, ev.y)?;
    let (down, up) = match ev.button {
        PointerButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        PointerButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        PointerButton::None => return Ok(()),
        PointerButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
    };
    match ev.action {
        InputAction::Move => Ok(()),
        InputAction::Down => windows_mouse_flags(down),
        InputAction::Up => windows_mouse_flags(up),
        InputAction::Click => {
            windows_mouse_flags(down)?;
            windows_mouse_flags(up)
        }
    }
}

#[cfg(windows)]
fn windows_scroll(ev: &DesktopInput) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL};

    windows_move_abs(ev.x, ev.y)?;
    if ev.dy.abs() > f32::EPSILON {
        windows_mouse_wheel(MOUSEEVENTF_WHEEL, ev.dy)?;
    }
    if ev.dx.abs() > f32::EPSILON {
        windows_mouse_wheel(MOUSEEVENTF_HWHEEL, ev.dx)?;
    }
    Ok(())
}

#[cfg(windows)]
fn windows_move_abs(nx: f32, ny: f32) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE};

    let (dx, dy) = primary_to_absolute(nx, ny);
    windows_mouse_move(MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE, dx, dy, 0)
}

#[cfg(windows)]
fn primary_to_absolute(nx: f32, ny: f32) -> (i32, i32) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN,
        SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };

    unsafe {
        let virt_l = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let virt_t = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let virt_w = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let virt_h = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let (pl, pt, pw, ph) = if GetMonitorInfoW(mon, &mut info).as_bool() {
            let r = info.rcMonitor;
            (
                r.left,
                r.top,
                (r.right - r.left).max(1),
                (r.bottom - r.top).max(1),
            )
        } else {
            (
                0,
                0,
                GetSystemMetrics(SM_CXSCREEN).max(1),
                GetSystemMetrics(SM_CYSCREEN).max(1),
            )
        };
        let px = pl as f32 + nx.clamp(0.0, 1.0) * pw as f32;
        let py = pt as f32 + ny.clamp(0.0, 1.0) * ph as f32;
        let ax = ((px - virt_l as f32) / virt_w as f32 * 65535.0).round() as i32;
        let ay = ((py - virt_t as f32) / virt_h as f32 * 65535.0).round() as i32;
        (ax.clamp(0, 65535), ay.clamp(0, 65535))
    }
}

#[cfg(windows)]
fn windows_mouse_flags(
    flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
) -> Result<(), String> {
    windows_mouse_move(flags, 0, 0, 0)
}

#[cfg(windows)]
fn windows_mouse_wheel(
    flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
    delta: f32,
) -> Result<(), String> {
    let data = (delta * 120.0).round() as i32;
    windows_mouse_move(flags, 0, 0, data)
}

#[cfg(windows)]
fn windows_mouse_move(
    flags: windows::Win32::UI::Input::KeyboardAndMouse::MOUSE_EVENT_FLAGS,
    dx: i32,
    dy: i32,
    data: i32,
) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEINPUT,
    };

    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    send_inputs(&[input])
}

#[cfg(windows)]
fn windows_key(ev: &DesktopInput) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    };

    let Some(vk) = vk_from_name(&ev.key) else {
        return Err("unknown_key".into());
    };
    let down = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: Default::default(),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let up = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    match ev.action {
        InputAction::Down => send_inputs(&[down]),
        InputAction::Up => send_inputs(&[up]),
        InputAction::Click | InputAction::Move => send_inputs(&[down, up]),
    }
}

#[cfg(windows)]
fn windows_text(text: &str) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
        VIRTUAL_KEY,
    };

    if text.is_empty() {
        return Ok(());
    }
    if text.len() > 1024 {
        return Err("text_too_long".into());
    }
    let mut batch = Vec::new();
    for ch in text.encode_utf16() {
        batch.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: ch,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
        batch.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: ch,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }
    send_inputs(&batch)
}

#[cfg(windows)]
fn send_inputs(
    inputs: &[windows::Win32::UI::Input::KeyboardAndMouse::INPUT],
) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::SendInput;

    if inputs.is_empty() {
        return Ok(());
    }
    let n = unsafe {
        SendInput(
            inputs,
            std::mem::size_of::<windows::Win32::UI::Input::KeyboardAndMouse::INPUT>() as i32,
        )
    };
    if n as usize != inputs.len() {
        return Err("SendInput failed — input injection may be blocked".into());
    }
    Ok(())
}

#[cfg(windows)]
fn vk_from_name(name: &str) -> Option<u16> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VkKeyScanW;

    match name.trim().to_ascii_lowercase().as_str() {
        "enter" | "return" => Some(0x0D),
        "backspace" => Some(0x08),
        "tab" => Some(0x09),
        "escape" | "esc" => Some(0x1B),
        "space" => Some(0x20),
        "up" | "arrowup" => Some(0x26),
        "down" | "arrowdown" => Some(0x28),
        "left" | "arrowleft" => Some(0x25),
        "right" | "arrowright" => Some(0x27),
        "ctrl" | "control" => Some(0x11),
        "alt" => Some(0x12),
        "shift" => Some(0x10),
        "win" | "meta" => Some(0x5B),
        "delete" | "del" => Some(0x2E),
        other if other.chars().count() == 1 => {
            let c = other.chars().next()?;
            let scan = unsafe { VkKeyScanW(c as u16) };
            if scan == -1 {
                None
            } else {
                Some((scan as u16) & 0xFF)
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_names_are_browse_touch_fullscreen() {
        assert_eq!(DesktopViewMode::Browse.label_zh(), "浏览");
        assert_eq!(DesktopViewMode::Touch.label_zh(), "触控");
        assert_eq!(DesktopViewMode::Fullscreen.label_zh(), "全屏");
        assert!(!DesktopViewMode::Browse.allows_input());
        assert!(DesktopViewMode::Touch.allows_input());
        assert!(DesktopViewMode::Fullscreen.allows_input());
        assert_eq!(DesktopViewMode::parse("触控"), DesktopViewMode::Touch);
        assert_eq!(
            DesktopViewMode::parse("fullscreen"),
            DesktopViewMode::Fullscreen
        );
        assert_eq!(DesktopViewMode::parse("nope"), DesktopViewMode::Browse);
    }

    #[test]
    fn jpeg_roundtrip_is_real_pixels() {
        let jpeg = solid_jpeg(64, 48, 20, 80, 160);
        assert!(jpeg.starts_with(&[0xFF, 0xD8, 0xFF]), "must be JPEG SOI");
        assert!(jpeg.len() > 80);
    }

    #[test]
    fn scale_rgb_shrinks_wide_frames() {
        let rgb = vec![10u8; 2000 * 10 * 3];
        let (w, h, out) = scale_rgb(2000, 10, &rgb, 1280);
        assert_eq!(w, 1280);
        assert_eq!(h, 6);
        assert_eq!(out.len(), 1280 * 6 * 3);
    }

    #[test]
    fn input_json_parses_pointer_and_text() {
        let v = serde_json::json!({
            "type": "pointer",
            "x": 0.25,
            "y": 0.5,
            "button": "right",
            "action": "down"
        });
        let ev = DesktopInput::from_json(&v).unwrap();
        assert_eq!(ev.kind, InputKind::Pointer);
        assert_eq!(ev.button, PointerButton::Right);
        assert_eq!(ev.action, InputAction::Down);
        assert!((ev.x - 0.25).abs() < 1e-6);

        let v = serde_json::json!({"type":"text","text":"hi"});
        let ev = DesktopInput::from_json(&v).unwrap();
        assert_eq!(ev.kind, InputKind::Text);
        assert_eq!(ev.text, "hi");
    }
}
