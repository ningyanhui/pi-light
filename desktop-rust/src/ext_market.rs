// SPDX-License-Identifier: Apache-2.0
//
// 扩展市场（M1）：浏览 npm `pi-package` 目录（与 pi.dev/packages 同源），
// 类型滤镜，详情卡，确认后经 sidecar `pi install`，已安装列表与卸载。

use std::{
    path::Path,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use eframe::egui;

use crate::pi_pkg::{
    display_name_version, install_npm, normalize_source, npm_spec, remove_npm, InstalledPackage,
};

const NPM_SEARCH: &str = "https://registry.npmjs.org/-/v1/search";
const NPM_REGISTRY: &str = "https://registry.npmjs.org";
const CATALOG_URL: &str = "https://pi.dev/packages";
const USER_AGENT: &str = "pi-light/0.2.2 (+https://github.com/ningyanhui/pi-light)";

/// 原型「精选扩展」名单（浏览用；不把包打进发行树，那是 M2）。
const FEATURED: &[FeaturedMeta] = &[
    FeaturedMeta {
        name: "pi-marketplace",
        blurb_zh: "Pi 官方扩展市场客户端。发现、安装和管理 Pi 生态扩展。",
        official: true,
        kinds: &[PkgKind::Extension],
        features_zh: &[
            "搜索并安装 Pi 包",
            "元数据与源码关键词审计",
            "安装前必须确认",
        ],
    },
    FeaturedMeta {
        name: "pi-mcp-adapter",
        blurb_zh: "为 Pi 提供 MCP（Model Context Protocol）适配能力。",
        official: true,
        kinds: &[PkgKind::Extension],
        features_zh: &["连接 MCP 服务器", "把工具暴露给 Pi"],
    },
    FeaturedMeta {
        name: "pi-web-access",
        blurb_zh: "为 Pi 提供网页访问和内容获取能力。",
        official: true,
        kinds: &[PkgKind::Extension, PkgKind::Skill],
        features_zh: &["网页抓取", "搜索与内容提取"],
    },
    FeaturedMeta {
        name: "pi-subagents",
        blurb_zh: "在 Pi 中创建和管理子代理，构建多代理工作流。",
        official: true,
        kinds: &[PkgKind::Extension],
        features_zh: &["子代理委托", "脚本化多代理流程"],
    },
    FeaturedMeta {
        name: "pi-lens",
        blurb_zh: "增强 Pi 的代码理解能力，支持实时分析。",
        official: true,
        kinds: &[PkgKind::Extension],
        features_zh: &["LSP / lint / 格式化", "结构分析"],
    },
    FeaturedMeta {
        name: "pi-memory",
        blurb_zh: "为 Pi 提供长期记忆存储和回忆能力。",
        official: true,
        kinds: &[PkgKind::Skill, PkgKind::Extension],
        features_zh: &["长期记忆", "语义检索"],
    },
    FeaturedMeta {
        name: "billion-context",
        blurb_zh: "上下文压缩插件，可拉长单会话可用上下文。",
        official: false,
        kinds: &[PkgKind::Extension],
        features_zh: &["上下文压缩", "长会话"],
    },
];

struct FeaturedMeta {
    name: &'static str,
    blurb_zh: &'static str,
    official: bool,
    kinds: &'static [PkgKind],
    features_zh: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkgKind {
    Extension,
    Skill,
    Prompt,
    Theme,
}

impl PkgKind {
    pub fn label(self) -> &'static str {
        match self {
            PkgKind::Extension => "Extension",
            PkgKind::Skill => "Skill",
            PkgKind::Prompt => "Prompt",
            PkgKind::Theme => "Theme",
        }
    }

    fn from_keyword(kw: &str) -> Option<Self> {
        let k = kw.to_ascii_lowercase();
        match k.as_str() {
            "extension" | "extensions" | "pi-extension" | "pi-extensions" => {
                Some(PkgKind::Extension)
            }
            "skill" | "skills" | "pi-skill" => Some(PkgKind::Skill),
            "prompt" | "prompts" | "prompt-template" | "prompt-templates" => Some(PkgKind::Prompt),
            "theme" | "themes" | "pi-theme" => Some(PkgKind::Theme),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KindFilter {
    #[default]
    All,
    Kind(PkgKind),
}

impl KindFilter {
    const ALL: [KindFilter; 5] = [
        KindFilter::All,
        KindFilter::Kind(PkgKind::Extension),
        KindFilter::Kind(PkgKind::Skill),
        KindFilter::Kind(PkgKind::Prompt),
        KindFilter::Kind(PkgKind::Theme),
    ];

    fn label(self) -> &'static str {
        match self {
            KindFilter::All => "全部",
            KindFilter::Kind(k) => k.label(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarketPage {
    #[default]
    Browse,
    Installed,
}

impl MarketPage {
    fn label(self) -> &'static str {
        match self {
            MarketPage::Browse => "浏览",
            MarketPage::Installed => "已安装",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CatalogPackage {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: Option<String>,
    pub kinds: Vec<PkgKind>,
    pub monthly_downloads: Option<u64>,
    pub featured: bool,
    pub official: bool,
}

#[derive(Debug, Clone)]
pub struct AuditFinding {
    pub yellow: bool,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct AuditReport {
    pub high: u32,
    pub yellow: u32,
    pub notes: Vec<AuditFinding>,
}

#[derive(Debug, Clone)]
pub struct PackageDetail {
    pub catalog: CatalogPackage,
    pub spec: String,
    pub features: Vec<String>,
    pub audit: AuditReport,
    pub has_install_script: bool,
}

#[derive(Debug, Clone, Default)]
pub enum CatalogStatus {
    #[default]
    Idle,
    Loading,
    Ready,
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum JobKind {
    Install { spec: String },
    Remove { spec: String },
}

#[derive(Debug, Clone)]
pub struct JobResult {
    pub kind: JobKind,
    pub outcome: Result<String, String>,
}

enum BgMsg {
    Catalog(Result<Vec<CatalogPackage>, String>),
    Detail(Result<PackageDetail, String>),
    Job(JobResult),
}

pub struct ExtMarketState {
    pub page: MarketPage,
    pub query: String,
    pub kind_filter: KindFilter,
    pub catalog: Vec<CatalogPackage>,
    pub catalog_status: CatalogStatus,
    pub installed: Vec<InstalledPackage>,
    pub installed_query: String,
    pub detail: Option<PackageDetail>,
    pub confirm_remove: Option<String>,
    pub job_busy: bool,
    pub job_label: Option<String>,
    pub last_job: Option<JobResult>,
    pub reload_hint: Option<String>,
    tx: Sender<BgMsg>,
    rx: Receiver<BgMsg>,
}

impl std::fmt::Debug for ExtMarketState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtMarketState")
            .field("page", &self.page)
            .field("query", &self.query)
            .field("catalog_len", &self.catalog.len())
            .field("installed_len", &self.installed.len())
            .field("job_busy", &self.job_busy)
            .finish_non_exhaustive()
    }
}

impl Default for ExtMarketState {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            page: MarketPage::Browse,
            query: String::new(),
            kind_filter: KindFilter::All,
            catalog: featured_packages(),
            catalog_status: CatalogStatus::Idle,
            installed: Vec::new(),
            installed_query: String::new(),
            detail: None,
            confirm_remove: None,
            job_busy: false,
            job_label: None,
            last_job: None,
            reload_hint: None,
            tx,
            rx,
        }
    }
}

impl ExtMarketState {
    pub fn ensure_catalog(&mut self) {
        if matches!(self.catalog_status, CatalogStatus::Idle) {
            self.refresh_catalog();
        }
    }

    pub fn refresh_catalog(&mut self) {
        self.catalog_status = CatalogStatus::Loading;
        let query = self.query.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(BgMsg::Catalog(fetch_npm_search(&query)));
        });
    }

    pub fn poll(&mut self) -> bool {
        let mut restart = false;
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                BgMsg::Catalog(Ok(mut pkgs)) => {
                    merge_featured(&mut pkgs);
                    self.catalog = pkgs;
                    self.catalog_status = CatalogStatus::Ready;
                }
                BgMsg::Catalog(Err(err)) => {
                    if self.catalog.is_empty() {
                        self.catalog = featured_packages();
                    }
                    self.catalog_status = CatalogStatus::Failed(err);
                }
                BgMsg::Detail(Ok(detail)) => self.detail = Some(detail),
                BgMsg::Detail(Err(err)) => {
                    if let Some(current) = self.detail.as_mut() {
                        current.audit.notes.push(AuditFinding {
                            yellow: true,
                            text: format!("未能拉取完整元数据：{err}"),
                        });
                        current.audit.yellow += 1;
                    }
                }
                BgMsg::Job(result) => {
                    self.job_busy = false;
                    self.job_label = None;
                    match &result {
                        JobResult {
                            kind: JobKind::Install { spec },
                            outcome: Ok(_),
                        } => {
                            self.reload_hint = Some(format!(
                                "已安装 {spec}。已请求重启 sidecar；也可在对话中输入 /reload。"
                            ));
                            self.detail = None;
                            restart = true;
                        }
                        JobResult {
                            kind: JobKind::Remove { spec },
                            outcome: Ok(_),
                        } => {
                            self.reload_hint = Some(format!("已移除 {spec}。已请求重启 sidecar。"));
                            self.confirm_remove = None;
                            restart = true;
                        }
                        JobResult {
                            outcome: Err(err), ..
                        } => {
                            self.reload_hint = Some(err.clone());
                        }
                    }
                    self.last_job = Some(result);
                }
            }
        }
        restart
    }

    pub fn open_detail(&mut self, pkg: &CatalogPackage) {
        let mut detail = detail_from_catalog(pkg);
        if let Some(meta) = featured_meta(&pkg.name) {
            detail.features = meta.features_zh.iter().map(|s| (*s).to_owned()).collect();
        }
        self.detail = Some(detail);
        let name = pkg.name.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let _ = tx.send(BgMsg::Detail(fetch_package_detail(&name)));
        });
    }

    pub fn start_install(&mut self, bin: Option<&Path>, spec: &str) -> Result<(), String> {
        let bin = bin.ok_or_else(|| "未找到 Pi 程序，无法安装。".to_owned())?;
        if self.job_busy {
            return Err("已有安装或卸载任务在进行。".to_owned());
        }
        let spec = normalize_source(spec)?;
        self.job_busy = true;
        self.job_label = Some(format!("正在安装 {spec} …"));
        let bin = bin.to_path_buf();
        let spec_clone = spec.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let outcome = install_npm(&bin, &spec_clone, true);
            let _ = tx.send(BgMsg::Job(JobResult {
                kind: JobKind::Install { spec: spec_clone },
                outcome,
            }));
        });
        Ok(())
    }

    pub fn start_remove(&mut self, bin: Option<&Path>, spec: &str) -> Result<(), String> {
        let bin = bin.ok_or_else(|| "未找到 Pi 程序，无法卸载。".to_owned())?;
        if self.job_busy {
            return Err("已有安装或卸载任务在进行。".to_owned());
        }
        let spec = normalize_source(spec)?;
        self.job_busy = true;
        self.job_label = Some(format!("正在卸载 {spec} …"));
        let bin = bin.to_path_buf();
        let spec_clone = spec;
        let tx = self.tx.clone();
        thread::spawn(move || {
            let outcome = remove_npm(&bin, &spec_clone, true);
            let _ = tx.send(BgMsg::Job(JobResult {
                kind: JobKind::Remove { spec: spec_clone },
                outcome,
            }));
        });
        Ok(())
    }

    pub fn is_installed(&self, name: &str) -> bool {
        self.installed.iter().any(|p| p.name == name)
    }
}

pub fn featured_packages() -> Vec<CatalogPackage> {
    FEATURED.iter().map(featured_to_catalog).collect()
}

fn featured_meta(name: &str) -> Option<&'static FeaturedMeta> {
    FEATURED.iter().find(|f| f.name == name)
}

fn featured_to_catalog(meta: &FeaturedMeta) -> CatalogPackage {
    CatalogPackage {
        name: meta.name.to_owned(),
        version: String::new(),
        description: meta.blurb_zh.to_owned(),
        author: if meta.official {
            "官方".to_owned()
        } else {
            "社区".to_owned()
        },
        license: Some("MIT".to_owned()),
        kinds: meta.kinds.to_vec(),
        monthly_downloads: None,
        featured: true,
        official: meta.official,
    }
}

fn merge_featured(pkgs: &mut Vec<CatalogPackage>) {
    for meta in FEATURED {
        if let Some(existing) = pkgs.iter_mut().find(|p| p.name == meta.name) {
            existing.featured = true;
            existing.official = meta.official;
            if existing.description.is_empty() {
                existing.description = meta.blurb_zh.to_owned();
            }
            if existing.kinds.is_empty() {
                existing.kinds = meta.kinds.to_vec();
            }
        } else {
            pkgs.insert(0, featured_to_catalog(meta));
        }
    }
    pkgs.sort_by(|a, b| b.featured.cmp(&a.featured).then(a.name.cmp(&b.name)));
}

pub fn filter_catalog<'a>(
    pkgs: &'a [CatalogPackage],
    query: &str,
    kind: KindFilter,
) -> Vec<&'a CatalogPackage> {
    let needle = query.trim().to_lowercase();
    pkgs.iter()
        .filter(|p| match kind {
            KindFilter::All => true,
            KindFilter::Kind(k) => p.kinds.contains(&k),
        })
        .filter(|p| {
            if needle.is_empty() {
                return true;
            }
            p.name.to_lowercase().contains(&needle)
                || p.description.to_lowercase().contains(&needle)
                || p.author.to_lowercase().contains(&needle)
        })
        .collect()
}

fn kinds_from_keywords(keywords: &[String]) -> Vec<PkgKind> {
    let mut out = Vec::new();
    for kw in keywords {
        if let Some(kind) = PkgKind::from_keyword(kw) {
            if !out.contains(&kind) {
                out.push(kind);
            }
        }
    }
    out
}

fn kinds_from_pi_manifest(pi: &serde_json::Value) -> Vec<PkgKind> {
    let mut out = Vec::new();
    let mapping = [
        ("extensions", PkgKind::Extension),
        ("skills", PkgKind::Skill),
        ("prompts", PkgKind::Prompt),
        ("themes", PkgKind::Theme),
    ];
    for (key, kind) in mapping {
        let present = pi
            .get(key)
            .map(|v| match v {
                serde_json::Value::Array(arr) => !arr.is_empty(),
                serde_json::Value::String(s) => !s.is_empty(),
                serde_json::Value::Bool(b) => *b,
                _ => true,
            })
            .unwrap_or(false);
        if present {
            out.push(kind);
        }
    }
    out
}

pub fn parse_npm_search(json: &str) -> Result<Vec<CatalogPackage>, String> {
    let val: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("目录 JSON 无法解析：{e}"))?;
    let objects = val
        .get("objects")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "目录响应缺少 objects".to_owned())?;
    let mut out = Vec::new();
    for obj in objects {
        let pkg = obj.get("package").unwrap_or(obj);
        let name = pkg
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        if name.is_empty() {
            continue;
        }
        let keywords: Vec<String> = pkg
            .get("keywords")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let author = pkg
            .get("publisher")
            .and_then(|v| v.get("username"))
            .and_then(|v| v.as_str())
            .or_else(|| {
                pkg.get("author")
                    .and_then(|v| v.get("name").or(Some(v)))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("")
            .to_owned();
        let monthly = obj
            .get("downloads")
            .and_then(|v| v.get("monthly"))
            .and_then(|v| v.as_u64());
        let mut kinds = kinds_from_keywords(&keywords);
        if kinds.is_empty() {
            kinds.push(PkgKind::Extension);
        }
        out.push(CatalogPackage {
            name,
            version: pkg
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned(),
            description: pkg
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned(),
            author,
            license: pkg
                .get("license")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            kinds,
            monthly_downloads: monthly,
            featured: false,
            official: false,
        });
    }
    Ok(out)
}

pub fn audit_metadata(
    license: Option<&str>,
    keywords: &[String],
    has_install_script: bool,
    insecure: bool,
) -> AuditReport {
    let mut notes = Vec::new();
    let mut yellow = 0;
    let mut high = 0;
    if !keywords
        .iter()
        .any(|k| k.eq_ignore_ascii_case("pi-package"))
    {
        yellow += 1;
        notes.push(AuditFinding {
            yellow: true,
            text: "未声明 pi-package 关键词，可能不是 Pi 包。".to_owned(),
        });
    }
    if license.map(str::trim).filter(|s| !s.is_empty()).is_none() {
        yellow += 1;
        notes.push(AuditFinding {
            yellow: true,
            text: "未公布许可证。".to_owned(),
        });
    }
    if has_install_script {
        yellow += 1;
        notes.push(AuditFinding {
            yellow: true,
            text: "含 install / postinstall 脚本，安装时可能执行代码。".to_owned(),
        });
    }
    if insecure {
        high += 1;
        notes.push(AuditFinding {
            yellow: false,
            text: "npm 标记为 insecure。".to_owned(),
        });
    }
    if notes.is_empty() {
        notes.push(AuditFinding {
            yellow: true,
            text: "仅做了注册表元数据检查，未扫描源码。第三方包仍可执行代码。".to_owned(),
        });
        yellow = 1;
    } else if high == 0 {
        notes.push(AuditFinding {
            yellow: true,
            text: "未扫描安装包源码；关键词扫描无法发现混淆代码。".to_owned(),
        });
        yellow += 1;
    }
    AuditReport {
        high,
        yellow,
        notes,
    }
}

fn detail_from_catalog(pkg: &CatalogPackage) -> PackageDetail {
    let spec = npm_spec(&pkg.name, none_if_empty(&pkg.version))
        .unwrap_or_else(|_| format!("npm:{}", pkg.name));
    let audit = audit_metadata(
        pkg.license.as_deref(),
        &["pi-package".to_owned()],
        false,
        false,
    );
    PackageDetail {
        catalog: pkg.clone(),
        spec,
        features: if pkg.description.is_empty() {
            Vec::new()
        } else {
            vec![pkg.description.clone()]
        },
        audit,
        has_install_script: false,
    }
}

fn none_if_empty(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn fetch_npm_search(query: &str) -> Result<Vec<CatalogPackage>, String> {
    let text = if query.trim().is_empty() {
        "keywords:pi-package".to_owned()
    } else {
        format!("keywords:pi-package {}", query.trim())
    };
    let url = format!("{NPM_SEARCH}?text={}&size=40", percent_encode(&text));
    let body = http_get(&url)?;
    parse_npm_search(&body)
}

fn fetch_package_detail(name: &str) -> Result<PackageDetail, String> {
    let url = format!("{NPM_REGISTRY}/{}", percent_encode_path(name));
    let body = http_get(&url)?;
    parse_registry_detail(name, &body)
}

pub fn parse_registry_detail(name: &str, json: &str) -> Result<PackageDetail, String> {
    let val: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("包详情无法解析：{e}"))?;
    let latest = val
        .pointer("/dist-tags/latest")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .or_else(|| {
            val.get("version")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let version_obj = val
        .get("versions")
        .and_then(|v| v.get(&latest))
        .cloned()
        .unwrap_or(val.clone());
    let description = version_obj
        .get("description")
        .or_else(|| val.get("description"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let license = version_obj
        .get("license")
        .or_else(|| val.get("license"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let author = version_obj
        .get("author")
        .and_then(|v| v.get("name").or(Some(v)))
        .and_then(|v| v.as_str())
        .or_else(|| {
            val.get("author")
                .and_then(|v| v.get("name").or(Some(v)))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_owned();
    let keywords: Vec<String> = version_obj
        .get("keywords")
        .or_else(|| val.get("keywords"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let pi = version_obj.get("pi").or_else(|| val.get("pi"));
    let mut kinds = pi.map(kinds_from_pi_manifest).unwrap_or_default();
    if kinds.is_empty() {
        kinds = kinds_from_keywords(&keywords);
    }
    if kinds.is_empty() {
        kinds.push(PkgKind::Extension);
    }
    let scripts = version_obj.get("scripts").and_then(|v| v.as_object());
    let has_install_script = scripts
        .map(|s| {
            s.contains_key("install")
                || s.contains_key("postinstall")
                || s.contains_key("preinstall")
        })
        .unwrap_or(false);
    let insecure = val
        .pointer("/flags/insecure")
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        > 0;
    let catalog = CatalogPackage {
        name: name.to_owned(),
        version: latest.clone(),
        description: description.clone(),
        author,
        license: license.clone(),
        kinds,
        monthly_downloads: None,
        featured: featured_meta(name).is_some(),
        official: featured_meta(name).is_some_and(|m| m.official),
    };
    let mut features = Vec::new();
    if let Some(meta) = featured_meta(name) {
        features.extend(meta.features_zh.iter().map(|s| (*s).to_owned()));
    }
    if let Some(pi) = pi {
        for key in ["extensions", "skills", "prompts", "themes"] {
            if let Some(arr) = pi.get(key).and_then(|v| v.as_array()) {
                if !arr.is_empty() {
                    features.push(format!("{key} × {}", arr.len()));
                }
            }
        }
    }
    if features.is_empty() && !description.is_empty() {
        features.push(description);
    }
    let spec = npm_spec(name, none_if_empty(&latest))?;
    Ok(PackageDetail {
        catalog,
        spec,
        features,
        audit: audit_metadata(license.as_deref(), &keywords, has_install_script, insecure),
        has_install_script,
    })
}

fn http_get(url: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(12))
        .user_agent(USER_AGENT)
        .build();
    let resp = agent
        .get(url)
        .call()
        .map_err(|e| format!("请求失败：{e}"))?;
    resp.into_string().map_err(|e| format!("读取响应失败：{e}"))
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn percent_encode_path(name: &str) -> String {
    if let Some(rest) = name.strip_prefix('@') {
        if let Some((scope, pkg)) = rest.split_once('/') {
            return format!("@{scope}%2F{pkg}");
        }
    }
    name.to_owned()
}

// ── UI ────────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct MarketUiResult {
    pub error: Option<String>,
    pub restart: bool,
}

pub fn render_marketplace(
    ui: &mut egui::Ui,
    state: &mut ExtMarketState,
    installed: &[InstalledPackage],
    pi_ready: bool,
) -> MarketUiResult {
    let mut result = MarketUiResult::default();
    state.installed = installed.to_vec();
    state.ensure_catalog();

    ui.horizontal(|ui| {
        for page in [MarketPage::Browse, MarketPage::Installed] {
            let selected = state.page == page;
            if ui
                .add(egui::SelectableLabel::new(selected, page.label()))
                .clicked()
            {
                state.page = page;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("刷新目录").clicked() {
                state.refresh_catalog();
            }
        });
    });
    ui.add_space(6.0);

    if let Some(hint) = state.reload_hint.clone() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(hint).size(12.0));
            if ui.small_button("×").clicked() {
                state.reload_hint = None;
            }
        });
    }
    if let Some(label) = &state.job_label {
        ui.label(
            egui::RichText::new(label)
                .color(egui::Color32::from_rgb(180, 160, 70))
                .size(12.0),
        );
    }
    if !pi_ready {
        ui.colored_label(
            egui::Color32::from_rgb(220, 140, 80),
            "未找到 Pi 程序，浏览可用，安装 / 卸载不可用。",
        );
    }

    match state.page {
        MarketPage::Browse => render_browse(ui, state),
        MarketPage::Installed => render_installed(ui, state, &mut result),
    }

    ui.add_space(10.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("所有扩展均来自").weak().size(11.0));
        ui.hyperlink_to("pi.dev/packages", CATALOG_URL);
        ui.label(
            egui::RichText::new("· 第三方包可执行代码，安装前请确认。")
                .weak()
                .size(11.0),
        );
    });
    result
}

fn render_browse(ui: &mut egui::Ui, state: &mut ExtMarketState) {
    let mut search = false;
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.query)
                .hint_text("搜索 Pi 包")
                .desired_width(ui.available_width() - 72.0),
        );
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            search = true;
        }
        if ui.button("搜索").clicked() {
            search = true;
        }
    });
    if search {
        state.refresh_catalog();
    }
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("筛选：").weak().size(12.0));
        for filter in KindFilter::ALL {
            let selected = state.kind_filter == filter;
            if ui
                .add(egui::SelectableLabel::new(selected, filter.label()))
                .clicked()
            {
                state.kind_filter = filter;
            }
        }
    });
    match &state.catalog_status {
        CatalogStatus::Loading => {
            ui.label(
                egui::RichText::new("正在从 npm 拉取 pi-package 目录…")
                    .weak()
                    .size(12.0),
            );
        }
        CatalogStatus::Failed(err) => {
            ui.label(
                egui::RichText::new(format!("目录请求失败，已显示精选列表。{err}"))
                    .color(egui::Color32::from_rgb(200, 140, 70))
                    .size(11.0),
            );
        }
        _ => {}
    }
    ui.add_space(8.0);
    ui.label(egui::RichText::new("精选扩展").strong().size(13.0));
    ui.add_space(4.0);

    let filtered = filter_catalog(&state.catalog, &state.query, state.kind_filter);
    if filtered.is_empty() {
        ui.label(
            egui::RichText::new("没有匹配的包")
                .weak()
                .italics()
                .size(12.0),
        );
        return;
    }

    let mut open_name: Option<String> = None;
    for pkg in filtered {
        let installed = state.is_installed(&pkg.name);
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
            .rounding(egui::Rounding::same(8.0))
            .fill(ui.visuals().extreme_bg_color)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new(&pkg.name).strong().size(13.0));
                            if pkg.featured {
                                ui.label(
                                    egui::RichText::new("精选")
                                        .color(egui::Color32::from_rgb(80, 160, 220))
                                        .size(11.0),
                                );
                            }
                        });
                        if !pkg.description.is_empty() {
                            ui.label(
                                egui::RichText::new(truncate_chars(&pkg.description, 140))
                                    .size(12.0),
                            );
                        }
                        let kinds = if pkg.kinds.is_empty() {
                            "Extension".to_owned()
                        } else {
                            pkg.kinds
                                .iter()
                                .map(|k| k.label())
                                .collect::<Vec<_>>()
                                .join(" · ")
                        };
                        let origin = if pkg.official { "官方" } else { &pkg.author };
                        let ver = if pkg.version.is_empty() {
                            String::new()
                        } else {
                            format!("  v{}", pkg.version)
                        };
                        ui.label(
                            egui::RichText::new(format!("{origin} · {kinds}{ver}"))
                                .weak()
                                .size(11.0),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if installed {
                            ui.label(
                                egui::RichText::new("已安装")
                                    .color(egui::Color32::from_rgb(90, 180, 110))
                                    .size(12.0),
                            );
                        } else if ui
                            .add(
                                egui::Button::new("安装")
                                    .fill(egui::Color32::from_rgb(46, 140, 90)),
                            )
                            .clicked()
                        {
                            open_name = Some(pkg.name.clone());
                        }
                        ui.label(
                            egui::RichText::new(
                                pkg.kinds.first().map(|k| k.label()).unwrap_or("Extension"),
                            )
                            .weak()
                            .size(11.0),
                        );
                    });
                });
            });
        ui.add_space(6.0);
    }
    if let Some(name) = open_name {
        if let Some(pkg) = state.catalog.iter().find(|p| p.name == name).cloned() {
            state.open_detail(&pkg);
        }
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
}

fn render_installed(ui: &mut egui::Ui, state: &mut ExtMarketState, result: &mut MarketUiResult) {
    ui.add(
        egui::TextEdit::singleline(&mut state.installed_query)
            .hint_text("搜索已安装包")
            .desired_width(f32::INFINITY),
    );
    ui.add_space(6.0);
    let action = render_installed_table(ui, &state.installed, &state.installed_query, true);
    match action {
        InstalledAction::Remove(source) => state.confirm_remove = Some(source),
        InstalledAction::Reload => result.restart = true,
        InstalledAction::None => {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstalledAction {
    None,
    Remove(String),
    Reload,
}

pub fn render_installed_table(
    ui: &mut egui::Ui,
    installed: &[InstalledPackage],
    query: &str,
    show_reload: bool,
) -> InstalledAction {
    let mut action = InstalledAction::None;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("已安装 Pi 包").strong().size(13.0));
        ui.label(
            egui::RichText::new(format!("共 {} 个", installed.len()))
                .weak()
                .size(11.0),
        );
        if show_reload
            && ui
                .small_button("重启 sidecar /reload")
                .on_hover_text("让新安装的包生效，无需手敲 /reload")
                .clicked()
        {
            action = InstalledAction::Reload;
        }
    });

    if installed.is_empty() {
        ui.label(
            egui::RichText::new("还没有经 sidecar 安装的 Pi 包。可在「浏览」页确认后安装。")
                .weak()
                .italics()
                .size(12.0),
        );
        return action;
    }

    let needle = query.trim().to_lowercase();
    ui.add_space(4.0);
    for pkg in installed {
        if !needle.is_empty()
            && !pkg.name.to_lowercase().contains(&needle)
            && !pkg.source.to_lowercase().contains(&needle)
        {
            continue;
        }
        egui::Frame::none()
            .inner_margin(egui::Margin::symmetric(10.0, 6.0))
            .rounding(egui::Rounding::same(6.0))
            .fill(ui.visuals().extreme_bg_color)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(&pkg.name).strong().size(13.0));
                        let ver = pkg.version.as_deref().unwrap_or("—");
                        ui.label(
                            egui::RichText::new(format!(
                                "{}  {}  {}",
                                pkg.source,
                                ver,
                                pkg.scope.label()
                            ))
                            .weak()
                            .monospace()
                            .size(11.0),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new("移除")
                                    .fill(egui::Color32::from_rgb(160, 70, 70)),
                            )
                            .clicked()
                        {
                            action = InstalledAction::Remove(pkg.source.clone());
                        }
                    });
                });
            });
        ui.add_space(4.0);
    }
    action
}

pub fn render_market_modals(
    ctx: &egui::Context,
    state: &mut ExtMarketState,
    pi_bin: Option<&Path>,
) -> MarketUiResult {
    let mut result = MarketUiResult::default();
    if let Some(detail) = state.detail.clone() {
        let mut close = false;
        egui::Window::new("包详情")
            .id(egui::Id::new("pi_light_pkg_detail"))
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(460.0)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&detail.catalog.name)
                            .strong()
                            .size(18.0),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("×").clicked() {
                            close = true;
                        }
                    });
                });
                ui.label(
                    egui::RichText::new(&detail.catalog.description)
                        .weak()
                        .size(12.0),
                );
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    meta_chip(
                        ui,
                        "版本",
                        if detail.catalog.version.is_empty() {
                            "—"
                        } else {
                            &detail.catalog.version
                        },
                    );
                    meta_chip(ui, "作者", &detail.catalog.author);
                    meta_chip(
                        ui,
                        "许可证",
                        detail.catalog.license.as_deref().unwrap_or("—"),
                    );
                    let kinds = detail
                        .catalog
                        .kinds
                        .iter()
                        .map(|k| k.label())
                        .collect::<Vec<_>>()
                        .join(" / ");
                    meta_chip(ui, "类型", &kinds);
                });
                ui.add_space(8.0);
                ui.columns(2, |cols| {
                    cols[0].label(egui::RichText::new("功能特性").strong().size(13.0));
                    for feat in &detail.features {
                        cols[0].label(egui::RichText::new(format!("· {feat}")).size(12.0));
                    }
                    cols[1].label(egui::RichText::new("安全审计").strong().size(13.0));
                    let high_color = if detail.audit.high == 0 {
                        egui::Color32::from_rgb(90, 180, 110)
                    } else {
                        egui::Color32::from_rgb(210, 80, 60)
                    };
                    cols[1].label(
                        egui::RichText::new(if detail.audit.high == 0 {
                            "未发现高危问题（元数据检查）"
                        } else {
                            "存在高危标记"
                        })
                        .color(high_color)
                        .size(12.0),
                    );
                    cols[1].label(
                        egui::RichText::new(format!(
                            "高危 {}    黄色 {}",
                            detail.audit.high, detail.audit.yellow
                        ))
                        .size(12.0),
                    );
                    for note in &detail.audit.notes {
                        cols[1].label(egui::RichText::new(format!("· {}", note.text)).size(11.0));
                    }
                });
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(
                        "第三方 Pi 包可执行代码。点确认后才会调用 sidecar：pi install",
                    )
                    .weak()
                    .size(11.0),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let already = state.is_installed(&detail.catalog.name);
                    let can = pi_bin.is_some() && !state.job_busy && !already;
                    let label = format!("确认安装 {}", detail.spec);
                    if ui
                        .add_enabled(
                            can,
                            egui::Button::new(label).fill(egui::Color32::from_rgb(110, 70, 180)),
                        )
                        .clicked()
                    {
                        if let Err(err) = state.start_install(pi_bin, &detail.spec) {
                            result.error = Some(err);
                        }
                    }
                    if ui.button("取消").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            state.detail = None;
        }
    }

    if let Some(source) = state.confirm_remove.clone() {
        let mut close = false;
        egui::Window::new("确认卸载")
            .id(egui::Id::new("pi_light_pkg_remove"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 40.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let (name, _) = display_name_version(&source);
                ui.label(format!("从 Pi 配置中移除 {name}？"));
                ui.label(
                    egui::RichText::new(source.clone())
                        .weak()
                        .monospace()
                        .size(11.0),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new("确认移除")
                                .fill(egui::Color32::from_rgb(160, 70, 70)),
                        )
                        .clicked()
                    {
                        if let Err(err) = state.start_remove(pi_bin, &source) {
                            result.error = Some(err);
                        }
                    }
                    if ui.button("取消").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            state.confirm_remove = None;
        }
    }
    result
}

fn meta_chip(ui: &mut egui::Ui, label: &str, value: &str) {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .rounding(egui::Rounding::same(4.0))
        .fill(ui.visuals().faint_bg_color)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(label).weak().size(10.0));
                ui.label(egui::RichText::new(value).size(12.0));
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH_JSON: &str = r#"{
        "objects": [
            {
                "downloads": {"monthly": 100},
                "package": {
                    "name": "pi-web-access",
                    "version": "0.46.0",
                    "description": "web",
                    "keywords": ["pi-package", "extension"],
                    "license": "MIT",
                    "publisher": {"username": "nicopreme"}
                }
            },
            {
                "package": {
                    "name": "pretty-theme",
                    "version": "1.0.0",
                    "description": "a theme",
                    "keywords": ["pi-package", "theme"],
                    "author": {"name": "Ada"}
                }
            }
        ]
    }"#;

    #[test]
    fn featured_contains_prototype_names() {
        let names: Vec<_> = featured_packages().into_iter().map(|p| p.name).collect();
        for expected in [
            "pi-marketplace",
            "pi-mcp-adapter",
            "pi-web-access",
            "pi-subagents",
            "pi-lens",
            "pi-memory",
            "billion-context",
        ] {
            assert!(names.iter().any(|n| n == expected), "missing {expected}");
        }
    }

    #[test]
    fn parse_search_and_filter_theme() {
        let pkgs = parse_npm_search(SEARCH_JSON).unwrap();
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].author, "nicopreme");
        let themes = filter_catalog(&pkgs, "", KindFilter::Kind(PkgKind::Theme));
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].name, "pretty-theme");
        let q = filter_catalog(&pkgs, "web", KindFilter::All);
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn audit_flags_missing_keyword_and_scripts() {
        let report = audit_metadata(None, &["other".into()], true, false);
        assert_eq!(report.high, 0);
        assert!(report.yellow >= 2);
        assert!(report.notes.iter().any(|n| n.text.contains("许可证")));
        assert!(report.notes.iter().any(|n| n.text.contains("install")));
    }

    #[test]
    fn audit_high_on_insecure_flag() {
        let report = audit_metadata(Some("MIT"), &["pi-package".into()], false, true);
        assert_eq!(report.high, 1);
    }

    #[test]
    fn parse_detail_reads_pi_manifest_and_scripts() {
        let json = r#"{
            "name": "demo-pkg",
            "dist-tags": {"latest": "1.2.3"},
            "versions": {
                "1.2.3": {
                    "description": "hello",
                    "license": "MIT",
                    "keywords": ["pi-package", "skill"],
                    "author": {"name": "Pat"},
                    "scripts": {"postinstall": "node x.js"},
                    "pi": {"skills": ["./skills"], "extensions": []}
                }
            }
        }"#;
        let detail = parse_registry_detail("demo-pkg", json).unwrap();
        assert_eq!(detail.spec, "npm:demo-pkg@1.2.3");
        assert!(detail.catalog.kinds.contains(&PkgKind::Skill));
        assert!(detail.has_install_script);
        assert!(detail.audit.yellow >= 1);
    }

    #[test]
    fn kind_from_keywords() {
        assert_eq!(
            PkgKind::from_keyword("pi-extension"),
            Some(PkgKind::Extension)
        );
        assert_eq!(PkgKind::from_keyword("skills"), Some(PkgKind::Skill));
        assert!(PkgKind::from_keyword("pi-package").is_none());
    }
}
