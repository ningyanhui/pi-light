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

use pi_light::{
    chat::ChatMessage,
    pi_rpc::{PiRpcClient, PiRpcEvent},
    session_ref,
};
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
    let mut client =
        PiRpcClient::spawn(&bin, None, || {}).expect("failed to spawn Pi / mock-pi binary");

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
    let mut client =
        PiRpcClient::spawn(&bin, None, || {}).expect("failed to spawn Pi / mock-pi binary");

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
    let mut client =
        PiRpcClient::spawn(&bin, None, || {}).expect("failed to spawn Pi / mock-pi binary");

    client.send_prompt("Hi").expect("send_prompt failed");
    // Shut down immediately without waiting for settled — verifies that
    // Drop / shutdown correctly closes stdin and kills the child.
    client.shutdown(Duration::from_secs(3));
    // If we reach here without hanging, the process was reaped correctly.
}

#[test]
fn smoke_session_stats_and_trace_after_prompt() {
    let bin = pi_bin();
    let mut client =
        PiRpcClient::spawn(&bin, None, || {}).expect("failed to spawn Pi / mock-pi binary");

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
    let mut client = PiRpcClient::spawn(&bin, None, || {}).expect("spawn mock-pi");
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

// ── M5′ # 会话引用集成测试 ────────────────────────────────────────────────

/// 有候选时，选择器过滤出的结果数量应与预期一致。
#[test]
fn unit_ref_chooser_filters_messages() {
    let messages = vec![
        ChatMessage::User("如何使用 pi-light？".to_owned()),
        ChatMessage::Assistant("pi-light 是一个轻量 Rust Shell。".to_owned()),
        ChatMessage::User("谢谢".to_owned()),
    ];

    let all = session_ref::candidates_from_messages(&messages);
    assert_eq!(all.len(), 3);

    let mut chooser = session_ref::RefChooserState::default();
    // 空 query → 全部候选
    chooser.activate(&all, "", 0);
    assert!(chooser.active);
    assert_eq!(chooser.filtered.len(), 3);

    // 过滤「用户」→ 2 条
    chooser.activate(&all, "用户", 0);
    assert_eq!(chooser.filtered.len(), 2);

    // 过滤「助手」→ 1 条
    chooser.activate(&all, "助手", 0);
    assert_eq!(chooser.filtered.len(), 1);
}

/// 空会话时选择器应呈现空态（filtered 为空），不 panic。
#[test]
fn unit_ref_chooser_empty_state_no_panic() {
    let mut chooser = session_ref::RefChooserState::default();
    chooser.activate(&[], "", 0);
    assert!(chooser.active);
    assert!(chooser.filtered.is_empty());
    // 确认空态不 panic
    assert!(chooser.confirm().is_none());
    assert!(!chooser.active);
}

/// 确认引用后，chip 内容应正确注入 prompt 文本。
#[test]
fn unit_ref_confirm_and_build_prompt() {
    let messages = vec![ChatMessage::User("第一条消息".to_owned())];
    let all = session_ref::candidates_from_messages(&messages);

    let mut chooser = session_ref::RefChooserState::default();
    chooser.activate(&all, "", 5); // hash_pos = 5

    let chip = chooser.confirm().expect("confirm should return chip");
    assert!(chip.content.contains("[引用消息#1"));
    assert!(chip.content.contains("第一条消息"));

    // 构建 prompt 文本
    let effective = session_ref::build_prompt_text(&[chip], "请解释一下");
    assert!(effective.contains("[引用消息#1"));
    assert!(effective.ends_with("请解释一下"));
}

/// 通过 mock-pi 发送带引用上下文的 prompt，应收到 AgentSettled。
#[test]
fn smoke_prompt_with_reference_context() {
    let messages = vec![
        ChatMessage::User("用户的第一条消息".to_owned()),
        ChatMessage::Assistant("助手的第一条回复".to_owned()),
    ];
    let all = session_ref::candidates_from_messages(&messages);
    assert!(!all.is_empty(), "应有候选");

    // 构建引用 chip
    let mut chooser = session_ref::RefChooserState::default();
    chooser.activate(&all, "", 0);
    let chip = chooser.confirm().expect("confirm should return chip");

    // 构建有引用的 prompt
    let effective = session_ref::build_prompt_text(&[chip], "能详细说明吗？");
    assert!(effective.contains("[引用消息"), "prompt 应含引用上下文块");

    // 通过 mock-pi 发送
    let bin = pi_bin();
    let mut client =
        PiRpcClient::spawn(&bin, None, || {}).expect("failed to spawn Pi / mock-pi binary");
    client.send_prompt(&effective).expect("send_prompt failed");

    let events = drain_until(&mut client, Duration::from_secs(10), |ev| {
        matches!(ev, PiRpcEvent::AgentSettled { .. })
    });

    let got_settled = events
        .iter()
        .any(|ev| matches!(ev, PiRpcEvent::AgentSettled { aborted: false }));
    assert!(
        got_settled,
        "期望 AgentSettled{{aborted:false}}，实际事件：{events:?}"
    );

    client.shutdown(Duration::from_secs(3));
}

/// find_hash_trigger + remove_hash_trigger 的端到端验证。
#[test]
fn unit_hash_trigger_roundtrip() {
    let input = "请引用这条消息 #用户".to_owned();
    let pos = session_ref::find_hash_trigger(&input).expect("应找到 # 触发位置");
    let query = session_ref::extract_query(&input, pos);
    assert_eq!(query, "用户");

    let mut cleaned = input.clone();
    session_ref::remove_hash_trigger(&mut cleaned, pos);
    assert_eq!(cleaned, "请引用这条消息 ");
    // 清理后不再触发
    assert!(session_ref::find_hash_trigger(&cleaned).is_none());
}

/// # 后紧跟空格不应触发选择器（普通 Markdown 用法）。
#[test]
fn unit_hash_space_not_a_trigger() {
    assert!(session_ref::find_hash_trigger("# 标题").is_none());
    assert!(session_ref::find_hash_trigger("test # comment").is_none());
    // 末尾 # 无后续字符 → 允许触发（刚按下 #）
    assert!(session_ref::find_hash_trigger("test #").is_some());
}

#[test]
fn smoke_spawn_with_cwd() {
    let bin = pi_bin();
    let dir = tempfile::tempdir().expect("tempdir");
    let mut client = PiRpcClient::spawn(&bin, Some(dir.path()), || {})
        .expect("spawn with existing cwd should succeed");
    client.send_prompt("cwd").expect("prompt");
    let events = drain_until(&mut client, Duration::from_secs(10), |ev| {
        matches!(ev, PiRpcEvent::AgentSettled { .. })
    });
    assert!(events
        .iter()
        .any(|ev| matches!(ev, PiRpcEvent::AgentSettled { .. })));
    client.shutdown(Duration::from_secs(3));
}

#[test]
fn smoke_spawn_rejects_missing_cwd() {
    let bin = pi_bin();
    let missing = std::env::temp_dir().join("pi-light-missing-cwd-does-not-exist");
    let result = PiRpcClient::spawn(&bin, Some(&missing), || {});
    assert!(result.is_err(), "missing cwd should fail at spawn");
}

#[test]
fn smoke_new_session_command() {
    let bin = pi_bin();
    let mut client = PiRpcClient::spawn(&bin, None, || {}).expect("spawn");
    client.send_new_session().expect("new_session");
    let events = drain_until(&mut client, Duration::from_secs(5), |ev| {
        matches!(ev, PiRpcEvent::SessionCommandDone { .. })
    });
    assert!(
        events.iter().any(|ev| matches!(
            ev,
            PiRpcEvent::SessionCommandDone {
                command,
                cancelled: false
            } if command == "new_session"
        )),
        "expected new_session SessionCommandDone, got {events:?}"
    );
    client.shutdown(Duration::from_secs(3));
}
