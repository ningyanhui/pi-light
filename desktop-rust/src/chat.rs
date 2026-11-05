// SPDX-License-Identifier: Apache-2.0
//
// Chat state machine for the pi-light chat UI (M2′ + 多会话 / 重试).

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::message_tree::{MessageTree, NodeId, PathRow, TreeRow, TreeSnapshot};
use crate::pi_rpc::PiRpcEvent;
use crate::session_insights::SessionInsights;

// ── Message types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatMessage {
    User(String),
    Assistant(String),
}

// ── Chat state ────────────────────────────────────────────────────────────

/// Tracks the full chat history and the current streaming state.
#[derive(Default, Clone)]
pub struct ChatState {
    /// 同会话消息树（含未展示的兄弟分支）。
    pub tree: MessageTree,
    /// 当前活动路径上的消息（由树派生，给引用 / 远程快照复用）。
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
    /// 活动路径已偏离 Pi 当前线性上下文，下次发送需要 new_session 回放。
    pub pi_stale: bool,
    /// 已写入会话记忆的 trace 步数。
    pub memory_mark: usize,
    /// 本轮已结束、待写入会话记忆。
    pub memory_pending: bool,
}

impl ChatState {
    fn sync_visible(&mut self) {
        self.messages = self.tree.active_messages();
        self.scroll_to_bottom = true;
    }

    pub fn path_rows(&self) -> Vec<PathRow> {
        self.tree.active_path_rows()
    }

    pub fn tree_rows(&self) -> Vec<TreeRow> {
        self.tree.tree_rows()
    }

    /// Add a finalized user message to the history.
    pub fn push_user(&mut self, text: String) {
        self.tree.push_user(text);
        self.sync_visible();
    }

    pub fn push_assistant(&mut self, text: String) {
        self.tree.push_assistant(text);
        self.sync_visible();
    }

    /// Mark that Pi has started generating (clear the streaming buffer).
    pub fn begin_streaming(&mut self) {
        self.is_streaming = true;
        self.streaming_buf.clear();
        self.error = None;
        self.insights.on_turn_start();
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
            self.tree.push_assistant(text);
            self.sync_visible();
        } else if aborted {
            self.tree.push_assistant("[已中止]".to_owned());
            self.sync_visible();
        }
        self.insights.on_turn_end();
        self.scroll_to_bottom = true;
    }

    /// Record a fatal error (process exit, spawn failure, etc.).
    pub fn set_error(&mut self, error: String) {
        self.is_streaming = false;
        self.streaming_buf.clear();
        self.insights.on_turn_end();
        self.error = Some(error);
    }

    /// Retry is available when idle and a previous prompt exists.
    pub fn can_retry(&self) -> bool {
        !self.is_streaming && self.last_sent_prompt.is_some()
    }

    /// Drop a trailing assistant message so retry can regenerate it.
    pub fn drop_trailing_assistant(&mut self) {
        if let Some(id) = self.tree.last_assistant_on_path() {
            self.tree.prepare_regen(id);
            self.sync_visible();
        }
        self.streaming_buf.clear();
        self.error = None;
    }

    /// 从助手消息重新生成：隐藏该分支后续，返回 (祖先前缀, 用户原文)。
    pub fn prepare_regenerate(&mut self, assistant: NodeId) -> Option<(Vec<ChatMessage>, String)> {
        let user_id = self.tree.prepare_regen(assistant)?;
        let user_text = self.tree.text(user_id)?.to_owned();
        let prefix = self.tree.ancestor_messages(user_id);
        self.sync_visible();
        self.streaming_buf.clear();
        self.error = None;
        self.pi_stale = true;
        Some((prefix, user_text))
    }

    /// 编辑用户消息并分出兄弟分支。返回该用户之前的前缀。
    pub fn prepare_user_branch(
        &mut self,
        user: NodeId,
        new_text: String,
    ) -> Option<Vec<ChatMessage>> {
        let prefix = self.tree.ancestor_messages(user);
        self.tree.branch_user(user, new_text)?;
        self.sync_visible();
        self.streaming_buf.clear();
        self.error = None;
        self.pi_stale = true;
        Some(prefix)
    }

    pub fn select_sibling(&mut self, id: NodeId, delta: i32) -> bool {
        if self.tree.select_sibling(id, delta).is_some() {
            self.sync_visible();
            self.pi_stale = true;
            true
        } else {
            false
        }
    }

    pub fn navigate_to(&mut self, id: NodeId) -> bool {
        if self.tree.activate(id) {
            self.sync_visible();
            self.pi_stale = true;
            true
        } else {
            false
        }
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
                if matches!(
                    command.as_str(),
                    "set_model"
                        | "get_available_models"
                        | "get_state"
                        | "set_thinking_level"
                        | "get_available_thinking_levels"
                        | "get_commands"
                        | "clone"
                        | "fork"
                        | "set_session_name"
                ) {
                    return;
                }
                self.set_error(format!("命令 `{command}` 失败：{error}"));
            }
            PiRpcEvent::UsageUpdate(_)
            | PiRpcEvent::SessionStats(_)
            | PiRpcEvent::ToolTraceStart { .. }
            | PiRpcEvent::ToolTraceEnd { .. }
            | PiRpcEvent::StateInfo { .. }
            | PiRpcEvent::AvailableModels(_)
            | PiRpcEvent::ModelChanged(_)
            | PiRpcEvent::SessionCommandDone { .. }
            | PiRpcEvent::ThinkingLevels(_)
            | PiRpcEvent::ThinkingLevelChanged(_)
            | PiRpcEvent::AvailableCommands(_)
            | PiRpcEvent::SessionCloned { .. } => {}
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
    pub workdir: Option<PathBuf>,
    pub updated_at: u64,
}

impl ChatSession {
    fn new(id: u64) -> Self {
        Self {
            id,
            title: format!("会话 {id}"),
            session_file: None,
            session_id: None,
            workdir: None,
            updated_at: now_secs(),
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
    pub tree: TreeSnapshot,
    pub pi_stale: bool,
    pub workdir: Option<PathBuf>,
    pub updated_at: u64,
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

    pub fn chat_at(&self, index: usize) -> Option<&ChatState> {
        self.chats.get(index)
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
        let inherited = self.active().workdir.clone();
        let id = self.next_id;
        self.next_id += 1;
        let mut session = ChatSession::new(id);
        session.workdir = inherited;
        self.sessions.push(session);
        self.chats.push(ChatState::default());
        self.active = self.sessions.len() - 1;
        self.active
    }

    pub fn set_active_workdir(&mut self, path: PathBuf) {
        self.sessions[self.active].workdir = Some(path);
        self.touch_active();
    }

    pub fn fill_missing_workdirs(&mut self, fallback: &Path) {
        for session in &mut self.sessions {
            if session.workdir.is_none() {
                session.workdir = Some(fallback.to_path_buf());
            }
        }
    }

    pub fn touch_active(&mut self) {
        self.sessions[self.active].updated_at = now_secs();
    }

    /// 按更新时间倒序的原始下标（当前会话并列时仍可稳定点选）。
    pub fn indices_by_updated(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.sessions.len()).collect();
        indices.sort_by(|a, b| {
            self.sessions[*b]
                .updated_at
                .cmp(&self.sessions[*a].updated_at)
                .then_with(|| a.cmp(b))
        });
        indices
    }

    /// Switch the active slot. Returns false if `index` is out of range.
    pub fn switch_to(&mut self, index: usize) -> bool {
        if index >= self.sessions.len() {
            return false;
        }
        self.active = index;
        self.touch_active();
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

    pub fn rename(&mut self, index: usize, title: &str) -> bool {
        let title = title.trim();
        if title.is_empty() || index >= self.sessions.len() {
            return false;
        }
        self.sessions[index].title = truncate_title(title, 40);
        self.sessions[index].updated_at = now_secs();
        true
    }

    /// 壳侧删除。删到空则新建空槽；删当前则落到邻项。
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.sessions.len() {
            return false;
        }
        if self.sessions.len() == 1 {
            *self = Self::new();
            return true;
        }
        self.sessions.remove(index);
        self.chats.remove(index);
        if self.active == index {
            self.active = index.min(self.sessions.len() - 1).saturating_sub(0);
            if self.active >= self.sessions.len() {
                self.active = self.sessions.len() - 1;
            }
        } else if self.active > index {
            self.active -= 1;
        }
        true
    }

    /// 复制槽为新会话。不复制 Pi session_file（除非随后 RPC clone 回填）。
    pub fn fork_slot(&mut self, index: usize) -> Option<usize> {
        if index >= self.sessions.len() {
            return None;
        }
        let mut session = self.sessions[index].clone();
        let mut chat = self.chats[index].clone();
        let id = self.next_id;
        self.next_id += 1;
        session.id = id;
        session.title = format!("{} 副本", truncate_title(&session.title, 20));
        session.session_file = None;
        session.session_id = None;
        session.updated_at = now_secs();
        chat.is_streaming = false;
        chat.streaming_buf.clear();
        chat.error = None;
        chat.pi_stale = true;
        self.sessions.push(session);
        self.chats.push(chat);
        self.active = self.sessions.len() - 1;
        Some(self.active)
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
                let mut tree = chat.tree.clone();
                if !chat.streaming_buf.is_empty() {
                    tree.push_assistant(chat.streaming_buf.clone());
                }
                let messages = tree.active_messages();
                ExportedSession {
                    id: session.id,
                    title: session.title.clone(),
                    session_file: session.session_file.clone(),
                    session_id: session.session_id.clone(),
                    last_sent_prompt: chat.last_sent_prompt.clone(),
                    input: chat.input.clone(),
                    messages,
                    tree: tree.to_snapshot(),
                    pi_stale: chat.pi_stale || chat.tree.has_branches(),
                    workdir: session.workdir.clone(),
                    updated_at: session.updated_at,
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
                workdir: session.workdir,
                updated_at: if session.updated_at == 0 {
                    now_secs()
                } else {
                    session.updated_at
                },
            });
            let tree = if session.tree.nodes.is_empty() {
                MessageTree::from_linear(&session.messages)
            } else {
                MessageTree::from_snapshot(session.tree)
            };
            let pi_stale = session.pi_stale || tree.has_branches();
            chats.push(ChatState {
                messages: tree.active_messages(),
                tree,
                input: session.input,
                last_sent_prompt: session.last_sent_prompt,
                pi_stale,
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

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
        chat.push_assistant("old".to_owned());
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
    fn new_session_inherits_workdir_and_sorts_by_updated() {
        let mut hub = SessionHub::new();
        hub.set_active_workdir(PathBuf::from("/tmp/a"));
        hub.active_chat_mut().push_user("old".to_owned());
        std::thread::sleep(std::time::Duration::from_millis(15));
        hub.create_new();
        assert_eq!(hub.active().workdir.as_deref(), Some(Path::new("/tmp/a")));
        hub.set_active_workdir(PathBuf::from("/tmp/b"));
        let order = hub.indices_by_updated();
        assert_eq!(order.first().copied(), Some(hub.active_index()));
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

    #[test]
    fn regenerate_keeps_old_assistant_as_sibling() {
        let mut chat = ChatState::default();
        chat.push_user("q".to_owned());
        chat.push_assistant("old".to_owned());
        let assistant = chat.tree.last_assistant_on_path().unwrap();
        let (prefix, user) = chat.prepare_regenerate(assistant).unwrap();
        assert!(prefix.is_empty());
        assert_eq!(user, "q");
        assert_eq!(chat.messages.len(), 1);
        assert!(chat.pi_stale);
        chat.push_assistant("new".to_owned());
        let rows = chat.path_rows();
        assert_eq!(rows[1].sibling_count, 2);
        assert_eq!(rows[1].text, "new");
        chat.select_sibling(rows[1].id, -1);
        assert_eq!(
            chat.messages.last().map(|m| match m {
                ChatMessage::Assistant(t) => t.as_str(),
                ChatMessage::User(_) => "",
            }),
            Some("old")
        );
    }

    #[test]
    fn export_roundtrip_keeps_inactive_branch() {
        let mut hub = SessionHub::new();
        hub.active_chat_mut().push_user("u".to_owned());
        hub.active_chat_mut().push_assistant("a".to_owned());
        let user = hub.active_chat().tree.last_user_on_path().unwrap();
        hub.active_chat_mut()
            .prepare_user_branch(user, "u2".to_owned());
        hub.active_chat_mut().push_assistant("a2".to_owned());
        let mut loaded = SessionHub::from_exported(hub.to_exported());
        assert!(loaded.active_chat().tree.has_branches());
        assert_eq!(loaded.active_chat().messages.len(), 2);
        let first = loaded
            .active_chat()
            .tree_rows()
            .into_iter()
            .find(|r| r.text == "a")
            .unwrap();
        loaded.active_chat_mut().navigate_to(first.id);
        assert_eq!(
            loaded.active_chat().messages,
            vec![
                ChatMessage::User("u".to_owned()),
                ChatMessage::Assistant("a".to_owned())
            ]
        );
    }

    #[test]
    fn hub_rename_and_fork_and_remove() {
        let mut hub = SessionHub::new();
        hub.active_chat_mut().push_user("keep".to_owned());
        assert!(hub.rename(0, "工作区改造"));
        assert_eq!(hub.active().title, "工作区改造");
        let forked = hub.fork_slot(0).unwrap();
        assert_eq!(forked, 1);
        assert_eq!(hub.active().title, "工作区改造 副本");
        assert!(hub.active().session_file.is_none());
        assert!(hub.active_chat().pi_stale);
        assert_eq!(hub.active_chat().messages.len(), 1);
        assert!(hub.remove(1));
        assert_eq!(hub.list().len(), 1);
        assert_eq!(hub.active_index(), 0);
        assert!(hub.remove(0));
        assert_eq!(hub.list().len(), 1);
        assert!(hub.active_chat().messages.is_empty());
    }
}
