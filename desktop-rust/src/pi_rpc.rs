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

/// Events delivered from the Pi stdout reader to the UI thread.
#[derive(Debug)]
pub enum PiRpcEvent {
    /// A text fragment to append to the current streaming assistant message.
    TextDelta(String),
    /// The current session-level run has fully settled; Pi will not continue.
    AgentSettled { aborted: bool },
    /// The Pi process exited (normal or abnormal).
    ProcessExited { code: Option<i32> },
    /// A command returned `success: false`.
    CommandError { command: String, error: String },
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
                if let Some(event) = parse_event(&val) {
                    // Capture exit code hint if Pi reports it
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

/// Convert a raw stdout JSONL record into a typed `PiRpcEvent`, if relevant.
///
/// Records that the UI does not need (e.g. `agent_start`, `turn_start`,
/// `tool_execution_*`, compaction events, etc.) return `None` and are
/// silently discarded — the client only tracks what matters for the chat UI.
fn parse_event(val: &Value) -> Option<PiRpcEvent> {
    let event_type = val.get("type")?.as_str()?;

    match event_type {
        // ── Streaming text ─────────────────────────────────────────────
        "message_update" => {
            let ae = val.get("assistantMessageEvent")?;
            let ae_type = ae.get("type")?.as_str()?;
            if ae_type == "text_delta" {
                let delta = ae.get("delta")?.as_str()?;
                if delta.is_empty() {
                    return None;
                }
                Some(PiRpcEvent::TextDelta(delta.to_owned()))
            } else {
                None
            }
        }

        // ── Turn completion ────────────────────────────────────────────
        "agent_settled" => {
            let aborted = val.get("aborted").and_then(Value::as_bool).unwrap_or(false);
            Some(PiRpcEvent::AgentSettled { aborted })
        }

        // ── Command errors ─────────────────────────────────────────────
        "response" => {
            let success = val.get("success").and_then(Value::as_bool).unwrap_or(true);
            if !success {
                let command = val
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_owned();
                let error = val
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
                    .to_owned();
                Some(PiRpcEvent::CommandError { command, error })
            } else {
                None // successful responses are not surfaced to the UI
            }
        }

        // ── All other events (agent_start, turn_*, tool_*, etc.) ───────
        _ => None,
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_val(json_str: &str) -> Value {
        serde_json::from_str(json_str).unwrap()
    }

    #[test]
    fn parse_text_delta() {
        let val = make_val(
            r#"{"type":"message_update","usage":{},"assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":"Hello"}}"#,
        );
        let ev = parse_event(&val);
        assert!(matches!(ev, Some(PiRpcEvent::TextDelta(d)) if d == "Hello"));
    }

    #[test]
    fn parse_agent_settled_normal() {
        let val = make_val(r#"{"type":"agent_settled","aborted":false}"#);
        let ev = parse_event(&val);
        assert!(
            matches!(ev, Some(PiRpcEvent::AgentSettled { aborted: false })),
            "expected AgentSettled{{aborted:false}}, got {ev:?}"
        );
    }

    #[test]
    fn parse_agent_settled_aborted() {
        let val = make_val(r#"{"type":"agent_settled","aborted":true}"#);
        let ev = parse_event(&val);
        assert!(matches!(
            ev,
            Some(PiRpcEvent::AgentSettled { aborted: true })
        ));
    }

    #[test]
    fn parse_command_error() {
        let val = make_val(
            r#"{"type":"response","command":"prompt","success":false,"error":"model not configured"}"#,
        );
        let ev = parse_event(&val);
        assert!(
            matches!(ev, Some(PiRpcEvent::CommandError { ref command, ref error })
                if command == "prompt" && error == "model not configured"
            )
        );
    }

    #[test]
    fn parse_successful_response_returns_none() {
        let val = make_val(
            r#"{"id":"1","type":"response","command":"prompt","success":true,"data":{"disposition":"started"}}"#,
        );
        assert!(parse_event(&val).is_none());
    }

    #[test]
    fn parse_irrelevant_event_returns_none() {
        for raw in &[
            r#"{"type":"agent_start"}"#,
            r#"{"type":"turn_start"}"#,
            r#"{"type":"agent_end","messages":[],"willRetry":false}"#,
            r#"{"type":"tool_execution_start","toolCallId":"x","toolName":"bash","args":{}}"#,
        ] {
            let val = make_val(raw);
            assert!(
                parse_event(&val).is_none(),
                "expected None for {raw}, but got Some"
            );
        }
    }

    #[test]
    fn parse_empty_delta_returns_none() {
        let val = make_val(
            r#"{"type":"message_update","usage":{},"assistantMessageEvent":{"type":"text_delta","contentIndex":0,"delta":""}}"#,
        );
        assert!(parse_event(&val).is_none());
    }

    #[test]
    fn parse_non_delta_message_update_returns_none() {
        let val = make_val(
            r#"{"type":"message_update","usage":{},"assistantMessageEvent":{"type":"text_end","contentIndex":0,"content":"Hello"}}"#,
        );
        assert!(parse_event(&val).is_none());
    }
}
