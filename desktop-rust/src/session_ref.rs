// SPDX-License-Identifier: Apache-2.0
//
// M5′ `#` 会话引用 — 状态机、候选数据源与 prompt 拼合。
//
// 候选优先级（诚实策略）：
//   1. 当前会话历史消息（始终可用，最多取最近 20 条）
//   2. Pi 本地会话文件（~/.pi/sessions/*.json，可选，无则静默跳过）
// 无候选时展示"暂无引用"空态，不伪造数据。

use crate::chat::ChatMessage;

// ── 引用候选 ─────────────────────────────────────────────────────────────

/// 可引用对象的分类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefKind {
    /// 当前会话中的历史消息。
    CurrentMessage { index: usize },
    /// 从 Pi 本地会话文件读取的历史会话。
    SessionFile { path: String },
}

/// 一个可被 `#` 引用的候选对象。
#[derive(Debug, Clone)]
pub struct RefCandidate {
    pub id: String,
    /// picker 中展示的单行文本。
    pub display: String,
    /// 注入 prompt 的内容（可能多行）。
    pub preview: String,
    pub kind: RefKind,
}

impl RefCandidate {
    /// 创建来自当前会话消息的候选。
    pub fn from_message(index: usize, msg: &ChatMessage) -> Self {
        let (role, text) = match msg {
            ChatMessage::User(t) => ("用户", t.as_str()),
            ChatMessage::Assistant(t) => ("助手", t.as_str()),
        };
        let preview_text = if text.len() > 300 {
            format!("{}…", &text[..300])
        } else {
            text.to_owned()
        };
        let display_snippet = if text.len() > 60 {
            format!("{}…", &text[..60])
        } else {
            text.to_owned()
        };
        Self {
            id: format!("msg:{index}"),
            display: format!("[{}] {}", role, display_snippet),
            preview: format!("[引用消息#{} ({})]\n{}", index + 1, role, preview_text),
            kind: RefKind::CurrentMessage { index },
        }
    }

    /// query 为空则全部匹配；否则 case-insensitive 子串匹配 display。
    pub fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        self.display.to_lowercase().contains(&query.to_lowercase())
    }
}

// ── 已确认的引用 chip ────────────────────────────────────────────────────

/// 用户已确认选择的引用，以 chip 形式显示在输入框上方。
#[derive(Debug, Clone)]
pub struct RefChip {
    pub id: String,
    pub display: String,
    /// 注入 prompt 的内容。
    pub content: String,
}

// ── 选择器状态机 ──────────────────────────────────────────────────────────

/// `#` 会话引用选择器的完整状态。
#[derive(Default)]
pub struct RefChooserState {
    /// 选择器当前是否展开。
    pub active: bool,
    /// 候选列表（已按 query 过滤）。
    pub filtered: Vec<RefCandidate>,
    /// 当前光标行（高亮项）。
    pub cursor: usize,
    /// 输入框内触发 `#` 的字节偏移量（确认时用于删除触发词）。
    pub hash_byte_offset: usize,
}

impl RefChooserState {
    /// 激活或刷新选择器：传入全量候选与当前 query 及 `#` 位置。
    pub fn activate(&mut self, all: &[RefCandidate], query: &str, hash_pos: usize) {
        self.active = true;
        self.hash_byte_offset = hash_pos;
        self.filtered = all.iter().filter(|c| c.matches(query)).cloned().collect();
        // 保持光标合法
        if self.cursor >= self.filtered.len() {
            self.cursor = 0;
        }
    }

    /// 关闭选择器并重置状态。
    pub fn close(&mut self) {
        self.active = false;
        self.filtered.clear();
        self.cursor = 0;
    }

    /// 向上移动光标。
    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// 向下移动光标。
    pub fn move_down(&mut self) {
        if !self.filtered.is_empty() && self.cursor + 1 < self.filtered.len() {
            self.cursor += 1;
        }
    }

    /// 确认当前选中项，返回 `RefChip`；若无候选则返回 None。
    /// 无论是否有候选，调用后均关闭选择器（Enter/Tab 在空态时起关闭作用）。
    pub fn confirm(&mut self) -> Option<RefChip> {
        if self.filtered.is_empty() {
            self.close();
            return None;
        }
        let candidate = self.filtered.get(self.cursor)?.clone();
        self.close();
        Some(RefChip {
            id: candidate.id,
            display: candidate.display,
            content: candidate.preview,
        })
    }
}

// ── 候选构建 ──────────────────────────────────────────────────────────────

/// 从当前会话消息构建候选列表（最近的在前，最多取 20 条）。
pub fn candidates_from_messages(messages: &[ChatMessage]) -> Vec<RefCandidate> {
    messages
        .iter()
        .enumerate()
        .rev()
        .take(20)
        .map(|(i, msg)| RefCandidate::from_message(i, msg))
        .collect()
}

/// 尝试从 Pi 本地会话目录读取历史会话摘要。
///
/// Pi v1.4.0 无官方会话列表 RPC，此处以本地文件发现为 fallback。
/// 读取失败（目录不存在 / 格式不符）则静默返回空列表（诚实空态）。
pub fn candidates_from_pi_sessions() -> Vec<RefCandidate> {
    let base = match home_dir() {
        Some(d) => d,
        None => return Vec::new(),
    };

    // Pi 会话目录候选路径（Pi 未公开精确规范，按惯例探测两个常见位置）
    let dirs = [
        base.join(".pi").join("sessions"),
        base.join(".pi").join("agent").join("sessions"),
    ];

    let mut candidates = Vec::new();
    for dir in &dirs {
        if !dir.exists() {
            continue;
        }
        let read_dir = match std::fs::read_dir(dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let raw = match std::fs::read_to_string(&path) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let val: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // 取 title / summary / sessionId 作为显示名（Pi 格式未公开，尽力解析）
            let title = val
                .get("title")
                .or_else(|| val.get("summary"))
                .or_else(|| val.get("sessionId"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("（无标题）")
                .to_owned();

            let preview_content = val
                .get("content")
                .or_else(|| val.get("summary"))
                .and_then(serde_json::Value::as_str)
                .map(|s| {
                    if s.len() > 300 {
                        format!("{}…", &s[..300])
                    } else {
                        s.to_owned()
                    }
                })
                .unwrap_or_else(|| title.clone());

            let file_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_owned();

            candidates.push(RefCandidate {
                id: format!("session:{file_stem}"),
                display: format!("[历史会话] {title}"),
                preview: format!("[引用历史会话: {title}]\n{preview_content}"),
                kind: RefKind::SessionFile {
                    path: path.to_string_lossy().to_string(),
                },
            });

            if candidates.len() >= 10 {
                break;
            }
        }
        if !candidates.is_empty() {
            break; // 找到一个有效目录即停
        }
    }
    candidates
}

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from)
}

// ── 触发词解析 ────────────────────────────────────────────────────────────

/// 查找输入字符串中触发引用选择器的 `#` 位置（字节偏移）。
///
/// 策略：取最后一个 `#`，且其后不以空格或换行开头。
/// "# " 视为普通排版用法，不触发选择器。
pub fn find_hash_trigger(input: &str) -> Option<usize> {
    let pos = input.rfind('#')?;
    let after = &input[pos + 1..];
    if after.starts_with(' ') || after.starts_with('\n') || after.starts_with('\r') {
        return None;
    }
    Some(pos)
}

/// 从输入中提取 `#` 后的 query 文本（hash_pos 是 `#` 的字节偏移）。
pub fn extract_query(input: &str, hash_pos: usize) -> &str {
    &input[hash_pos + 1..]
}

/// 将输入中 `#query` 部分删除（确认引用后清理触发词）。
pub fn remove_hash_trigger(input: &mut String, hash_pos: usize) {
    *input = input[..hash_pos].to_owned();
}

// ── Prompt 拼合 ───────────────────────────────────────────────────────────

/// 将 chips 内容与用户输入拼合为最终发送给 Pi 的 prompt 文本。
///
/// 若无 chips，直接返回 user_text；否则在 user_text 前插入引用上下文块。
pub fn build_prompt_text(chips: &[RefChip], user_text: &str) -> String {
    if chips.is_empty() {
        return user_text.to_owned();
    }
    let mut parts: Vec<String> = chips.iter().map(|c| c.content.clone()).collect();
    parts.push(user_text.to_owned());
    parts.join("\n\n")
}

// ── 单元测试 ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_messages() -> Vec<ChatMessage> {
        vec![
            ChatMessage::User("第一条用户消息".to_owned()),
            ChatMessage::Assistant(
                "第一条助手回复，内容很长很长很长很长很长很长很长很长很长很长".to_owned(),
            ),
            ChatMessage::User("第二条用户消息".to_owned()),
        ]
    }

    // ── RefCandidate ──────────────────────────────────────────────────────

    #[test]
    fn candidate_from_message_user_fields() {
        let msg = ChatMessage::User("Hello world".to_owned());
        let c = RefCandidate::from_message(0, &msg);
        assert_eq!(c.id, "msg:0");
        assert!(c.display.contains("用户"));
        assert!(c.display.contains("Hello world"));
        assert!(c.preview.contains("[引用消息#1 (用户)]"));
        assert!(c.preview.contains("Hello world"));
        assert!(matches!(c.kind, RefKind::CurrentMessage { index: 0 }));
    }

    #[test]
    fn candidate_from_message_assistant_fields() {
        let msg = ChatMessage::Assistant("Reply text".to_owned());
        let c = RefCandidate::from_message(2, &msg);
        assert_eq!(c.id, "msg:2");
        assert!(c.display.contains("助手"));
        assert!(c.preview.contains("[引用消息#3 (助手)]"));
    }

    #[test]
    fn candidate_matches_empty_query() {
        let c = RefCandidate::from_message(0, &ChatMessage::User("Hi".to_owned()));
        assert!(c.matches(""));
    }

    #[test]
    fn candidate_matches_case_insensitive() {
        let c = RefCandidate::from_message(0, &ChatMessage::User("Hello".to_owned()));
        assert!(c.matches("hello"));
        assert!(c.matches("HELLO"));
        assert!(!c.matches("xyz123"));
    }

    // ── candidates_from_messages ──────────────────────────────────────────

    #[test]
    fn candidates_from_messages_reversed_most_recent_first() {
        let msgs = make_messages();
        let cands = candidates_from_messages(&msgs);
        // 最近的（index 2）应在前
        assert_eq!(cands.len(), 3);
        assert_eq!(cands[0].id, "msg:2");
        assert_eq!(cands[1].id, "msg:1");
        assert_eq!(cands[2].id, "msg:0");
    }

    #[test]
    fn candidates_from_messages_empty_gives_empty() {
        assert!(candidates_from_messages(&[]).is_empty());
    }

    #[test]
    fn candidates_from_messages_max_20() {
        let msgs: Vec<ChatMessage> = (0..30)
            .map(|i| ChatMessage::User(format!("msg {i}")))
            .collect();
        let cands = candidates_from_messages(&msgs);
        assert_eq!(cands.len(), 20);
    }

    // ── RefChooserState ───────────────────────────────────────────────────

    #[test]
    fn chooser_activate_empty_query_all_candidates() {
        let msgs = make_messages();
        let all = candidates_from_messages(&msgs);
        let mut chooser = RefChooserState::default();

        chooser.activate(&all, "", 0);
        assert!(chooser.active);
        assert_eq!(chooser.filtered.len(), 3);
        assert_eq!(chooser.cursor, 0);
    }

    #[test]
    fn chooser_activate_filtered_query() {
        let msgs = make_messages();
        let all = candidates_from_messages(&msgs);
        let mut chooser = RefChooserState::default();

        chooser.activate(&all, "用户", 0);
        assert_eq!(chooser.filtered.len(), 2); // 两条用户消息
    }

    #[test]
    fn chooser_navigate_up_down() {
        let msgs = make_messages();
        let all = candidates_from_messages(&msgs);
        let mut chooser = RefChooserState::default();
        chooser.activate(&all, "", 0);

        assert_eq!(chooser.cursor, 0);
        chooser.move_down();
        assert_eq!(chooser.cursor, 1);
        chooser.move_down();
        assert_eq!(chooser.cursor, 2);
        // 不越界
        chooser.move_down();
        assert_eq!(chooser.cursor, 2);
        chooser.move_up();
        assert_eq!(chooser.cursor, 1);
        chooser.move_up();
        chooser.move_up();
        assert_eq!(chooser.cursor, 0);
    }

    #[test]
    fn chooser_confirm_returns_chip_and_closes() {
        let msgs = make_messages();
        let all = candidates_from_messages(&msgs);
        let mut chooser = RefChooserState::default();
        chooser.activate(&all, "", 5);

        let chip = chooser.confirm().expect("should return chip");
        assert!(!chip.id.is_empty());
        assert!(!chip.display.is_empty());
        assert!(!chip.content.is_empty());
        // 确认后关闭
        assert!(!chooser.active);
        assert!(chooser.filtered.is_empty());
    }

    #[test]
    fn chooser_confirm_empty_candidates_returns_none_and_closes() {
        let mut chooser = RefChooserState::default();
        chooser.activate(&[], "", 0);
        assert!(chooser.active);
        // 空态下 confirm() 返回 None 并关闭选择器（Enter 在空态起关闭作用）
        assert!(chooser.confirm().is_none());
        assert!(!chooser.active);
    }

    #[test]
    fn chooser_close_resets_state() {
        let msgs = make_messages();
        let all = candidates_from_messages(&msgs);
        let mut chooser = RefChooserState::default();
        chooser.activate(&all, "", 0);
        chooser.move_down();
        chooser.close();
        assert!(!chooser.active);
        assert_eq!(chooser.cursor, 0);
        assert!(chooser.filtered.is_empty());
    }

    // ── 触发词解析 ────────────────────────────────────────────────────────

    #[test]
    fn find_hash_trigger_at_end() {
        assert_eq!(find_hash_trigger("hello #"), Some(6));
    }

    #[test]
    fn find_hash_trigger_with_query() {
        assert_eq!(find_hash_trigger("hello #world"), Some(6));
    }

    #[test]
    fn find_hash_trigger_space_after_suppresses() {
        assert!(find_hash_trigger("# regular").is_none());
        assert!(find_hash_trigger("test # text").is_none());
    }

    #[test]
    fn find_hash_trigger_no_hash() {
        assert!(find_hash_trigger("no hash here").is_none());
        assert!(find_hash_trigger("").is_none());
    }

    #[test]
    fn find_hash_trigger_last_hash_wins() {
        // 多个 `#` 取最后一个
        let pos = find_hash_trigger("first #a second #b").unwrap();
        assert_eq!(&"first #a second #b"[pos..], "#b");
    }

    #[test]
    fn extract_query_returns_after_hash() {
        assert_eq!(extract_query("hello #world", 6), "world");
        assert_eq!(extract_query("hello #", 6), "");
    }

    #[test]
    fn remove_hash_trigger_clears_correctly() {
        let mut input = "some text #query".to_owned();
        let pos = find_hash_trigger(&input).unwrap();
        remove_hash_trigger(&mut input, pos);
        assert_eq!(input, "some text ");
    }

    #[test]
    fn remove_hash_trigger_at_start() {
        let mut input = "#".to_owned();
        remove_hash_trigger(&mut input, 0);
        assert!(input.is_empty());
    }

    // ── build_prompt_text ─────────────────────────────────────────────────

    #[test]
    fn build_prompt_no_chips_passthrough() {
        let result = build_prompt_text(&[], "user message");
        assert_eq!(result, "user message");
    }

    #[test]
    fn build_prompt_with_chips_prepends_context() {
        let chips = vec![RefChip {
            id: "msg:0".to_owned(),
            display: "[用户] Hello".to_owned(),
            content: "[引用消息#1 (用户)]\nHello world".to_owned(),
        }];
        let result = build_prompt_text(&chips, "请解释");
        assert!(result.starts_with("[引用消息#1 (用户)]"));
        assert!(result.ends_with("请解释"));
        assert!(result.contains("\n\n"));
    }

    #[test]
    fn build_prompt_multiple_chips_ordered() {
        let chips = vec![
            RefChip {
                id: "msg:0".to_owned(),
                display: "[用户] A".to_owned(),
                content: "引用A内容".to_owned(),
            },
            RefChip {
                id: "msg:1".to_owned(),
                display: "[助手] B".to_owned(),
                content: "引用B内容".to_owned(),
            },
        ];
        let result = build_prompt_text(&chips, "问题");
        let a_pos = result.find("引用A内容").unwrap();
        let b_pos = result.find("引用B内容").unwrap();
        let q_pos = result.find("问题").unwrap();
        assert!(a_pos < b_pos);
        assert!(b_pos < q_pos);
    }

    // ── Pi 会话文件（诚实空态） ───────────────────────────────────────────

    #[test]
    fn candidates_from_pi_sessions_no_panic_on_missing_dir() {
        // 测试环境无 Pi 会话目录 → 静默返回空列表
        let result = candidates_from_pi_sessions();
        // 结果任意（有/无都行），但不能 panic
        let _ = result;
    }
}
