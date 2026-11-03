// SPDX-License-Identifier: Apache-2.0
//
// 同会话消息树（壳侧）。Pi v1.4.0 有 get_tree / fork，但 fork 会新开会话文件，
// RPC 无 navigate_tree，因此分支切换不走 Pi 原生树。

use std::collections::BTreeMap;

use crate::chat::ChatMessage;

/// 用户可见的诚实说明：分支存在壳内，不是 Pi 同会话原生跳转。
pub const BRANCH_HONESTY: &str =
    "分支由壳维护。Pi v1.4.0 无同会话 navigate_tree；改写或切换分支时会对 Pi 发 new_session 并回放当前路径。";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub role: NodeRole,
    pub text: String,
    pub children: Vec<NodeId>,
}

/// 活动路径上的一行，供聊天气泡渲染。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathRow {
    pub id: NodeId,
    pub role: NodeRole,
    pub text: String,
    pub sibling_index: usize,
    pub sibling_count: usize,
}

/// 整棵树的缩进行，供导航面板。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub id: NodeId,
    pub depth: usize,
    pub role: NodeRole,
    pub text: String,
    pub on_active_path: bool,
}

/// 可落盘的树快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeSnapshot {
    pub next_id: u64,
    pub nodes: Vec<TreeNode>,
    pub roots: Vec<NodeId>,
    pub active: Vec<(Option<NodeId>, NodeId)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageTree {
    nodes: BTreeMap<u64, TreeNode>,
    next_id: u64,
    roots: Vec<NodeId>,
    /// parent_key → 当前选中的子节点。parent_key=0 表示根。
    active_child: BTreeMap<u64, u64>,
    /// 重新生成时活动路径停在该用户节点，不展示旧助手。
    regen_from: Option<NodeId>,
}

impl Default for MessageTree {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            next_id: 1,
            roots: Vec::new(),
            active_child: BTreeMap::new(),
            regen_from: None,
        }
    }
}

impl MessageTree {
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    pub fn has_branches(&self) -> bool {
        self.roots.len() > 1 || self.nodes.values().any(|n| n.children.len() > 1)
    }

    pub fn node(&self, id: NodeId) -> Option<&TreeNode> {
        self.nodes.get(&id.0)
    }

    pub fn from_linear(messages: &[ChatMessage]) -> Self {
        let mut tree = Self::default();
        for msg in messages {
            match msg {
                ChatMessage::User(text) => {
                    tree.push_user(text.clone());
                }
                ChatMessage::Assistant(text) => {
                    tree.push_assistant(text.clone());
                }
            }
        }
        tree
    }

    pub fn from_snapshot(snap: TreeSnapshot) -> Self {
        let mut nodes = BTreeMap::new();
        let mut max_id = 0u64;
        for node in snap.nodes {
            max_id = max_id.max(node.id.0);
            nodes.insert(node.id.0, node);
        }
        let mut active_child = BTreeMap::new();
        for (parent, child) in snap.active {
            active_child.insert(parent_key(parent), child.0);
        }
        Self {
            nodes,
            next_id: snap.next_id.max(max_id.saturating_add(1)).max(1),
            roots: snap.roots,
            active_child,
            regen_from: None,
        }
    }

    pub fn to_snapshot(&self) -> TreeSnapshot {
        TreeSnapshot {
            next_id: self.next_id,
            nodes: self.nodes.values().cloned().collect(),
            roots: self.roots.clone(),
            active: self
                .active_child
                .iter()
                .map(|(&pk, &child)| (node_from_parent_key(pk), NodeId(child)))
                .collect(),
        }
    }

    pub fn push_user(&mut self, text: String) -> NodeId {
        self.append_node(NodeRole::User, text)
    }

    pub fn push_assistant(&mut self, text: String) -> NodeId {
        self.append_node(NodeRole::Assistant, text)
    }

    /// 在指定用户消息处长出兄弟分支，活动路径切到新节点。
    pub fn branch_user(&mut self, at: NodeId, new_text: String) -> Option<NodeId> {
        let parent = self.nodes.get(&at.0)?.parent;
        if self.nodes.get(&at.0)?.role != NodeRole::User {
            return None;
        }
        let id = self.alloc(parent, NodeRole::User, new_text);
        self.insert_sibling(parent, at, id);
        self.set_active(parent, id);
        self.regen_from = Some(id);
        Some(id)
    }

    /// 重新生成：活动路径停在该助手的父用户消息。
    pub fn prepare_regen(&mut self, assistant: NodeId) -> Option<NodeId> {
        let node = self.nodes.get(&assistant.0)?;
        if node.role != NodeRole::Assistant {
            return None;
        }
        let parent = node.parent?;
        if self.nodes.get(&parent.0)?.role != NodeRole::User {
            return None;
        }
        self.regen_from = Some(parent);
        Some(parent)
    }

    /// 把指定节点所在分支设为活动路径（祖先指针全部改到通往该节点）。
    pub fn activate(&mut self, id: NodeId) -> bool {
        if !self.nodes.contains_key(&id.0) {
            return false;
        }
        let mut cur = id;
        loop {
            let parent = self.nodes.get(&cur.0).and_then(|n| n.parent);
            self.set_active(parent, cur);
            match parent {
                Some(p) => cur = p,
                None => break,
            }
        }
        self.regen_from = None;
        true
    }

    pub fn select_sibling(&mut self, id: NodeId, delta: i32) -> Option<NodeId> {
        let siblings = self.sibling_ids(id);
        if siblings.len() <= 1 {
            return None;
        }
        let idx = siblings.iter().position(|s| *s == id)?;
        let len = siblings.len() as i32;
        let next = (idx as i32 + delta).rem_euclid(len) as usize;
        let chosen = siblings[next];
        let parent = self.nodes.get(&id.0)?.parent;
        self.set_active(parent, chosen);
        self.regen_from = None;
        Some(chosen)
    }

    pub fn sibling_info(&self, id: NodeId) -> (usize, usize) {
        let siblings = self.sibling_ids(id);
        let idx = siblings.iter().position(|s| *s == id).unwrap_or(0);
        (idx, siblings.len())
    }

    pub fn last_on_path(&self) -> Option<NodeId> {
        self.active_ids().last().copied()
    }

    pub fn last_assistant_on_path(&self) -> Option<NodeId> {
        self.active_ids().into_iter().rev().find(|id| {
            self.nodes
                .get(&id.0)
                .is_some_and(|n| n.role == NodeRole::Assistant)
        })
    }

    pub fn last_user_on_path(&self) -> Option<NodeId> {
        self.active_ids().into_iter().rev().find(|id| {
            self.nodes
                .get(&id.0)
                .is_some_and(|n| n.role == NodeRole::User)
        })
    }

    pub fn text(&self, id: NodeId) -> Option<&str> {
        self.nodes.get(&id.0).map(|n| n.text.as_str())
    }

    pub fn active_messages(&self) -> Vec<ChatMessage> {
        self.active_path_rows()
            .into_iter()
            .map(|row| match row.role {
                NodeRole::User => ChatMessage::User(row.text),
                NodeRole::Assistant => ChatMessage::Assistant(row.text),
            })
            .collect()
    }

    pub fn active_path_rows(&self) -> Vec<PathRow> {
        self.active_ids()
            .into_iter()
            .filter_map(|id| {
                let node = self.nodes.get(&id.0)?;
                let (sibling_index, sibling_count) = self.sibling_info(id);
                Some(PathRow {
                    id,
                    role: node.role,
                    text: node.text.clone(),
                    sibling_index,
                    sibling_count,
                })
            })
            .collect()
    }

    /// 指定节点的祖先消息（不含自身）。
    pub fn ancestor_messages(&self, id: NodeId) -> Vec<ChatMessage> {
        let mut ids = Vec::new();
        let mut cur = self.nodes.get(&id.0).and_then(|n| n.parent);
        while let Some(pid) = cur {
            ids.push(pid);
            cur = self.nodes.get(&pid.0).and_then(|n| n.parent);
        }
        ids.reverse();
        ids.into_iter()
            .filter_map(|nid| {
                let node = self.nodes.get(&nid.0)?;
                Some(match node.role {
                    NodeRole::User => ChatMessage::User(node.text.clone()),
                    NodeRole::Assistant => ChatMessage::Assistant(node.text.clone()),
                })
            })
            .collect()
    }

    pub fn tree_rows(&self) -> Vec<TreeRow> {
        let active: Vec<NodeId> = self.active_ids();
        let mut out = Vec::new();
        for root in &self.roots {
            self.collect_tree_rows(*root, 0, &active, &mut out);
        }
        out
    }

    fn collect_tree_rows(
        &self,
        id: NodeId,
        depth: usize,
        active: &[NodeId],
        out: &mut Vec<TreeRow>,
    ) {
        let Some(node) = self.nodes.get(&id.0) else {
            return;
        };
        out.push(TreeRow {
            id,
            depth,
            role: node.role,
            text: node.text.clone(),
            on_active_path: active.contains(&id),
        });
        for child in &node.children {
            self.collect_tree_rows(*child, depth + 1, active, out);
        }
    }

    fn append_node(&mut self, role: NodeRole, text: String) -> NodeId {
        let parent = if let Some(from) = self.regen_from.take() {
            Some(from)
        } else {
            self.last_on_path()
        };
        let id = self.alloc(parent, role, text);
        match parent {
            Some(pid) => {
                if let Some(node) = self.nodes.get_mut(&pid.0) {
                    if !node.children.contains(&id) {
                        node.children.push(id);
                    }
                }
            }
            None => {
                if !self.roots.contains(&id) {
                    self.roots.push(id);
                }
            }
        }
        self.set_active(parent, id);
        id
    }

    fn insert_sibling(&mut self, parent: Option<NodeId>, after: NodeId, id: NodeId) {
        let siblings = match parent {
            Some(pid) => self
                .nodes
                .get_mut(&pid.0)
                .map(|n| &mut n.children)
                .expect("parent must exist"),
            None => &mut self.roots,
        };
        if !siblings.contains(&id) {
            let pos = siblings
                .iter()
                .position(|s| *s == after)
                .unwrap_or(siblings.len());
            siblings.insert(pos + 1, id);
        }
    }

    fn alloc(&mut self, parent: Option<NodeId>, role: NodeRole, text: String) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        self.nodes.insert(
            id.0,
            TreeNode {
                id,
                parent,
                role,
                text,
                children: Vec::new(),
            },
        );
        id
    }

    fn set_active(&mut self, parent: Option<NodeId>, child: NodeId) {
        self.active_child.insert(parent_key(parent), child.0);
    }

    fn sibling_ids(&self, id: NodeId) -> Vec<NodeId> {
        match self.nodes.get(&id.0).and_then(|n| n.parent) {
            Some(pid) => self
                .nodes
                .get(&pid.0)
                .map(|n| n.children.clone())
                .unwrap_or_else(|| vec![id]),
            None => self.roots.clone(),
        }
    }

    fn active_ids(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut parent = None;
        while let Some(id) = self
            .active_child
            .get(&parent_key(parent))
            .copied()
            .map(NodeId)
        {
            if !self.nodes.contains_key(&id.0) {
                break;
            }
            out.push(id);
            if self.regen_from == Some(id) {
                break;
            }
            parent = Some(id);
        }
        out
    }
}

fn parent_key(parent: Option<NodeId>) -> u64 {
    parent.map(|p| p.0).unwrap_or(0)
}

fn node_from_parent_key(key: u64) -> Option<NodeId> {
    if key == 0 {
        None
    } else {
        Some(NodeId(key))
    }
}

/// 把选定分支的历史拼进下一次 prompt。空前缀则原样发送。
pub fn replay_prompt(prefix: &[ChatMessage], next_user: &str) -> String {
    if prefix.is_empty() {
        return next_user.to_owned();
    }
    let mut out = String::from(
        "【pi-light 分支回放】以下是当前选定分支的已有对话，请当作已经发生过的上下文，不要重新回答这些历史消息。\n\n",
    );
    for msg in prefix {
        match msg {
            ChatMessage::User(text) => {
                out.push_str("用户：");
                out.push_str(text);
                out.push_str("\n\n");
            }
            ChatMessage::Assistant(text) => {
                out.push_str("助手：");
                out.push_str(text);
                out.push_str("\n\n");
            }
        }
    }
    out.push_str("【回放结束】请只回答下面这条用户消息：\n");
    out.push_str(next_user);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_roundtrip_matches_messages() {
        let msgs = vec![
            ChatMessage::User("u1".into()),
            ChatMessage::Assistant("a1".into()),
            ChatMessage::User("u2".into()),
            ChatMessage::Assistant("a2".into()),
        ];
        let tree = MessageTree::from_linear(&msgs);
        assert!(!tree.has_branches());
        assert_eq!(tree.active_messages(), msgs);
    }

    #[test]
    fn branch_user_switches_visible_path() {
        let mut tree = MessageTree::from_linear(&[
            ChatMessage::User("hello".into()),
            ChatMessage::Assistant("hi".into()),
        ]);
        let first = tree.last_user_on_path().unwrap();
        let branched = tree.branch_user(first, "hello again".into()).unwrap();
        assert_eq!(
            tree.active_messages(),
            vec![ChatMessage::User("hello again".into())]
        );
        assert!(tree.has_branches());
        tree.activate(first);
        assert_eq!(
            tree.active_messages(),
            vec![
                ChatMessage::User("hello".into()),
                ChatMessage::Assistant("hi".into())
            ]
        );
        tree.activate(branched);
        assert_eq!(
            tree.active_messages(),
            vec![ChatMessage::User("hello again".into())]
        );
    }

    #[test]
    fn regen_hides_old_assistant_until_push() {
        let mut tree = MessageTree::from_linear(&[
            ChatMessage::User("q".into()),
            ChatMessage::Assistant("old".into()),
        ]);
        let assistant = tree.last_assistant_on_path().unwrap();
        assert_eq!(tree.prepare_regen(assistant), tree.last_user_on_path());
        assert_eq!(tree.active_messages(), vec![ChatMessage::User("q".into())]);
        tree.push_assistant("new".into());
        let rows = tree.active_path_rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].text, "new");
        assert_eq!(rows[1].sibling_count, 2);
        assert_eq!(rows[1].sibling_index, 1);
        tree.select_sibling(rows[1].id, -1);
        assert_eq!(
            tree.active_messages(),
            vec![
                ChatMessage::User("q".into()),
                ChatMessage::Assistant("old".into())
            ]
        );
    }

    #[test]
    fn sibling_wraps_and_tree_rows_include_inactive() {
        let mut tree = MessageTree::from_linear(&[ChatMessage::User("a".into())]);
        let first = tree.last_user_on_path().unwrap();
        tree.branch_user(first, "b".into()).unwrap();
        tree.branch_user(tree.last_user_on_path().unwrap(), "c".into())
            .unwrap();
        let current = tree.last_user_on_path().unwrap();
        let (idx, count) = tree.sibling_info(current);
        assert_eq!(count, 3);
        assert_eq!(idx, 2);
        let prev = tree.select_sibling(current, 1).unwrap();
        assert_eq!(tree.text(prev), Some("a"));
        let rows = tree.tree_rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows.iter().filter(|r| r.on_active_path).count(), 1);
    }

    #[test]
    fn snapshot_preserves_inactive_branch() {
        let mut tree = MessageTree::from_linear(&[
            ChatMessage::User("u".into()),
            ChatMessage::Assistant("a".into()),
        ]);
        let user = tree.last_user_on_path().unwrap();
        tree.branch_user(user, "u2".into()).unwrap();
        tree.push_assistant("a2".into());
        let mut restored = MessageTree::from_snapshot(tree.to_snapshot());
        assert_eq!(restored.active_messages(), tree.active_messages());
        assert!(restored.has_branches());
        let first_assistant = restored
            .tree_rows()
            .into_iter()
            .find(|r| r.role == NodeRole::Assistant && r.text == "a")
            .unwrap();
        restored.activate(first_assistant.id);
        assert_eq!(
            restored.active_messages(),
            vec![
                ChatMessage::User("u".into()),
                ChatMessage::Assistant("a".into())
            ]
        );
    }

    #[test]
    fn replay_prompt_empty_prefix_is_passthrough() {
        assert_eq!(replay_prompt(&[], "hi"), "hi");
    }

    #[test]
    fn replay_prompt_includes_history_and_next() {
        let text = replay_prompt(
            &[
                ChatMessage::User("q1".into()),
                ChatMessage::Assistant("a1".into()),
            ],
            "q2",
        );
        assert!(text.contains("用户：q1"));
        assert!(text.contains("助手：a1"));
        assert!(text.contains("q2"));
        assert!(text.contains("分支回放"));
    }

    #[test]
    fn ancestor_messages_exclude_self() {
        let tree = MessageTree::from_linear(&[
            ChatMessage::User("u1".into()),
            ChatMessage::Assistant("a1".into()),
            ChatMessage::User("u2".into()),
        ]);
        let last = tree.last_user_on_path().unwrap();
        let prefix = tree.ancestor_messages(last);
        assert_eq!(
            prefix,
            vec![
                ChatMessage::User("u1".into()),
                ChatMessage::Assistant("a1".into())
            ]
        );
    }
}
