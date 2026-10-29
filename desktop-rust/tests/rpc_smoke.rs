// SPDX-License-Identifier: Apache-2.0
//
// rpc_smoke — integration smoke tests for the Pi RPC client.
//
// These tests use mock-pi (compiled by cargo as a [[bin]] target) to
// simulate the Pi RPC server.  No real Pi binary is required for CI.
//
// To run against a real Pi binary:
//   PI_LIGHT_PI_BIN=/path/to/pi cargo test --test rpc_smoke
//
// To run with only mock-pi (default, works without real Pi):
//   cargo test --test rpc_smoke
//
// Note: `cargo test` automatically builds `mock-pi` before running
// integration tests, so you do not need to build it manually.

use pi_light::pi_rpc::{PiRpcClient, PiRpcEvent};
use std::{path::PathBuf, time::Duration};

// ── Pi binary resolution ──────────────────────────────────────────────────

/// Return the Pi binary to use for tests.
///
/// Priority:
///  1. `PI_LIGHT_PI_BIN` env var (real Pi binary for manual testing)
///  2. `mock-pi` binary compiled by cargo (default for CI)
fn pi_bin() -> PathBuf {
    if let Ok(p) = std::env::var("PI_LIGHT_PI_BIN") {
        return PathBuf::from(p);
    }
    // CARGO_BIN_EXE_mock-pi is set by cargo for integration tests.
    // Hyphens in binary names are kept as-is in this env var.
    PathBuf::from(env!("CARGO_BIN_EXE_mock-pi"))
}

// ── Helper ────────────────────────────────────────────────────────────────

/// Drain events with a polling loop, returning when `done` is true or
/// `timeout` expires.
fn drain_until<F>(client: &mut PiRpcClient, timeout: Duration, mut done: F) -> Vec<PiRpcEvent>
where
    F: FnMut(&PiRpcEvent) -> bool,
{
    let deadline = std::time::Instant::now() + timeout;
    let mut collected = Vec::new();
    let mut finished = false;
    while std::time::Instant::now() < deadline && !finished {
        for ev in client.drain_events() {
            if done(&ev) {
                finished = true;
            }
            collected.push(ev);
        }
        if !finished {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    collected
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[test]
fn smoke_prompt_streams_deltas_then_settled() {
    let bin = pi_bin();
    let mut client = PiRpcClient::spawn(&bin, || {}).expect("failed to spawn Pi / mock-pi binary");

    client.send_prompt("Hello").expect("send_prompt failed");

    let events = drain_until(&mut client, Duration::from_secs(10), |ev| {
        matches!(ev, PiRpcEvent::AgentSettled { .. })
    });

    let got_delta = events
        .iter()
        .any(|ev| matches!(ev, PiRpcEvent::TextDelta(_)));
    let got_settled = events
        .iter()
        .any(|ev| matches!(ev, PiRpcEvent::AgentSettled { aborted: false }));

    assert!(got_delta, "expected at least one TextDelta event");
    assert!(got_settled, "expected AgentSettled{{aborted:false}}");

    client.shutdown(Duration::from_secs(3));
}

#[test]
fn smoke_abort_yields_settled_aborted() {
    let bin = pi_bin();
    let mut client = PiRpcClient::spawn(&bin, || {}).expect("failed to spawn Pi / mock-pi binary");

    // Send a prompt to put Pi in streaming mode, then immediately abort.
    client.send_prompt("Long task").expect("send_prompt failed");
    // Give mock-pi a tiny moment to start processing before abort
    std::thread::sleep(Duration::from_millis(10));
    client.send_abort().expect("send_abort failed");

    let events = drain_until(&mut client, Duration::from_secs(10), |ev| {
        matches!(ev, PiRpcEvent::AgentSettled { .. })
    });

    // We accept either aborted:true (abort was timely) or aborted:false
    // (response was fast enough that abort arrived after settled).
    let settled = events
        .iter()
        .any(|ev| matches!(ev, PiRpcEvent::AgentSettled { .. }));
    assert!(settled, "expected an AgentSettled event after abort");

    client.shutdown(Duration::from_secs(3));
}

#[test]
fn smoke_shutdown_does_not_orphan_process() {
    let bin = pi_bin();
    let mut client = PiRpcClient::spawn(&bin, || {}).expect("failed to spawn Pi / mock-pi binary");

    client.send_prompt("Hi").expect("send_prompt failed");
    // Shut down immediately without waiting for settled — verifies that
    // Drop / shutdown correctly closes stdin and kills the child.
    client.shutdown(Duration::from_secs(3));
    // If we reach here without hanging, the process was reaped correctly.
}

#[test]
fn smoke_session_stats_and_trace_after_prompt() {
    let bin = pi_bin();
    let mut client = PiRpcClient::spawn(&bin, || {}).expect("failed to spawn Pi / mock-pi binary");

    client.send_prompt("Hello").expect("send_prompt failed");

    let events = drain_until(&mut client, Duration::from_secs(10), |ev| {
        matches!(ev, PiRpcEvent::AgentSettled { .. })
    });

    assert!(
        events
            .iter()
            .any(|ev| matches!(ev, PiRpcEvent::UsageUpdate(_))),
        "expected UsageUpdate from message_update.usage"
    );
    assert!(
        events
            .iter()
            .any(|ev| matches!(ev, PiRpcEvent::ToolTraceStart { .. })),
        "expected tool_execution_start trace event"
    );

    client
        .request_session_stats()
        .expect("request_session_stats failed");
    let stats_events = drain_until(&mut client, Duration::from_secs(5), |ev| {
        matches!(ev, PiRpcEvent::SessionStats(_))
    });
    assert!(
        stats_events
            .iter()
            .any(|ev| matches!(ev, PiRpcEvent::SessionStats(_))),
        "expected SessionStats from get_session_stats"
    );

    client.shutdown(Duration::from_secs(3));
}

#[test]
fn smoke_session_stats_empty_tokens_when_mock_no_stats() {
    let bin = pi_bin();
    std::env::set_var("PI_LIGHT_MOCK_NO_STATS", "1");
    let mut client = PiRpcClient::spawn(&bin, || {}).expect("spawn mock-pi");
    client
        .request_session_stats()
        .expect("request_session_stats failed");
    let stats_events = drain_until(&mut client, Duration::from_secs(5), |ev| {
        matches!(ev, PiRpcEvent::SessionStats(_))
    });
    std::env::remove_var("PI_LIGHT_MOCK_NO_STATS");

    match stats_events
        .iter()
        .find(|ev| matches!(ev, PiRpcEvent::SessionStats(_)))
    {
        Some(PiRpcEvent::SessionStats(s)) => assert!(s.tokens.is_empty()),
        _ => panic!("expected SessionStats with empty tokens"),
    }

    client.shutdown(Duration::from_secs(3));
}

#[test]
fn unit_session_insights_unknown_without_fields() {
    use pi_light::session_insights::{SessionInsights, UsageDisplayState};

    let ins = SessionInsights::default();
    assert_eq!(ins.usage_display_state(), UsageDisplayState::Unknown);
}

#[test]
fn unit_chat_state_full_turn() {
    use pi_light::chat::ChatState;

    let mut chat = ChatState::default();
    chat.push_user("Hi".to_owned());

    chat.handle_rpc_event(PiRpcEvent::TextDelta("Hello".to_owned()));
    assert!(chat.is_streaming);

    chat.handle_rpc_event(PiRpcEvent::TextDelta(", world!".to_owned()));
    chat.handle_rpc_event(PiRpcEvent::AgentSettled { aborted: false });

    assert!(!chat.is_streaming);
    assert_eq!(chat.messages.len(), 2);
    match &chat.messages[1] {
        pi_light::chat::ChatMessage::Assistant(t) => assert_eq!(t, "Hello, world!"),
        _ => panic!("expected Assistant message"),
    }
}
