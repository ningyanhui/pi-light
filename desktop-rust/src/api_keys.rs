// SPDX-License-Identifier: Apache-2.0
//
// 在设置页写入 Pi auth.json 的 API 密钥。
// 凭据形状与 Pi v1.4.0 一致：{ "<provider>": { "type": "api_key", "key": "..." } }。
// 界面只保留未保存的输入；保存后清空，已存密钥只显示固定掩码，不回显明文。

use std::{fs, io, path::Path};

use eframe::egui;
use serde_json::{Map, Value};

/// 固定掩码。不从已存密钥推导，避免界面持有明文。
pub const STORED_KEY_MASK: &str = "••••••••";

const MAX_KEY_LEN: usize = 4096;

/// 与 Pi provider id / 环境变量对应。`PI_API_KEY` 不是 Pi 的 provider id，不在此列表。
const PROVIDERS: &[(&str, &str, &str)] = &[
    ("anthropic", "Anthropic", "ANTHROPIC_API_KEY"),
    ("openai", "OpenAI", "OPENAI_API_KEY"),
    ("google", "Gemini", "GEMINI_API_KEY"),
    ("deepseek", "DeepSeek", "DEEPSEEK_API_KEY"),
    ("groq", "Groq", "GROQ_API_KEY"),
];

/// 内置厂商目录：`(id, 显示名, 环境变量)`。
pub fn builtin_providers() -> &'static [(&'static str, &'static str, &'static str)] {
    PROVIDERS
}

pub fn is_builtin_provider(provider: &str) -> bool {
    PROVIDERS.iter().any(|(id, _, _)| *id == provider)
}

/// 内置或自定义提供商 ID：字母开头，仅字母数字、短横线、下划线，最长 64。
pub fn validate_provider_id(provider: &str) -> Result<(), String> {
    if provider.is_empty() {
        return Err("Provider ID 不能为空。".to_owned());
    }
    if provider.len() > 64 {
        return Err("Provider ID 过长。".to_owned());
    }
    let mut chars = provider.chars();
    let Some(first) = chars.next() else {
        return Err("Provider ID 不能为空。".to_owned());
    };
    if !first.is_ascii_lowercase() {
        return Err("Provider ID 须以小写字母开头。".to_owned());
    }
    if !provider
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err("Provider ID 只能用小写字母、数字、短横线和下划线。".to_owned());
    }
    Ok(())
}

fn is_writable_provider_id(provider: &str) -> bool {
    validate_provider_id(provider).is_ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPresence {
    Absent,
    /// auth.json 里有非空 api_key。密钥本身不进入本枚举。
    Stored,
    EnvOnly,
    Oauth,
}

pub struct ApiKeyForm {
    pub rows: Vec<ApiKeyRow>,
}

pub struct ApiKeyRow {
    pub provider_id: String,
    pub label: String,
    pub env_var: String,
    pub presence: KeyPresence,
    /// 仅未保存的输入。保存成功后清空。
    pub draft: String,
    pub confirming_clear: bool,
}

#[derive(Debug, Default)]
pub struct KeyUiResult {
    pub restart: bool,
    pub error: Option<String>,
}

impl Default for ApiKeyForm {
    fn default() -> Self {
        Self {
            rows: PROVIDERS
                .iter()
                .map(|(id, label, env_var)| ApiKeyRow {
                    provider_id: (*id).to_owned(),
                    label: (*label).to_owned(),
                    env_var: (*env_var).to_owned(),
                    presence: KeyPresence::Absent,
                    draft: String::new(),
                    confirming_clear: false,
                })
                .collect(),
        }
    }
}

impl std::fmt::Debug for ApiKeyForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.rows.iter().map(|row| {
                format!(
                    "{}:{:?}:has_draft={}",
                    row.provider_id,
                    row.presence,
                    !row.draft.is_empty()
                )
            }))
            .finish()
    }
}

impl ApiKeyForm {
    /// 按文件刷新状态。不读取密钥正文到界面，保留各行未保存的输入。
    pub fn refresh(&mut self, auth_path: Option<&Path>) {
        let on_disk = auth_path.map(scan_presence).unwrap_or_default();
        for row in &mut self.rows {
            let from_file = on_disk
                .get(&row.provider_id)
                .copied()
                .unwrap_or(KeyPresence::Absent);
            row.presence = combine_presence(from_file, env_is_set(&row.env_var));
        }
    }
}

pub fn combine_presence(file: KeyPresence, env_set: bool) -> KeyPresence {
    match file {
        KeyPresence::Absent if env_set => KeyPresence::EnvOnly,
        other => other,
    }
}

pub fn render(ui: &mut egui::Ui, form: &mut ApiKeyForm, auth_path: Option<&Path>) -> KeyUiResult {
    ui.label(egui::RichText::new("API 密钥").strong().size(14.0));
    ui.label(
        egui::RichText::new("写入本机 Pi 的 auth.json。保存后输入框清空，已存密钥只显示掩码。")
            .weak()
            .size(11.0),
    );
    ui.label(
        egui::RichText::new("保存后会重启 Pi。以 ! 开头的内容不会写入。")
            .weak()
            .size(11.0),
    );
    ui.add_space(4.0);

    let mut pending: Option<RowAction> = None;
    for (index, row) in form.rows.iter_mut().enumerate() {
        ui.add_space(6.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&row.label).strong().size(13.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if row.presence == KeyPresence::Stored {
                    if row.confirming_clear {
                        if ui.small_button("取消").clicked() {
                            pending = Some(RowAction::CancelClear(index));
                        }
                        if ui.small_button("确认清除").clicked() {
                            pending = Some(RowAction::Clear(index));
                        }
                    } else if ui.small_button("清除").clicked() {
                        pending = Some(RowAction::ArmClear(index));
                    }
                }
            });
        });
        match row.presence {
            KeyPresence::Stored => {
                ui.colored_label(
                    egui::Color32::from_rgb(60, 160, 80),
                    format!("已保存 · {STORED_KEY_MASK}"),
                );
            }
            KeyPresence::EnvOnly => {
                ui.label(
                    egui::RichText::new(format!(
                        "未写入文件 · 环境变量 {} 已设置（不显示值）",
                        row.env_var
                    ))
                    .weak()
                    .size(11.0),
                );
            }
            KeyPresence::Oauth => {
                ui.label(
                    egui::RichText::new("OAuth 已登录 · 不显示令牌 · 本页不覆盖")
                        .weak()
                        .size(11.0),
                );
            }
            KeyPresence::Absent => {
                ui.label(egui::RichText::new("未配置").weak().size(11.0));
            }
        }
        if row.presence != KeyPresence::Oauth {
            let hint = if row.presence == KeyPresence::Stored {
                "输入新密钥以替换"
            } else {
                "输入 API 密钥"
            };
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 68.0).max(80.0);
                ui.add_sized(
                    [width, 24.0],
                    egui::TextEdit::singleline(&mut row.draft)
                        .password(true)
                        .hint_text(hint),
                );
                let enabled = !row.draft.trim().is_empty();
                if ui.add_enabled(enabled, egui::Button::new("保存")).clicked() {
                    pending = Some(RowAction::Save(index));
                }
            });
        }
    }

    apply_action(form, auth_path, pending)
}

enum RowAction {
    Save(usize),
    Clear(usize),
    ArmClear(usize),
    CancelClear(usize),
}

fn apply_action(
    form: &mut ApiKeyForm,
    auth_path: Option<&Path>,
    pending: Option<RowAction>,
) -> KeyUiResult {
    let Some(action) = pending else {
        return KeyUiResult::default();
    };
    match action {
        RowAction::ArmClear(index) => {
            for (i, row) in form.rows.iter_mut().enumerate() {
                row.confirming_clear = i == index;
            }
            KeyUiResult::default()
        }
        RowAction::CancelClear(index) => {
            if let Some(row) = form.rows.get_mut(index) {
                row.confirming_clear = false;
            }
            KeyUiResult::default()
        }
        RowAction::Save(index) => match save_row(form, index, auth_path) {
            Ok(()) => KeyUiResult {
                restart: true,
                error: None,
            },
            Err(error) => KeyUiResult {
                restart: false,
                error: Some(error),
            },
        },
        RowAction::Clear(index) => match clear_row(form, index, auth_path) {
            Ok(()) => KeyUiResult {
                restart: true,
                error: None,
            },
            Err(error) => KeyUiResult {
                restart: false,
                error: Some(error),
            },
        },
    }
}

pub fn clear_row_at(
    form: &mut ApiKeyForm,
    index: usize,
    auth_path: Option<&Path>,
) -> Result<(), String> {
    clear_row(form, index, auth_path)
}

fn save_row(form: &mut ApiKeyForm, index: usize, auth_path: Option<&Path>) -> Result<(), String> {
    let row = form
        .rows
        .get(index)
        .ok_or_else(|| "提供商不存在".to_owned())?;
    if row.presence == KeyPresence::Oauth {
        return Err("该提供商已有 OAuth 登录，未覆盖。".to_owned());
    }
    let key = row.draft.trim().to_owned();
    let provider = row.provider_id.clone();
    let path = auth_path.ok_or_else(|| "未找到 Pi agent 目录，无法写入 auth.json".to_owned())?;
    upsert_api_key(path, &provider, &key)?;
    if let Some(row) = form.rows.get_mut(index) {
        wipe_secret(&mut row.draft);
        row.presence = KeyPresence::Stored;
        row.confirming_clear = false;
    }
    Ok(())
}

fn clear_row(form: &mut ApiKeyForm, index: usize, auth_path: Option<&Path>) -> Result<(), String> {
    let row = form
        .rows
        .get(index)
        .ok_or_else(|| "提供商不存在".to_owned())?;
    let provider = row.provider_id.clone();
    let env_var = row.env_var.clone();
    let path = auth_path.ok_or_else(|| "未找到 Pi agent 目录，无法写入 auth.json".to_owned())?;
    delete_api_key(path, &provider)?;
    if let Some(row) = form.rows.get_mut(index) {
        row.presence = combine_presence(KeyPresence::Absent, env_is_set(&env_var));
        row.confirming_clear = false;
    }
    Ok(())
}

pub fn upsert_api_key(path: &Path, provider: &str, key: &str) -> Result<(), String> {
    validate_literal_key(key)?;
    if !is_writable_provider_id(provider) {
        return Err("提供商 ID 不合法，未写入。".to_owned());
    }
    let mut root = read_root(path)?;
    if matches!(
        root.get(provider).map(classify_credential),
        Some(KeyPresence::Oauth)
    ) {
        return Err("该提供商已有 OAuth 登录，未覆盖。请先在 Pi 里 /logout。".to_owned());
    }
    let env = root
        .get(provider)
        .and_then(|value| value.get("env"))
        .filter(|value| value.is_object())
        .cloned();
    let mut cred = Map::new();
    cred.insert("type".to_owned(), Value::String("api_key".to_owned()));
    cred.insert("key".to_owned(), Value::String(key.to_owned()));
    if let Some(env) = env {
        cred.insert("env".to_owned(), env);
    }
    root.insert(provider.to_owned(), Value::Object(cred));
    write_root(path, &root)
}

pub fn delete_api_key(path: &Path, provider: &str) -> Result<(), String> {
    if !is_writable_provider_id(provider) {
        return Err("提供商 ID 不合法，未改写。".to_owned());
    }
    let mut root = read_root(path)?;
    if matches!(
        root.get(provider).map(classify_credential),
        Some(KeyPresence::Oauth)
    ) {
        return Err("OAuth 登录不在此清除。".to_owned());
    }
    if root.remove(provider).is_none() {
        return Ok(());
    }
    write_root(path, &root)
}

fn validate_literal_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("密钥为空，未写入。".to_owned());
    }
    if key.chars().count() > MAX_KEY_LEN {
        return Err("密钥过长，未写入。".to_owned());
    }
    if key.starts_with('!') {
        return Err("以 ! 开头的值会被 Pi 当成命令执行，未写入。".to_owned());
    }
    if key.contains('\n') || key.contains('\r') || key.contains('\0') {
        return Err("密钥不能包含换行，未写入。".to_owned());
    }
    Ok(())
}

fn scan_presence(path: &Path) -> std::collections::BTreeMap<String, KeyPresence> {
    let Ok(root) = read_root(path) else {
        return std::collections::BTreeMap::new();
    };
    root.into_iter()
        .map(|(provider, value)| (provider, classify_credential(&value)))
        .collect()
}

fn classify_credential(value: &Value) -> KeyPresence {
    let Some(obj) = value.as_object() else {
        return KeyPresence::Absent;
    };
    match obj.get("type").and_then(Value::as_str) {
        Some("oauth") => KeyPresence::Oauth,
        Some("api_key") => {
            let nonempty = obj
                .get("key")
                .and_then(Value::as_str)
                .is_some_and(|key| !key.is_empty());
            if nonempty {
                KeyPresence::Stored
            } else {
                KeyPresence::Absent
            }
        }
        _ => KeyPresence::Absent,
    }
}

fn read_root(path: &Path) -> Result<Map<String, Value>, String> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let text = fs::read_to_string(path).map_err(|err| format!("读取 auth.json 失败：{err}"))?;
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    // serde 的错误信息可能带出文件片段（含密钥），这里不把解析错误原文给界面。
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err("auth.json 不是对象，未改写。".to_owned()),
        Err(_) => Err("auth.json 无法解析，未改写。".to_owned()),
    }
}

fn write_root(path: &Path, root: &Map<String, Value>) -> Result<(), String> {
    let text = serde_json::to_string_pretty(root).map_err(|_| "无法序列化 auth.json".to_owned())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建目录失败：{err}"))?;
    }
    write_private(path, text.as_bytes()).map_err(|err| format!("写入 auth.json 失败：{err}"))
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path)?;
    io::Write::write_all(&mut file, bytes)?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn env_is_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn wipe_secret(value: &mut String) {
    value.clear();
    value.shrink_to_fit();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_is_constant() {
        assert_eq!(STORED_KEY_MASK, "••••••••");
    }

    #[test]
    fn combine_presence_prefers_file_over_env() {
        assert_eq!(
            combine_presence(KeyPresence::Absent, true),
            KeyPresence::EnvOnly
        );
        assert_eq!(
            combine_presence(KeyPresence::Stored, true),
            KeyPresence::Stored
        );
        assert_eq!(
            combine_presence(KeyPresence::Oauth, true),
            KeyPresence::Oauth
        );
        assert_eq!(
            combine_presence(KeyPresence::Absent, false),
            KeyPresence::Absent
        );
    }

    #[test]
    fn debug_form_hides_draft_and_stored_secret() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        fs::write(
            &path,
            r#"{"openai":{"type":"api_key","key":"sk-test-secret-xyz"}}"#,
        )
        .unwrap();
        let mut form = ApiKeyForm::default();
        form.rows[0].draft = "sk-draft-secret".to_owned();
        form.refresh(Some(&path));
        let openai = form
            .rows
            .iter()
            .find(|row| row.provider_id == "openai")
            .unwrap();
        assert_eq!(openai.presence, KeyPresence::Stored);
        assert!(openai.draft.is_empty());
        assert_eq!(form.rows[0].draft, "sk-draft-secret");
        let rendered = format!("{form:?}");
        assert!(!rendered.contains("sk-test-secret-xyz"));
        assert!(!rendered.contains("sk-draft-secret"));
        assert!(rendered.contains("has_draft=true"));
    }

    #[test]
    fn upsert_merges_preserves_env_and_other_provider() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        fs::write(
            &path,
            r#"{
                "openai": {"type": "api_key", "key": "sk-old", "env": {"FOO": "bar"}},
                "groq": {"type": "oauth", "access": "a", "refresh": "b", "expires": 1}
            }"#,
        )
        .unwrap();
        upsert_api_key(&path, "openai", "sk-new-value").unwrap();
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["openai"]["type"], "api_key");
        assert_eq!(value["openai"]["key"], "sk-new-value");
        assert_eq!(value["openai"]["env"]["FOO"], "bar");
        assert_eq!(value["groq"]["type"], "oauth");
        assert_eq!(value["groq"]["access"], "a");
    }

    #[test]
    fn oauth_is_not_overwritten_and_error_hides_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let original =
            r#"{"groq":{"type":"oauth","access":"access-secret","refresh":"r","expires":1}}"#;
        fs::write(&path, original).unwrap();
        let err = upsert_api_key(&path, "groq", "sk-should-not-land").unwrap_err();
        assert!(!err.contains("sk-should-not-land"));
        assert!(!err.contains("access-secret"));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn rejects_command_prefix_and_newline_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing").join("auth.json");
        assert!(upsert_api_key(&path, "openai", "!echo hi").is_err());
        assert!(upsert_api_key(&path, "openai", "sk\nleak").is_err());
        assert!(upsert_api_key(&path, "not a provider", "sk-abc").is_err());
        assert!(upsert_api_key(&path, "OpenAI", "sk-abc").is_err());
        assert!(!path.exists());
    }

    #[test]
    fn upsert_allows_custom_provider_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        upsert_api_key(&path, "my-gateway", "sk-custom-key").unwrap();
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["my-gateway"]["type"], "api_key");
        assert_eq!(value["my-gateway"]["key"], "sk-custom-key");
    }

    #[test]
    fn validate_provider_id_rules() {
        assert!(validate_provider_id("openai").is_ok());
        assert!(validate_provider_id("my-gateway").is_ok());
        assert!(validate_provider_id("1abc").is_err());
        assert!(validate_provider_id("a/b").is_err());
        assert!(validate_provider_id("OpenAI").is_err());
        assert!(is_builtin_provider("anthropic"));
        assert!(!is_builtin_provider("my-gateway"));
    }

    #[test]
    fn parse_error_does_not_echo_file_body() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        fs::write(&path, r#"{"openai":{"key":"sk-leak-me"}"#).unwrap();
        let err = upsert_api_key(&path, "anthropic", "sk-new").unwrap_err();
        assert!(!err.contains("sk-leak-me"));
        assert!(!err.contains("sk-new"));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            r#"{"openai":{"key":"sk-leak-me"}"#
        );
    }

    #[test]
    fn delete_removes_only_that_provider() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        fs::write(
            &path,
            r#"{"openai":{"type":"api_key","key":"sk-a"},"groq":{"type":"api_key","key":"sk-b"}}"#,
        )
        .unwrap();
        delete_api_key(&path, "openai").unwrap();
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(value.get("openai").is_none());
        assert_eq!(value["groq"]["key"], "sk-b");
    }

    #[test]
    #[cfg(unix)]
    fn auth_file_mode_is_user_rw_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        upsert_api_key(&path, "anthropic", "sk-mode").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
