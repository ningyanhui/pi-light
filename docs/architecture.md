# pi-light 架构说明

> **当前版本**：v0.2.0 — M1′–M5′ + 便携 bash + Claude 对标缺口；M6′ 维持 Android Option C
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
│  │ 新建/切换 │ │  流式聊天 + 工具卡片 + Token/Trace                 │ │
│  │ 工作目录  │ │  输入框 + 发送 / 停止 / 重试                       │ │
│  └──────────┘ └──────────────────────────────────────────────────┘ │
│  顶栏：标题 + Pi 状态灯                                              │
│  底栏左下角：⚙ 设置（居中对话框：模型与密钥 / Skills / MCP / 记忆 / 高级）│
│  底栏右侧：重新探测                                                  │
│                                                                       │
│  pi_path 模块：路径解析（env → sidecar → PATH）                        │
│  pi_rpc 模块（M2′+M4′+）：JSONL over stdin/stdout                     │
│  admin_panel 模块（M4′+）：壳侧全量：Skills开关/MCP/记忆编辑/模型      │
└────────────────────────────┬────────────────────────────────────────┘
                             │ std::process::Command（stdin/stdout piped）
                             │ 后台线程读 stdout → mpsc channel
                             │ UI 每帧 drain channel
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│  Pi sidecar（官方 MIT，不修改）                                        │
│  spawn: pi --mode rpc  （可选 cwd；启动后 new_session 得到干净首槽）   │
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
│   │   ├── bash_path.rs          # 便携 bash 路径探测 + PATH 注入辅助（M5′+）
│   │   ├── pi_path.rs            # Pi 三态路径探测
│   │   ├── pi_rpc.rs             # JSONL RPC 客户端（spawn 时注入 bash PATH）
│   │   ├── chat.rs               # 聊天状态机（M2′）
│   │   ├── session_insights.rs   # Token / Trace（M3′）
│   │   ├── admin_panel.rs        # 管理面（M4′）
│   │   ├── onboarding.rs         # 首次打开：模型引导 / 缺程序说明
│   │   └── bin/mock_pi.rs        # 集成测试用 Pi 替身
│   └── tests/rpc_smoke.rs        # 集成烟测
├── android/                      # Option C 占位壳
├── docs/
│   ├── architecture.md
│   └── signing.md                # Windows 签名策略与 Authenticode 占位
├── scripts/
│   ├── fetch-pi-sidecar.ps1      # Pi sidecar 下载/SHA-256 校验
│   ├── fetch-bash-sidecar.ps1    # MinGit 下载/SHA-256 校验（便携 bash）
│   └── verify_pi_sha256.sh       # Linux/macOS CI 辅助校验
└── .github/workflows/            # CI / Release / Android APK
```

---

## 模块职责

| 模块 | 职责 |
|------|------|
| `bash_path` | 便携 bash 路径探测（捆绑 MinGit → 系统 PATH）；`bundled_path_dirs()` 供 `pi_rpc` spawn 注入 PATH |
| `pi_path` | Pi 二进制路径探测三态；`PiProbe` 枚举 |
| `pi_rpc` | `PiRpcClient`：spawn（可选 cwd，注入 bash PATH）→ JSONL；`validate_cwd` 入口单次校验；`new_session` / `switch_session` |
| `chat` | 聊天状态机 + `SessionHub`（本进程多会话）；重试上一条 prompt |
| `session_insights` | Token 用量（`get_session_stats` + `message_update.usage`）；工具 Trace（含 args 预览） |
| `admin_panel` | M4′+ 设置面：四 tab（Skills/MCP/记忆/模型）；Skills 开关写 settings；MCP enabled 字段；记忆内联编辑；模型写回+重启 |
| `onboarding` | 首次打开主路径：Pi 就绪时进入模型与密钥；未就绪时安装说明保持折叠 |
| `session_ref` | M5′ `#` 会话引用：`RefCandidate` / `RefChooserState` / `RefChip`；候选构建；prompt 拼合 |
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
├── NOTICE                        # SHA-256 + 版权声明（Pi 和 MinGit）
├── LICENSE                       # Apache-2.0
└── sidecar/
    ├── pi/
    │   └── pi.exe                # Pi sidecar（MIT，由 earendil-works/pi 发布）
    └── bash/                     # MinGit（GPL-2.0 + 例外，便携 bash/MSYS 环境）
        ├── usr/bin/              # sh.exe；Release 复制为 bash.exe 供 Pi 查找
        ├── mingw64/bin/          # 工具链
        └── cmd/                  # git.exe
```

### Pi agent 目录（M4′ 发现路径）

默认：`~/.pi/agent/`（可被 `PI_CODING_AGENT_DIR` 覆盖）

| 文件/目录 | 用途 |
|-----------|------|
| `settings.json` | defaultProvider / defaultModel 读写；`skills` 数组排除列表读写（M4′+） |
| `mcp.json` | MCP 服务列表；官方 `enabled` 字段读写，兼容旧 `disabled`（M4′+） |
| `models.json` | 自定义端点（只读展示） |
| `auth.json` | API 凭据（只检测存在性，不读内容，不展示明文） |
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
| **Android C** | `android/` debug 占位 APK（`assembleDebug`） | ✅ 已合入；v0.2.0 仍为此路径 |
| **R0 / v0.1.0** | Release（Win 便携 zip + 签名策略） | ✅ 已发布 |
| **v0.2.0** | 文档换轨 + 便携 bash + Claude 对标缺口合入后的发版 | 本发版 |

---

## M6′ 决策：维持 Android Option C

桌面 Host（`desktop-rust` egui + Pi RPC）**没有**长出 Android 真 Host 所需的共享面，因此 **不升 Option A**。

Option A 需要的最小面（LAN / HTTP 配对、可被手机连上的 Host API、非本机 stdin/stdout 的会话通道）在 tip 上均不存在：

- `pi_rpc` 只做 `std::process::Command` 拉起本机 `pi --mode rpc`，JSONL 走 stdin/stdout
- 仓库内无 HTTP 服务、无 WebSocket、无配对协议、无 Android 可调用的 Host crate
- 正式 sidecar 是 Windows x64 的 `pi-windows-x64.zip`，不能原样塞进 APK

按 StayNing：零大改则跳过。强行做 A 等于新写一层 Host 协议 + 手机联网，超出「接线」。

**Android 继续 Option C**：`android/` 为 Kotlin Compose debug 占位壳，CI / Release 只跑 `assembleDebug`，不内嵌 Pi、不连桌面。完整会话以 Windows `desktop-rust/` 为准。

---

## M4′+ 诚实限制

| 类目 | 限制说明 |
|------|---------|
| API Key 编辑 | 不在 UI 存储或展示明文 key；请用环境变量或 Pi `/login` |
| 会话持久化 | 启动时 `new_session` 得到干净首槽；本进程活动槽可新建/切换；历史文件只走 `#` 引用 |
| Token 用量 | 依赖 Pi 上报 `message_update.usage` 或 `get_session_stats`；Pi 未上报时显示「用量未知 / 不支持」 |

## M5′ 诚实限制

| 类目 | 限制说明 |
|------|---------|
| 会话列表 RPC | Pi v1.4.0 无会话列表 RPC；当前会话消息始终可引用，历史会话通过本地文件探测（`~/.pi/sessions/*.json`） |
| Pi 会话文件格式 | Pi 未公开精确格式；按字段名尽力解析 `title` / `summary` / `sessionId`；解析失败则静默跳过 |
| 跨会话引用 | 仅支持在同一 Pi 进程会话内直接发送；不支持跨实例或远程会话引用 |
| 消息分支树 | M5′ 不实现消息分支树 UI；若 Pi 官方支持则后续里程碑跟进 |
| Taskboard 布局 | 不实现 Taskboard；不引入 CDP / 双后端 / dsh |

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

*最后更新：v0.2.0（M6′ 维持 Android Option C；README 产品向重写）*
