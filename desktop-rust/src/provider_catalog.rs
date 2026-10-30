// SPDX-License-Identifier: Apache-2.0
//
// M0 模型 IA：内置厂商卡片 + 自定义 API（对齐 prototypical 厂商/自定义双 Tab）。
// 自定义端点写入 Pi `models.json` 的 `providers` 对象；密钥仍走 auth.json，
// 保存后不回显明文，不覆盖 OAuth。

use std::{fs, io, path::Path};

use eframe::egui;
use serde_json::{json, Map, Value};

use crate::api_keys::{self, ApiKeyForm, KeyPresence, KeyUiResult, STORED_KEY_MASK};

/// Pi `models.json` 支持的 API 协议（界面文案，值为写入文件的 id）。
pub const API_PROTOCOLS: &[(&str, &str)] = &[
    ("openai-completions", "OpenAI Chat Completions"),
    ("openai-responses", "OpenAI Responses"),
    ("anthropic-messages", "Anthropic Messages"),
    ("google-generative-ai", "Google Generative AI"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AddProviderTab {
    #[default]
    Builtin,
    Custom,
}

impl AddProviderTab {
    pub fn label(self) -> &'static str {
        match self {
            AddProviderTab::Builtin => "第三方模型提供商",
            AddProviderTab::Custom => "自定义模型 API",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CustomProvider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api: String,
    pub models: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AddProviderForm {
    pub open: bool,
    pub tab: AddProviderTab,
    /// 编辑已有自定义提供商时锁定 ID。
    pub editing_custom: bool,
    pub builtin_provider: String,
    pub builtin_key: String,
    pub custom_id: String,
    pub custom_name: String,
    pub custom_base_url: String,
    pub custom_api: String,
    pub custom_key: String,
    pub custom_models: Vec<String>,
    pub custom_model_draft: String,
    pub fetch_hint: Option<String>,
}

impl Default for AddProviderForm {
    fn default() -> Self {
        Self {
            open: false,
            tab: AddProviderTab::Builtin,
            editing_custom: false,
            builtin_provider: api_keys::builtin_providers()[0].0.to_owned(),
            builtin_key: String::new(),
            custom_id: String::new(),
            custom_name: String::new(),
            custom_base_url: String::new(),
            custom_api: "openai-completions".to_owned(),
            custom_key: String::new(),
            custom_models: Vec::new(),
            custom_model_draft: String::new(),
            fetch_hint: None,
        }
    }
}

impl AddProviderForm {
    pub fn open_builtin(&mut self, provider_id: Option<&str>) {
        wipe_secret(&mut self.builtin_key);
        wipe_secret(&mut self.custom_key);
        self.open = true;
        self.tab = AddProviderTab::Builtin;
        self.editing_custom = false;
        self.fetch_hint = None;
        if let Some(id) = provider_id {
            self.builtin_provider = id.to_owned();
        }
    }

    pub fn open_custom_new(&mut self) {
        wipe_secret(&mut self.builtin_key);
        wipe_secret(&mut self.custom_key);
        self.open = true;
        self.tab = AddProviderTab::Custom;
        self.editing_custom = false;
        self.custom_id.clear();
        self.custom_name.clear();
        self.custom_base_url = "https://".to_owned();
        self.custom_api = "openai-completions".to_owned();
        self.custom_models.clear();
        self.custom_model_draft.clear();
        self.fetch_hint = None;
    }

    pub fn open_custom_edit(&mut self, provider: &CustomProvider) {
        wipe_secret(&mut self.builtin_key);
        wipe_secret(&mut self.custom_key);
        self.open = true;
        self.tab = AddProviderTab::Custom;
        self.editing_custom = true;
        self.custom_id = provider.id.clone();
        self.custom_name = provider.name.clone();
        self.custom_base_url = provider.base_url.clone();
        self.custom_api = provider.api.clone();
        self.custom_models = provider.models.clone();
        self.custom_model_draft.clear();
        self.fetch_hint = None;
    }

    pub fn close(&mut self) {
        wipe_secret(&mut self.builtin_key);
        wipe_secret(&mut self.custom_key);
        self.open = false;
        self.fetch_hint = None;
    }
}

#[derive(Debug, Default)]
pub struct ModelsTabResult {
    pub set_model: Option<(String, String)>,
    pub restart: bool,
    pub error: Option<String>,
    pub write_default: bool,
}

/// `provider/modelId` → (provider, modelId)。
pub fn parse_model_spec(spec: &str) -> Option<(String, String)> {
    let spec = spec.trim();
    let (provider, model) = spec.split_once('/')?;
    if provider.is_empty() || model.is_empty() {
        return None;
    }
    Some((provider.to_owned(), model.to_owned()))
}

pub fn current_model_spec(
    live: Option<&str>,
    default_provider: Option<&str>,
    default_model: Option<&str>,
) -> String {
    if let Some(live) = live.filter(|s| !s.is_empty()) {
        return live.to_owned();
    }
    match (default_provider, default_model) {
        (Some(p), Some(m)) if !p.is_empty() && !m.is_empty() => format!("{p}/{m}"),
        _ => String::new(),
    }
}

/// 模型下拉。返回新选中的 `provider/modelId`。
pub fn show_model_combo(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    current: &str,
    models: &[String],
    width: f32,
) -> Option<String> {
    let mut selected: Option<String> = None;
    let label = if current.is_empty() {
        "尚未选择".to_owned()
    } else {
        current.to_owned()
    };
    let mut choices: Vec<String> = models.to_vec();
    if !current.is_empty() && !choices.iter().any(|m| m == current) {
        choices.insert(0, current.to_owned());
    }
    egui::ComboBox::from_id_salt(id_salt)
        .width(width)
        .selected_text(truncate_chars(&label, 42))
        .show_ui(ui, |ui| {
            if choices.is_empty() {
                ui.weak("连上 Pi 后会列出可用模型");
                return;
            }
            for model in &choices {
                if ui.selectable_label(model == current, model).clicked() {
                    selected = Some(model.clone());
                }
            }
        });
    selected.filter(|m| m != current)
}

pub fn protocol_label(api: &str) -> String {
    API_PROTOCOLS
        .iter()
        .find(|(id, _)| *id == api)
        .map(|(_, label)| (*label).to_owned())
        .unwrap_or_else(|| api.to_owned())
}

pub fn load_providers(path: &Path) -> Vec<CustomProvider> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(val) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    parse_providers(&val)
}

pub fn parse_providers(val: &Value) -> Vec<CustomProvider> {
    if let Some(obj) = val.get("providers").and_then(Value::as_object) {
        let mut out: Vec<CustomProvider> = obj
            .iter()
            .map(|(id, cfg)| CustomProvider {
                id: id.clone(),
                name: cfg
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_owned(),
                base_url: cfg
                    .get("baseUrl")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                api: cfg
                    .get("api")
                    .and_then(Value::as_str)
                    .unwrap_or("openai-completions")
                    .to_owned(),
                models: cfg
                    .get("models")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|item| {
                                item.get("id")
                                    .and_then(Value::as_str)
                                    .map(str::to_owned)
                                    .or_else(|| item.as_str().map(str::to_owned))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        return out;
    }
    // 旧版数组：[{ "name", "baseUrl" }]
    if let Some(arr) = val.as_array() {
        return arr
            .iter()
            .filter_map(|item| {
                let name = item.get("name")?.as_str()?.to_owned();
                let base_url = item.get("baseUrl")?.as_str()?.to_owned();
                Some(CustomProvider {
                    id: slug_from_name(&name),
                    name,
                    base_url,
                    api: "openai-completions".to_owned(),
                    models: Vec::new(),
                })
            })
            .collect();
    }
    Vec::new()
}

pub fn upsert_custom_provider(path: &Path, provider: &CustomProvider) -> io::Result<()> {
    validate_custom_provider(provider)
        .map_err(|msg| io::Error::new(io::ErrorKind::InvalidInput, msg))?;

    let mut root = read_models_root(path)?;
    let providers = root
        .entry("providers".to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !providers.is_object() {
        *providers = Value::Object(Map::new());
    }
    let providers = providers
        .as_object_mut()
        .expect("providers was forced to object");

    let mut entry = Map::new();
    let display = if provider.name.trim().is_empty() {
        provider.id.as_str()
    } else {
        provider.name.trim()
    };
    entry.insert("name".to_owned(), Value::String(display.to_owned()));
    entry.insert(
        "baseUrl".to_owned(),
        Value::String(provider.base_url.trim().to_owned()),
    );
    entry.insert("api".to_owned(), Value::String(provider.api.clone()));
    let models: Vec<Value> = provider
        .models
        .iter()
        .filter(|id| !id.trim().is_empty())
        .map(|id| json!({ "id": id.trim() }))
        .collect();
    entry.insert("models".to_owned(), Value::Array(models));
    providers.insert(provider.id.clone(), Value::Object(entry));

    write_models_root(path, &root)
}

pub fn delete_custom_provider(path: &Path, id: &str) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut root = read_models_root(path)?;
    if let Some(providers) = root.get_mut("providers").and_then(Value::as_object_mut) {
        providers.remove(id);
    }
    write_models_root(path, &root)
}

pub fn validate_custom_provider(provider: &CustomProvider) -> Result<(), String> {
    api_keys::validate_provider_id(&provider.id)?;
    if api_keys::is_builtin_provider(&provider.id) {
        return Err("该 ID 是内置提供商，请改用「第三方模型提供商」页。".to_owned());
    }
    validate_base_url(&provider.base_url)?;
    if provider.models.iter().all(|m| m.trim().is_empty()) {
        return Err("至少添加一个模型 ID。".to_owned());
    }
    if !API_PROTOCOLS.iter().any(|(id, _)| *id == provider.api) {
        return Err("不支持的 API 协议。".to_owned());
    }
    Ok(())
}

pub fn validate_base_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("请填写 API 地址。".to_owned());
    }
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("API 地址须以 http:// 或 https:// 开头。".to_owned());
    }
    if url.contains('\n') || url.contains('\r') || url.contains('\0') {
        return Err("API 地址不能包含换行。".to_owned());
    }
    Ok(())
}

fn read_models_root(path: &Path) -> io::Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let text = fs::read_to_string(path)?;
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "models.json 不是对象",
        )),
        Err(err) => Err(io::Error::new(io::ErrorKind::InvalidData, err)),
    }
}

fn write_models_root(path: &Path, root: &Map<String, Value>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = root.clone();
    out.entry("$schema".to_owned())
        .or_insert_with(|| Value::String("https://pi.dev/schemas/models.schema.json".to_owned()));
    let text = serde_json::to_string_pretty(&Value::Object(out))
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    fs::write(path, text.as_bytes())
}

fn slug_from_name(name: &str) -> String {
    let mut slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-').to_owned();
    if slug.is_empty() {
        "custom".to_owned()
    } else {
        slug
    }
}

fn wipe_secret(value: &mut String) {
    value.clear();
    value.shrink_to_fit();
}

fn truncate_chars(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_owned()
    } else {
        format!("{}…", chars[..max].iter().collect::<String>())
    }
}

fn presence_caption(presence: KeyPresence, env_var: &str) -> (String, egui::Color32) {
    match presence {
        KeyPresence::Stored => (
            format!("已配置 · {STORED_KEY_MASK}"),
            egui::Color32::from_rgb(90, 180, 110),
        ),
        KeyPresence::EnvOnly => (
            format!("环境变量 {env_var}"),
            egui::Color32::from_rgb(90, 180, 110),
        ),
        KeyPresence::Oauth => (
            "OAuth 已登录 · 本页不覆盖".to_owned(),
            egui::Color32::from_rgb(90, 160, 200),
        ),
        KeyPresence::Absent => ("未配置".to_owned(), egui::Color32::from_rgb(200, 150, 70)),
    }
}

pub struct ModelsTabInput<'a> {
    pub form: &'a mut ApiKeyForm,
    pub add: &'a mut AddProviderForm,
    pub custom: &'a [CustomProvider],
    pub current_spec: &'a str,
    pub available_models: &'a [String],
    pub auth_path: Option<&'a Path>,
    pub models_path: Option<&'a Path>,
}

/// 模型 Tab：默认模型下拉 + 厂商卡片 + 添加提供商。
pub fn render_models_tab(ui: &mut egui::Ui, input: ModelsTabInput<'_>) -> ModelsTabResult {
    let ModelsTabInput {
        form,
        add,
        custom,
        current_spec,
        available_models,
        auth_path,
        models_path,
    } = input;
    let mut result = ModelsTabResult::default();

    ui.label(egui::RichText::new("默认模型").strong().size(14.0));
    ui.label(
        egui::RichText::new("新对话将默认使用此模型。")
            .weak()
            .size(11.0),
    );
    ui.add_space(4.0);
    if let Some(spec) = show_model_combo(
        ui,
        "settings_default_model",
        current_spec,
        available_models,
        360.0,
    ) {
        if let Some((provider, model_id)) = parse_model_spec(&spec) {
            result.set_model = Some((provider, model_id));
            result.write_default = true;
        }
    }

    ui.add_space(14.0);
    ui.label(egui::RichText::new("模型提供商").strong().size(14.0));
    ui.label(
        egui::RichText::new("配置和管理您的模型提供商及 API 密钥。保存后只显示掩码，不回显明文。")
            .weak()
            .size(11.0),
    );
    ui.add_space(8.0);

    let mut configure_builtin: Option<String> = None;
    let mut clear_builtin: Option<String> = None;
    let mut cancel_clear: Option<String> = None;
    let mut edit_custom: Option<String> = None;
    let mut delete_custom: Option<String> = None;

    for row in &mut form.rows {
        let (caption, color) = presence_caption(row.presence, &row.env_var);
        provider_card(ui, &row.label, &caption, color, |ui| {
            if row.presence == KeyPresence::Oauth {
                ui.label(egui::RichText::new("OAuth").weak().size(11.0));
            } else if row.presence == KeyPresence::Stored {
                if row.confirming_clear {
                    if ui.small_button("取消").clicked() {
                        cancel_clear = Some(row.provider_id.clone());
                    }
                    if ui.small_button("确认清除").clicked() {
                        clear_builtin = Some(row.provider_id.clone());
                    }
                } else {
                    if ui.small_button("清除").clicked() {
                        clear_builtin = Some(format!("arm:{}", row.provider_id));
                    }
                    if ui.small_button("配置").clicked() {
                        configure_builtin = Some(row.provider_id.clone());
                    }
                }
            } else if ui.small_button("配置").clicked() {
                configure_builtin = Some(row.provider_id.clone());
            }
        });
        ui.add_space(6.0);
    }

    if !custom.is_empty() {
        ui.add_space(6.0);
        ui.label(egui::RichText::new("自定义 API").strong().size(13.0));
        ui.add_space(4.0);
        for provider in custom {
            let subtitle = if provider.models.is_empty() {
                provider.base_url.clone()
            } else {
                format!("{} · {}", provider.base_url, provider.models.join(", "))
            };
            let title = if provider.name.is_empty() {
                provider.id.clone()
            } else {
                format!("{} ({})", provider.name, provider.id)
            };
            provider_card(
                ui,
                &title,
                &subtitle,
                egui::Color32::from_rgb(160, 170, 190),
                |ui| {
                    if ui.small_button("删除").clicked() {
                        delete_custom = Some(provider.id.clone());
                    }
                    if ui.small_button("编辑").clicked() {
                        edit_custom = Some(provider.id.clone());
                    }
                },
            );
            ui.add_space(6.0);
        }
    }

    ui.add_space(8.0);
    if ui
        .add(egui::Button::new("+ 添加模型提供商").min_size([ui.available_width(), 32.0].into()))
        .clicked()
    {
        add.open_builtin(None);
    }

    if let Some(id) = configure_builtin {
        add.open_builtin(Some(&id));
    }
    if let Some(id) = edit_custom {
        if let Some(provider) = custom.iter().find(|p| p.id == id) {
            add.open_custom_edit(provider);
        }
    }
    if let Some(token) = clear_builtin {
        if let Some(id) = token.strip_prefix("arm:") {
            for row in &mut form.rows {
                row.confirming_clear = row.provider_id == id;
            }
        } else {
            let key_result = clear_named(form, auth_path, &token);
            merge_key_result(&mut result, key_result);
        }
    }
    if let Some(id) = cancel_clear {
        for row in &mut form.rows {
            if row.provider_id == id {
                row.confirming_clear = false;
            }
        }
    }
    if let Some(id) = delete_custom {
        if let Some(path) = models_path {
            match delete_custom_provider(path, &id) {
                Ok(()) => {
                    if let Some(auth) = auth_path {
                        let _ = api_keys::delete_api_key(auth, &id);
                    }
                    result.restart = true;
                }
                Err(err) => result.error = Some(format!("删除自定义提供商失败：{err}")),
            }
        } else {
            result.error = Some("未找到 models.json 写入路径。".to_owned());
        }
    }

    result
}

fn clear_named(form: &mut ApiKeyForm, auth_path: Option<&Path>, id: &str) -> KeyUiResult {
    let Some(index) = form.rows.iter().position(|row| row.provider_id == id) else {
        return KeyUiResult {
            restart: false,
            error: Some("提供商不存在".to_owned()),
        };
    };
    match api_keys::clear_row_at(form, index, auth_path) {
        Ok(()) => KeyUiResult {
            restart: true,
            error: None,
        },
        Err(error) => KeyUiResult {
            restart: false,
            error: Some(error),
        },
    }
}

fn merge_key_result(out: &mut ModelsTabResult, key: KeyUiResult) {
    if key.restart {
        out.restart = true;
    }
    if out.error.is_none() {
        out.error = key.error;
    }
}

fn provider_card(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    subtitle_color: egui::Color32,
    trailing: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .rounding(egui::Rounding::same(8.0))
        .stroke(egui::Stroke::new(
            1.0_f32,
            egui::Color32::from_rgb(48, 52, 68),
        ))
        .fill(ui.visuals().extreme_bg_color)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(title).strong().size(14.0));
                    ui.label(
                        egui::RichText::new(subtitle)
                            .color(subtitle_color)
                            .size(11.0),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), trailing);
            });
        });
}

/// 「添加模型提供商」对话框。
pub fn render_add_provider(
    ctx: &egui::Context,
    add: &mut AddProviderForm,
    form: &mut ApiKeyForm,
    auth_path: Option<&Path>,
    models_path: Option<&Path>,
) -> ModelsTabResult {
    let mut result = ModelsTabResult::default();
    if !add.open {
        return result;
    }

    let mut still_open = true;
    let mut want_save = false;
    let mut want_cancel = false;
    let mut add_model = false;
    let mut fetch_models = false;
    let mut remove_model: Option<usize> = None;

    egui::Window::new("添加模型提供商")
        .id(egui::Id::new("pi_light_add_provider"))
        .open(&mut still_open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Tooltip)
        .default_size(egui::vec2(520.0, 480.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for tab in [AddProviderTab::Builtin, AddProviderTab::Custom] {
                    let selected = add.tab == tab;
                    if ui
                        .add_sized(
                            [230.0, 28.0],
                            egui::SelectableLabel::new(selected, tab.label()),
                        )
                        .clicked()
                    {
                        add.tab = tab;
                    }
                }
            });
            ui.add_space(8.0);

            match add.tab {
                AddProviderTab::Builtin => {
                    ui.label(
                        egui::RichText::new(
                            "从内置目录中选择 OpenAI、Anthropic、DeepSeek 等提供商，填入其 API 密钥即可使用。",
                        )
                        .weak()
                        .size(12.0),
                    );
                    ui.add_space(10.0);
                    ui.label("提供商");
                    let current_label = api_keys::builtin_providers()
                        .iter()
                        .find(|(id, _, _)| *id == add.builtin_provider)
                        .map(|(_, label, _)| *label)
                        .unwrap_or(add.builtin_provider.as_str());
                    egui::ComboBox::from_id_salt("add_builtin_provider")
                        .selected_text(current_label)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (id, label, _) in api_keys::builtin_providers() {
                                if ui
                                    .selectable_label(&add.builtin_provider == id, *label)
                                    .clicked()
                                {
                                    add.builtin_provider = (*id).to_owned();
                                }
                            }
                        });
                    ui.add_space(10.0);
                    let env_var = api_keys::builtin_providers()
                        .iter()
                        .find(|(id, _, _)| *id == add.builtin_provider)
                        .map(|(_, _, env)| *env)
                        .unwrap_or("");
                    ui.horizontal(|ui| {
                        ui.label("API 密钥");
                        ui.weak("?");
                    });
                    ui.add(
                        egui::TextEdit::singleline(&mut add.builtin_key)
                            .password(true)
                            .hint_text(format!("留空使用环境认证 ({env_var})"))
                            .desired_width(f32::INFINITY),
                    );
                    let env_set = std::env::var_os(env_var).is_some_and(|v| !v.is_empty());
                    if add.builtin_key.trim().is_empty() && !env_set {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 160, 70),
                            format!("未检测到环境变量。请填写 API 密钥，或设置环境变量 {env_var}。"),
                        );
                    } else if env_set && add.builtin_key.trim().is_empty() {
                        ui.label(
                            egui::RichText::new(format!("将使用环境变量 {env_var}（不显示值）。"))
                                .weak()
                                .size(11.0),
                        );
                    }
                    if let Some(row) = form
                        .rows
                        .iter()
                        .find(|row| row.provider_id == add.builtin_provider)
                    {
                        if row.presence == KeyPresence::Oauth {
                            ui.colored_label(
                                egui::Color32::from_rgb(220, 160, 70),
                                "该提供商已有 OAuth 登录，保存不会覆盖。",
                            );
                        } else if row.presence == KeyPresence::Stored {
                            ui.label(
                                egui::RichText::new(format!("已保存 · {STORED_KEY_MASK}"))
                                    .weak()
                                    .size(11.0),
                            );
                        }
                    }
                    ui.add_space(8.0);
                    egui::CollapsingHeader::new("自定义设置")
                        .default_open(false)
                        .id_salt("add_builtin_advanced")
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(
                                    "内置提供商的地址由 Pi 管理。若要接中转或自建网关，请改用「自定义模型 API」。",
                                )
                                .weak()
                                .size(11.0),
                            );
                        });
                }
                AddProviderTab::Custom => {
                    ui.label(
                        egui::RichText::new(
                            "连接中转站、自部署服务或其他兼容 OpenAI / Anthropic 协议的接口，需填写 API 地址、协议和模型。",
                        )
                        .weak()
                        .size(12.0),
                    );
                    ui.add_space(10.0);
                    ui.label("Provider ID");
                    ui.add_enabled(
                        !add.editing_custom,
                        egui::TextEdit::singleline(&mut add.custom_id)
                            .hint_text("my-gateway")
                            .desired_width(f32::INFINITY),
                    );
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 160, 70),
                        "创建后不可改",
                    );
                    ui.add_space(6.0);
                    ui.label("显示名称");
                    ui.add(
                        egui::TextEdit::singleline(&mut add.custom_name)
                            .hint_text("可选")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(6.0);
                    ui.label("API 地址");
                    ui.add(
                        egui::TextEdit::singleline(&mut add.custom_base_url)
                            .hint_text("https://gateway.example/v1")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(6.0);
                    ui.label("API 协议");
                    let proto_label = protocol_label(&add.custom_api);
                    egui::ComboBox::from_id_salt("add_custom_api")
                        .selected_text(proto_label)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (id, label) in API_PROTOCOLS {
                                if ui.selectable_label(&add.custom_api == id, *label).clicked() {
                                    add.custom_api = (*id).to_owned();
                                }
                            }
                        });
                    ui.add_space(6.0);
                    ui.label("API 密钥");
                    ui.add(
                        egui::TextEdit::singleline(&mut add.custom_key)
                            .password(true)
                            .hint_text("保存后只显示掩码")
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("模型目录");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("获取可用模型").clicked() {
                                fetch_models = true;
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut add.custom_model_draft)
                                .hint_text("deepseek-chat")
                                .desired_width((ui.available_width() - 72.0).max(120.0)),
                        );
                        if ui.button("添加").clicked()
                            || (resp.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                        {
                            add_model = true;
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (i, model) in add.custom_models.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(model)
                                        .monospace()
                                        .size(12.0),
                                );
                                if ui.small_button("×").clicked() {
                                    remove_model = Some(i);
                                }
                            });
                        }
                    });
                    if let Some(hint) = &add.fetch_hint {
                        ui.label(
                            egui::RichText::new(hint)
                                .weak()
                                .size(11.0),
                        );
                    }
                }
            }

            ui.add_space(16.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Button::new("保存").min_size([72.0, 28.0].into()))
                    .clicked()
                {
                    want_save = true;
                }
                if ui.button("取消").clicked() {
                    want_cancel = true;
                }
            });
        });

    if add_model {
        let id = add.custom_model_draft.trim().to_owned();
        if !id.is_empty() && !add.custom_models.iter().any(|m| m == &id) {
            add.custom_models.push(id);
        }
        add.custom_model_draft.clear();
    }
    if let Some(i) = remove_model {
        if i < add.custom_models.len() {
            add.custom_models.remove(i);
        }
    }
    if fetch_models {
        add.fetch_hint = Some(
            "请手动添加模型 ID（例如 deepseek-chat）。远程目录拉取不在本里程碑范围。".to_owned(),
        );
    }

    if want_save {
        match add.tab {
            AddProviderTab::Builtin => {
                let key = add.builtin_key.trim().to_owned();
                if key.is_empty() {
                    let env_var = api_keys::builtin_providers()
                        .iter()
                        .find(|(id, _, _)| *id == add.builtin_provider)
                        .map(|(_, _, env)| *env)
                        .unwrap_or("");
                    if std::env::var_os(env_var).is_some_and(|v| !v.is_empty()) {
                        add.close();
                    } else {
                        result.error = Some(format!(
                            "未检测到环境变量。请填写 API 密钥，或设置环境变量 {env_var}。"
                        ));
                    }
                } else {
                    match auth_path {
                        Some(path) => {
                            match api_keys::upsert_api_key(path, &add.builtin_provider, &key) {
                                Ok(()) => {
                                    form.refresh(auth_path);
                                    add.close();
                                    result.restart = true;
                                }
                                Err(err) => result.error = Some(err),
                            }
                        }
                        None => {
                            result.error =
                                Some("未找到 Pi agent 目录，无法写入 auth.json".to_owned());
                        }
                    }
                }
            }
            AddProviderTab::Custom => {
                let provider = CustomProvider {
                    id: add.custom_id.trim().to_owned(),
                    name: add.custom_name.trim().to_owned(),
                    base_url: add.custom_base_url.trim().to_owned(),
                    api: add.custom_api.clone(),
                    models: add.custom_models.clone(),
                };
                match (|| -> Result<(), String> {
                    let models_path =
                        models_path.ok_or_else(|| "未找到 models.json 写入路径。".to_owned())?;
                    upsert_custom_provider(models_path, &provider)
                        .map_err(|err| format!("写入 models.json 失败：{err}"))?;
                    let key = add.custom_key.trim();
                    if !key.is_empty() {
                        let auth = auth_path
                            .ok_or_else(|| "未找到 Pi agent 目录，无法写入 auth.json".to_owned())?;
                        api_keys::upsert_api_key(auth, &provider.id, key)?;
                    }
                    Ok(())
                })() {
                    Ok(()) => {
                        form.refresh(auth_path);
                        add.close();
                        result.restart = true;
                    }
                    Err(err) => result.error = Some(err),
                }
            }
        }
    }

    if want_cancel || !still_open {
        add.close();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_model_spec_splits_provider() {
        assert_eq!(
            parse_model_spec("openai/gpt-4o"),
            Some(("openai".to_owned(), "gpt-4o".to_owned()))
        );
        assert_eq!(parse_model_spec("openai"), None);
        assert_eq!(parse_model_spec("/gpt"), None);
    }

    #[test]
    fn current_model_prefers_live() {
        assert_eq!(
            current_model_spec(Some("openai/gpt-4o"), Some("anthropic"), Some("claude")),
            "openai/gpt-4o"
        );
        assert_eq!(
            current_model_spec(None, Some("anthropic"), Some("claude")),
            "anthropic/claude"
        );
        assert_eq!(current_model_spec(None, None, None), "");
    }

    #[test]
    fn parse_providers_object() {
        let val = serde_json::json!({
            "providers": {
                "my-gateway": {
                    "name": "Gateway",
                    "baseUrl": "https://gateway.example/v1",
                    "api": "openai-completions",
                    "models": [{ "id": "deepseek-chat" }, { "id": "deepseek-reasoner" }]
                }
            }
        });
        let list = parse_providers(&val);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "my-gateway");
        assert_eq!(list[0].models, vec!["deepseek-chat", "deepseek-reasoner"]);
    }

    #[test]
    fn parse_legacy_array() {
        let val = serde_json::json!([
            { "name": "Local Ollama", "baseUrl": "http://127.0.0.1:11434/v1" }
        ]);
        let list = parse_providers(&val);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "local-ollama");
        assert_eq!(list[0].base_url, "http://127.0.0.1:11434/v1");
    }

    #[test]
    fn upsert_merges_without_clobber() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("models.json");
        upsert_custom_provider(
            &path,
            &CustomProvider {
                id: "alpha".to_owned(),
                name: "Alpha".to_owned(),
                base_url: "https://a.example/v1".to_owned(),
                api: "openai-completions".to_owned(),
                models: vec!["m1".to_owned()],
            },
        )
        .unwrap();
        upsert_custom_provider(
            &path,
            &CustomProvider {
                id: "beta".to_owned(),
                name: "Beta".to_owned(),
                base_url: "https://b.example/v1".to_owned(),
                api: "anthropic-messages".to_owned(),
                models: vec!["m2".to_owned()],
            },
        )
        .unwrap();
        let list = load_providers(&path);
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|p| p.id == "alpha"));
        assert!(list
            .iter()
            .any(|p| p.id == "beta" && p.api == "anthropic-messages"));
    }

    #[test]
    fn reject_builtin_id_and_empty_models() {
        let err = validate_custom_provider(&CustomProvider {
            id: "openai".to_owned(),
            name: "x".to_owned(),
            base_url: "https://example/v1".to_owned(),
            api: "openai-completions".to_owned(),
            models: vec!["m".to_owned()],
        })
        .unwrap_err();
        assert!(err.contains("内置"));

        let err = validate_custom_provider(&CustomProvider {
            id: "my-gateway".to_owned(),
            name: "x".to_owned(),
            base_url: "https://example/v1".to_owned(),
            api: "openai-completions".to_owned(),
            models: vec![],
        })
        .unwrap_err();
        assert!(err.contains("至少"));
    }

    #[test]
    fn reject_bad_url() {
        assert!(validate_base_url("ftp://x").is_err());
        assert!(validate_base_url("https://ok/v1").is_ok());
    }

    #[test]
    fn add_provider_tabs() {
        assert_eq!(AddProviderTab::Builtin.label(), "第三方模型提供商");
        assert_eq!(AddProviderTab::Custom.label(), "自定义模型 API");
    }

    #[test]
    fn protocol_labels_cover_openai_completions() {
        assert_eq!(
            protocol_label("openai-completions"),
            "OpenAI Chat Completions"
        );
    }
}
