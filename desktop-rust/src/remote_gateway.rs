// SPDX-License-Identifier: Apache-2.0
//
// 本机 loopback 控制面。Cloudflare Tunnel 把这里暴露到公网。
// M5：health / pair info / pair claim。
// M6-lite：已授权设备可 POST/GET /v1/chat，写入桌面当前 Pi 会话。
// 增量：GET 快照含 tools/can_stop；POST 可 SSE 流式；POST /v1/chat/stop 中止本轮。

use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

pub const DEFAULT_PORT: u16 = 17831;
pub const CLAIM_PATH: &str = "/v1/pair/claim";
pub const CHAT_PATH: &str = "/v1/chat";
pub const STOP_PATH: &str = "/v1/chat/stop";
pub const PROTOCOL: &str = "pi-light-companion/1";
const CHAT_WAIT_SECS: u64 = 120;

#[derive(Debug, Clone)]
pub struct PairOffer {
    pub code: String,
    pub expires_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorizedDevice {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub authorized_unix: u64,
    pub token_sha256: String,
}

#[derive(Debug, Clone)]
pub struct ClaimedDevice {
    pub device: AuthorizedDevice,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotMsg {
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotTool {
    pub id: String,
    pub name: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatSnapshot {
    pub host: String,
    pub session_title: String,
    pub writable: bool,
    pub streaming: bool,
    pub can_stop: bool,
    pub partial: String,
    pub messages: Vec<SnapshotMsg>,
    pub tools: Vec<SnapshotTool>,
}

impl Default for ChatSnapshot {
    fn default() -> Self {
        Self {
            host: "pi-light".into(),
            session_title: "会话".into(),
            writable: true,
            streaming: false,
            can_stop: false,
            partial: String::new(),
            messages: Vec::new(),
            tools: Vec::new(),
        }
    }
}

/// UI 线程把本轮事件发回网关 HTTP 线程。
#[derive(Debug, Clone)]
pub enum CompanionChatEvent {
    Delta { text: String },
    Tool(SnapshotTool),
    Settled { text: String, aborted: bool },
    Error { code: String, message: String },
}

/// 等待 UI 把这条用户消息写入当前 Pi 会话。
pub struct PendingChat {
    pub text: String,
    pub device_id: String,
    pub reply: mpsc::Sender<CompanionChatEvent>,
}

#[derive(Debug, Deserialize)]
struct ClaimRequest {
    #[serde(default)]
    code: String,
    #[serde(default)]
    device_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    platform: String,
}

#[derive(Debug, Deserialize)]
struct ChatRequest {
    #[serde(default)]
    text: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    stream: bool,
}

struct GatewayShared {
    offer: Option<PairOffer>,
    public_url: Option<String>,
    new_devices: Vec<ClaimedDevice>,
    devices: Vec<AuthorizedDevice>,
    pending: Option<PendingChat>,
    pending_stop: bool,
    snapshot: ChatSnapshot,
    /// 仅测试：跳过 UI 等待，立即回这条文本。
    auto_reply: Option<String>,
}

pub struct GatewayHandle {
    pub port: u16,
    shutdown: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    shared: Arc<Mutex<GatewayShared>>,
}

impl std::fmt::Debug for GatewayHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayHandle")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

impl Drop for GatewayHandle {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        // 碰一下本机端口，缩短 accept 等待。
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

impl GatewayHandle {
    pub fn start() -> io::Result<Self> {
        let listener = match TcpListener::bind(("127.0.0.1", DEFAULT_PORT)) {
            Ok(l) => l,
            Err(_) => TcpListener::bind(("127.0.0.1", 0))?,
        };
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(GatewayShared {
            offer: None,
            public_url: None,
            new_devices: Vec::new(),
            devices: Vec::new(),
            pending: None,
            pending_stop: false,
            snapshot: ChatSnapshot::default(),
            auto_reply: None,
        }));
        let flag = shutdown.clone();
        let state = shared.clone();
        let thread = thread::Builder::new()
            .name("pi-light-remote-gw".into())
            .spawn(move || loop {
                if flag.load(Ordering::Relaxed) {
                    break;
                }
                match listener.accept() {
                    Ok((stream, _)) => {
                        if flag.load(Ordering::Relaxed) {
                            break;
                        }
                        let conn_state = state.clone();
                        let _ = thread::Builder::new()
                            .name("pi-light-gw-conn".into())
                            .spawn(move || {
                                let _ = stream.set_nonblocking(false);
                                let _ = stream.set_read_timeout(Some(Duration::from_secs(8)));
                                let _ = stream.set_write_timeout(Some(Duration::from_secs(8)));
                                handle_conn(stream, &conn_state);
                            });
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(40));
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            })?;
        Ok(Self {
            port,
            shutdown,
            thread: Some(thread),
            shared,
        })
    }

    pub fn set_offer(&self, offer: Option<PairOffer>) {
        if let Ok(mut g) = self.shared.lock() {
            g.offer = offer;
        }
    }

    pub fn set_public_url(&self, url: Option<String>) {
        if let Ok(mut g) = self.shared.lock() {
            g.public_url = url;
        }
    }

    pub fn set_authorized_devices(&self, devices: Vec<AuthorizedDevice>) {
        if let Ok(mut g) = self.shared.lock() {
            g.devices = devices;
        }
    }

    pub fn set_chat_snapshot(&self, snapshot: ChatSnapshot) {
        if let Ok(mut g) = self.shared.lock() {
            g.snapshot = snapshot;
        }
    }

    pub fn take_new_devices(&self) -> Vec<ClaimedDevice> {
        self.shared
            .lock()
            .map(|mut g| std::mem::take(&mut g.new_devices))
            .unwrap_or_default()
    }

    pub fn take_pending_chat(&self) -> Option<PendingChat> {
        self.shared.lock().ok()?.pending.take()
    }

    pub fn has_pending_chat(&self) -> bool {
        self.shared
            .lock()
            .map(|g| g.pending.is_some())
            .unwrap_or(false)
    }

    pub fn take_pending_stop(&self) -> bool {
        self.shared
            .lock()
            .map(|mut g| {
                let flagged = g.pending_stop;
                g.pending_stop = false;
                flagged
            })
            .unwrap_or(false)
    }

    pub fn local_base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn local_host_label() -> String {
    for key in ["COMPUTERNAME", "HOSTNAME"] {
        if let Ok(s) = std::env::var(key) {
            let t = s.trim();
            if !t.is_empty() {
                return t.to_owned();
            }
        }
    }
    if let Ok(s) = std::fs::read_to_string("/etc/hostname") {
        let t = s.trim();
        if !t.is_empty() {
            return t.to_owned();
        }
    }
    "pi-light".to_owned()
}

enum DispatchOut {
    Ready(u16, String),
    ChatWait(mpsc::Receiver<CompanionChatEvent>),
    ChatStream(mpsc::Receiver<CompanionChatEvent>),
}

fn handle_conn(mut stream: TcpStream, shared: &Mutex<GatewayShared>) {
    let req = match read_request(&mut stream) {
        Ok(r) => r,
        Err(_) => {
            let _ = write_json(&mut stream, 400, r#"{"ok":false,"error":"bad_request"}"#);
            return;
        }
    };
    match dispatch(&req, shared) {
        DispatchOut::Ready(status, body) => {
            let _ = write_json(&mut stream, status, &body);
        }
        DispatchOut::ChatWait(rx) => {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(CHAT_WAIT_SECS + 5)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(CHAT_WAIT_SECS + 5)));
            let (status, body) = wait_chat_reply(rx);
            let _ = write_json(&mut stream, status, &body);
        }
        DispatchOut::ChatStream(rx) => {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(CHAT_WAIT_SECS + 5)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(CHAT_WAIT_SECS + 5)));
            wait_chat_stream(&mut stream, rx);
        }
    }
}

fn wait_chat_reply(rx: mpsc::Receiver<CompanionChatEvent>) -> (u16, String) {
    let deadline = std::time::Instant::now() + Duration::from_secs(CHAT_WAIT_SECS);
    loop {
        let remain = deadline.saturating_duration_since(std::time::Instant::now());
        if remain.is_zero() {
            return timeout_body();
        }
        match rx.recv_timeout(remain) {
            Ok(CompanionChatEvent::Settled { text, aborted }) => {
                return (
                    200,
                    serde_json::json!({
                        "ok": true,
                        "reply": text,
                        "aborted": aborted,
                        "writable": true,
                        "protocol": PROTOCOL,
                    })
                    .to_string(),
                );
            }
            Ok(CompanionChatEvent::Error { code, message }) => {
                return error_body(&code, &message);
            }
            Ok(CompanionChatEvent::Delta { .. } | CompanionChatEvent::Tool(_)) => {}
            Err(_) => return timeout_body(),
        }
    }
}

fn wait_chat_stream(stream: &mut TcpStream, rx: mpsc::Receiver<CompanionChatEvent>) {
    if write_sse_headers(stream).is_err() {
        return;
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(CHAT_WAIT_SECS);
    loop {
        let remain = deadline.saturating_duration_since(std::time::Instant::now());
        if remain.is_zero() {
            let (_, body) = timeout_body();
            let _ = write_sse_event(stream, "error", &body);
            return;
        }
        match rx.recv_timeout(remain) {
            Ok(CompanionChatEvent::Delta { text }) => {
                let data = serde_json::json!({"text": text}).to_string();
                if write_sse_event(stream, "delta", &data).is_err() {
                    return;
                }
            }
            Ok(CompanionChatEvent::Tool(tool)) => {
                let data = serde_json::to_string(&tool).unwrap_or_else(|_| "{}".into());
                if write_sse_event(stream, "tool", &data).is_err() {
                    return;
                }
            }
            Ok(CompanionChatEvent::Settled { text, aborted }) => {
                let data = serde_json::json!({
                    "ok": true,
                    "reply": text,
                    "aborted": aborted,
                    "writable": true,
                    "protocol": PROTOCOL,
                })
                .to_string();
                let _ = write_sse_event(stream, "settled", &data);
                return;
            }
            Ok(CompanionChatEvent::Error { code, message }) => {
                let (_, body) = error_body(&code, &message);
                let _ = write_sse_event(stream, "error", &body);
                return;
            }
            Err(_) => {
                let (_, body) = timeout_body();
                let _ = write_sse_event(stream, "error", &body);
                return;
            }
        }
    }
}

fn timeout_body() -> (u16, String) {
    (
        504,
        serde_json::json!({
            "ok": false,
            "error": "timeout",
            "message": "desktop did not finish this turn in time",
        })
        .to_string(),
    )
}

fn error_body(code: &str, message: &str) -> (u16, String) {
    let status = match code {
        "busy" => 409,
        "pi_unavailable" => 503,
        "unauthorized" => 401,
        _ => 502,
    };
    (
        status,
        serde_json::json!({
            "ok": false,
            "error": code,
            "message": message,
        })
        .to_string(),
    )
}

struct HttpReq {
    method: String,
    path: String,
    body: Vec<u8>,
    authorization: Option<String>,
    accept: Option<String>,
}

fn dispatch(req: &HttpReq, shared: &Mutex<GatewayShared>) -> DispatchOut {
    let path = req.path.split('?').next().unwrap_or(req.path.as_str());
    match (req.method.as_str(), path) {
        ("GET", "/health") | ("GET", "/") => DispatchOut::Ready(
            200,
            serde_json::json!({
                "ok": true,
                "app": "pi-light",
                "role": "remote-gateway",
                "protocol": PROTOCOL,
                "chat": true,
                "writable": true,
                "stream": true,
                "stop": true,
                "stop_path": STOP_PATH,
            })
            .to_string(),
        ),
        ("GET", "/v1/pair/info") => {
            let (st, body) = pair_info(shared);
            DispatchOut::Ready(st, body)
        }
        ("POST", CLAIM_PATH) => {
            let (st, body) = claim(shared, &req.body);
            DispatchOut::Ready(st, body)
        }
        ("GET", CHAT_PATH) => chat_get(req, shared),
        ("POST", CHAT_PATH) => chat_post(req, shared),
        ("POST", STOP_PATH) => chat_stop(req, shared),
        _ => DispatchOut::Ready(
            404,
            serde_json::json!({"ok":false,"error":"not_found"}).to_string(),
        ),
    }
}

fn pair_info(shared: &Mutex<GatewayShared>) -> (u16, String) {
    let Ok(g) = shared.lock() else {
        return (
            500,
            serde_json::json!({"ok":false,"error":"busy"}).to_string(),
        );
    };
    let (has, exp) = match &g.offer {
        Some(o) => (true, Some(o.expires_unix)),
        None => (false, None),
    };
    (
        200,
        serde_json::json!({
            "ok": true,
            "app": "pi-light",
            "v": 1,
            "claim": CLAIM_PATH,
            "chat": CHAT_PATH,
            "stop": STOP_PATH,
            "has_offer": has,
            "exp": exp,
            "relay": g.public_url,
            "writable": true,
        })
        .to_string(),
    )
}

fn claim(shared: &Mutex<GatewayShared>, body: &[u8]) -> (u16, String) {
    let parsed: ClaimRequest = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => {
            return (
                400,
                serde_json::json!({"ok":false,"error":"invalid_json"}).to_string(),
            );
        }
    };
    let code = parsed.code.trim();
    if code.is_empty() {
        return (
            400,
            serde_json::json!({"ok":false,"error":"code_required"}).to_string(),
        );
    }
    let Ok(mut g) = shared.lock() else {
        return (
            500,
            serde_json::json!({"ok":false,"error":"busy"}).to_string(),
        );
    };
    let Some(offer) = g.offer.clone() else {
        return (
            409,
            serde_json::json!({"ok":false,"error":"no_offer"}).to_string(),
        );
    };
    if unix_now() > offer.expires_unix {
        return (
            410,
            serde_json::json!({"ok":false,"error":"expired"}).to_string(),
        );
    }
    if !offer.code.eq_ignore_ascii_case(code) {
        return (
            401,
            serde_json::json!({"ok":false,"error":"invalid_code"}).to_string(),
        );
    }
    let name_raw = if !parsed.device_name.trim().is_empty() {
        parsed.device_name.trim()
    } else if !parsed.name.trim().is_empty() {
        parsed.name.trim()
    } else {
        "companion"
    };
    let name = sanitize_label(name_raw, 64);
    let platform = sanitize_label(
        if parsed.platform.trim().is_empty() {
            "unknown"
        } else {
            parsed.platform.trim()
        },
        32,
    );
    let id = format!("dev_{}", hex_bytes(&random_bytes::<8>()));
    let token = hex_bytes(&random_bytes::<32>());
    let token_sha256 = crate::relay_mgr::sha256_hex(token.as_bytes());
    let device = AuthorizedDevice {
        id: id.clone(),
        name,
        platform,
        authorized_unix: unix_now(),
        token_sha256,
    };
    g.devices.push(device.clone());
    g.new_devices.push(ClaimedDevice {
        device: device.clone(),
    });
    (
        200,
        serde_json::json!({
            "ok": true,
            "device_id": id,
            "token": token,
            "relay_url": g.public_url,
            "chat_path": CHAT_PATH,
            "stop_path": STOP_PATH,
            "protocol": PROTOCOL,
            "writable": true,
            "host": g.snapshot.host,
        })
        .to_string(),
    )
}

fn bearer_token(auth: Option<&str>) -> Option<&str> {
    let raw = auth?.trim();
    let rest = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))?;
    let t = rest.trim();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn ct_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn authorize<'a>(
    g: &'a GatewayShared,
    auth: Option<&str>,
) -> Result<&'a AuthorizedDevice, (u16, String)> {
    let Some(token) = bearer_token(auth) else {
        return Err((
            401,
            serde_json::json!({"ok":false,"error":"unauthorized"}).to_string(),
        ));
    };
    let got = crate::relay_mgr::sha256_hex(token.as_bytes());
    g.devices
        .iter()
        .find(|d| ct_eq(&d.token_sha256, &got))
        .ok_or_else(|| {
            (
                401,
                serde_json::json!({"ok":false,"error":"unauthorized"}).to_string(),
            )
        })
}

fn chat_get(req: &HttpReq, shared: &Mutex<GatewayShared>) -> DispatchOut {
    let Ok(g) = shared.lock() else {
        return DispatchOut::Ready(
            500,
            serde_json::json!({"ok":false,"error":"busy"}).to_string(),
        );
    };
    if let Err((st, body)) = authorize(&g, req.authorization.as_deref()) {
        return DispatchOut::Ready(st, body);
    }
    let snap = &g.snapshot;
    let can_stop = snap.can_stop || snap.streaming || g.pending.is_some();
    DispatchOut::Ready(
        200,
        serde_json::json!({
            "ok": true,
            "writable": snap.writable,
            "streaming": snap.streaming,
            "can_stop": can_stop,
            "partial": snap.partial,
            "host": snap.host,
            "session_title": snap.session_title,
            "messages": snap.messages,
            "tools": snap.tools,
            "protocol": PROTOCOL,
            "relay": g.public_url,
        })
        .to_string(),
    )
}

fn chat_post(req: &HttpReq, shared: &Mutex<GatewayShared>) -> DispatchOut {
    let parsed: ChatRequest = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(_) => {
            return DispatchOut::Ready(
                400,
                serde_json::json!({"ok":false,"error":"invalid_json"}).to_string(),
            );
        }
    };
    let text = first_nonempty(&[&parsed.text, &parsed.message, &parsed.prompt]);
    if text.is_empty() {
        return DispatchOut::Ready(
            400,
            serde_json::json!({"ok":false,"error":"text_required"}).to_string(),
        );
    }
    if text.len() > 16 * 1024 {
        return DispatchOut::Ready(
            413,
            serde_json::json!({"ok":false,"error":"too_large"}).to_string(),
        );
    }
    let Ok(mut g) = shared.lock() else {
        return DispatchOut::Ready(
            500,
            serde_json::json!({"ok":false,"error":"busy"}).to_string(),
        );
    };
    let device_id = match authorize(&g, req.authorization.as_deref()) {
        Ok(d) => d.id.clone(),
        Err((st, body)) => return DispatchOut::Ready(st, body),
    };
    if g.pending.is_some() || g.snapshot.streaming {
        return DispatchOut::Ready(
            409,
            serde_json::json!({"ok":false,"error":"busy"}).to_string(),
        );
    }
    if let Some(reply) = g.auto_reply.clone() {
        return DispatchOut::Ready(
            200,
            serde_json::json!({
                "ok": true,
                "reply": reply,
                "aborted": false,
                "writable": true,
                "protocol": PROTOCOL,
                "device_id": device_id,
            })
            .to_string(),
        );
    }
    let wants_stream = parsed.stream || wants_event_stream(req.accept.as_deref());
    let (tx, rx) = mpsc::channel();
    g.pending = Some(PendingChat {
        text,
        device_id,
        reply: tx,
    });
    if wants_stream {
        DispatchOut::ChatStream(rx)
    } else {
        DispatchOut::ChatWait(rx)
    }
}

fn chat_stop(req: &HttpReq, shared: &Mutex<GatewayShared>) -> DispatchOut {
    let Ok(mut g) = shared.lock() else {
        return DispatchOut::Ready(
            500,
            serde_json::json!({"ok":false,"error":"busy"}).to_string(),
        );
    };
    if let Err((st, body)) = authorize(&g, req.authorization.as_deref()) {
        return DispatchOut::Ready(st, body);
    }
    if g.pending.is_none() && !g.snapshot.streaming && !g.snapshot.can_stop {
        return DispatchOut::Ready(
            409,
            serde_json::json!({
                "ok": false,
                "error": "not_streaming",
                "message": "desktop is not generating",
            })
            .to_string(),
        );
    }
    g.pending_stop = true;
    DispatchOut::Ready(
        200,
        serde_json::json!({
            "ok": true,
            "accepted": true,
            "protocol": PROTOCOL,
        })
        .to_string(),
    )
}

fn wants_event_stream(accept: Option<&str>) -> bool {
    accept.unwrap_or("").split(',').any(|part| {
        part.trim()
            .to_ascii_lowercase()
            .starts_with("text/event-stream")
    })
}

fn first_nonempty(parts: &[&str]) -> String {
    for p in parts {
        let t = p.trim();
        if !t.is_empty() {
            return t.to_owned();
        }
    }
    String::new()
}

fn sanitize_label(s: &str, max: usize) -> String {
    let t: String = s.chars().filter(|c| !c.is_control()).take(max).collect();
    if t.is_empty() {
        "device".to_owned()
    } else {
        t
    }
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    let _ = getrandom::getrandom(&mut buf);
    buf
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

fn read_request(stream: &mut TcpStream) -> io::Result<HttpReq> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    let header_end = loop {
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "eof"));
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() > 64 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
        }
        if let Some(end) = find_header_end(&buf) {
            break end;
        }
    };
    let header = std::str::from_utf8(&buf[..header_end])
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut lines = header.split("\r\n");
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_owned();
    let path = parts.next().unwrap_or("/").to_owned();
    let mut content_len = 0usize;
    let mut authorization = None;
    let mut accept = None;
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_len = v.trim().parse().unwrap_or(0);
        } else if lower.starts_with("authorization:") {
            if let Some((_, v)) = line.split_once(':') {
                authorization = Some(v.trim().to_owned());
            }
        } else if lower.starts_with("accept:") {
            if let Some((_, v)) = line.split_once(':') {
                accept = Some(v.trim().to_owned());
            }
        }
    }
    if content_len > 32 * 1024 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "body too large"));
    }
    let mut body = buf[header_end..].to_vec();
    while body.len() < content_len {
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
        if body.len() > 32 * 1024 {
            break;
        }
    }
    body.truncate(content_len);
    Ok(HttpReq {
        method,
        path,
        body,
        authorization,
        accept,
    })
}

fn write_json(stream: &mut TcpStream, status: u16, body: &str) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        410 => "Gone",
        413 => "Payload Too Large",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Error",
    };
    let bytes = body.as_bytes();
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(bytes)?;
    stream.flush()
}

fn write_sse_headers(stream: &mut TcpStream) -> io::Result<()> {
    stream.write_all(
        b"HTTP/1.1 200 OK\r\n\
Content-Type: text/event-stream; charset=utf-8\r\n\
Cache-Control: no-cache, no-transform\r\n\
Connection: close\r\n\
X-Accel-Buffering: no\r\n\r\n",
    )?;
    stream.flush()
}

pub fn format_sse_event(event: &str, data: &str) -> String {
    let mut out = format!("event: {event}\n");
    if data.is_empty() {
        out.push_str("data: \n");
    } else {
        for line in data.split('\n') {
            out.push_str("data: ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push('\n');
    out
}

fn write_sse_event(stream: &mut TcpStream, event: &str, data: &str) -> io::Result<()> {
    stream.write_all(format_sse_event(event, data).as_bytes())?;
    stream.flush()
}

/// 测试辅助：在当前线程处理一条已解析请求（不绑端口）。
#[cfg(test)]
pub(crate) fn dispatch_for_test(
    method: &str,
    path: &str,
    body: &[u8],
    offer: Option<PairOffer>,
) -> (u16, serde_json::Value) {
    dispatch_for_test_ex(method, path, body, offer, None, Vec::new(), None)
}

#[cfg(test)]
pub(crate) fn dispatch_for_test_ex(
    method: &str,
    path: &str,
    body: &[u8],
    offer: Option<PairOffer>,
    authorization: Option<&str>,
    devices: Vec<AuthorizedDevice>,
    auto_reply: Option<String>,
) -> (u16, serde_json::Value) {
    let shared = Mutex::new(GatewayShared {
        offer,
        public_url: Some("https://demo.trycloudflare.com".into()),
        new_devices: Vec::new(),
        devices,
        pending: None,
        pending_stop: false,
        snapshot: ChatSnapshot {
            host: "demo-host".into(),
            session_title: "日常咨询".into(),
            writable: true,
            streaming: false,
            can_stop: false,
            partial: String::new(),
            messages: vec![SnapshotMsg {
                role: "user".into(),
                text: "hi".into(),
            }],
            tools: vec![SnapshotTool {
                id: "call-1".into(),
                name: "bash".into(),
                status: "done".into(),
                args: Some("{\"command\":\"echo hi\"}".into()),
                duration_ms: Some(12),
            }],
        },
        auto_reply,
    });
    let req = HttpReq {
        method: method.to_owned(),
        path: path.to_owned(),
        body: body.to_vec(),
        authorization: authorization.map(str::to_owned),
        accept: None,
    };
    let (status, text) = match dispatch(&req, &shared) {
        DispatchOut::Ready(s, b) => (s, b),
        DispatchOut::ChatWait(_) | DispatchOut::ChatStream(_) => (
            503,
            serde_json::json!({"ok":false,"error":"desktop_offline"}).to_string(),
        ),
    };
    let val = serde_json::from_str(&text).unwrap_or(serde_json::json!({"raw": text}));
    (status, val)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(code: &str, ttl_secs: i64) -> PairOffer {
        let now = unix_now() as i64;
        PairOffer {
            code: code.to_owned(),
            expires_unix: (now + ttl_secs).max(0) as u64,
        }
    }

    fn device_with_token(token: &str) -> AuthorizedDevice {
        AuthorizedDevice {
            id: "dev_aa".into(),
            name: "Pixel".into(),
            platform: "Android".into(),
            authorized_unix: 1,
            token_sha256: crate::relay_mgr::sha256_hex(token.as_bytes()),
        }
    }

    #[test]
    fn health_ok() {
        let (st, v) = dispatch_for_test("GET", "/health", b"", None);
        assert_eq!(st, 200);
        assert_eq!(v["ok"], true);
        assert_eq!(v["chat"], true);
        assert_eq!(v["writable"], true);
        assert_eq!(v["app"], "pi-light");
    }

    #[test]
    fn chat_requires_bearer() {
        let (st, v) = dispatch_for_test("GET", "/v1/chat", b"", None);
        assert_eq!(st, 401);
        assert_eq!(v["error"], "unauthorized");

        let (st, v) = dispatch_for_test("POST", "/v1/chat", br#"{"text":"hi"}"#, None);
        assert_eq!(st, 401);
        assert_eq!(v["error"], "unauthorized");
    }

    #[test]
    fn chat_auto_reply_with_valid_token() {
        let token = "aabbccdd";
        let (st, v) = dispatch_for_test_ex(
            "POST",
            "/v1/chat",
            br#"{"text":"hello from phone"}"#,
            None,
            Some(&format!("Bearer {token}")),
            vec![device_with_token(token)],
            Some("ok from desktop".into()),
        );
        assert_eq!(st, 200);
        assert_eq!(v["ok"], true);
        assert_eq!(v["reply"], "ok from desktop");
        assert_eq!(v["writable"], true);
    }

    #[test]
    fn chat_rejects_revoked_token() {
        let token = "deadbeef";
        let (st, v) = dispatch_for_test_ex(
            "POST",
            "/v1/chat",
            br#"{"message":"hi"}"#,
            None,
            Some(&format!("Bearer {token}")),
            Vec::new(),
            Some("should-not-run".into()),
        );
        assert_eq!(st, 401);
        assert_eq!(v["error"], "unauthorized");
        assert!(!v.to_string().contains("deadbeef"));
    }

    #[test]
    fn chat_get_returns_snapshot() {
        let token = "snap-token";
        let (st, v) = dispatch_for_test_ex(
            "GET",
            "/v1/chat",
            b"",
            None,
            Some(&format!("Bearer {token}")),
            vec![device_with_token(token)],
            None,
        );
        assert_eq!(st, 200);
        assert_eq!(v["writable"], true);
        assert_eq!(v["host"], "demo-host");
        assert_eq!(v["session_title"], "日常咨询");
        assert_eq!(v["messages"][0]["text"], "hi");
        assert_eq!(v["can_stop"], false);
        assert_eq!(v["tools"][0]["name"], "bash");
        assert_eq!(v["tools"][0]["status"], "done");
    }

    #[test]
    fn stop_requires_bearer_and_active_turn() {
        let (st, v) = dispatch_for_test("POST", STOP_PATH, b"{}", None);
        assert_eq!(st, 401);
        assert_eq!(v["error"], "unauthorized");

        let token = "stop-token";
        let (st, v) = dispatch_for_test_ex(
            "POST",
            STOP_PATH,
            b"{}",
            None,
            Some(&format!("Bearer {token}")),
            vec![device_with_token(token)],
            None,
        );
        assert_eq!(st, 409);
        assert_eq!(v["error"], "not_streaming");
    }

    #[test]
    fn sse_event_format_is_incremental() {
        let frame = format_sse_event("delta", r#"{"text":"hi"}"#);
        assert_eq!(frame, "event: delta\ndata: {\"text\":\"hi\"}\n\n");
        let multi = format_sse_event("tool", "{\n\"name\":\"bash\"\n}");
        assert!(multi.contains("event: tool\n"));
        assert!(multi.contains("data: {"));
        assert!(multi.contains("data: \"name\":\"bash\""));
    }

    #[test]
    fn pair_info_does_not_leak_code() {
        let (st, v) = dispatch_for_test("GET", "/v1/pair/info", b"", Some(offer("AB12CD", 600)));
        assert_eq!(st, 200);
        assert_eq!(v["has_offer"], true);
        assert!(v.get("code").is_none());
        let dumped = v.to_string();
        assert!(!dumped.contains("AB12CD"));
        assert_eq!(v["claim"], CLAIM_PATH);
        assert_eq!(v["writable"], true);
    }

    #[test]
    fn claim_rejects_wrong_and_expired() {
        let body = br#"{"code":"NOPE","device_name":"Pixel","platform":"Android"}"#;
        let (st, v) = dispatch_for_test("POST", CLAIM_PATH, body, Some(offer("AB12CD", 600)));
        assert_eq!(st, 401);
        assert_eq!(v["error"], "invalid_code");
        assert!(!v.to_string().contains("AB12CD"));

        let body = br#"{"code":"AB12CD","device_name":"Pixel"}"#;
        let (st, _) = dispatch_for_test("POST", CLAIM_PATH, body, Some(offer("AB12CD", -10)));
        assert_eq!(st, 410);

        let (st, _) = dispatch_for_test("POST", CLAIM_PATH, body, None);
        assert_eq!(st, 409);
    }

    #[test]
    fn claim_accepts_matching_code() {
        let body = br#"{"code":"ab12cd","device_name":"Pixel","platform":"Android"}"#;
        let (st, v) = dispatch_for_test("POST", CLAIM_PATH, body, Some(offer("AB12CD", 600)));
        assert_eq!(st, 200);
        assert_eq!(v["ok"], true);
        assert!(v["device_id"].as_str().unwrap().starts_with("dev_"));
        assert_eq!(v["token"].as_str().unwrap().len(), 64);
        assert_eq!(v["chat_path"], CHAT_PATH);
        assert_eq!(v["protocol"], PROTOCOL);
        assert_eq!(v["writable"], true);
    }

    #[test]
    fn live_gateway_health_and_claim() {
        let gw = GatewayHandle::start().unwrap();
        gw.set_offer(Some(offer("ZZ99AA", 600)));
        gw.set_public_url(Some("https://demo.trycloudflare.com".into()));
        let base = gw.local_base();
        let health_text = ureq::get(&format!("{base}/health"))
            .timeout(Duration::from_secs(3))
            .call()
            .unwrap()
            .into_string()
            .unwrap();
        let health: serde_json::Value = serde_json::from_str(&health_text).unwrap();
        assert_eq!(health["ok"], true);
        assert_eq!(health["chat"], true);

        let body =
            serde_json::json!({"code":"ZZ99AA","name":"Pixel","platform":"Android"}).to_string();
        let claimed_text = ureq::post(&format!("{base}{CLAIM_PATH}"))
            .timeout(Duration::from_secs(3))
            .set("Content-Type", "application/json")
            .send_string(&body)
            .unwrap()
            .into_string()
            .unwrap();
        let claimed: serde_json::Value = serde_json::from_str(&claimed_text).unwrap();
        assert_eq!(claimed["ok"], true);
        let new = gw.take_new_devices();
        assert_eq!(new.len(), 1);
        assert_eq!(new[0].device.name, "Pixel");
        assert_eq!(new[0].device.platform, "Android");
        assert_eq!(new[0].device.token_sha256.len(), 64);
    }

    #[test]
    fn live_gateway_chat_roundtrip_and_revoke() {
        let gw = GatewayHandle::start().unwrap();
        gw.set_offer(Some(offer("QQ11WW", 600)));
        gw.set_public_url(Some("https://demo.trycloudflare.com".into()));
        let base = gw.local_base();
        let claimed_text = ureq::post(&format!("{base}{CLAIM_PATH}"))
            .timeout(Duration::from_secs(3))
            .set("Content-Type", "application/json")
            .send_string(r#"{"code":"QQ11WW","device_name":"Pixel","platform":"Android"}"#)
            .unwrap()
            .into_string()
            .unwrap();
        let claimed: serde_json::Value = serde_json::from_str(&claimed_text).unwrap();
        let token = claimed["token"].as_str().unwrap().to_owned();
        let new = gw.take_new_devices();
        gw.set_authorized_devices(vec![new[0].device.clone()]);

        let waiter_base = base.clone();
        let waiter_token = token.clone();
        let handle = thread::spawn(move || {
            ureq::post(&format!("{waiter_base}{CHAT_PATH}"))
                .timeout(Duration::from_secs(5))
                .set("Content-Type", "application/json")
                .set("Authorization", &format!("Bearer {waiter_token}"))
                .send_string(r#"{"text":"hello from companion"}"#)
                .unwrap()
                .into_string()
                .unwrap()
        });

        let pending = (0..50)
            .find_map(|_| {
                thread::sleep(Duration::from_millis(20));
                gw.take_pending_chat()
            })
            .expect("pending chat");
        assert_eq!(pending.text, "hello from companion");
        pending
            .reply
            .send(CompanionChatEvent::Settled {
                text: "hi from desktop session".into(),
                aborted: false,
            })
            .unwrap();

        let body: serde_json::Value = serde_json::from_str(&handle.join().unwrap()).unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["reply"], "hi from desktop session");
        assert_eq!(body["writable"], true);

        gw.set_authorized_devices(Vec::new());
        let revoked = ureq::post(&format!("{base}{CHAT_PATH}"))
            .timeout(Duration::from_secs(3))
            .set("Content-Type", "application/json")
            .set("Authorization", &format!("Bearer {token}"))
            .send_string(r#"{"text":"after revoke"}"#);
        let err = revoked.expect_err("revoked token must fail");
        let resp = err.into_response().expect("http error");
        assert_eq!(resp.status(), 401);
    }

    #[test]
    fn live_gateway_stream_and_stop() {
        let gw = GatewayHandle::start().unwrap();
        gw.set_offer(Some(offer("SS22TT", 600)));
        gw.set_public_url(Some("https://demo.trycloudflare.com".into()));
        let base = gw.local_base();
        let claimed_text = ureq::post(&format!("{base}{CLAIM_PATH}"))
            .timeout(Duration::from_secs(3))
            .set("Content-Type", "application/json")
            .send_string(r#"{"code":"SS22TT","device_name":"Pixel","platform":"Android"}"#)
            .unwrap()
            .into_string()
            .unwrap();
        let claimed: serde_json::Value = serde_json::from_str(&claimed_text).unwrap();
        let token = claimed["token"].as_str().unwrap().to_owned();
        let new = gw.take_new_devices();
        gw.set_authorized_devices(vec![new[0].device.clone()]);

        gw.set_chat_snapshot(ChatSnapshot {
            streaming: true,
            can_stop: true,
            tools: vec![SnapshotTool {
                id: "t1".into(),
                name: "bash".into(),
                status: "running".into(),
                args: Some("ls".into()),
                duration_ms: None,
            }],
            ..ChatSnapshot {
                host: "demo-host".into(),
                ..ChatSnapshot::default()
            }
        });
        let snap_text = ureq::get(&format!("{base}{CHAT_PATH}"))
            .timeout(Duration::from_secs(3))
            .set("Authorization", &format!("Bearer {token}"))
            .call()
            .unwrap()
            .into_string()
            .unwrap();
        let snap: serde_json::Value = serde_json::from_str(&snap_text).unwrap();
        assert_eq!(snap["can_stop"], true);
        assert_eq!(snap["tools"][0]["status"], "running");

        let stop_text = ureq::post(&format!("{base}{STOP_PATH}"))
            .timeout(Duration::from_secs(3))
            .set("Authorization", &format!("Bearer {token}"))
            .set("Content-Type", "application/json")
            .send_string("{}")
            .unwrap()
            .into_string()
            .unwrap();
        let stop: serde_json::Value = serde_json::from_str(&stop_text).unwrap();
        assert_eq!(stop["ok"], true);
        assert_eq!(stop["accepted"], true);
        assert!(gw.take_pending_stop());

        gw.set_chat_snapshot(ChatSnapshot {
            host: "demo-host".into(),
            ..ChatSnapshot::default()
        });

        let waiter_base = base.clone();
        let waiter_token = token.clone();
        let handle = thread::spawn(move || {
            ureq::post(&format!("{waiter_base}{CHAT_PATH}"))
                .timeout(Duration::from_secs(5))
                .set("Content-Type", "application/json")
                .set("Accept", "text/event-stream")
                .set("Authorization", &format!("Bearer {waiter_token}"))
                .send_string(r#"{"text":"stream please","stream":true}"#)
                .unwrap()
                .into_string()
                .unwrap()
        });

        let pending = (0..50)
            .find_map(|_| {
                thread::sleep(Duration::from_millis(20));
                gw.take_pending_chat()
            })
            .expect("pending streamed chat");
        pending
            .reply
            .send(CompanionChatEvent::Delta { text: "Hel".into() })
            .unwrap();
        pending
            .reply
            .send(CompanionChatEvent::Tool(SnapshotTool {
                id: "c1".into(),
                name: "bash".into(),
                status: "running".into(),
                args: Some("echo".into()),
                duration_ms: None,
            }))
            .unwrap();
        pending
            .reply
            .send(CompanionChatEvent::Settled {
                text: "Hello".into(),
                aborted: false,
            })
            .unwrap();

        let body = handle.join().unwrap();
        assert!(body.contains("event: delta"), "{body}");
        assert!(body.contains("event: tool"), "{body}");
        assert!(body.contains("event: settled"), "{body}");
        assert!(body.contains("Hello"), "{body}");
    }

    #[test]
    fn unknown_path_404() {
        let (st, _) = dispatch_for_test("GET", "/nope", b"", None);
        assert_eq!(st, 404);
    }

    #[test]
    fn ct_eq_rejects_length_mismatch() {
        assert!(!ct_eq("aa", "aabb"));
        assert!(ct_eq("abcd", "abcd"));
    }
}
