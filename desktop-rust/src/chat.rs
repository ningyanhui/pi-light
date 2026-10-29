// SPDX-License-Identifier: Apache-2.0
//
// Chat state machine for the pi-light chat UI (M2′).

use crate::pi_rpc::PiRpcEvent;

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

    /// Process a single `PiRpcEvent` from the RPC client.
    pub fn handle_rpc_event(&mut self, event: PiRpcEvent) {
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
        }
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

        // Simulate Pi streaming
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
}
