// SPDX-License-Identifier: Apache-2.0
//
// 壳侧会话历史：写入 ~/.pi-light/sessions.json（或 PI_LIGHT_STATE_PATH），
// 启动时读回。不保存 API 密钥。

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::chat::{ChatMessage, ExportedHub, ExportedSession, SessionHub};
use crate::message_tree::{MessageTree, NodeId, NodeRole, TreeNode, TreeSnapshot};

const VERSION: u32 = 2;
const VERSION_V1: u32 = 1;

pub struct LoadedSessions {
    pub sessions: SessionHub,
    pub workdir: Option<PathBuf>,
}

/// 启动 Pi 时：会话文件还在就切回去，否则新开会话。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RpcAttach {
    SwitchTo(String),
    NewSession,
}

pub fn rpc_attach(session_file: Option<&str>) -> RpcAttach {
    match session_file.map(str::trim).filter(|p| !p.is_empty()) {
        Some(path) if Path::new(path).is_file() => RpcAttach::SwitchTo(path.to_owned()),
        _ => RpcAttach::NewSession,
    }
}

/// 默认 `~/.pi-light/sessions.json`。`PI_LIGHT_STATE_PATH` 可指向完整文件路径。
pub fn default_path() -> Option<PathBuf> {
    if let Some(raw) = std::env::var_os("PI_LIGHT_STATE_PATH") {
        let path = PathBuf::from(raw);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }
    home_dir().map(|h| h.join(".pi-light").join("sessions.json"))
}

pub fn load(path: &Path) -> Option<LoadedSessions> {
    let text = fs::read_to_string(path).ok()?;
    match serde_json::from_str::<FileState>(&text) {
        Ok(state) if state.version == VERSION || state.version == VERSION_V1 => {
            Some(state.into_loaded())
        }
        _ => {
            let bak = path.with_extension("json.bak");
            let _ = fs::rename(path, bak);
            None
        }
    }
}

pub fn save(path: &Path, hub: &SessionHub, workdir: &Path) -> io::Result<()> {
    let state = FileState::from_hub(hub, workdir);
    let bytes = serde_json::to_vec_pretty(&state)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    atomic_write(path, &bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp, path)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[derive(Debug, Serialize, Deserialize)]
struct FileState {
    version: u32,
    active: usize,
    next_id: u64,
    #[serde(default)]
    workdir: Option<String>,
    #[serde(default)]
    sessions: Vec<FileSession>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileSession {
    id: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    session_file: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    last_sent_prompt: Option<String>,
    #[serde(default)]
    input: String,
    #[serde(default)]
    messages: Vec<FileMessage>,
    #[serde(default)]
    tree: Option<FileTree>,
    #[serde(default)]
    pi_stale: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileTree {
    #[serde(default)]
    next_id: u64,
    #[serde(default)]
    nodes: Vec<FileTreeNode>,
    #[serde(default)]
    roots: Vec<u64>,
    #[serde(default)]
    active: Vec<FileActiveEdge>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileTreeNode {
    id: u64,
    #[serde(default)]
    parent: Option<u64>,
    role: String,
    text: String,
    #[serde(default)]
    children: Vec<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileActiveEdge {
    parent: Option<u64>,
    child: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileMessage {
    role: String,
    text: String,
}

impl FileState {
    fn from_hub(hub: &SessionHub, workdir: &Path) -> Self {
        let exported = hub.to_exported();
        Self::from_exported(&exported, workdir)
    }

    fn from_exported(exported: &ExportedHub, workdir: &Path) -> Self {
        Self {
            version: VERSION,
            active: exported.active,
            next_id: exported.next_id,
            workdir: Some(workdir.to_string_lossy().into_owned()),
            sessions: exported
                .sessions
                .iter()
                .map(FileSession::from_exported)
                .collect(),
        }
    }

    fn into_loaded(self) -> LoadedSessions {
        let exported = ExportedHub {
            active: self.active,
            next_id: self.next_id,
            sessions: self
                .sessions
                .into_iter()
                .map(FileSession::into_exported)
                .collect(),
        };
        LoadedSessions {
            workdir: self.workdir.map(PathBuf::from),
            sessions: SessionHub::from_exported(exported),
        }
    }
}

impl FileSession {
    fn from_exported(session: &ExportedSession) -> Self {
        Self {
            id: session.id,
            title: session.title.clone(),
            session_file: session.session_file.clone(),
            session_id: session.session_id.clone(),
            last_sent_prompt: session.last_sent_prompt.clone(),
            input: session.input.clone(),
            messages: session
                .messages
                .iter()
                .map(|msg| match msg {
                    ChatMessage::User(text) => FileMessage {
                        role: "user".to_owned(),
                        text: text.clone(),
                    },
                    ChatMessage::Assistant(text) => FileMessage {
                        role: "assistant".to_owned(),
                        text: text.clone(),
                    },
                })
                .collect(),
            tree: Some(FileTree::from_snapshot(&session.tree)),
            pi_stale: session.pi_stale,
        }
    }

    fn into_exported(self) -> ExportedSession {
        let messages: Vec<ChatMessage> = self
            .messages
            .into_iter()
            .filter_map(|msg| match msg.role.as_str() {
                "user" => Some(ChatMessage::User(msg.text)),
                "assistant" => Some(ChatMessage::Assistant(msg.text)),
                _ => None,
            })
            .collect();
        let tree = self
            .tree
            .map(FileTree::into_snapshot)
            .filter(|snap| !snap.nodes.is_empty())
            .unwrap_or_else(|| MessageTree::from_linear(&messages).to_snapshot());
        ExportedSession {
            id: self.id,
            title: self.title,
            session_file: self.session_file,
            session_id: self.session_id,
            last_sent_prompt: self.last_sent_prompt,
            input: self.input,
            messages,
            tree,
            pi_stale: self.pi_stale,
        }
    }
}

impl FileTree {
    fn from_snapshot(snap: &TreeSnapshot) -> Self {
        Self {
            next_id: snap.next_id,
            nodes: snap
                .nodes
                .iter()
                .map(|n| FileTreeNode {
                    id: n.id.0,
                    parent: n.parent.map(|p| p.0),
                    role: match n.role {
                        NodeRole::User => "user".to_owned(),
                        NodeRole::Assistant => "assistant".to_owned(),
                    },
                    text: n.text.clone(),
                    children: n.children.iter().map(|c| c.0).collect(),
                })
                .collect(),
            roots: snap.roots.iter().map(|r| r.0).collect(),
            active: snap
                .active
                .iter()
                .map(|(parent, child)| FileActiveEdge {
                    parent: parent.map(|p| p.0),
                    child: child.0,
                })
                .collect(),
        }
    }

    fn into_snapshot(self) -> TreeSnapshot {
        TreeSnapshot {
            next_id: self.next_id,
            nodes: self
                .nodes
                .into_iter()
                .filter_map(|n| {
                    let role = match n.role.as_str() {
                        "user" => NodeRole::User,
                        "assistant" => NodeRole::Assistant,
                        _ => return None,
                    };
                    Some(TreeNode {
                        id: NodeId(n.id),
                        parent: n.parent.map(NodeId),
                        role,
                        text: n.text,
                        children: n.children.into_iter().map(NodeId).collect(),
                    })
                })
                .collect(),
            roots: self.roots.into_iter().map(NodeId).collect(),
            active: self
                .active
                .into_iter()
                .map(|e| (e.parent.map(NodeId), NodeId(e.child)))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_keeps_history_active_slot_and_workdir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.json");
        let workdir = dir.path().join("proj");
        fs::create_dir_all(&workdir).unwrap();

        let mut hub = SessionHub::new();
        hub.active_chat_mut().push_user("第一条".to_owned());
        hub.active_chat_mut().push_assistant("回复".to_owned());
        hub.set_active_title_from_user("第一条");
        hub.set_active_session_file(Some("sess.jsonl".to_owned()), Some("sid".to_owned()));
        hub.active_chat_mut().input = "草稿".to_owned();
        hub.active_chat_mut().last_sent_prompt = Some("第一条".to_owned());
        hub.create_new();
        hub.active_chat_mut().push_user("第二条".to_owned());
        assert!(hub.switch_to(0));

        save(&path, &hub, &workdir).unwrap();
        let mut loaded = load(&path).unwrap();
        assert_eq!(loaded.sessions.list().len(), 2);
        assert_eq!(loaded.sessions.active_index(), 0);
        assert_eq!(loaded.sessions.active().title, "第一条");
        assert_eq!(
            loaded.sessions.active().session_file.as_deref(),
            Some("sess.jsonl")
        );
        assert_eq!(loaded.sessions.active_chat().messages.len(), 2);
        assert_eq!(loaded.sessions.active_chat().input, "草稿");
        assert_eq!(
            loaded.sessions.active_chat().last_sent_prompt.as_deref(),
            Some("第一条")
        );
        assert_eq!(loaded.workdir.as_deref(), Some(workdir.as_path()));
        assert!(loaded.sessions.switch_to(1));
        assert_eq!(loaded.sessions.active_chat().messages.len(), 1);
    }

    #[test]
    fn missing_file_is_none_without_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.json");
        assert!(load(&path).is_none());
        assert!(!path.with_extension("json.bak").exists());
    }

    #[test]
    fn corrupt_file_moves_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.json");
        fs::write(&path, "{not json").unwrap();
        assert!(load(&path).is_none());
        assert!(!path.exists());
        assert!(path.with_extension("json.bak").is_file());
    }

    #[test]
    fn unknown_message_role_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.json");
        fs::write(
            &path,
            r#"{
                "version": 1,
                "active": 0,
                "next_id": 2,
                "sessions": [{
                    "id": 1,
                    "title": "t",
                    "messages": [
                        {"role": "user", "text": "hi"},
                        {"role": "tool", "text": "skip"},
                        {"role": "assistant", "text": "yo"}
                    ]
                }]
            }"#,
        )
        .unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.sessions.active_chat().messages.len(), 2);
    }

    #[test]
    fn v2_roundtrip_keeps_inactive_branch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sessions.json");
        let workdir = dir.path().join("proj");
        fs::create_dir_all(&workdir).unwrap();

        let mut hub = SessionHub::new();
        hub.active_chat_mut().push_user("u".to_owned());
        hub.active_chat_mut().push_assistant("a".to_owned());
        let user = hub.active_chat().tree.last_user_on_path().unwrap();
        hub.active_chat_mut()
            .prepare_user_branch(user, "u2".to_owned());
        hub.active_chat_mut().push_assistant("a2".to_owned());

        save(&path, &hub, &workdir).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"version\": 2"));
        let mut loaded = load(&path).unwrap();
        assert!(loaded.sessions.active_chat().tree.has_branches());
        assert_eq!(loaded.sessions.active_chat().messages.len(), 2);
        let first = loaded
            .sessions
            .active_chat()
            .tree_rows()
            .into_iter()
            .find(|r| r.text == "a")
            .unwrap();
        loaded.sessions.active_chat_mut().navigate_to(first.id);
        assert_eq!(loaded.sessions.active_chat().messages.len(), 2);
        match &loaded.sessions.active_chat().messages[1] {
            ChatMessage::Assistant(t) => assert_eq!(t, "a"),
            other => panic!("expected inactive assistant, got {other:?}"),
        }
    }

    #[test]
    fn rpc_attach_switches_only_when_file_exists() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("s.jsonl");
        assert_eq!(rpc_attach(None), RpcAttach::NewSession);
        assert_eq!(rpc_attach(Some("")), RpcAttach::NewSession);
        assert_eq!(
            rpc_attach(Some(file.to_str().unwrap())),
            RpcAttach::NewSession
        );
        fs::write(&file, "{}\n").unwrap();
        assert_eq!(
            rpc_attach(Some(file.to_str().unwrap())),
            RpcAttach::SwitchTo(file.to_string_lossy().into_owned())
        );
    }
}
