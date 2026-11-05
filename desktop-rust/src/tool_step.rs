// SPDX-License-Identifier: Apache-2.0
//! Classify Pi tool calls into timeline steps (verb + target + optional diff).

use serde_json::Value;

const MAX_DIFF_LINES: usize = 40;
const MAX_LINE_CHARS: usize = 200;
const MAX_TARGET_CHARS: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Read,
    Run,
    Edit,
    Write,
    Search,
    Other,
}

impl StepKind {
    pub fn from_tool(tool_name: &str) -> Self {
        match tool_name.to_ascii_lowercase().as_str() {
            "read" | "read_file" | "view" | "cat" => Self::Read,
            "bash" | "shell" | "exec" | "run" | "terminal" => Self::Run,
            "edit" | "multi_edit" | "multiedit" | "str_replace" | "apply_patch" | "patch" => {
                Self::Edit
            }
            "write" | "write_file" | "create" => Self::Write,
            "grep" | "find" | "glob" | "ls" | "search" | "list" => Self::Search,
            _ => Self::Other,
        }
    }

    pub fn verb(self) -> &'static str {
        match self {
            Self::Read => "读取",
            Self::Run => "执行",
            Self::Edit => "修改",
            Self::Write => "写入",
            Self::Search => "搜索",
            Self::Other => "调用",
        }
    }

    pub fn touches_file(self) -> bool {
        matches!(self, Self::Read | Self::Edit | Self::Write)
    }
}

/// Removed / added lines of a file change, capped for display.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepDiff {
    pub removed: Vec<String>,
    pub added: Vec<String>,
    pub removed_total: usize,
    pub added_total: usize,
}

impl StepDiff {
    fn push(&mut self, old: &str, new: &str) {
        push_lines(&mut self.removed, &mut self.removed_total, old);
        push_lines(&mut self.added, &mut self.added_total, new);
    }

    pub fn is_empty(&self) -> bool {
        self.removed_total == 0 && self.added_total == 0
    }

    pub fn hidden_lines(&self) -> usize {
        (self.removed_total + self.added_total)
            .saturating_sub(self.removed.len() + self.added.len())
    }
}

/// Human-readable target of a tool call plus the change it makes, if any.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepSummary {
    pub target: String,
    pub diff: Option<StepDiff>,
}

pub fn summarize(tool_name: &str, args: Option<&Value>) -> Option<StepSummary> {
    let args = args?.as_object()?;
    let kind = StepKind::from_tool(tool_name);
    let text = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| args.get(*k).and_then(Value::as_str))
            .map(str::to_owned)
    };
    let path = text(&["path", "file_path", "filePath", "file"]);
    let target = match kind {
        StepKind::Run => text(&["command", "cmd", "script"]),
        StepKind::Search => match (text(&["pattern", "query", "glob"]), path.clone()) {
            (Some(p), Some(dir)) => Some(format!("{p}  ·  {dir}")),
            (p, dir) => p.or(dir),
        },
        _ => path.clone(),
    }
    .or_else(|| args.values().find_map(Value::as_str).map(str::to_owned))?;
    let target = truncate(first_line(&target), MAX_TARGET_CHARS);

    let diff = match kind {
        StepKind::Edit => {
            let mut diff = StepDiff::default();
            if let Some(edits) = args.get("edits").and_then(Value::as_array) {
                for edit in edits {
                    diff.push(
                        str_field(edit, &["oldText", "old_string", "old"]),
                        str_field(edit, &["newText", "new_string", "new"]),
                    );
                }
            } else {
                let obj = Value::Object(args.clone());
                diff.push(
                    str_field(&obj, &["oldText", "old_string", "old"]),
                    str_field(&obj, &["newText", "new_string", "new"]),
                );
            }
            Some(diff)
        }
        StepKind::Write => {
            let mut diff = StepDiff::default();
            diff.push("", &text(&["content", "text"]).unwrap_or_default());
            Some(diff)
        }
        _ => None,
    }
    .filter(|d| !d.is_empty());
    Some(StepSummary { target, diff })
}

fn str_field<'a>(v: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str))
        .unwrap_or("")
}

fn push_lines(out: &mut Vec<String>, total: &mut usize, text: &str) {
    if text.is_empty() {
        return;
    }
    for line in text.lines() {
        *total += 1;
        if out.len() < MAX_DIFF_LINES {
            out.push(truncate(line, MAX_LINE_CHARS));
        }
    }
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("").trim()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classifies_pi_builtin_tools() {
        assert_eq!(StepKind::from_tool("read"), StepKind::Read);
        assert_eq!(StepKind::from_tool("bash"), StepKind::Run);
        assert_eq!(StepKind::from_tool("edit"), StepKind::Edit);
        assert_eq!(StepKind::from_tool("write"), StepKind::Write);
        assert_eq!(StepKind::from_tool("grep"), StepKind::Search);
        assert_eq!(StepKind::from_tool("mcp_custom"), StepKind::Other);
    }

    #[test]
    fn bash_target_is_first_command_line() {
        let s = summarize("bash", Some(&json!({"command": "cargo test\necho done"}))).unwrap();
        assert_eq!(s.target, "cargo test");
        assert!(s.diff.is_none());
    }

    #[test]
    fn edit_builds_diff_from_old_and_new_text() {
        let args = json!({"path": "src/a.rs", "oldText": "a\nb", "newText": "c"});
        let s = summarize("edit", Some(&args)).unwrap();
        assert_eq!(s.target, "src/a.rs");
        let d = s.diff.unwrap();
        assert_eq!(d.removed, vec!["a", "b"]);
        assert_eq!(d.added, vec!["c"]);
        assert_eq!((d.removed_total, d.added_total), (2, 1));
    }

    #[test]
    fn multi_edit_and_write_are_supported() {
        let args = json!({"path": "x", "edits": [
            {"oldText": "1", "newText": "2"},
            {"oldText": "3", "newText": "4\n5"}
        ]});
        let d = summarize("edit", Some(&args)).unwrap().diff.unwrap();
        assert_eq!((d.removed_total, d.added_total), (2, 3));

        let long: String = (0..100).map(|i| format!("l{i}\n")).collect();
        let w = summarize("write", Some(&json!({"path": "y", "content": long})))
            .unwrap()
            .diff
            .unwrap();
        assert_eq!(w.added_total, 100);
        assert_eq!(w.added.len(), MAX_DIFF_LINES);
        assert_eq!(w.hidden_lines(), 100 - MAX_DIFF_LINES);
    }

    #[test]
    fn search_combines_pattern_and_path() {
        let s = summarize("grep", Some(&json!({"pattern": "fn main", "path": "src"}))).unwrap();
        assert_eq!(s.target, "fn main  ·  src");
    }

    #[test]
    fn missing_or_empty_args_yield_none() {
        assert!(summarize("bash", None).is_none());
        assert!(summarize("bash", Some(&json!({}))).is_none());
    }
}
