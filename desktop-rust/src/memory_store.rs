// SPDX-License-Identifier: Apache-2.0
//! 记忆文件：按工作区隔离的 pi-memory 目录、会话摘要自动写入、记忆文件枚举。
//!
//! 项目记忆目录：`<agent_dir>/memory/projects/<slug>/`，启动 Pi 时作为
//! `PI_MEMORY_DIR` 注入，pi-memory 的 MEMORY.md / SCRATCHPAD.md / daily/ 因此按项目隔离。
//! 会话记忆：每轮结束后壳侧写 `sessions/session-<id>.md`，不调用模型。

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::pi_rpc::TraceStep;
use crate::tool_step::StepKind;

pub const MEMORY_DIR_ENV: &str = "PI_MEMORY_DIR";

/// 工作区目录名（安全字符）+ 完整路径的 FNV-1a 短哈希，避免同名项目冲突。
pub fn project_slug(workdir: &Path) -> String {
    let name: String = workdir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("workspace")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let key = workdir
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
        .to_lowercase();
    let key = key.trim_end_matches('/');
    let mut hash: u32 = 0x811c_9dc5;
    for b in key.bytes() {
        hash ^= u32::from(b);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    let name = if name.trim_matches('-').is_empty() {
        "workspace".to_owned()
    } else {
        name
    };
    format!("{name}-{hash:08x}")
}

pub fn project_memory_dir(agent_dir: &Path, workdir: &Path) -> PathBuf {
    agent_dir
        .join("memory")
        .join("projects")
        .join(project_slug(workdir))
}

/// 用户自己设置了 `PI_MEMORY_DIR` 时尊重它，不覆盖。
pub fn sidecar_memory_dir(agent_dir: Option<&Path>, workdir: &Path) -> Option<PathBuf> {
    if std::env::var_os(MEMORY_DIR_ENV).is_some_and(|v| !v.is_empty()) {
        return None;
    }
    Some(project_memory_dir(agent_dir?, workdir))
}

pub fn session_memory_path(project_dir: &Path, session_id: u64) -> PathBuf {
    project_dir
        .join("sessions")
        .join(format!("session-{session_id}.md"))
}

/// 一轮对话的摘要输入。
pub struct TurnDigest<'a> {
    pub session_title: &'a str,
    pub workdir: &'a Path,
    pub user: &'a str,
    pub assistant: &'a str,
    pub steps: &'a [TraceStep],
}

pub fn append_turn_digest(path: &Path, turn: &TurnDigest<'_>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let is_new = !path.exists();
    let mut out = String::new();
    if is_new {
        out.push_str(&format!(
            "# 会话记忆：{}\n\n- 工作区：`{}`\n- 由 pi-light 在每轮结束后自动记录\n",
            one_line(turn.session_title, 80),
            turn.workdir.display()
        ));
    }
    out.push_str(&format_turn(turn, now_label()));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(out.as_bytes())
}

pub fn format_turn(turn: &TurnDigest<'_>, when: String) -> String {
    let mut files: Vec<(&'static str, String)> = Vec::new();
    let mut commands: Vec<String> = Vec::new();
    for step in turn.steps {
        let Some(target) = step.summary.as_ref().map(|s| s.target.clone()) else {
            continue;
        };
        if target.is_empty() {
            continue;
        }
        match StepKind::from_tool(&step.tool_name) {
            StepKind::Read => push_unique(&mut files, ("读取", target)),
            StepKind::Edit => push_unique(&mut files, ("修改", target)),
            StepKind::Write => push_unique(&mut files, ("写入", target)),
            StepKind::Run => {
                let cmd = one_line(&target, 160);
                if !commands.contains(&cmd) {
                    commands.push(cmd);
                }
            }
            StepKind::Search | StepKind::Other => {}
        }
    }
    let mut s = format!("\n## {when}\n\n**问题：** {}\n", one_line(turn.user, 400));
    if !files.is_empty() {
        s.push_str("\n**文件：**\n");
        for (verb, path) in &files {
            s.push_str(&format!("- {verb} `{path}`\n"));
        }
    }
    if !commands.is_empty() {
        s.push_str("\n**命令：**\n");
        for cmd in commands.iter().take(12) {
            s.push_str(&format!("- `{cmd}`\n"));
        }
    }
    let answer = one_line(turn.assistant, 600);
    if !answer.is_empty() {
        s.push_str(&format!("\n**结论：** {answer}\n"));
    }
    s
}

fn push_unique(list: &mut Vec<(&'static str, String)>, item: (&'static str, String)) {
    if !list.iter().any(|(v, p)| *v == item.0 && *p == item.1) {
        list.push(item);
    }
}

fn one_line(text: &str, max_chars: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max_chars {
        flat
    } else {
        let mut cut: String = flat.chars().take(max_chars).collect();
        cut.push('…');
        cut
    }
}

/// UTC 时间标签 `YYYY-MM-DD HH:MM UTC`（不引入时间库）。
fn now_label() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_utc(secs)
}

pub fn format_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        (rem % 3600) / 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ── 记忆文件枚举 ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryGroup {
    /// 工作区里的指令文件（AGENTS.md / CLAUDE.md / .pi/*）。
    ProjectInstructions,
    /// 本项目的 pi-memory 目录（MEMORY / SCRATCHPAD / daily）。
    ProjectMemory,
    /// 壳侧自动写入的会话摘要。
    Sessions,
    /// 用户级跨项目指令（~/.pi/agent/AGENTS.md）。
    User,
    /// 旧的全局 pi-memory 目录（~/.pi/agent/memory/）。
    Global,
}

impl MemoryGroup {
    pub const ALL: [MemoryGroup; 5] = [
        MemoryGroup::ProjectInstructions,
        MemoryGroup::ProjectMemory,
        MemoryGroup::Sessions,
        MemoryGroup::User,
        MemoryGroup::Global,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MemoryGroup::ProjectInstructions => "项目指令",
            MemoryGroup::ProjectMemory => "项目记忆",
            MemoryGroup::Sessions => "会话记忆",
            MemoryGroup::User => "用户指令",
            MemoryGroup::Global => "全局记忆",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            MemoryGroup::ProjectInstructions => {
                "工作区内的 AGENTS.md / CLAUDE.md / .pi/*，Pi 每轮读取"
            }
            MemoryGroup::ProjectMemory => "pi-memory 按本工作区自动记录：长期记忆、待办、每日日志",
            MemoryGroup::Sessions => "每轮回复结束后自动摘要：问题、读写文件、命令、结论",
            MemoryGroup::User => "~/.pi/agent 下的跨项目指令",
            MemoryGroup::Global => "未按项目隔离前写入的 pi-memory 记录（只在此查看）",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryFile {
    pub group: MemoryGroup,
    pub label: String,
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
}

pub fn list_memory_files(workdir: Option<&Path>, agent_dir: Option<&Path>) -> Vec<MemoryFile> {
    let mut out = Vec::new();
    if let Some(ws) = workdir {
        for rel in [
            "AGENTS.md",
            "CLAUDE.md",
            "AGENTS.override.md",
            ".pi/APPEND_SYSTEM.md",
            ".pi/SYSTEM.md",
        ] {
            push_file(
                &mut out,
                MemoryGroup::ProjectInstructions,
                rel,
                ws.join(rel),
            );
        }
    }
    let project_dir = match (std::env::var_os(MEMORY_DIR_ENV), workdir, agent_dir) {
        (Some(v), _, _) if !v.is_empty() => Some(PathBuf::from(v)),
        (_, Some(ws), Some(agent)) => Some(project_memory_dir(agent, ws)),
        _ => None,
    };
    if let Some(dir) = &project_dir {
        push_pi_memory(&mut out, MemoryGroup::ProjectMemory, dir);
        let mut sessions = md_files(&dir.join("sessions"));
        sessions.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        for (path, modified) in sessions {
            let label = file_label(&path);
            out.push(MemoryFile {
                group: MemoryGroup::Sessions,
                label,
                path,
                modified,
            });
        }
    }
    if let Some(agent) = agent_dir {
        for name in ["AGENTS.md", "CLAUDE.md", "AGENTS.override.md"] {
            push_file(&mut out, MemoryGroup::User, name, agent.join(name));
        }
        let global = agent.join("memory");
        if project_dir.as_deref() != Some(global.as_path()) {
            push_pi_memory(&mut out, MemoryGroup::Global, &global);
        }
    }
    out
}

fn push_pi_memory(out: &mut Vec<MemoryFile>, group: MemoryGroup, dir: &Path) {
    push_file(out, group, "MEMORY.md", dir.join("MEMORY.md"));
    push_file(out, group, "SCRATCHPAD.md", dir.join("SCRATCHPAD.md"));
    let mut daily = md_files(&dir.join("daily"));
    daily.sort_by(|a, b| b.0.cmp(&a.0));
    for (path, modified) in daily {
        let label = format!("daily/{}", file_label(&path));
        out.push(MemoryFile {
            group,
            label,
            path,
            modified,
        });
    }
}

fn push_file(out: &mut Vec<MemoryFile>, group: MemoryGroup, label: &str, path: PathBuf) {
    if let Ok(meta) = fs::metadata(&path) {
        if meta.is_file() {
            out.push(MemoryFile {
                group,
                label: label.to_owned(),
                path,
                modified: meta.modified().ok(),
            });
        }
    }
}

fn md_files(dir: &Path) -> Vec<(PathBuf, Option<SystemTime>)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("md"))
        .map(|p| {
            let modified = fs::metadata(&p).and_then(|m| m.modified()).ok();
            (p, modified)
        })
        .collect()
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_step::StepSummary;

    fn step(tool: &str, target: &str) -> TraceStep {
        TraceStep {
            tool_call_id: "1".into(),
            tool_name: tool.into(),
            running: false,
            is_error: false,
            duration_ms: Some(5),
            args_preview: None,
            summary: Some(StepSummary {
                target: target.into(),
                diff: None,
            }),
        }
    }

    #[test]
    fn slug_is_stable_and_distinguishes_paths() {
        let a = project_slug(Path::new("/home/u/work/app"));
        let b = project_slug(Path::new("/home/u/other/app"));
        assert!(a.starts_with("app-"));
        assert_ne!(a, b);
        assert_eq!(a, project_slug(Path::new("/home/u/work/app/")));
        assert!(project_slug(Path::new("/tmp/我的 项目")).starts_with("我的-项目-"));
    }

    #[test]
    fn turn_digest_lists_files_and_commands() {
        let steps = vec![
            step("read", "src/a.rs"),
            step("edit", "src/a.rs"),
            step("bash", "cargo test"),
            step("bash", "cargo test"),
        ];
        let turn = TurnDigest {
            session_title: "t",
            workdir: Path::new("/w"),
            user: "fix\nthe bug",
            assistant: "Done.",
            steps: &steps,
        };
        let s = format_turn(&turn, "T".into());
        assert!(s.contains("**问题：** fix the bug"));
        assert!(s.contains("- 读取 `src/a.rs`"));
        assert!(s.contains("- 修改 `src/a.rs`"));
        assert_eq!(s.matches("`cargo test`").count(), 1);
        assert!(s.contains("**结论：** Done."));
    }

    #[test]
    fn append_and_list_memory_files() {
        let root = std::env::temp_dir().join(format!("pl-mem-{}", std::process::id()));
        let ws = root.join("ws");
        let agent = root.join("agent");
        fs::create_dir_all(&ws).unwrap();
        fs::write(ws.join("AGENTS.md"), "rules").unwrap();
        let project = project_memory_dir(&agent, &ws);
        fs::create_dir_all(project.join("daily")).unwrap();
        fs::write(project.join("MEMORY.md"), "m").unwrap();
        fs::write(project.join("daily").join("2026-01-02.md"), "d").unwrap();
        let turn = TurnDigest {
            session_title: "s",
            workdir: &ws,
            user: "q",
            assistant: "a",
            steps: &[],
        };
        let path = session_memory_path(&project, 7);
        append_turn_digest(&path, &turn).unwrap();
        append_turn_digest(&path, &turn).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.matches("# 会话记忆").count(), 1);
        assert_eq!(text.matches("**问题：** q").count(), 2);

        let files = list_memory_files(Some(&ws), Some(&agent));
        let labels: Vec<_> = files.iter().map(|f| (f.group, f.label.as_str())).collect();
        assert!(labels.contains(&(MemoryGroup::ProjectInstructions, "AGENTS.md")));
        assert!(labels.contains(&(MemoryGroup::ProjectMemory, "MEMORY.md")));
        assert!(labels.contains(&(MemoryGroup::ProjectMemory, "daily/2026-01-02.md")));
        assert!(labels.contains(&(MemoryGroup::Sessions, "session-7.md")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn utc_format() {
        assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_utc(1_772_323_200), "2026-03-01 00:00 UTC");
    }
}
