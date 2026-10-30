// SPDX-License-Identifier: Apache-2.0
//
// 本机 loopback 控制面。Cloudflare Tunnel 把这里暴露到公网。
// M5：health / pair info / pair claim。M6 的 `/v1/chat` 返回 501 作为明确钩子。

use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

pub const DEFAULT_PORT: u16 = 17831;
pub const CLAIM_PATH: &str = "/v1/pair/claim";
pub const CHAT_PATH: &str = "/v1/chat";
pub const PROTOCOL: &str = "pi-light-companion/1";

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

struct GatewayShared {
    offer: Option<PairOffer>,
    public_url: Option<String>,
    new_devices: Vec<ClaimedDevice>,
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
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(8)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(8)));
                        handle_conn(stream, &state);
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

    pub fn take_new_devices(&self) -> Vec<ClaimedDevice> {
        self.shared
            .lock()
            .map(|mut g| std::mem::take(&mut g.new_devices))
            .unwrap_or_default()
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

fn handle_conn(mut stream: TcpStream, shared: &Mutex<GatewayShared>) {
    let req = match read_request(&mut stream) {
        Ok(r) => r,
        Err(_) => {
            let _ = write_json(&mut stream, 400, r#"{"ok":false,"error":"bad_request"}"#);
            return;
        }
    };
    let (status, body) = dispatch(&req, shared);
    let _ = write_json(&mut stream, status, &body);
}

struct HttpReq {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn dispatch(req: &HttpReq, shared: &Mutex<GatewayShared>) -> (u16, String) {
    let path = req.path.split('?').next().unwrap_or(req.path.as_str());
    match (req.method.as_str(), path) {
        ("GET", "/health") | ("GET", "/") => (
            200,
            serde_json::json!({
                "ok": true,
                "app": "pi-light",
                "role": "remote-gateway",
                "protocol": PROTOCOL,
                "chat": false,
            })
            .to_string(),
        ),
        ("GET", "/v1/pair/info") => pair_info(shared),
        ("POST", CLAIM_PATH) => claim(shared, &req.body),
        ("GET", CHAT_PATH) | ("POST", CHAT_PATH) => (
            501,
            serde_json::json!({
                "ok": false,
                "error": "not_implemented",
                "milestone": "M6",
                "hint": "Remote chat websocket/HTTP is reserved for the Android companion.",
                "path": CHAT_PATH,
            })
            .to_string(),
        ),
        _ => (
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
            "has_offer": has,
            "exp": exp,
            "relay": g.public_url,
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
            "protocol": PROTOCOL,
        })
        .to_string(),
    )
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
    for line in lines {
        let l = line.to_ascii_lowercase();
        if let Some(v) = l.strip_prefix("content-length:") {
            content_len = v.trim().parse().unwrap_or(0);
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
    Ok(HttpReq { method, path, body })
}

fn write_json(stream: &mut TcpStream, status: u16, body: &str) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        410 => "Gone",
        501 => "Not Implemented",
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

/// 测试辅助：在当前线程处理一条已解析请求（不绑端口）。
#[cfg(test)]
pub(crate) fn dispatch_for_test(
    method: &str,
    path: &str,
    body: &[u8],
    offer: Option<PairOffer>,
) -> (u16, serde_json::Value) {
    let shared = Mutex::new(GatewayShared {
        offer,
        public_url: Some("https://demo.trycloudflare.com".into()),
        new_devices: Vec::new(),
    });
    let req = HttpReq {
        method: method.to_owned(),
        path: path.to_owned(),
        body: body.to_vec(),
    };
    let (status, text) = dispatch(&req, &shared);
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

    #[test]
    fn health_ok() {
        let (st, v) = dispatch_for_test("GET", "/health", b"", None);
        assert_eq!(st, 200);
        assert_eq!(v["ok"], true);
        assert_eq!(v["chat"], false);
        assert_eq!(v["app"], "pi-light");
    }

    #[test]
    fn chat_is_reserved_for_m6() {
        let (st, v) = dispatch_for_test("GET", "/v1/chat", b"", None);
        assert_eq!(st, 501);
        assert_eq!(v["milestone"], "M6");
        assert_eq!(v["path"], CHAT_PATH);
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
    fn unknown_path_404() {
        let (st, _) = dispatch_for_test("GET", "/nope", b"", None);
        assert_eq!(st, 404);
    }
}
