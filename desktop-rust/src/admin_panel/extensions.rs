// SPDX-License-Identifier: Apache-2.0
//! Skills 元数据与 MCP 服务详情：读 SKILL.md 描述、读写 mcp.json 的 args / env、删除服务。

use super::*;
use std::collections::BTreeMap;

/// SKILL.md frontmatter 的 `description`（单行或 `>` / `|` 块）；无 frontmatter 时取首段正文。
pub fn parse_skill_description(text: &str) -> Option<String> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_owned);
    }
    let mut desc: Option<String> = None;
    let mut block = false;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if block {
            if line.starts_with(' ') || line.starts_with('\t') {
                let d = desc.get_or_insert_with(String::new);
                if !d.is_empty() {
                    d.push(' ');
                }
                d.push_str(line.trim());
                continue;
            }
            block = false;
        }
        if let Some(rest) = line.strip_prefix("description:") {
            let rest = rest.trim();
            if matches!(rest, "" | ">" | "|" | ">-" | "|-") {
                block = true;
                desc = Some(String::new());
            } else {
                desc = Some(rest.trim_matches(|c| c == '"' || c == '\'').to_owned());
            }
        }
    }
    desc.filter(|d| !d.is_empty())
}

pub fn skill_descriptions(items: &[ManagedItem]) -> BTreeMap<String, String> {
    items
        .iter()
        .filter_map(|item| {
            let text = fs::read_to_string(item.config_path.as_ref()?).ok()?;
            Some((item.id.clone(), parse_skill_description(&text)?))
        })
        .collect()
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct McpDetail {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub url: Option<String>,
}

pub fn read_mcp_details(path: &Path) -> BTreeMap<String, McpDetail> {
    let Some(val) = fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    else {
        return BTreeMap::new();
    };
    let Some(servers) = val.get("mcpServers").and_then(|v| v.as_object()) else {
        return BTreeMap::new();
    };
    let text = |cfg: &serde_json::Value, key: &str| {
        cfg.get(key).and_then(|v| v.as_str()).map(str::to_owned)
    };
    servers
        .iter()
        .map(|(id, cfg)| {
            let args = cfg
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            let env = cfg
                .get("env")
                .and_then(|v| v.as_object())
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                        .collect()
                })
                .unwrap_or_default();
            let detail = McpDetail {
                command: text(cfg, "command").unwrap_or_default(),
                args,
                env,
                url: text(cfg, "url"),
            };
            (id.clone(), detail)
        })
        .collect()
}

pub fn discover_mcp_details() -> BTreeMap<String, McpDetail> {
    let mut out = BTreeMap::new();
    for path in [
        pi_project_dir().map(|d| d.join("mcp.json")),
        pi_agent_dir().map(|d| d.join("mcp.json")),
    ]
    .into_iter()
    .flatten()
    {
        for (id, detail) in read_mcp_details(&path) {
            out.entry(id).or_insert(detail);
        }
    }
    out
}

/// 每行一个参数。
pub fn parse_args_text(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// 每行 `KEY=VALUE`。
pub fn parse_env_text(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut env = BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("环境变量格式应为 KEY=VALUE：{line}"));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("环境变量名不能为空：{line}"));
        }
        env.insert(key.to_owned(), value.trim().to_owned());
    }
    Ok(env)
}

fn edit_mcp_json(
    path: &Path,
    edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> bool,
) -> io::Result<bool> {
    let content = fs::read_to_string(path)?;
    let mut val: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let Some(servers) = val.get_mut("mcpServers").and_then(|s| s.as_object_mut()) else {
        return Ok(false);
    };
    let changed = edit(servers);
    if changed {
        let out = serde_json::to_string_pretty(&val)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(path, out.as_bytes())?;
    }
    Ok(changed)
}

/// 新增或更新服务的 command / args / env，并打开它。空 args / env 会移除对应字段。
pub fn upsert_mcp_server_full(
    path: &Path,
    id: &str,
    command: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
) -> io::Result<()> {
    upsert_mcp_server(path, id, command)?;
    edit_mcp_json(path, |servers| {
        let Some(entry) = servers.get_mut(id.trim()).and_then(|e| e.as_object_mut()) else {
            return false;
        };
        if args.is_empty() {
            entry.remove("args");
        } else {
            entry.insert("args".to_owned(), serde_json::json!(args));
        }
        if env.is_empty() {
            entry.remove("env");
        } else {
            entry.insert("env".to_owned(), serde_json::json!(env));
        }
        true
    })?;
    Ok(())
}

pub fn remove_mcp_server(path: &Path, id: &str) -> io::Result<bool> {
    edit_mcp_json(path, |servers| servers.remove(id).is_some())
}

/// MCP 页本帧的用户动作（在面板闭包外统一应用）。
#[derive(Debug, Default)]
pub struct McpPageUi {
    pub toggle: Option<String>,
    pub save: bool,
    pub edit: Option<String>,
    pub cancel_edit: bool,
    pub remove_request: Option<String>,
    pub remove: Option<String>,
    pub cancel_remove: bool,
    pub toggle_json: bool,
}

fn agent_mcp_path() -> Option<PathBuf> {
    pi_agent_dir().map(|d| d.join("mcp.json"))
}

fn read_mcp_json_text() -> String {
    match agent_mcp_path() {
        Some(p) => fs::read_to_string(&p).unwrap_or_else(|_| format!("（{} 不存在）", p.display())),
        None => "（找不到 Pi 配置目录）".to_owned(),
    }
}

impl AdminPanelState {
    pub(super) fn open_skill_view(&mut self, path: PathBuf) {
        let text = fs::read_to_string(&path).unwrap_or_else(|e| format!("读取失败：{e}"));
        self.skill_view = Some((path, text));
    }

    fn mcp_path_for(&self, id: &str) -> Option<PathBuf> {
        self.mcp_items
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| i.config_path.clone())
            .or_else(agent_mcp_path)
    }

    fn clear_mcp_form(&mut self) {
        self.mcp_new_id.clear();
        self.mcp_new_command.clear();
        self.mcp_new_args.clear();
        self.mcp_new_env.clear();
        self.mcp_editing = None;
    }

    pub(super) fn reload_mcp(&mut self) {
        self.mcp_items = discover_mcp();
        self.mcp_details = discover_mcp_details();
        if self.mcp_show_json {
            self.mcp_json_text = read_mcp_json_text();
        }
    }

    pub(super) fn apply_mcp_page(&mut self, page: McpPageUi) {
        if let Some(id) = page.toggle {
            if let Err(e) = self.toggle_mcp(&id) {
                self.last_error = Some(e);
            }
        }
        if let Some(id) = page.edit {
            let detail = self.mcp_details.get(&id).cloned().unwrap_or_default();
            self.mcp_new_id = id.clone();
            self.mcp_new_command = detail.command;
            self.mcp_new_args = detail.args.join("\n");
            self.mcp_new_env = detail
                .env
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("\n");
            self.mcp_editing = Some(id);
        }
        if page.cancel_edit {
            self.clear_mcp_form();
        }
        if let Some(id) = page.remove_request {
            self.mcp_confirm_remove = Some(id);
        }
        if page.cancel_remove {
            self.mcp_confirm_remove = None;
        }
        if let Some(id) = page.remove {
            self.mcp_confirm_remove = None;
            match self.mcp_path_for(&id) {
                Some(path) => match remove_mcp_server(&path, &id) {
                    Ok(_) => {
                        if self.mcp_editing.as_deref() == Some(id.as_str()) {
                            self.clear_mcp_form();
                        }
                        self.reload_mcp();
                        self.sidecar_restart_needed = true;
                    }
                    Err(e) => self.last_error = Some(format!("删除 MCP 失败：{e}")),
                },
                None => self.last_error = Some("找不到 mcp.json。".to_owned()),
            }
        }
        if page.toggle_json {
            self.mcp_show_json = !self.mcp_show_json;
            if self.mcp_show_json {
                self.mcp_json_text = read_mcp_json_text();
            }
        }
        if page.save {
            let env = match parse_env_text(&self.mcp_new_env) {
                Ok(env) => env,
                Err(e) => {
                    self.last_error = Some(e);
                    return;
                }
            };
            let args = parse_args_text(&self.mcp_new_args);
            let path = match self.mcp_editing.clone() {
                Some(id) => self.mcp_path_for(&id),
                None => agent_mcp_path(),
            };
            let Some(path) = path else {
                self.last_error = Some("找不到 Pi 配置目录，无法写入 mcp.json。".to_owned());
                return;
            };
            let id = self.mcp_new_id.trim().to_owned();
            match upsert_mcp_server_full(&path, &id, &self.mcp_new_command, &args, &env) {
                Ok(()) => {
                    self.clear_mcp_form();
                    self.reload_mcp();
                    self.sidecar_restart_needed = true;
                }
                Err(e) => self.last_error = Some(format!("保存 MCP 失败：{e}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_description_inline_block_and_plain() {
        let inline = "---\nname: a\ndescription: \"Does X\"\n---\nbody";
        assert_eq!(parse_skill_description(inline).as_deref(), Some("Does X"));
        let block = "---\ndescription: >\n  line one\n  line two\nname: b\n---\n";
        assert_eq!(
            parse_skill_description(block).as_deref(),
            Some("line one line two")
        );
        let plain = "# Title\n\nFirst paragraph.\n";
        assert_eq!(
            parse_skill_description(plain).as_deref(),
            Some("First paragraph.")
        );
    }

    #[test]
    fn mcp_full_upsert_read_and_remove() {
        let dir = std::env::temp_dir().join(format!("pl-mcp-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mcp.json");
        let args = vec!["-y".to_owned(), "server".to_owned()];
        let env = parse_env_text("TOKEN=abc\n\nMODE = fast").unwrap();
        upsert_mcp_server_full(&path, "fs", "npx", &args, &env).unwrap();
        let details = read_mcp_details(&path);
        assert_eq!(details["fs"].command, "npx");
        assert_eq!(details["fs"].args, args);
        assert_eq!(details["fs"].env["MODE"], "fast");
        upsert_mcp_server_full(&path, "fs", "uvx", &[], &BTreeMap::new()).unwrap();
        let details = read_mcp_details(&path);
        assert!(details["fs"].args.is_empty() && details["fs"].env.is_empty());
        assert!(remove_mcp_server(&path, "fs").unwrap());
        assert!(!remove_mcp_server(&path, "fs").unwrap());
        assert!(parse_env_text("NOEQUALS").is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
