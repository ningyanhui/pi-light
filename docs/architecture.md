# pi-light 架构说明

> **当前版本**：v0.2.2 + v3 M0–M2 + **M5 桌面远程** — 右设置抽屉七 Tab；市场 + Slim 预置；远程访问：配对码/二维码、Cloudflare Tunnel、已授权设备
> **主路径**：`desktop-rust/`（egui/eframe）
> **许可**：Apache-2.0 洁净室

---

## 一句话定位

**pi-light = 内嵌官方 Pi sidecar 的轻量通用桌面 Agent Shell（egui/eframe，Apache-2.0 洁净室）**

---

## 宏观层次图

```
┌─────────────────────────────────────────────────────────────────────┐
│  desktop-rust/   egui + eframe（主桌面客户端）                        │
│                                                                       │
│  ┌──────────┐ ┌──────────────────────────────────────────────────┐ │
│  │ 会话列表  │ │  主区（CentralPanel）                              │ │
│  │ 恢复/新建 │ │  流式聊天 + 工具卡片 + Token/Trace                 │ │
│  │ 工作目录  │ │  输入框 + 发送 / 停止 / 重试                       │ │
│  └──────────┘ └──────────────────────────────────────────────────┘ │
│  顶栏：标题 + 状态灯 + 模型下拉 + 目录名                             │
│  底栏左下角：⚙ 设置（右侧抽屉：模型 / Skills / MCP / 扩展市场 /     │
│                      记忆 / 远程访问 / 通用）                         │
│  底栏右侧：重新探测                                                  │
│                                                                       │
│  pi_path 模块：路径解析（env → sidecar → PATH）                        │
│  pi_rpc 模块（M2′+M4′+）：JSONL over stdin/stdout                     │
│  admin_panel 模块：设置抽屉（七 Tab）+ provider_catalog + ext_market / pi_pkg │
│  remote_access + relay_mgr + remote_gateway：配对 / Cloudflare Tunnel / 设备   │
└────────────────────────────┬────────────────────────────────────────┘
                             │ std::process::Command（stdin/stdout piped）
                             │ 后台线程读 stdout → mpsc channel
                             │ UI 每帧 drain channel
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│  Pi sidecar（官方 MIT，不修改）                                        │
│  spawn: pi --mode rpc  （可选 cwd；有已存会话文件则 switch_session，否则 new_session） │
│  协议: JSONL（官方 rpc.md / rpc-commands.md / json.md）               │
│  布局: {exe_dir}/sidecar/pi/pi.exe                                   │
│  覆盖: PI_LIGHT_PI_BIN                                               │
└─────────────────────────────────────────────────────────────────────┘

android/   Kotlin Compose（Option C：debug 占位 APK，不内嵌 Pi；M6′ 维持 C，见下文）
desktop/   旧 Tauri 树（已标 deprecated，不是文档/实现主路径）
```

---

## 目录结构（便携 bash / R0+）

```
pi-light/
├── Cargo.toml                    # workspace
├── desktop-rust/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs                # 库入口（供集成测试用）
│   │   ├── main.rs               # egui/eframe App + 事件循环
│   │   ├── fonts.rs              # 系统 CJK 字体回退（Windows 优先微软雅黑）
│   │   ├── bash_path.rs          # 便携 bash 路径探测 + PATH 注入辅助（M5′+）
│   │   ├── pi_path.rs            # Pi 三态路径探测
│   │   ├── pi_rpc.rs             # JSONL RPC 客户端（spawn 时注入 bash PATH）
│   │   ├── chat.rs               # 聊天状态机（M2′）
│   │   ├── session_store.rs      # 会话历史落盘与启动恢复
│   │   ├── session_insights.rs   # Token / Trace（M3′）
│   │   ├── admin_panel.rs        # 设置抽屉（七 Tab）
│   │   ├── ext_market.rs         # 扩展市场 UI（浏览 / 详情 / 确认安装 / 一键启用）
│   │   ├── pi_pkg.rs             # `pi install` / `list` / `remove` + settings.json
│   │   ├── preset_packages.rs    # Slim/操控 lockfile、settings 本地 path 同步
│   │   ├── api_keys.rs           # 应用内 API 密钥（写入 auth.json，保存后掩码）
│   │   ├── provider_catalog.rs   # 内置厂商 + 自定义 API / models.json
│   │   ├── onboarding.rs         # 首次打开：只引导配置模型与密钥
│   │   ├── remote_access.rs      # 远程访问页（配对码 / QR / 设备）
│   │   ├── remote_gateway.rs     # 本机 loopback 控制面
│   │   ├── relay_mgr.rs          # cloudflared 探测 / 下载 / 隧道
│   │   └── bin/mock_pi.rs        # 集成测试用 Pi 替身
│   └── tests/rpc_smoke.rs        # 集成烟测
├── android/                      # Option C 占位壳
├── docs/
│   ├── architecture.md
│   ├── preset-packages.md        # Slim / 操控包钉版本与 tarball SHA-256
│   ├── remote-acceptance.md      # M5 公网配对与中继验收
│   ├── companion-protocol.md     # M6 消费的 claim / chat 钩子
│   └── signing.md                # Windows 签名策略与 Authenticode 占位
├── scripts/
│   ├── fetch-pi-sidecar.ps1      # Pi sidecar 下载/SHA-256 校验
│   ├── fetch-cloudflared.ps1     # cloudflared 下载/SHA-256 校验
│   ├── cloudflared.lock.json     # cloudflared 版本与 SHA 钉定
│   ├── fetch-bash-sidecar.ps1    # MinGit 下载/SHA-256 校验（便携 bash）
│   ├── fetch-pi-packages.py      # 预置 npm 包：下载、SHA-256、解压
│   └── preset-packages.lock.json # Slim + 操控包钉版本
└── .github/workflows/            # CI / Release / Android APK
```

---

## 模块职责

| 模块 | 职责 |
|------|------|
| `bash_path` | 便携 bash 路径探测（捆绑 MinGit → 系统 PATH）；`bundled_path_dirs()` 供 `pi_rpc` spawn 注入 PATH |
| `pi_path` | Pi 二进制路径探测三态；`PiProbe` 枚举 |
| `pi_rpc` | `PiRpcClient`：spawn（可选 cwd，注入 bash PATH）→ JSONL；`validate_cwd` 入口单次校验；`new_session` / `switch_session` |
| `chat` | 聊天状态机 + `SessionHub`（多会话）；重试上一条 prompt；可导出快照 |
| `session_store` | 把会话列表、消息、草稿、工作目录写入 `~/.pi-light/sessions.json`，启动时读回 |
| `fonts` | 系统 CJK 字体作为 egui 回退；Windows 优先 `msyh.ttc` |
| `api_keys` | 设置页写入 Pi `auth.json` 的 API 密钥；保存后掩码，不回显明文；允许自定义 provider id |
| `provider_catalog` | 内置厂商卡片 + 自定义 API 表单；读写 `models.json` 的 `providers` 对象 |
| `session_insights` | Token 用量（`get_session_stats` + `message_update.usage`）；工具 Trace（含 args 预览） |
| `admin_panel` | 左下角齿轮打开右侧抽屉。Tab：模型 / Skills / MCP / 扩展市场 / 记忆 / 远程访问 / 通用。密钥写入 `auth.json` 后只显示掩码 |
| `ext_market` | 扩展市场：npm `pi-package` 目录、类型滤镜、详情卡、元数据审计、确认安装；操控三包一键启用 |
| `pi_pkg` | 经 sidecar 执行 `pi install` / `pi list` / `pi remove`；解析 `settings.json` `packages` |
| `preset_packages` | 嵌入 lockfile；Slim 本地 path 写入 settings；操控包启用/停用；computer-use 权限向导闸门 |
| `onboarding` | 首次打开主路径：Pi 就绪时只引导配置模型与密钥；未就绪时安装说明保持折叠 |
| `session_ref` | M5′ `#` 会话引用：`RefCandidate` / `RefChooserState` / `RefChip`；候选构建；prompt 拼合 |
| `remote_access` | 远程访问页：允许手机远程、配对码/QR（TTL 10 分钟）、设备列表撤销、中继选项 |
| `remote_gateway` | 本机 `127.0.0.1` HTTP：`/health`、`/v1/pair/info`、`POST /v1/pair/claim`；`/v1/chat` 返回 501 |
| `relay_mgr` | cloudflared 探测顺序、SHA 校验下载、trycloudflare / 命名隧道启停与状态 |
| `main` | `PiLightApp`：组合以上模块；egui 渲染；事件 drain |

---

## Pi sidecar 集成

### 版本钉定

| 项 | 值 |
|----|----|
| 上游 | `earendil-works/pi`（MIT） |
| 版本 | v1.4.0 |
| 发行物 | `pi-windows-x64.zip` |
| SHA-256 | `0f7256ea8bdac3af20012b94fdc1252a31cc6dc0d2f9b9eb539555d22e3e321b` |

### 便携 bash（MinGit）版本钉定

| 项 | 值 |
|----|----|
| 上游 | `git-for-windows/git`（GPL-2.0 + 例外） |
| 版本 | v2.47.1.windows.2 |
| 发行物 | `MinGit-2.47.1.2-64-bit.zip` |
| SHA-256 | `5bafb35dfb249b89d726b37824eeb5022379f0e51f5fbf9c29f49bef57e85b42` |
| sidecar 路径 | `{exe_dir}/sidecar/bash/` |

MinGit 提供便携 bash/MSYS 环境，Pi 运行 shell 命令时自动使用。
**禁止捆绑 Node.js**；Pi 所需 JavaScript 运行时不在 pi-light 分发范围内。

### 便携包目录布局

```
pi-light-windows-x64/
├── pi-light.exe                  # 主程序，用户唯一入口
├── NOTICE                        # SHA-256 + 版权声明（Pi、MinGit、预置包、cloudflared）
├── LICENSE                       # Apache-2.0
├── preset-packages.md            # 预置包人类可读钉表
└── sidecar/
    ├── pi/
    │   └── pi.exe                # Pi sidecar（MIT，由 earendil-works/pi 发布）
    ├── bash/                     # MinGit（GPL-2.0 + 例外，便携 bash/MSYS 环境）
    │   ├── usr/bin/              # sh.exe；Release 复制为 bash.exe 供 Pi 查找
    │   ├── mingw64/bin/          # 工具链
    │   └── cmd/                  # git.exe
    ├── pi-packages/              # npm tarball 解压树（Pi 本地 path 包）
    │   ├── preset-lock.json
    │   ├── pi-marketplace/
    │   ├── pi-mcp-adapter/
    │   ├── pi-web-access/
    │   ├── pi-memory/
    │   ├── pi-subagents/
    │   ├── pi-browser-use/         # 默认不写入 settings
    │   ├── pi-chrome-connector/
    │   └── @injaneity/pi-computer-use/
    └── relay/
        └── cloudflared.exe       # Cloudflare Tunnel（Apache-2.0；SHA 见 NOTICE）
```

### Pi agent 目录（M4′ 发现路径）

默认：`~/.pi/agent/`（可被 `PI_CODING_AGENT_DIR` 覆盖）

| 文件/目录 | 用途 |
|-----------|------|
| `settings.json` | defaultProvider / defaultModel；`skills` 排除列表；`packages` 已安装 Pi 包（M1 经 sidecar 读写；M2 Slim 为本地 path） |
| `mcp.json` | MCP 服务列表；官方 `enabled` 字段读写，兼容旧 `disabled`（M4′+） |
| `models.json` | 自定义 API 提供商（`providers` 对象：baseUrl / api / models）；M0 可写 |
| `auth.json` | API 凭据。设置页可写入 `api_key`；界面不读回明文。OAuth 条目不在此覆盖 |
| `skills/` | 用户级 skills 目录（M4′+ 发现 + settings 推导启用状态） |
| `AGENTS.md` / `CLAUDE.md` | 用户级记忆指令（M4′+ 摘要展示 + 内联编辑写回） |

### Skills 发现路径（优先级由高到低）

1. `.pi/skills/`（项目级）
2. `.agents/skills/`（项目级，Agent Skills 规范）
3. `~/.pi/agent/skills/`（用户级，Pi agent 目录）
4. `~/.agents/skills/`（用户级，Agent Skills 规范）

### RPC 命令（M4′ / 多会话）

| 命令 | 用途 |
|------|------|
| `get_state` | 获取当前模型与 `sessionFile` / `sessionId` |
| `get_available_models` | 获取可用模型列表 |
| `set_model` | 切换模型（provider + modelId） |
| `new_session` | 新建 Pi 会话（壳侧同时新增活动槽） |
| `switch_session` | 按 `sessionPath` 切回已落盘会话 |

---

## 里程碑与状态

| 里程碑 | 主要功能 | 状态 |
|--------|---------|------|
| **M0′** | 仓骨架、egui 窗、pi_path stub | ✅ 已合入 |
| **M1′** | 三态 UI（Bundled Ready / Detected Local / Not Ready） | ✅ 已合入 |
| **M2′** | pi_rpc（JSONL RPC 真连）、流式聊天、停止、进程生命周期 | ✅ 已合入 |
| **M3′** | Token 用量条、工具 Trace 列表 | ✅ 已合入 |
| **M4′** | 管理面：Skills/MCP/插件/记忆/模型列表+开关 MVP | ✅ 已合入 |
| **M4′+** | 管理面全量落地：Skills 开关/状态推导、MCP enabled 字段、记忆内联编辑、模型写回+重启 | ✅ 已合入 |
| **M5′** | `#` 会话引用最小可用（ZCode 风格引用、chip UI、键盘导航） | ✅ 已合入 |
| **便携 bash / R0+** | 单 exe 便携包：内嵌 MinGit bash；`bash_path` 模块；PATH 注入；signing.md 完善 | ✅ 已合入 |
| **Claude 对标缺口** | 多会话列表、工具卡片、停止/重试、设置可达、cwd 切换 | ✅ 已合入（#11） |
| **M6′** | Android：仅当 Host 已自然长出共享面时升 Option A | **维持 C**（见下节） |
| **Android C** | `android/` debug 占位 APK（`assembleDebug`） | ✅ 已合入；v0.2.2 仍为此路径 |
| **R0 / v0.1.0** | Release（Win 便携 zip + 签名策略） | ✅ 已发布 |
| **v0.2.0** | 文档换轨 + 便携 bash + Claude 对标缺口合入后的发版 | ✅ 已发布 |
| **v0.2.1** | 中文显示、应用内密钥、会话恢复、设置页顺序 | ✅ 已发布 |
| **v0.2.2** | 首次模型与密钥引导、左下角设置对话框、Skills/MCP 卡片、自绘闪电底纹 | ✅ 已发布 |
| **v3 M0** | 设置抽屉七 Tab；厂商 + 自定义 API；主聊天模型下拉；firstrun 只配密钥 | ✅ 已合入 |
| **v3 M1** | 扩展市场浏览 / 搜索 / 类型滤镜；确认后 sidecar `pi install`；list/remove；`/reload` 提示 | ✅ 已合入 |
| **v3 M2** | Slim 五包预置进 `sidecar/pi-packages/`；NOTICE tarball SHA；操控三包默认关 + 一键启用壳 | ✅ 已合入 |
| **v3 M5** | 远程访问页：配对码/二维码、Cloudflare Tunnel 状态、已授权设备；本机 claim 钩子 | 本 PR |

---

## M6′ / Android：占位 APK，桌面钩子已在 M5 长出

v3 **M5** 已在桌面提供：

- 本机 loopback HTTP 网关（`remote_gateway`）
- 配对码 / 二维码 / `POST /v1/pair/claim`
- Cloudflare Tunnel 公网出口（`relay_mgr`）

这是 companion 真连所需的 Host 面，供 **M6-lite** 消费（见 `docs/companion-protocol.md`）。

**本里程碑仍不把 Android 升为真 companion**：`android/` 继续是 Kotlin Compose debug 占位壳，CI / Release 只跑 `assembleDebug`，不内嵌 Pi、不扫码、不连中继。完整会话以 Windows `desktop-rust/` 为准。M6 再把 APK 接到上述钩子。

Pi sidecar 仍是 Windows x64 的 `pi-windows-x64.zip`，不能原样塞进 APK。`pi_rpc` 仍只托管本机 `pi --mode rpc`。

---

## M4′+ 诚实限制

| 类目 | 限制说明 |
|------|---------|
| API Key | 设置抽屉的「模型」页可写入 `auth.json` 的字面 API 密钥。保存后只显示固定掩码，不回显明文，错误信息也不包含密钥。OAuth 不在此覆盖。环境变量与 `/login` 仍可用 |
| 会话持久化 | `~/.pi-light/sessions.json` 保存壳内会话并在启动时恢复。Pi 会话文件仍在则 `switch_session`，否则 `new_session` 并保留壳内记录。无会话树 / fork |
| Token 用量 | 依赖 Pi 上报 `message_update.usage` 或 `get_session_stats`；Pi 未上报时显示「用量未知 / 不支持」 |

## M5′ 诚实限制

| 类目 | 限制说明 |
|------|---------|
| 会话列表 RPC | Pi v1.4.0 无会话列表 RPC；当前会话消息始终可引用，历史会话通过本地文件探测（`~/.pi/sessions/*.json`） |
| Pi 会话文件格式 | Pi 未公开精确格式；按字段名尽力解析 `title` / `summary` / `sessionId`；解析失败则静默跳过 |
| 跨会话引用 | 仅支持在同一 Pi 进程会话内直接发送；不支持跨实例或远程会话引用 |
| 消息分支树 | M5′ 不实现消息分支树 UI；若 Pi 官方支持则后续里程碑跟进 |
| Taskboard 布局 | 不实现 Taskboard；不引入 CDP / 双后端 / dsh |

## v3 M5 诚实限制

| 类目 | 限制说明 |
|------|---------|
| 中继 | 依赖本机网络与公网可达性。临时 trycloudflare 无需 Cloudflare 账号，主机名每次启动会变。命名隧道 / Tailscale 需要账号。同一局域网不能代替公网中继 |
| 配对握手 | 桌面校验配对码后自动授权。手机 UI 与远程聊天是 M6。`/v1/chat` 返回 501 |
| 远程桌面 | 浏览 / 触控 / 全屏未开放 |
| cloudflared | 未钉平台（如 macOS）不提供应用内下载，只探测 PATH / 环境变量 |

## M5′ `#` 引用工作流

```
用户输入 "# " 或 "#keyword" → find_hash_trigger 检测触发词
      ↓
candidates_from_messages（当前会话消息，最多 20 条，最近在前）
candidates_from_pi_sessions（~/.pi/sessions/*.json，可选，无则静默跳过）
      ↓
RefChooserState.activate(all_candidates, query, hash_pos)
      ↓  ↑↓ 导航  Enter/Tab 确认  Esc 关闭  鼠标点击确认
RefChip 附在输入区上方（蓝色小 chip，点击可移除）
      ↓
do_send():
  build_prompt_text(chips, user_text)
  →  "[引用消息#N (角色)]\n内容\n\n用户输入"
  → client.send_prompt(effective_prompt)     # 现有 prompt RPC
```

---

## 洁净室边界

- 禁止引入 GPUI / Zed vendor 代码
- 禁止使用 pi-flash crate（含 pi-link）
- 禁止双后端切换 UI 或捆绑 dsh
- 完整禁令见 [AGENTS.md](../AGENTS.md)

---

## 测试策略

| 类型 | 位置 | 说明 |
|------|------|------|
| 单元测试 | `pi_rpc::tests` | JSONL 解析（text_delta、agent_settled、set_model 等） |
| 单元测试 | `pi_path::tests` | 路径探测三态 |
| 单元测试 | `chat::tests` | 聊天状态机 |
| 单元测试 | `session_insights::tests` | Token / Trace |
| 单元测试 | `admin_panel::tests` | 管理面：MCP 解析/写回、settings.json 写回、toggle 逻辑、skills 发现 |
| 单元测试 | `session_ref::tests` | M5′：RefCandidate / RefChooserState / 触发词解析 / build_prompt_text（共 24 测试） |
| 集成测试 | `tests/rpc_smoke.rs` | spawn → prompt → 流式 → settled；mock-pi 替代真实 Pi；M5′ 新增引用上下文 prompt 发送 / 空态 / 键盘确认流程 |

---

*最后更新：v3 M0（设置抽屉七 Tab、内置厂商 + 自定义 API、主聊天模型下拉）*
