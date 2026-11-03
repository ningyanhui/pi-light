// SPDX-License-Identifier: Apache-2.0
//
// Composer `/` `@` `#` 唤起。对齐原生 Pi 能经 RPC/壳做到的部分：
//   /  → get_commands 列表，选中后以 prompt "/name" 发给 Pi；`/new` 为壳侧新建
//   @  → 工作区文件相对路径，插入 `@path`（无独立 @file RPC）
//   #  → 既有会话/消息引用 chip
// TUI 内置项（/settings、/hotkeys 等）RPC 不会执行，不假装可点。

use crate::chat::ChatSession;
use crate::session_ref::{self, RefCandidate, RefChip};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvokeKind {
    Slash,
    At,
    Hash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvokeAction {
    /// 替换触发词为文本，不发送。
    Insert,
    /// 替换触发词并立刻作为 prompt 发送（Pi 扩展/技能/模板）。
    SendPrompt,
    /// 壳侧新建会话（诚实：不是 Pi TUI `/new` RPC）。
    NewSession,
    /// 生成 `#` 引用 chip。
    Quote,
}

#[derive(Debug, Clone)]
pub struct InvokeCandidate {
    pub id: String,
    pub display: String,
    pub insert: String,
    pub detail: String,
    pub action: InvokeAction,
    pub chip: Option<RefChip>,
}

#[derive(Debug, Clone)]
pub struct PiCommand {
    pub name: String,
    pub description: String,
    pub source: String,
}

#[derive(Default)]
pub struct InvokeChooser {
    pub kind: Option<InvokeKind>,
    pub active: bool,
    pub filtered: Vec<InvokeCandidate>,
    pub cursor: usize,
    pub trigger_byte_offset: usize,
}

impl InvokeChooser {
    pub fn activate(
        &mut self,
        kind: InvokeKind,
        items: &[InvokeCandidate],
        query: &str,
        pos: usize,
    ) {
        self.kind = Some(kind);
        self.active = true;
        self.trigger_byte_offset = pos;
        self.filtered = items
            .iter()
            .filter(|c| candidate_matches(c, query))
            .cloned()
            .collect();
        if self.cursor >= self.filtered.len() {
            self.cursor = 0;
        }
    }

    pub fn close(&mut self) {
        self.active = false;
        self.kind = None;
        self.filtered.clear();
        self.cursor = 0;
    }

    pub fn move_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if !self.filtered.is_empty() && self.cursor + 1 < self.filtered.len() {
            self.cursor += 1;
        }
    }

    /// 空列表不关闭（避免 `#` 空态回车把弹层“神秘关掉”）。
    pub fn confirm(&mut self) -> Option<InvokeCandidate> {
        if self.filtered.is_empty() {
            return None;
        }
        let item = self.filtered.get(self.cursor)?.clone();
        self.close();
        Some(item)
    }
}

pub fn find_invoke_trigger(input: &str) -> Option<(InvokeKind, usize)> {
    let bytes = input.as_bytes();
    let mut last: Option<(InvokeKind, usize)> = None;
    for (i, ch) in input.char_indices() {
        let kind = match ch {
            '/' => Some(InvokeKind::Slash),
            '@' => Some(InvokeKind::At),
            '#' => Some(InvokeKind::Hash),
            _ => None,
        };
        let Some(kind) = kind else {
            continue;
        };
        if !is_token_start(bytes, i) {
            continue;
        }
        let after = &input[i + ch.len_utf8()..];
        if after.starts_with(' ') || after.starts_with('\n') || after.starts_with('\r') {
            continue;
        }
        last = Some((kind, i));
    }
    last
}

pub fn extract_query(input: &str, pos: usize) -> &str {
    let rest = &input[pos..];
    let skip = rest.chars().next().map(|c| c.len_utf8()).unwrap_or(0);
    &input[pos + skip..]
}

pub fn replace_trigger(input: &mut String, pos: usize, replacement: &str) {
    *input = format!("{}{}", &input[..pos], replacement);
}

fn is_token_start(bytes: &[u8], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    matches!(bytes[i - 1], b' ' | b'\n' | b'\t' | b'\r')
}

fn candidate_matches(c: &InvokeCandidate, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let q = query.to_lowercase();
    c.display.to_lowercase().contains(&q)
        || c.insert.to_lowercase().contains(&q)
        || c.detail.to_lowercase().contains(&q)
}

pub fn slash_candidates(commands: &[PiCommand]) -> Vec<InvokeCandidate> {
    let mut out = vec![InvokeCandidate {
        id: "shell:new".to_owned(),
        display: "/new".to_owned(),
        insert: String::new(),
        detail: "壳侧新建会话（Pi TUI /new 不经 RPC）".to_owned(),
        action: InvokeAction::NewSession,
        chip: None,
    }];
    for cmd in commands {
        let name = cmd.name.trim();
        if name.is_empty() {
            continue;
        }
        let insert = if name.starts_with('/') {
            name.to_owned()
        } else {
            format!("/{name}")
        };
        out.push(InvokeCandidate {
            id: format!("cmd:{name}"),
            display: insert.clone(),
            insert,
            detail: if cmd.description.is_empty() {
                format!("Pi {} 命令", cmd.source)
            } else {
                cmd.description.clone()
            },
            action: InvokeAction::SendPrompt,
            chip: None,
        });
    }
    out
}

pub fn at_candidates(paths: &[String]) -> Vec<InvokeCandidate> {
    paths
        .iter()
        .map(|rel| {
            let token = if rel.starts_with('@') {
                rel.clone()
            } else {
                format!("@{rel}")
            };
            InvokeCandidate {
                id: format!("file:{rel}"),
                display: token.clone(),
                insert: format!("{token} "),
                detail: "插入工作区相对路径（无独立 @file RPC）".to_owned(),
                action: InvokeAction::Insert,
                chip: None,
            }
        })
        .collect()
}

pub fn hash_candidates(messages: &[crate::chat::ChatMessage]) -> Vec<InvokeCandidate> {
    session_ref::candidates_from_messages(messages)
        .into_iter()
        .map(hash_from_ref)
        .collect()
}

pub fn hash_session_file_candidates() -> Vec<InvokeCandidate> {
    session_ref::candidates_from_pi_sessions()
        .into_iter()
        .map(hash_from_ref)
        .collect()
}

pub fn quote_hub_session(session: &ChatSession, preview: &str) -> RefChip {
    RefChip {
        id: format!("hub:{}", session.id),
        display: format!("[会话] {}", session.title),
        content: format!("[引用会话: {}]\n{}", session.title, preview),
    }
}

fn hash_from_ref(c: RefCandidate) -> InvokeCandidate {
    InvokeCandidate {
        id: c.id.clone(),
        display: c.display.clone(),
        insert: String::new(),
        detail: c.preview.clone(),
        action: InvokeAction::Quote,
        chip: Some(RefChip {
            id: c.id,
            display: c.display,
            content: c.preview,
        }),
    }
}

pub fn thinking_levels_usable(levels: &[String]) -> bool {
    levels.iter().any(|lv| lv != "off")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::ChatMessage;

    #[test]
    fn trigger_token_start_only() {
        assert_eq!(find_invoke_trigger("/"), Some((InvokeKind::Slash, 0)));
        assert_eq!(find_invoke_trigger("@src"), Some((InvokeKind::At, 0)));
        assert_eq!(find_invoke_trigger("see #a"), Some((InvokeKind::Hash, 4)));
        assert_eq!(find_invoke_trigger("a/b"), None);
        assert_eq!(find_invoke_trigger("email@x"), None);
        assert_eq!(find_invoke_trigger("# "), None);
        assert_eq!(find_invoke_trigger(" /skill"), Some((InvokeKind::Slash, 1)));
        let last = find_invoke_trigger("/x @f #s").unwrap();
        assert_eq!(last, (InvokeKind::Hash, 6));
    }

    #[test]
    fn slash_includes_shell_new_and_rpc_commands() {
        let items = slash_candidates(&[PiCommand {
            name: "skill:demo".to_owned(),
            description: "演示".to_owned(),
            source: "skill".to_owned(),
        }]);
        assert_eq!(items[0].action, InvokeAction::NewSession);
        assert!(items.iter().any(|c| c.insert == "/skill:demo"));
        assert!(!items.iter().any(|c| c.display.contains("/settings")));
    }

    #[test]
    fn at_inserts_path_token() {
        let items = at_candidates(&["src/ui/dock.rs".to_owned()]);
        assert_eq!(items[0].insert, "@src/ui/dock.rs ");
        assert_eq!(items[0].action, InvokeAction::Insert);
    }

    #[test]
    fn hash_from_messages_becomes_chip() {
        let items = hash_candidates(&[ChatMessage::User("hello".to_owned())]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].action, InvokeAction::Quote);
        assert!(items[0].chip.is_some());
    }

    #[test]
    fn empty_confirm_keeps_open() {
        let mut chooser = InvokeChooser::default();
        chooser.activate(InvokeKind::Hash, &[], "", 0);
        assert!(chooser.active);
        assert!(chooser.confirm().is_none());
        assert!(chooser.active);
    }

    #[test]
    fn confirm_selected_closes() {
        let items = at_candidates(&["a.rs".to_owned()]);
        let mut chooser = InvokeChooser::default();
        chooser.activate(InvokeKind::At, &items, "", 0);
        let got = chooser.confirm().unwrap();
        assert_eq!(got.insert, "@a.rs ");
        assert!(!chooser.active);
    }

    #[test]
    fn thinking_usable_rejects_off_only() {
        assert!(!thinking_levels_usable(&[]));
        assert!(!thinking_levels_usable(&["off".to_owned()]));
        assert!(thinking_levels_usable(&[
            "off".to_owned(),
            "low".to_owned()
        ]));
    }

    #[test]
    fn replace_trigger_keeps_prefix() {
        let mut s = "see @do".to_owned();
        replace_trigger(&mut s, 4, "@src/dock.rs ");
        assert_eq!(s, "see @src/dock.rs ");
    }
}
