// SPDX-License-Identifier: Apache-2.0
//
// 远程访问（v3 M5）：设置抽屉「远程访问」页。
// 配对码 / 二维码、Cloudflare Tunnel 状态、已授权设备；给 M6 companion 留 claim 钩子。

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::relay_mgr::{self, CloudflaredProbe, RelayManager, TunnelStatus};
use crate::remote_gateway::{
    AuthorizedDevice, GatewayHandle, PairOffer, CHAT_PATH, CLAIM_PATH, PROTOCOL,
};

pub const PAIR_TTL_SECS: u64 = 600;
const PAIR_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

pub const HONEST_LIMIT_ZH: &str =
    "中继依赖本机网络与公网可达性。临时 trycloudflare 不需要 Cloudflare 账号，但主机名每次启动都会变。命名隧道与 Tailscale 需要各自账号。同一局域网并不能代替公网中继；手机走蜂窝、电脑走家宽时必须经过中继。";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RelayKind {
    #[default]
    CloudflareQuick,
    CloudflareNamed,
    Tailscale,
    SelfHosted,
}

impl RelayKind {
    pub fn label_zh(self) -> &'static str {
        match self {
            RelayKind::CloudflareQuick => "Cloudflare Tunnel（临时 / 默认）",
            RelayKind::CloudflareNamed => "Cloudflare Tunnel（命名隧道）",
            RelayKind::Tailscale => "Tailscale",
            RelayKind::SelfHosted => "自建",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemotePersist {
    pub version: u32,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub relay: RelayKind,
    #[serde(default)]
    pub tailscale_url: String,
    #[serde(default)]
    pub self_hosted_url: String,
    #[serde(default)]
    pub named_hostname: String,
    #[serde(default)]
    pub named_token: String,
    #[serde(default)]
    pub devices: Vec<AuthorizedDevice>,
}

impl Default for RemotePersist {
    fn default() -> Self {
        Self {
            version: 1,
            enabled: false,
            relay: RelayKind::CloudflareQuick,
            tailscale_url: String::new(),
            self_hosted_url: String::new(),
            named_hostname: String::new(),
            named_token: String::new(),
            devices: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActivePair {
    pub code: String,
    pub expires_unix: u64,
}

pub struct RemoteAccessState {
    pub persist: RemotePersist,
    persist_path: Option<PathBuf>,
    pub pair: Option<ActivePair>,
    pub relay: RelayManager,
    pub gateway: Option<GatewayHandle>,
    pub last_error: Option<String>,
    copied_until: Option<Instant>,
    restore_attempted: bool,
    named_token_draft: String,
}

impl std::fmt::Debug for RemoteAccessState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteAccessState")
            .field("enabled", &self.persist.enabled)
            .field("relay", &self.persist.relay)
            .field("devices", &self.persist.devices.len())
            .finish_non_exhaustive()
    }
}

impl Default for RemoteAccessState {
    fn default() -> Self {
        Self::load_default()
    }
}

impl RemoteAccessState {
    pub fn new_in_memory() -> Self {
        Self {
            persist: RemotePersist::default(),
            persist_path: None,
            pair: None,
            relay: RelayManager::new(),
            gateway: None,
            last_error: None,
            copied_until: None,
            restore_attempted: true,
            named_token_draft: String::new(),
        }
    }

    pub fn load_default() -> Self {
        let path = default_remote_path();
        let persist = path
            .as_ref()
            .and_then(|p| load_persist(p).ok())
            .unwrap_or_default();
        let named_token_draft = persist.named_token.clone();
        Self {
            persist,
            persist_path: path,
            pair: None,
            relay: RelayManager::new(),
            gateway: None,
            last_error: None,
            copied_until: None,
            restore_attempted: false,
            named_token_draft,
        }
    }

    pub fn poll(&mut self) -> bool {
        let mut dirty = self.relay.poll();
        if let Some(gw) = &self.gateway {
            let new = gw.take_new_devices();
            if !new.is_empty() {
                for claimed in new {
                    if !self
                        .persist
                        .devices
                        .iter()
                        .any(|d| d.id == claimed.device.id)
                    {
                        self.persist.devices.push(claimed.device);
                    }
                }
                self.save();
                dirty = true;
            }
        }
        if let Some(pair) = &self.pair {
            if unix_now() > pair.expires_unix {
                self.pair = None;
                if let Some(gw) = &self.gateway {
                    gw.set_offer(None);
                }
                dirty = true;
            }
        }
        if self.persist.enabled && !self.restore_attempted {
            self.restore_attempted = true;
            if let Err(e) = self.apply_enabled(true) {
                self.last_error = Some(e);
            }
            dirty = true;
        }
        if let Some(url) = self.effective_public_url() {
            if let Some(gw) = &self.gateway {
                gw.set_public_url(Some(url));
            }
            if self.persist.enabled && self.pair.is_none() && self.has_public_endpoint() {
                self.refresh_pair();
                dirty = true;
            }
        }
        dirty || self.relay.wants_repaint()
    }

    pub fn effective_public_url(&self) -> Option<String> {
        match self.persist.relay {
            RelayKind::CloudflareQuick => self.relay.status.public_url().map(str::to_owned),
            RelayKind::CloudflareNamed => none_if_empty(&self.persist.named_hostname),
            RelayKind::Tailscale => none_if_empty(&self.persist.tailscale_url),
            RelayKind::SelfHosted => none_if_empty(&self.persist.self_hosted_url),
        }
    }

    pub fn has_public_endpoint(&self) -> bool {
        self.effective_public_url().is_some()
    }

    pub fn local_gateway_url(&self) -> Option<String> {
        self.gateway.as_ref().map(GatewayHandle::local_base)
    }

    fn apply_enabled(&mut self, on: bool) -> Result<(), String> {
        if on {
            if self.gateway.is_none() {
                let gw = GatewayHandle::start().map_err(|e| format!("本机网关启动失败：{e}"))?;
                self.gateway = Some(gw);
            }
            match self.persist.relay {
                RelayKind::CloudflareQuick => {
                    let port = self.gateway.as_ref().map(|g| g.port).unwrap_or(17831);
                    if self.relay.probe.path().is_none() {
                        self.relay.refresh_probe();
                    }
                    if self.relay.probe.path().is_none() {
                        return Err(
                            "未找到 cloudflared。点「下载 cloudflared」或把它放到 sidecar/relay/。"
                                .to_owned(),
                        );
                    }
                    self.relay.start_quick(port)?;
                }
                RelayKind::CloudflareNamed => {
                    if self.relay.probe.path().is_none() {
                        self.relay.refresh_probe();
                    }
                    self.relay.start_named(&self.persist.named_token)?;
                    if let Some(host) = none_if_empty(&self.persist.named_hostname) {
                        self.relay.mark_named_connected(host);
                    }
                }
                RelayKind::Tailscale | RelayKind::SelfHosted => {
                    // 不拉起 cloudflared；用户自行把公网/尾网指到本机网关。
                }
            }
            if self.has_public_endpoint() {
                self.refresh_pair();
            }
        } else {
            self.relay.stop();
            self.gateway = None;
            self.pair = None;
        }
        Ok(())
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.persist.enabled = on;
        self.restore_attempted = true;
        if let Err(e) = self.apply_enabled(on) {
            self.last_error = Some(e);
            if on {
                self.persist.enabled = false;
            }
        } else {
            self.last_error = None;
        }
        self.save();
    }

    pub fn set_relay_kind(&mut self, kind: RelayKind) {
        if self.persist.relay == kind {
            return;
        }
        self.persist.relay = kind;
        if self.persist.enabled {
            let _ = self.apply_enabled(false);
            if let Err(e) = self.apply_enabled(true) {
                self.last_error = Some(e);
            }
        }
        self.save();
    }

    pub fn refresh_pair(&mut self) {
        let code = generate_pair_code();
        let expires_unix = unix_now() + PAIR_TTL_SECS;
        self.pair = Some(ActivePair {
            code: code.clone(),
            expires_unix,
        });
        if let Some(gw) = &self.gateway {
            gw.set_offer(Some(PairOffer { code, expires_unix }));
            gw.set_public_url(self.effective_public_url());
        }
    }

    pub fn revoke(&mut self, id: &str) {
        self.persist.devices.retain(|d| d.id != id);
        self.save();
    }

    pub fn save(&self) {
        if let Some(path) = &self.persist_path {
            if let Err(e) = save_persist(path, &self.persist) {
                // 不把路径以外的敏感字段写进错误。
                let _ = e;
            }
        }
    }

    pub fn qr_payload(&self) -> Option<String> {
        let pair = self.pair.as_ref()?;
        Some(qr_payload(
            &pair.code,
            self.effective_public_url().as_deref(),
            pair.expires_unix,
        ))
    }

    pub fn remaining_secs(&self) -> Option<u64> {
        let pair = self.pair.as_ref()?;
        let now = unix_now();
        if now >= pair.expires_unix {
            Some(0)
        } else {
            Some(pair.expires_unix - now)
        }
    }
}

pub fn default_remote_path() -> Option<PathBuf> {
    if let Some(raw) = std::env::var_os("PI_LIGHT_REMOTE_PATH") {
        let path = PathBuf::from(raw);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    home_dir().map(|h| h.join(".pi-light").join("remote.json"))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn none_if_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_owned())
    }
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn generate_pair_code() -> String {
    let bytes = random_bytes::<6>();
    bytes
        .iter()
        .map(|b| PAIR_ALPHABET[(*b as usize) % PAIR_ALPHABET.len()] as char)
        .collect()
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    let _ = getrandom::getrandom(&mut buf);
    buf
}

pub fn format_pair_display(code: &str) -> String {
    if code.len() == 6 {
        format!("{} {}", &code[..3], &code[3..])
    } else {
        code.to_owned()
    }
}

/// Companion 扫码应解析的 JSON。不含局域网假定。
pub fn qr_payload(code: &str, relay_url: Option<&str>, expires_unix: u64) -> String {
    let mut map = serde_json::Map::new();
    map.insert("v".into(), serde_json::json!(1));
    map.insert("app".into(), serde_json::json!("pi-light"));
    map.insert("code".into(), serde_json::json!(code));
    map.insert("claim".into(), serde_json::json!(CLAIM_PATH));
    map.insert("chat".into(), serde_json::json!(CHAT_PATH));
    map.insert("protocol".into(), serde_json::json!(PROTOCOL));
    map.insert("exp".into(), serde_json::json!(expires_unix));
    if let Some(url) = relay_url {
        map.insert("relay".into(), serde_json::json!(url));
    }
    serde_json::Value::Object(map).to_string()
}

fn load_persist(path: &Path) -> io::Result<RemotePersist> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn save_persist(path: &Path, persist: &RemotePersist) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(persist)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write_private(path, &bytes)
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    io::Write::write_all(&mut file, bytes)?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn format_unix_utc(ts: u64) -> String {
    let days = (ts / 86400) as i64;
    let rem = ts % 86400;
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let (y, m, d) = civil_from_unix_days(days);
    format!("{y:04}-{m:02}-{d:02} {hour:02}:{min:02} UTC")
}

fn civil_from_unix_days(days: i64) -> (i32, u32, u32) {
    // Howard Hinnant, days since 1970-01-01 → civil (y, m, d)
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

// ── UI ────────────────────────────────────────────────────────────────────

pub fn render_tab(ui: &mut egui::Ui, state: &mut RemoteAccessState) {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new("通过云中继安全访问你的设备，不限局域网。")
            .weak()
            .size(12.0),
    );
    ui.add_space(8.0);

    if let Some(err) = state.last_error.clone() {
        ui.horizontal(|ui| {
            ui.colored_label(egui::Color32::from_rgb(220, 120, 80), format!("⚠ {err}"));
            if ui.small_button("×").clicked() {
                state.last_error = None;
            }
        });
        ui.add_space(6.0);
    }

    render_enable_and_pair(ui, state);
    ui.add_space(10.0);
    render_relay_card(ui, state);
    ui.add_space(10.0);
    render_relay_kind(ui, state);
    ui.add_space(10.0);
    render_devices(ui, state);
    ui.add_space(10.0);
    ui.label(egui::RichText::new("注意").strong().size(13.0));
    ui.label(egui::RichText::new(HONEST_LIMIT_ZH).weak().size(11.0));
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "手机 companion 扫码/输码后的远程聊天是后续里程碑（M6）。本页已提供配对码、中继出口与 POST /v1/pair/claim。远程桌面三档（浏览 / 触控 / 全屏）尚未开放。电脑操控需要操作系统权限，不会因为打开远程而自动启用。",
        )
        .weak()
        .size(11.0),
    );
}

fn render_enable_and_pair(ui: &mut egui::Ui, state: &mut RemoteAccessState) {
    let mut enabled = state.persist.enabled;
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("允许手机远程").strong().size(14.0));
                ui.label(
                    egui::RichText::new("开启后，手机可通过配对码或二维码远程连接到此设备。")
                        .weak()
                        .size(11.0),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.checkbox(&mut enabled, "").changed() {
                    state.set_enabled(enabled);
                }
            });
        });
        ui.add_space(8.0);
        if !state.persist.enabled {
            ui.label(
                egui::RichText::new("开关打开后才会启动本机网关和中继。")
                    .weak()
                    .italics()
                    .size(11.0),
            );
            return;
        }
        if !state.has_public_endpoint() {
            ui.label(
                egui::RichText::new(
                    "等待公网中继地址。未连上中继时，配对码不能用于蜂窝网手机访问。不要假定同一 Wi‑Fi 即可连。",
                )
                .weak()
                .size(11.0),
            );
            return;
        }

        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                ui.label(egui::RichText::new("配对二维码").strong().size(12.0));
                ui.label(
                    egui::RichText::new("使用 pi-light 手机客户端扫描二维码配对")
                        .weak()
                        .size(10.0),
                );
                ui.add_space(4.0);
                if let Some(payload) = state.qr_payload() {
                    render_qr(ui, &payload);
                }
            });
            cols[1].vertical(|ui| {
                ui.label(egui::RichText::new("配对码").strong().size(12.0));
                ui.label(
                    egui::RichText::new("在手机客户端输入此配对码")
                        .weak()
                        .size(10.0),
                );
                ui.add_space(8.0);
                let remaining = state.remaining_secs();
                let pair_code = state.pair.as_ref().map(|p| p.code.clone());
                if let Some(code) = pair_code {
                    ui.label(
                        egui::RichText::new(format_pair_display(&code))
                            .strong()
                            .size(28.0)
                            .monospace(),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("复制").clicked() {
                            ui.ctx().copy_text(code.clone());
                            state.copied_until = Some(Instant::now() + Duration::from_secs(2));
                        }
                        if ui.button("刷新配对码").clicked() {
                            state.refresh_pair();
                        }
                    });
                    if state.copied_until.is_some_and(|t| Instant::now() < t) {
                        ui.label(
                            egui::RichText::new("已复制")
                                .color(egui::Color32::from_rgb(90, 180, 110))
                                .size(11.0),
                        );
                    }
                    if let Some(secs) = remaining {
                        let mins = secs / 60;
                        ui.label(
                            egui::RichText::new(format!(
                                "配对码约 {mins} 分 {sec} 秒内有效（最长 10 分钟）",
                                sec = secs % 60
                            ))
                            .weak()
                            .size(11.0),
                        );
                    }
                }
            });
        });
    });
}

fn render_relay_card(ui: &mut egui::Ui, state: &mut RemoteAccessState) {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            let connected = state.relay.status.is_connected()
                || (state.persist.relay != RelayKind::CloudflareQuick
                    && state.has_public_endpoint()
                    && state.persist.enabled);
            let (dot, color) = if connected {
                ("●", egui::Color32::from_rgb(90, 180, 110))
            } else if matches!(
                state.relay.status,
                TunnelStatus::Starting | TunnelStatus::Downloading
            ) {
                ("●", egui::Color32::from_rgb(220, 180, 70))
            } else {
                ("●", egui::Color32::from_rgb(160, 90, 80))
            };
            ui.colored_label(color, dot);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("中继状态").strong().size(13.0));
                let kind = match state.persist.relay {
                    RelayKind::CloudflareQuick | RelayKind::CloudflareNamed => "Cloudflare Tunnel",
                    RelayKind::Tailscale => "Tailscale",
                    RelayKind::SelfHosted => "自建中继",
                };
                ui.label(
                    egui::RichText::new(format!("{kind} · {}", status_text(state))).size(12.0),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if connected {
                    ui.colored_label(egui::Color32::from_rgb(90, 180, 110), "已连接");
                }
            });
        });
        ui.add_space(6.0);
        if let Some(url) = state.effective_public_url() {
            ui.label(
                egui::RichText::new(format!("公网入口：{url}"))
                    .monospace()
                    .size(11.0),
            );
        }
        if let Some(local) = state.local_gateway_url() {
            ui.label(
                egui::RichText::new(format!("本机网关：{local}（仅 loopback，由中继引出）"))
                    .weak()
                    .size(10.0),
            );
        }
        ui.label(
            egui::RichText::new(format!("cloudflared：{}", state.relay.probe.source_label()))
                .weak()
                .size(10.0),
        );
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            match &state.relay.probe {
                CloudflaredProbe::Missing => {
                    if current_platform_can_download() {
                        if ui.button("下载 cloudflared").clicked() {
                            let dest = download_dest();
                            if let Err(e) = state.relay.start_download(dest) {
                                state.last_error = Some(e);
                            }
                        }
                    } else {
                        ui.label(
                            egui::RichText::new(
                                "当前平台未钉发行物。请自行安装 cloudflared 并加入 PATH，或设置 PI_LIGHT_CLOUDFLARED。",
                            )
                            .weak()
                            .size(11.0),
                        );
                    }
                }
                CloudflaredProbe::Found { .. } => {
                    if matches!(
                        state.persist.relay,
                        RelayKind::CloudflareQuick | RelayKind::CloudflareNamed
                    ) && state.persist.enabled
                    {
                        if state.relay.status.is_connected()
                            || matches!(state.relay.status, TunnelStatus::Starting)
                        {
                            if ui.button("停止中继").clicked() {
                                state.relay.stop();
                            }
                        } else if ui.button("启动中继").clicked() {
                            if let Err(e) = state.apply_enabled(true) {
                                state.last_error = Some(e);
                            }
                        }
                    }
                }
            }
            if ui.button("重新探测").clicked() {
                state.relay.refresh_probe();
            }
        });
        if matches!(state.relay.status, TunnelStatus::Downloading) {
            ui.label(
                egui::RichText::new("正在下载并校验 SHA-256…")
                    .weak()
                    .size(11.0),
            );
        }
        if let TunnelStatus::Error(e) = &state.relay.status {
            ui.colored_label(egui::Color32::from_rgb(210, 90, 70), e);
        }
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "默认使用 Cloudflare 临时隧道（trycloudflare）：无需登录，适合演示公网可达。正式稳定域名请改用命名隧道。",
            )
            .weak()
            .size(11.0),
        );
    });
}

fn render_relay_kind(ui: &mut egui::Ui, state: &mut RemoteAccessState) {
    card(ui, |ui| {
        ui.label(egui::RichText::new("中继方式").strong().size(13.0));
        ui.label(
            egui::RichText::new("默认 Cloudflare Tunnel。Tailscale / 自建可选手动配置，不挡 MVP。")
                .weak()
                .size(11.0),
        );
        ui.add_space(4.0);
        let mut kind = state.persist.relay;
        ui.radio_value(
            &mut kind,
            RelayKind::CloudflareQuick,
            RelayKind::CloudflareQuick.label_zh(),
        );
        ui.radio_value(
            &mut kind,
            RelayKind::CloudflareNamed,
            RelayKind::CloudflareNamed.label_zh(),
        );
        ui.radio_value(
            &mut kind,
            RelayKind::Tailscale,
            RelayKind::Tailscale.label_zh(),
        );
        ui.radio_value(
            &mut kind,
            RelayKind::SelfHosted,
            RelayKind::SelfHosted.label_zh(),
        );
        if kind != state.persist.relay {
            state.set_relay_kind(kind);
        }
        ui.add_space(6.0);
        match state.persist.relay {
            RelayKind::CloudflareQuick => {
                ui.label(
                    egui::RichText::new(
                        "将启动 `cloudflared tunnel --url http://127.0.0.1:<port>`，从日志解析 *.trycloudflare.com。",
                    )
                    .weak()
                    .size(11.0),
                );
            }
            RelayKind::CloudflareNamed => {
                ui.label(
                    egui::RichText::new(
                        "需要 Cloudflare 账号里创建的 Tunnel token，以及你已配置的 https 主机名。",
                    )
                    .weak()
                    .size(11.0),
                );
                ui.horizontal(|ui| {
                    ui.label("主机名");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.persist.named_hostname)
                            .hint_text("https://pi-light.example.com")
                            .desired_width(ui.available_width().max(160.0)),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Token");
                    ui.add(
                        egui::TextEdit::singleline(&mut state.named_token_draft)
                            .password(true)
                            .hint_text("Cloudflare Tunnel token")
                            .desired_width(ui.available_width().max(160.0)),
                    );
                });
                if ui.button("保存命名隧道设置").clicked() {
                    state.persist.named_token = state.named_token_draft.clone();
                    state.save();
                }
            }
            RelayKind::Tailscale => {
                ui.label(
                    egui::RichText::new(
                        "请先在本机安装并登录 Tailscale，再填写 tailnet 上能访问本机网关的地址（例如 http://pi-light:17831）。pi-light 不会代为安装 Tailscale。",
                    )
                    .weak()
                    .size(11.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut state.persist.tailscale_url)
                        .hint_text("http://machine:17831")
                        .desired_width(f32::INFINITY),
                );
                if ui.small_button("保存").clicked() {
                    state.save();
                }
            }
            RelayKind::SelfHosted => {
                ui.label(
                    egui::RichText::new(
                        "把你的反代 / frp / WireGuard 出口指到本机 loopback 网关，然后填写公网 https 地址。",
                    )
                    .weak()
                    .size(11.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut state.persist.self_hosted_url)
                        .hint_text("https://pi-light.example.net")
                        .desired_width(f32::INFINITY),
                );
                if ui.small_button("保存").clicked() {
                    state.save();
                }
            }
        }
    });
}

fn render_devices(ui: &mut egui::Ui, state: &mut RemoteAccessState) {
    card(ui, |ui| {
        ui.label(egui::RichText::new("已授权设备").strong().size(13.0));
        ui.label(
            egui::RichText::new("手机扫码或输码并完成 claim 后出现在这里。可随时撤销。")
                .weak()
                .size(11.0),
        );
        ui.add_space(6.0);
        if state.persist.devices.is_empty() {
            ui.label(
                egui::RichText::new("还没有已授权设备。")
                    .weak()
                    .italics()
                    .size(12.0),
            );
            return;
        }
        let mut revoke_id: Option<String> = None;
        for device in &state.persist.devices {
            ui.horizontal(|ui| {
                ui.label("📱");
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&device.name).strong().size(13.0));
                    ui.label(
                        egui::RichText::new(format!(
                            "{} 设备 · 已授权 ✓ {}",
                            device.platform,
                            format_unix_utc(device.authorized_unix)
                        ))
                        .weak()
                        .size(11.0),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("撤销").clicked() {
                        revoke_id = Some(device.id.clone());
                    }
                });
            });
            ui.add_space(4.0);
        }
        if let Some(id) = revoke_id {
            state.revoke(&id);
        }
    });
}

fn status_text(state: &RemoteAccessState) -> String {
    match state.persist.relay {
        RelayKind::CloudflareQuick | RelayKind::CloudflareNamed => state.relay.status.label_zh(),
        RelayKind::Tailscale | RelayKind::SelfHosted => {
            if state.has_public_endpoint() && state.persist.enabled {
                "已填写入口".to_owned()
            } else if state.persist.enabled {
                "请填写可从公网或尾网访问的地址".to_owned()
            } else {
                "未启用".to_owned()
            }
        }
    }
}

fn current_platform_can_download() -> bool {
    relay_mgr::current_artifact().is_some()
}

fn download_dest() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join("sidecar").join("relay");
            if dir.join("sidecar").is_dir() || fs::create_dir_all(&bundled).is_ok() {
                return bundled;
            }
        }
    }
    relay_mgr::user_relay_dir().unwrap_or_else(|| PathBuf::from("sidecar/relay"))
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .rounding(egui::Rounding::same(8.0))
        .fill(ui.visuals().extreme_bg_color)
        .stroke(egui::Stroke::new(
            1.0_f32,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .show(ui, add);
}

fn render_qr(ui: &mut egui::Ui, data: &str) {
    let Ok(qr) = qrcode::QrCode::new(data.as_bytes()) else {
        ui.colored_label(egui::Color32::from_rgb(210, 90, 70), "二维码生成失败");
        return;
    };
    let modules = qr.width();
    let size = 168.0_f32;
    let pad = 8.0_f32;
    let inner = size - pad * 2.0;
    let cell = inner / modules as f32;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, egui::Color32::WHITE);
    let dark = egui::Color32::from_rgb(92, 48, 168);
    let colors = qr.to_colors();
    for y in 0..modules {
        for x in 0..modules {
            if colors[y * modules + x] == qrcode::Color::Dark {
                let r = egui::Rect::from_min_size(
                    egui::pos2(
                        rect.min.x + pad + x as f32 * cell,
                        rect.min.y + pad + y as f32 * cell,
                    ),
                    egui::vec2(cell, cell),
                );
                painter.rect_filled(r, 0.0, dark);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn pair_code_charset_and_length() {
        for _ in 0..32 {
            let code = generate_pair_code();
            assert_eq!(code.len(), 6);
            assert!(code.bytes().all(|b| PAIR_ALPHABET.contains(&b)));
            assert_eq!(format_pair_display(&code).len(), 7);
        }
    }

    #[test]
    fn qr_payload_is_json_for_companion() {
        let json = qr_payload("AB12CD", Some("https://x.trycloudflare.com"), 1_700_000_000);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["app"], "pi-light");
        assert_eq!(v["v"], 1);
        assert_eq!(v["code"], "AB12CD");
        assert_eq!(v["claim"], "/v1/pair/claim");
        assert_eq!(v["chat"], "/v1/chat");
        assert_eq!(v["relay"], "https://x.trycloudflare.com");
        assert_eq!(v["protocol"], PROTOCOL);
        assert!(!json.contains("局域网即可"));
        assert!(!json.to_lowercase().contains("lan-only"));
    }

    #[test]
    fn persist_roundtrip_and_revoke() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("remote.json");
        let mut state = RemoteAccessState::new_in_memory();
        state.persist_path = Some(path.clone());
        state.persist.devices.push(AuthorizedDevice {
            id: "dev_aa".into(),
            name: "Pixel".into(),
            platform: "Android".into(),
            authorized_unix: 1_716_189_120,
            token_sha256: "ab".repeat(32),
        });
        state.save();
        let loaded = load_persist(&path).unwrap();
        assert_eq!(loaded.devices.len(), 1);
        assert_eq!(loaded.devices[0].name, "Pixel");
        state.revoke("dev_aa");
        assert!(state.persist.devices.is_empty());
        let loaded = load_persist(&path).unwrap();
        assert!(loaded.devices.is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn unix_date_epoch() {
        assert_eq!(format_unix_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_unix_utc(1_716_189_120), "2024-05-20 07:12 UTC");
    }

    #[test]
    fn honest_copy_rejects_lan_only_story() {
        assert!(HONEST_LIMIT_ZH.contains("并不能代替公网中继"));
        assert!(HONEST_LIMIT_ZH.contains("蜂窝"));
        assert!(!HONEST_LIMIT_ZH.contains("局域网即可"));
    }

    #[test]
    fn ttl_constant_is_ten_minutes() {
        assert_eq!(PAIR_TTL_SECS, 600);
    }

    #[test]
    fn default_relay_is_cloudflare_quick() {
        assert_eq!(RemotePersist::default().relay, RelayKind::CloudflareQuick);
    }
}
