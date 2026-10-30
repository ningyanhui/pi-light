// SPDX-License-Identifier: Apache-2.0
//
// Chat state machine for the pi-light chat UI (M2′ + 多会话 / 重试).

use crate::pi_rpc::PiRpcEvent;
use crate::session_insights::SessionInsights;

// ── Message types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ChatMessage {
    User(String),
    Assistant(String),
}

// ── Chat state ────────────────────────────────────────────────────────────

/// Tracks the full chat history and the current streaming state.
#[derive(Default)]
pub struct ChatState {
    /// Completed conversation messages (user + assistant, interleaved).
    pub messages: Vec<ChatMessage>,
    /// Text currently in the input field.
    pub input: String,
    /// True while Pi is generating a response.
    pub is_streaming: bool,
    /// Accumulator for text_delta fragments of the current assistant turn.
    pub streaming_buf: String,
    /// Most recent non-recoverable error (spawn failure, process exit, etc.).
    pub error: Option<String>,
    /// Scroll-to-bottom flag: set when new content arrives.
    pub scroll_to_bottom: bool,
    /// M3′: token usage + tool trace derived from Pi RPC.
    pub insights: SessionInsights,
    /// Last prompt actually sent to Pi (includes `#` 引用拼合). Used by retry.
    pub last_sent_prompt: Option<String>,
}

impl ChatState {
    /// Add a finalized user message to the history.
    pub fn push_user(&mut self, text: String) {
        self.messages.push(ChatMessage::User(text));
        self.scroll_to_bottom = true;
    }

    /// Mark that Pi has started generating (clear the streaming buffer).
    pub fn begin_streaming(&mut self) {
        self.is_streaming = true;
        self.streaming_buf.clear();
        self.error = None;
    }

    /// Append a text delta to the current streaming buffer.
    pub fn append_delta(&mut self, delta: &str) {
        self.streaming_buf.push_str(delta);
        self.scroll_to_bottom = true;
    }

    /// Called on `agent_settled`: move the accumulated buffer into the
    /// message list and reset streaming state.
    pub fn finalize_turn(&mut self, aborted: bool) {
        self.is_streaming = false;
        if !self.streaming_buf.is_empty() {
            let text = std::mem::take(&mut self.streaming_buf);
            self.messages.push(ChatMessage::Assistant(text));
        } else if aborted {
            self.messages
                .push(ChatMessage::Assistant("[已中止]".to_owned()));
        }
        self.scroll_to_bottom = true;
    }

    /// Record a fatal error (process exit, spawn failure, etc.).
    pub fn set_error(&mut self, error: String) {
        self.is_streaming = false;
        self.streaming_buf.clear();
        self.error = Some(error);
    }

    /// Retry is available when idle and a previous prompt exists.
    pub fn can_retry(&self) -> bool {
        !self.is_streaming && self.last_sent_prompt.is_some()
    }

    /// Drop a trailing assistant message so retry can regenerate it.
    pub fn drop_trailing_assistant(&mut self) {
        if matches!(self.messages.last(), Some(ChatMessage::Assistant(_))) {
            self.messages.pop();
        }
        self.streaming_buf.clear();
        self.error = None;
    }

    /// Process a single `PiRpcEvent` from the RPC client.
    pub fn handle_rpc_event(&mut self, event: PiRpcEvent) {
        self.insights.handle_rpc_event(&event);
        match event {
            PiRpcEvent::TextDelta(delta) => {
                if !self.is_streaming {
                    self.begin_streaming();
                }
                self.append_delta(&delta);
            }
            PiRpcEvent::AgentSettled { aborted } => {
                self.finalize_turn(aborted);
            }
            PiRpcEvent::ProcessExited { .. } => {
                self.set_error("Pi 进程已退出，请重新探测或重启。".to_owned());
            }
            PiRpcEvent::CommandError { command, error } => {
                self.set_error(format!("命令 `{command}` 失败：{error}"));
            }
            PiRpcEvent::UsageUpdate(_)
            | PiRpcEvent::SessionStats(_)
            | PiRpcEvent::ToolTraceStart { .. }
            | PiRpcEvent::ToolTraceEnd { .. }
            | PiRpcEvent::StateInfo { .. }
            | PiRpcEvent::AvailableModels(_)
            | PiRpcEvent::ModelChanged(_)
            | PiRpcEvent::SessionCommandDone { .. } => {}
        }
    }
}

// ── Multi-session hub ─────────────────────────────────────────────────────

/// One conversation slot in the shell (this-run live session).
///
/// Historical Pi session *files* remain `#` 引用 candidates (`session_ref`);
/// this hub does not re-scan those files.
#[derive(Debug, Clone)]
pub struct ChatSession {
    pub id: u64,
    pub title: String,
    pub session_file: Option<String>,
    pub session_id: Option<String>,
}

impl ChatSession {
    fn new(id: u64) -> Self {
        Self {
            id,
            title: format!("会话 {id}"),
            session_file: None,
            session_id: None,
        }
    }
}

/// 可落盘的会话快照。不含流式中的临时标记；未完成的助手文本会并进 messages。
#[derive(Debug, Clone)]
pub struct ExportedHub {
    pub active: usize,
    pub next_id: u64,
    pub sessions: Vec<ExportedSession>,
}

#[derive(Debug, Clone)]
pub struct ExportedSession {
    pub id: u64,
    pub title: String,
    pub session_file: Option<String>,
    pub session_id: Option<String>,
    pub last_sent_prompt: Option<String>,
    pub input: String,
    pub messages: Vec<ChatMessage>,
}

/// Live session list + per-slot chat state.
pub struct SessionHub {
    sessions: Vec<ChatSession>,
    chats: Vec<ChatState>,
    active: usize,
    next_id: u64,
}

impl Default for SessionHub {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionHub {
    pub fn new() -> Self {
        Self {
            sessions: vec![ChatSession::new(1)],
            chats: vec![ChatState::default()],
            active: 0,
            next_id: 2,
        }
    }

    pub fn list(&self) -> &[ChatSession] {
        &self.sessions
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> &ChatSession {
        &self.sessions[self.active]
    }

    pub fn active_chat(&self) -> &ChatState {
        &self.chats[self.active]
    }

    pub fn active_chat_mut(&mut self) -> &mut ChatState {
        &mut self.chats[self.active]
    }

    /// Copy streaming flag before taking a mutable borrow of `input`.
    pub fn active_is_streaming(&self) -> bool {
        self.chats[self.active].is_streaming
    }

    pub fn any_streaming(&self) -> bool {
        self.chats.iter().any(|c| c.is_streaming)
    }

    /// Create a new empty slot and make it active. Returns its index.
    pub fn create_new(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.sessions.push(ChatSession::new(id));
        self.chats.push(ChatState::default());
        self.active = self.sessions.len() - 1;
        self.active
    }

    /// Switch the active slot. Returns false if `index` is out of range.
    pub fn switch_to(&mut self, index: usize) -> bool {
        if index >= self.sessions.len() {
            return false;
        }
        self.active = index;
        true
    }

    pub fn set_active_session_file(&mut self, file: Option<String>, id: Option<String>) {
        let s = &mut self.sessions[self.active];
        if let Some(f) = file {
            s.session_file = Some(f);
        }
        if let Some(id) = id {
            s.session_id = Some(id);
        }
    }

    pub fn set_active_title_from_user(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        let s = &mut self.sessions[self.active];
        if s.title.starts_with("会话 ") {
            s.title = truncate_title(trimmed, 24);
        }
    }

    /// 清空为本进程内的单个空槽。启动恢复不走这里。
    pub fn reset_all(&mut self) {
        *self = Self::new();
    }

    pub fn to_exported(&self) -> ExportedHub {
        let sessions = self
            .sessions
            .iter()
            .zip(self.chats.iter())
            .map(|(session, chat)| {
                let mut messages = chat.messages.clone();
                if !chat.streaming_buf.is_empty() {
                    messages.push(ChatMessage::Assistant(chat.streaming_buf.clone()));
                }
                ExportedSession {
                    id: session.id,
                    title: session.title.clone(),
                    session_file: session.session_file.clone(),
                    session_id: session.session_id.clone(),
                    last_sent_prompt: chat.last_sent_prompt.clone(),
                    input: chat.input.clone(),
                    messages,
                }
            })
            .collect();
        ExportedHub {
            active: self.active,
            next_id: self.next_id,
            sessions,
        }
    }

    pub fn from_exported(exported: ExportedHub) -> Self {
        if exported.sessions.is_empty() {
            return Self::new();
        }
        let mut max_id = 0u64;
        let mut sessions = Vec::new();
        let mut chats = Vec::new();
        for session in exported.sessions {
            max_id = max_id.max(session.id);
            let title = if session.title.trim().is_empty() {
                format!("会话 {}", session.id)
            } else {
                session.title
            };
            sessions.push(ChatSession {
                id: session.id,
                title,
                session_file: session.session_file,
                session_id: session.session_id,
            });
            chats.push(ChatState {
                messages: session.messages,
                input: session.input,
                last_sent_prompt: session.last_sent_prompt,
                ..ChatState::default()
            });
        }
        let mut next_id = exported.next_id.max(max_id.saturating_add(1));
        if next_id == 0 {
            next_id = 1;
        }
        let active = exported.active.min(sessions.len() - 1);
        Self {
            sessions,
            chats,
            active,
            next_id,
        }
    }
}

fn truncate_title(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_owned()
    } else {
        format!("{}…", chars[..max_chars].iter().collect::<String>())
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_full_turn_cycle() {
        let mut chat = ChatState::default();
        chat.push_user("Hello".to_owned());
        assert_eq!(chat.messages.len(), 1);
        assert!(!chat.is_streaming);

        chat.handle_rpc_event(PiRpcEvent::TextDelta("Hi ".to_owned()));
        assert!(chat.is_streaming);
        chat.handle_rpc_event(PiRpcEvent::TextDelta("there!".to_owned()));
        assert_eq!(chat.streaming_buf, "Hi there!");

        chat.handle_rpc_event(PiRpcEvent::AgentSettled { aborted: false });
        assert!(!chat.is_streaming);
        assert!(chat.streaming_buf.is_empty());
        assert_eq!(chat.messages.len(), 2);
        if let ChatMessage::Assistant(text) = &chat.messages[1] {
            assert_eq!(text, "Hi there!");
        } else {
            panic!("expected Assistant message");
        }
    }

    #[test]
    fn aborted_turn_with_no_text_gets_placeholder() {
        let mut chat = ChatState::default();
        chat.begin_streaming();
        chat.handle_rpc_event(PiRpcEvent::AgentSettled { aborted: true });
        assert_eq!(chat.messages.len(), 1);
        if let ChatMessage::Assistant(text) = &chat.messages[0] {
            assert_eq!(text, "[已中止]");
        } else {
            panic!("expected Assistant placeholder");
        }
    }

    #[test]
    fn process_exit_sets_error() {
        let mut chat = ChatState::default();
        chat.handle_rpc_event(PiRpcEvent::ProcessExited { code: Some(1) });
        assert!(chat.error.is_some());
        assert!(!chat.is_streaming);
    }

    #[test]
    fn command_error_sets_error() {
        let mut chat = ChatState::default();
        chat.handle_rpc_event(PiRpcEvent::CommandError {
            command: "prompt".to_owned(),
            error: "model not configured".to_owned(),
        });
        assert!(chat.error.is_some());
    }

    #[test]
    fn delta_before_begin_streaming_auto_starts() {
        let mut chat = ChatState::default();
        chat.handle_rpc_event(PiRpcEvent::TextDelta("Hello".to_owned()));
        assert!(chat.is_streaming);
        assert_eq!(chat.streaming_buf, "Hello");
    }

    #[test]
    fn retry_drops_trailing_assistant_and_keeps_prompt() {
        let mut chat = ChatState::default();
        chat.push_user("Hi".to_owned());
        chat.last_sent_prompt = Some("Hi".to_owned());
        chat.messages.push(ChatMessage::Assistant("old".to_owned()));
        assert!(chat.can_retry());
        chat.drop_trailing_assistant();
        assert_eq!(chat.messages.len(), 1);
        assert!(matches!(chat.messages[0], ChatMessage::User(_)));
        assert_eq!(chat.last_sent_prompt.as_deref(), Some("Hi"));
    }

    #[test]
    fn hub_create_and_switch_keeps_per_slot_history() {
        let mut hub = SessionHub::new();
        hub.active_chat_mut().push_user("in first".to_owned());
        hub.set_active_title_from_user("in first");
        let idx = hub.create_new();
        assert_eq!(idx, 1);
        assert_eq!(hub.active_index(), 1);
        assert!(hub.active_chat().messages.is_empty());
        hub.active_chat_mut().push_user("in second".to_owned());
        assert!(hub.switch_to(0));
        assert_eq!(hub.active_chat().messages.len(), 1);
        if let ChatMessage::User(t) = &hub.active_chat().messages[0] {
            assert_eq!(t, "in first");
        } else {
            panic!("expected first session user message");
        }
        assert_eq!(hub.active().title, "in first");
        assert_eq!(hub.list().len(), 2);
    }

    #[test]
    fn export_keeps_streaming_partial_as_message_on_import() {
        let mut hub = SessionHub::new();
        hub.active_chat_mut().push_user("hi".to_owned());
        hub.active_chat_mut().begin_streaming();
        hub.active_chat_mut().append_delta("partial");
        hub.set_active_session_file(Some("s.jsonl".to_owned()), Some("id".to_owned()));
        hub.active_chat_mut().input = "draft".to_owned();
        let loaded = SessionHub::from_exported(hub.to_exported());
        assert_eq!(loaded.list().len(), 1);
        assert_eq!(loaded.active().session_file.as_deref(), Some("s.jsonl"));
        assert_eq!(loaded.active_chat().messages.len(), 2);
        assert!(!loaded.active_chat().is_streaming);
        assert_eq!(loaded.active_chat().input, "draft");
        if let ChatMessage::Assistant(text) = &loaded.active_chat().messages[1] {
            assert_eq!(text, "partial");
        } else {
            panic!("expected partial assistant text");
        }
    }

    #[test]
    fn hub_reset_all_returns_single_empty_slot() {
        let mut hub = SessionHub::new();
        hub.create_new();
        hub.create_new();
        hub.reset_all();
        assert_eq!(hub.list().len(), 1);
        assert!(hub.active_chat().messages.is_empty());
        assert_eq!(hub.active_index(), 0);
    }
}
