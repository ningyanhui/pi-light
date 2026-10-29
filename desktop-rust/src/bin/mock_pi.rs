// SPDX-License-Identifier: Apache-2.0
//
// mock-pi — mock Pi RPC server for integration testing.
//
// Simulates `pi --mode rpc` JSONL protocol:
//   • `prompt`  → response + streamed text deltas + agent_settled
//   • `abort`   → response + agent_settled {aborted:true}
//   • any other → generic success response
//   • stdin EOF → graceful exit
//
// Usage (in tests):  PI_LIGHT_PI_BIN=<path-to-mock-pi> cargo test
// Or let tests discover it via CARGO_BIN_EXE_mock-pi automatically.

use std::io::{self, BufRead, BufWriter, Write};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    let mut line = String::new();
    loop {
        line.clear();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF — Pi exits cleanly when stdin closes
            Ok(_) => {}
            Err(_) => break,
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let cmd: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                // Malformed JSON — emit a parse error response (per spec)
                emit(
                    &mut out,
                    serde_json::json!({
                        "type": "response",
                        "command": "parse",
                        "success": false,
                        "error": "mock-pi: failed to parse command"
                    }),
                );
                continue;
            }
        };

        let cmd_type = cmd["type"].as_str().unwrap_or("unknown");
        let cmd_id = cmd.get("id").and_then(|v| v.as_str()).map(str::to_owned);

        match cmd_type {
            "prompt" => handle_prompt(&mut out, cmd_id, &cmd),
            "abort" => handle_abort(&mut out, cmd_id),
            "get_session_stats" => handle_get_session_stats(&mut out, cmd_id),
            "get_state" => handle_get_state(&mut out, cmd_id),
            "get_available_models" => handle_get_available_models(&mut out, cmd_id),
            "set_model" => handle_set_model(&mut out, cmd_id, &cmd),
            "new_session" => handle_session_command(&mut out, cmd_id, "new_session"),
            "switch_session" => handle_session_command(&mut out, cmd_id, "switch_session"),
            _ => handle_generic(&mut out, cmd_id, cmd_type),
        }
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────

fn handle_prompt(out: &mut impl Write, id: Option<String>, cmd: &serde_json::Value) {
    // 1. Acknowledge the prompt (disposition: started)
    emit_with_id(
        out,
        &id,
        serde_json::json!({
            "type": "response",
            "command": "prompt",
            "success": true,
            "data": { "disposition": "started" }
        }),
    );

    // 2. Stream fake session events (minimal, realistic shape)
    emit(out, serde_json::json!({"type": "agent_start"}));
    emit(out, serde_json::json!({"type": "turn_start"}));

    emit(
        out,
        serde_json::json!({
            "type": "tool_execution_start",
            "toolCallId": "mock-call-1",
            "toolName": "bash",
            "args": { "command": "echo mock" }
        }),
    );
    emit(
        out,
        serde_json::json!({
            "type": "tool_execution_end",
            "toolCallId": "mock-call-1",
            "toolName": "bash",
            "result": { "content": [{ "type": "text", "text": "mock output" }], "details": {} },
            "isError": false,
            "durationMs": 12
        }),
    );

    // M5′: 若 prompt 中含有 `[引用消息` 前缀，则在回复中回显确认
    let has_ref = cmd
        .get("message")
        .and_then(|m| m.as_str())
        .map(|m| m.contains("[引用消息") || m.contains("[引用历史会话"))
        .unwrap_or(false);

    let reply = if has_ref {
        "已收到带引用上下文的消息，mock Pi 回复：Hello from mock Pi! This is a test response."
    } else {
        "Hello from mock Pi! This is a test response."
    };

    let chunks: Vec<&str> = reply.splitn(4, ' ').collect();
    for chunk in &chunks {
        emit(
            out,
            serde_json::json!({
                "type": "message_update",
                "usage": { "input": 0, "output": 0, "totalTokens": 0 },
                "assistantMessageEvent": {
                    "type": "text_delta",
                    "contentIndex": 0,
                    "delta": format!("{} ", chunk)
                }
            }),
        );
    }

    // text_end — authoritative final text
    emit(
        out,
        serde_json::json!({
            "type": "message_update",
            "usage": { "input": 10, "output": 14, "totalTokens": 24 },
            "assistantMessageEvent": {
                "type": "text_end",
                "contentIndex": 0,
                "content": reply
            }
        }),
    );

    emit(
        out,
        serde_json::json!({"type": "turn_end", "message": {}, "toolResults": []}),
    );
    emit(
        out,
        serde_json::json!({"type": "agent_end", "messages": [], "willRetry": false}),
    );
    emit(
        out,
        serde_json::json!({"type": "agent_settled", "aborted": false}),
    );
}

fn handle_get_session_stats(out: &mut impl Write, id: Option<String>) {
    let no_stats = std::env::var("PI_LIGHT_MOCK_NO_STATS").is_ok();
    let data = if no_stats {
        serde_json::json!({
            "sessionId": "mock-session",
            "userMessages": 0,
            "assistantMessages": 0,
            "toolCalls": 0,
            "toolResults": 0,
            "totalMessages": 0,
            "tokens": {}
        })
    } else {
        serde_json::json!({
            "sessionId": "mock-session",
            "userMessages": 1,
            "assistantMessages": 1,
            "toolCalls": 1,
            "toolResults": 1,
            "totalMessages": 3,
            "tokens": {
                "input": 100,
                "output": 50,
                "cacheRead": 0,
                "cacheWrite": 0,
                "total": 150
            },
            "cost": 0.0025,
            "contextUsage": {
                "tokens": 120,
                "contextWindow": 200000,
                "percent": 0.06
            }
        })
    };
    emit_with_id(
        out,
        &id,
        serde_json::json!({
            "type": "response",
            "command": "get_session_stats",
            "success": true,
            "data": data
        }),
    );
}

fn handle_abort(out: &mut impl Write, id: Option<String>) {
    emit_with_id(
        out,
        &id,
        serde_json::json!({
            "type": "response",
            "command": "abort",
            "success": true
        }),
    );
    emit(
        out,
        serde_json::json!({"type": "agent_settled", "aborted": true}),
    );
}

fn handle_generic(out: &mut impl Write, id: Option<String>, cmd_type: &str) {
    emit_with_id(
        out,
        &id,
        serde_json::json!({
            "type": "response",
            "command": cmd_type,
            "success": true
        }),
    );
}

// ── M4′ Model management handlers ─────────────────────────────────────────

fn handle_get_state(out: &mut impl Write, id: Option<String>) {
    emit_with_id(
        out,
        &id,
        serde_json::json!({
            "type": "response",
            "command": "get_state",
            "success": true,
            "data": {
                "model": {
                    "provider": "mock",
                    "modelId": "mock-model-1",
                    "name": "Mock Model"
                },
                "sessionFile": "/tmp/mock-session.jsonl",
                "sessionId": "mock-session",
                "sessionName": "mock"
            }
        }),
    );
}

fn handle_session_command(out: &mut impl Write, id: Option<String>, command: &str) {
    emit_with_id(
        out,
        &id,
        serde_json::json!({
            "type": "response",
            "command": command,
            "success": true,
            "data": { "cancelled": false }
        }),
    );
}

fn handle_get_available_models(out: &mut impl Write, id: Option<String>) {
    emit_with_id(
        out,
        &id,
        serde_json::json!({
            "type": "response",
            "command": "get_available_models",
            "success": true,
            "data": {
                "models": [
                    {"provider": "mock", "modelId": "mock-model-1", "name": "Mock Model 1"},
                    {"provider": "mock", "modelId": "mock-model-2", "name": "Mock Model 2"}
                ]
            }
        }),
    );
}

fn handle_set_model(out: &mut impl Write, id: Option<String>, cmd: &serde_json::Value) {
    let provider = cmd
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("mock");
    let model_id = cmd
        .get("modelId")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    emit_with_id(
        out,
        &id,
        serde_json::json!({
            "type": "response",
            "command": "set_model",
            "success": true,
            "data": {
                "model": {
                    "provider": provider,
                    "modelId": model_id,
                    "name": format!("{provider}/{model_id}")
                }
            }
        }),
    );
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn emit(out: &mut impl Write, mut val: serde_json::Value) {
    emit_with_id(out, &None::<String>, val.take());
}

fn emit_with_id(out: &mut impl Write, id: &Option<String>, mut val: serde_json::Value) {
    if let Some(id_str) = id {
        if let Some(obj) = val.as_object_mut() {
            obj.insert("id".to_owned(), serde_json::json!(id_str));
        }
    }
    let line = serde_json::to_string(&val).expect("json serialization cannot fail");
    writeln!(out, "{line}").ok();
    out.flush().ok();
}
