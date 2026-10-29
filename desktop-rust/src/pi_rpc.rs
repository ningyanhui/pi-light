// SPDX-License-Identifier: Apache-2.0
//
// Pi RPC client (M2′) — clean-room thin client for `pi --mode rpc` JSONL.
//
// Protocol reference: earendil-works/pi packages/coding-agent/docs/rpc.md
//                     rpc-commands.md, json.md
//
// Design:
//   - Spawn `pi --mode rpc --no-session` with piped stdin/stdout.
//   - Background thread reads stdout JSONL and sends PiRpcEvent via mpsc.
//   - UI thread drains the channel each frame via drain_events().
//   - Writing commands to stdin is mutex-free (single UI thread owner).
//   - Shutdown: close stdin → wait ≤ timeout → kill (no orphan/zombie).

use std::{
    io::{self, BufRead, BufReader, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use serde_json::{json, Value};

// ── Public event type ─────────────────────────────────────────────────────

/// Token fields we understand from Pi `usage` / `get_session_stats.data.tokens`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TokenUsage {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub total: Option<u64>,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
}

impl TokenUsage {
    pub fn is_empty(&self) -> bool {
        self.input.is_none()
            && self.output.is_none()
            && self.total.is_none()
            && self.cache_read.is_none()
            && self.cache_write.is_none()
    }

    /// Merge only keys present in `other` (does not clear existing fields).
    pub fn merge(&mut self, other: &TokenUsage) {
        if other.input.is_some() {
            self.input = other.input;
        }
        if other.output.is_some() {
            self.output = other.output;
        }
        if other.total.is_some() {
            self.total = other.total;
        }
        if other.cache_read.is_some() {
            self.cache_read = other.cache_read;
        }
        if other.cache_write.is_some() {
            self.cache_write = other.cache_write;
        }
    }
}

/// Subset of `get_session_stats` `data` we surface in the UI.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionStats {
    pub tokens: TokenUsage,
    pub cost: Option<f64>,
    pub tool_calls: Option<u64>,
    pub context_tokens: Option<u64>,
    pub context_window: Option<u64>,
    pub context_percent: Option<f64>,
}

/// One tool invocation step for the trace list.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceStep {
    pub tool_call_id: String,
    pub tool_name: String,
    pub running: bool,
    pub is_error: bool,
    pub duration_ms: Option<u64>,
}

/// Events delivered from the Pi stdout reader to the UI thread.
#[derive(Debug)]
pub enum PiRpcEvent {
    /// A text fragment to append to the current streaming assistant message.
    TextDelta(String),
    /// Cumulative usage from a `message_update` record (when Pi sends numeric fields).
    UsageUpdate(TokenUsage),
    /// Successful `get_session_stats` response payload.
    SessionStats(SessionStats),
    /// Tool execution started (`tool_execution_start`).
    ToolTraceStart {
        tool_call_id: String,
        tool_name: String,
    },
    /// Tool execution finished (`tool_execution_end`).
    ToolTraceEnd {
        tool_call_id: String,
        tool_name: String,
        is_error: bool,
        duration_ms: Option<u64>,
    },
    /// The current session-level run has fully settled; Pi will not continue.
    AgentSettled { aborted: bool },
    /// The Pi process exited (normal or abnormal).
    ProcessExited { code: Option<i32> },
    /// A command returned `success: false`.
    CommandError { command: String, error: String },
    /// M4′: `get_state` response — current model info.
    StateInfo {
        /// "{provider}/{modelId}" 格式，如 "anthropic/claude-sonnet-4-20250514"。
        model: Option<String>,
    },
    /// M4′: `get_available_models` response — list of "{provider}/{modelId}".
    AvailableModels(Vec<String>),
    /// M4′: `set_model` succeeded; new model in "{provider}/{modelId}" format.
    ModelChanged(String),
}

// ── Bash PATH injection ───────────────────────────────────────────────────

/// Prepend bundled bash/MSYS dirs to the child process PATH (Windows only).
///
/// This ensures Pi can locate `bash.exe` when running shell commands, using
/// the portable MinGit environment bundled in `sidecar/bash/`.  If no bundled
/// bash exists the function is a no-op; Pi falls back to whatever `bash` is in
/// the inherited PATH.
fn inject_bundled_bash_path(cmd: &mut Command) {
    let extra_dirs = crate::bash_path::bundled_path_dirs();
    if extra_dirs.is_empty() {
        return;
    }
    let base = std::env::var_os("PATH").unwrap_or_default();
    let all: Vec<std::path::PathBuf> = extra_dirs
        .into_iter()
        .chain(std::env::split_paths(&base))
        .collect();
    if let Ok(new_path) = std::env::join_paths(&all) {
        cmd.env("PATH", new_path);
    }
}

// ── Client ────────────────────────────────────────────────────────────────

/// Thin owner of the `pi --mode rpc` child process and its I/O.
///
/// Dropping this value triggers an orderly shutdown (close stdin → wait
/// up to 3 s → kill).  Call [`shutdown`] explicitly if you need a custom
/// timeout or error handling.
pub struct PiRpcClient {
    stdin: Option<ChildStdin>,
    event_rx: Receiver<PiRpcEvent>,
    child: Child,
    next_id: u64,
}

impl PiRpcClient {
    /// Spawn `pi --mode rpc --no-session` at `bin_path`.
    ///
    /// `on_event` is called from the background reader thread whenever a
    /// new event is queued (use it to call `egui::Context::request_repaint`
    /// or a test wake-up hook).
    pub fn spawn<F>(bin_path: &Path, on_event: F) -> io::Result<Self>
    where
        F: Fn() + Send + 'static,
    {
        let mut cmd = Command::new(bin_path);
        cmd.args(["--mode", "rpc", "--no-session"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()); // stderr carries diagnostics, not protocol data

        // Inject bundled bash/MSYS dirs (sidecar/bash/) into the child's PATH so Pi can
        // locate bash for shell commands.  Detection and injection happen once per spawn —
        // the "单次校验入口" constraint.  On non-Windows the dirs list is empty (no-op).
        inject_bundled_bash_path(&mut cmd);

        // Prevent a console window from flashing on Windows GUI builds.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || reader_thread(stdout, tx, on_event));

        Ok(Self {
            stdin: Some(stdin),
            event_rx: rx,
            child,
            next_id: 1,
        })
    }

    /// Send a user prompt.  Returns as soon as the JSONL line is flushed.
    ///
    /// Per the protocol, Pi may queue the prompt if already streaming;
    /// the response disposition (`started` / `queued` / `handled`) arrives
    /// as a future `response` record and is currently ignored by the client.
    pub fn send_prompt(&mut self, text: &str) -> io::Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        self.write_command(json!({
            "id": id.to_string(),
            "type": "prompt",
            "message": text,
        }))
    }

    /// Abort the current in-flight turn.  Pi will emit `agent_settled`
    /// with `aborted: true` once the abort is honoured.
    pub fn send_abort(&mut self) -> io::Result<()> {
        self.write_command(json!({"type": "abort"}))
    }

    /// Request session-level token / cost stats (`get_session_stats`).
    pub fn request_session_stats(&mut self) -> io::Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        self.write_command(json!({
            "id": id.to_string(),
            "type": "get_session_stats",
        }))
    }

    /// M4′: Query current state (model, session name, etc.) via `get_state`.
    pub fn request_get_state(&mut self) -> io::Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        self.write_command(json!({
            "id": id.to_string(),
            "type": "get_state",
        }))
    }

    /// M4′: Query all available models via `get_available_models`.
    pub fn request_available_models(&mut self) -> io::Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        self.write_command(json!({
            "id": id.to_string(),
            "type": "get_available_models",
        }))
    }

    /// M4′: Switch to a specific model via `set_model`.
    pub fn send_set_model(&mut self, provider: &str, model_id: &str) -> io::Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        self.write_command(json!({
            "id": id.to_string(),
            "type": "set_model",
            "provider": provider,
            "modelId": model_id,
        }))
    }

    /// Non-blocking drain of all pending events.
    pub fn drain_events(&mut self) -> Vec<PiRpcEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.event_rx.try_recv() {
            out.push(ev);
        }
        out
    }

    /// Orderly shutdown: close stdin → wait up to `timeout` → kill.
    ///
    /// Safe to call multiple times (no-op after the first call).
    pub fn shutdown(&mut self, timeout: Duration) {
        // Close stdin: signals Pi to finish up and exit.
        drop(self.stdin.take());

        // Wait for Pi to exit on its own.
        let deadline = Instant::now() + timeout;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() >= deadline => break,
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(_) => break,
            }
        }

        // Force-kill after timeout; wait so the kernel can reap the zombie.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    fn write_command(&mut self, cmd: Value) -> io::Result<()> {
        let stdin = match &mut self.stdin {
            Some(s) => s,
            None => return Ok(()), // already shut down
        };
        let mut line = serde_json::to_string(&cmd)
            .expect("serde_json::to_string of a json!() literal cannot fail");
        line.push('\n');
        stdin.write_all(line.as_bytes())?;
        stdin.flush()
    }
}

impl Drop for PiRpcClient {
    fn drop(&mut self) {
        self.shutdown(Duration::from_secs(3));
    }
}

// ── Background stdout reader ──────────────────────────────────────────────

fn reader_thread<F>(stdout: std::process::ChildStdout, tx: Sender<PiRpcEvent>, on_event: F)
where
    F: Fn(),
{
    let reader = BufReader::new(stdout);
    let mut exit_code: Option<i32> = None;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(trimmed) {
            Ok(val) => {
                let events = parse_events(&val);
                for event in events {
                    if let PiRpcEvent::ProcessExited { code } = &event {
                        exit_code = *code;
                    }
                    if tx.send(event).is_err() {
                        break; // receiver dropped
                    }
                    on_event();
                }
            }
            Err(_) => {
                // Ignore unparseable lines (e.g. startup banners)
            }
        }
    }

    // Notify the UI that the process has exited.
    let _ = tx.send(PiRpcEvent::ProcessExited { code: exit_code });
    on_event();
}

// ── JSONL event parser ────────────────────────────────────────────────────

/// Convert a raw stdout JSONL record into typed `PiRpcEvent` values.
fn parse_events(val: &Value) -> Vec<PiRpcEvent> {
    let Some(event_type) = val.get("type").and_then(Value::as_str) else {
        return Vec::new();
    };

    match event_type {
        // ── Streaming text + usage ─────────────────────────────────────
        "message_update" => {
            let mut out = Vec::new();
            if let Some(usage) = parse_usage_object(val.get("usage")) {
                if !usage.is_empty() {
                    out.push(PiRpcEvent::UsageUpdate(usage));
                }
            }
            if let Some(ae) = val.get("assistantMessageEvent") {
                if ae.get("type").and_then(Value::as_str) == Some("text_delta") {
                    if let Some(delta) = ae.get("delta").and_then(Value::as_str) {
                        if !delta.is_empty() {
                            out.push(PiRpcEvent::TextDelta(delta.to_owned()));
                        }
                    }
                }
            }
            out
        }

        "tool_execution_start" => {
            let tool_call_id = val
                .get("toolCallId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let tool_name = val
                .get("toolName")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_owned();
            if tool_call_id.is_empty() {
                Vec::new()
            } else {
                vec![PiRpcEvent::ToolTraceStart {
                    tool_call_id,
                    tool_name,
                }]
            }
        }

        "tool_execution_end" => {
            let tool_call_id = val
                .get("toolCallId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let tool_name = val
                .get("toolName")
                .and_then(Value::as_str)
                .unwrap_or("tool")
                .to_owned();
            if tool_call_id.is_empty() {
                Vec::new()
            } else {
                vec![PiRpcEvent::ToolTraceEnd {
                    tool_call_id,
                    tool_name,
                    is_error: val.get("isError").and_then(Value::as_bool).unwrap_or(false),
                    duration_ms: val.get("durationMs").and_then(as_u64),
                }]
            }
        }

        // ── Turn completion ────────────────────────────────────────────
        "agent_settled" => {
            let aborted = val.get("aborted").and_then(Value::as_bool).unwrap_or(false);
            vec![PiRpcEvent::AgentSettled { aborted }]
        }

        // ── Command responses ──────────────────────────────────────────
        "response" => {
            let success = val.get("success").and_then(Value::as_bool).unwrap_or(true);
            let command = val
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned();
            if !success {
                let error = val
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
                    .to_owned();
                vec![PiRpcEvent::CommandError { command, error }]
            } else if command == "get_session_stats" {
                if let Some(data) = val.get("data") {
                    vec![PiRpcEvent::SessionStats(parse_session_stats(data))]
                } else {
                    Vec::new()
                }
            } else if command == "get_state" {
                // M4′: parse current model from get_state response
                let model = val
                    .get("data")
                    .and_then(|d| d.get("model"))
                    .and_then(format_model_str);
                vec![PiRpcEvent::StateInfo { model }]
            } else if command == "get_available_models" {
                // M4′: list of model objects
                let models = val
                    .get("data")
                    .and_then(|d| d.get("models"))
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().filter_map(format_model_str).collect())
                    .unwrap_or_default();
                vec![PiRpcEvent::AvailableModels(models)]
            } else if command == "set_model" {
                // M4′: model was switched
                let model = val
                    .get("data")
                    .and_then(|d| d.get("model"))
                    .and_then(format_model_str)
                    .unwrap_or_default();
                vec![PiRpcEvent::ModelChanged(model)]
            } else {
                Vec::new()
            }
        }

        _ => Vec::new(),
    }
}

fn as_u64(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_f64().map(|f| f as u64))
}

/// M4′: 将 model 对象序列化为 "{provider}/{modelId}" 字符串。
fn format_model_str(model: &Value) -> Option<String> {
    let provider = model.get("provider").and_then(Value::as_str)?;
    let model_id = model.get("modelId").and_then(Value::as_str)?;
    Some(format!("{provider}/{model_id}"))
}

fn as_f64(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_u64().map(|u| u as f64))
}

fn parse_usage_object(usage: Option<&Value>) -> Option<TokenUsage> {
    let usage = usage?;
    if !usage.is_object() {
        return None;
    }
    let mut tokens = TokenUsage::default();
    if usage.get("input").is_some() {
        tokens.input = usage.get("input").and_then(as_u64);
    }
    if usage.get("output").is_some() {
        tokens.output = usage.get("output").and_then(as_u64);
    }
    if usage.get("total").is_some() {
        tokens.total = usage.get("total").and_then(as_u64);
    }
    if usage.get("totalTokens").is_some() {
        tokens.total = usage.get("totalTokens").and_then(as_u64);
    }
    if usage.get("cacheRead").is_some() {
        tokens.cache_read = usage.get("cacheRead").and_then(as_u64);
    }
    if usage.get("cacheWrite").is_some() {
        tokens.cache_write = usage.get("cacheWrite").and_then(as_u64);
    }
    Some(tokens)
}

fn parse_session_stats(data: &Value) -> SessionStats {
    let mut stats = SessionStats::default();
    if let Some(tokens_val) = data.get("tokens") {
        if let Some(parsed) = parse_usage_object(Some(tokens_val)) {
            stats.tokens = parsed;
        }
    }
    if data.get("cost").is_some() {
        stats.cost = data.get("cost").and_then(as_f64);
    }
    if data.get("toolCalls").is_some() {
        stats.tool_calls = data.get("toolCalls").and_then(as_u64);
    }
    if let Some(ctx) = data.get("contextUsage") {
        if ctx.get("tokens").is_some() {
            stats.context_tokens = ctx.get("tokens").and_then(as_u64);
        }
        if ctx.get("contextWindow").is_some() {
            stats.context_window = ctx.get("contextWindow").and_then(as_u64);
        }
        if ctx.get("percent").is_some() {
            stats.context_percent = ctx.get("percent").and_then(as_f64);
        }
    }
    stats
}

// ── Unit tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_val(json_str: &str) -> Value {
        serde_json::from_str(json_str).unwrap()
    }

    fn parse_one(json_str: &str) -> Vec<PiRpcEvent> {
        parse_events(&make_val(json_str))
    }

    #[test]
    fn parse_text_delta() {
        let events = parse_one(
            r#"{"type":"message_update","usage":{},"assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"Hello"}}"#,
        );
        assert!(events
            .iter()
            .any(|ev| { matches!(ev, PiRpcEvent::TextDelta(d) if d == "Hello") }));
    }

    #[test]
    fn parse_usage_update_from_message_update() {
        let events = parse_one(
            r#"{"type":"message_update","usage":{"input":10,"output":5,"totalTokens":15},"assistantMessageEvent":{"type":"text_end","contentIndex":0,"content":"x"}}"#,
        );
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            PiRpcEvent::UsageUpdate(ref u) if u.input == Some(10) && u.output == Some(5) && u.total == Some(15)
        ));
    }

    #[test]
    fn parse_empty_usage_object_emits_nothing() {
        let events = parse_one(
            r#"{"type":"message_update","usage":{},"assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"Hi"}}"#,
        );
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], PiRpcEvent::TextDelta(_)));
    }

    #[test]
    fn parse_session_stats_response() {
        let events = parse_one(
            r#"{"type":"response","command":"get_session_stats","success":true,"data":{"tokens":{"input":1,"output":2,"total":3},"cost":0.01,"toolCalls":4,"contextUsage":{"tokens":100,"contextWindow":200000,"percent":0.05}}}"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            PiRpcEvent::SessionStats(s) => {
                assert_eq!(s.tokens.input, Some(1));
                assert_eq!(s.cost, Some(0.01));
                assert_eq!(s.tool_calls, Some(4));
                assert_eq!(s.context_percent, Some(0.05));
            }
            other => panic!("expected SessionStats, got {other:?}"),
        }
    }

    #[test]
    fn parse_tool_trace_lifecycle() {
        let start = parse_one(
            r#"{"type":"tool_execution_start","toolCallId":"x","toolName":"bash","args":{}}"#,
        );
        assert!(matches!(
            start[0],
            PiRpcEvent::ToolTraceStart { ref tool_name, .. } if tool_name == "bash"
        ));
        let end = parse_one(
            r#"{"type":"tool_execution_end","toolCallId":"x","toolName":"bash","isError":false,"durationMs":99}"#,
        );
        assert!(matches!(
            end[0],
            PiRpcEvent::ToolTraceEnd {
                duration_ms: Some(99),
                ..
            }
        ));
    }

    #[test]
    fn parse_agent_settled_normal() {
        let events = parse_one(r#"{"type":"agent_settled","aborted":false}"#);
        assert!(
            matches!(events[0], PiRpcEvent::AgentSettled { aborted: false }),
            "expected AgentSettled{{aborted:false}}, got {events:?}"
        );
    }

    #[test]
    fn parse_agent_settled_aborted() {
        let events = parse_one(r#"{"type":"agent_settled","aborted":true}"#);
        assert!(matches!(
            events[0],
            PiRpcEvent::AgentSettled { aborted: true }
        ));
    }

    #[test]
    fn parse_command_error() {
        let events = parse_one(
            r#"{"type":"response","command":"prompt","success":false,"error":"model not configured"}"#,
        );
        assert!(
            matches!(events[0], PiRpcEvent::CommandError { ref command, ref error }
                if command == "prompt" && error == "model not configured"
            )
        );
    }

    #[test]
    fn parse_successful_prompt_response_returns_empty() {
        let events = parse_one(
            r#"{"id":"1","type":"response","command":"prompt","success":true,"data":{"disposition":"started"}}"#,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn parse_irrelevant_event_returns_empty() {
        for raw in &[
            r#"{"type":"agent_start"}"#,
            r#"{"type":"turn_start"}"#,
            r#"{"type":"agent_end","messages":[],"willRetry":false}"#,
        ] {
            assert!(
                parse_one(raw).is_empty(),
                "expected empty for {raw}, but got events"
            );
        }
    }

    #[test]
    fn parse_empty_delta_returns_none() {
        let events = parse_one(
            r#"{"type":"message_update","usage":{},"assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":""}}"#,
        );
        assert!(events.is_empty());
    }

    #[test]
    fn parse_non_delta_message_update_returns_none() {
        let events = parse_one(
            r#"{"type":"message_update","usage":{},"assistantMessageEvent":{"type":"text_end","contentIndex":0,"content":"Hello"}}"#,
        );
        assert!(events.is_empty());
    }
}
