# pi-light

**内嵌官方 Pi sidecar 的轻量 Rust 桌面 Agent（egui / eframe，Apache-2.0 洁净室）。**

用户入口是一个原生窗口：`pi-light.exe` 拉起官方 [Pi coding agent](https://github.com/earendil-works/pi)（`pi --mode rpc`），用 JSONL 做流式对话。正式发行是 Windows x64 便携 zip；Android companion 经公网中继写入同一桌面会话。

[![CI](https://github.com/ningyanhui/pi-light/actions/workflows/ci.yml/badge.svg)](https://github.com/ningyanhui/pi-light/actions/workflows/ci.yml)
[![Release](https://github.com/ningyanhui/pi-light/actions/workflows/release.yml/badge.svg)](https://github.com/ningyanhui/pi-light/actions/workflows/release.yml)
[![Android APK](https://github.com/ningyanhui/pi-light/actions/workflows/android-apk.yml/badge.svg)](https://github.com/ningyanhui/pi-light/actions/workflows/android-apk.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](./LICENSE)

当前版本：**v0.3.0**（Windows 便携包为主路径）。

---

## 目录

1. [它是什么](#1-它是什么)
2. [能做什么](#2-能做什么)
3. [安装与使用](#3-安装与使用)
4. [架构](#4-架构)
5. [从源码构建](#5-从源码构建)
6. [许可](#6-许可)
7. [文档索引](#7-文档索引)

---

## 1. 它是什么

pi-light 把官方 Pi（MIT，钉定 **v1.4.0**）以 sidecar 方式嵌进 [egui](https://github.com/emilk/egui) / eframe 桌面窗。

- **单一入口**：`pi-light.exe`。壳是 Apache-2.0 洁净室自写的 Rust 程序，主路径在 `desktop-rust/`。
- **本地进程**：`std::process::Command` 启动 `pi` / `pi.exe --mode rpc`，stdin/stdout 走官方 JSONL，后台线程读 stdout，UI 每帧 drain。
- **便携发行**：Windows zip 带上 Pi sidecar、MinGit bash、Slim 预置包与 Cloudflare Tunnel 客户端。正式包不含 Node 运行时，解压即可跑。
- **手机伴侣**：`android/` 是 debug companion APK，扫码或输码后经公网中继写入当前桌面 Pi 会话；不内嵌 Pi。

平台：Windows x64 为正式发行目标；Linux / macOS 可开发调试。

许可分层：壳 **Apache-2.0**；Pi 仍为 MIT；MinGit 仍为其原许可。摘要与版权见 [`NOTICE`](./NOTICE)。

---

## 2. 能做什么

### 对话工作台

找到 Pi 之后，主界面是侧栏 + 聊天区：

- **流式对话**：官方 JSONL RPC 真连；生成中可停止，空闲时可重发上一条 prompt。
- **多会话**：侧栏列表新建 / 切换；会话条也可切换。壳发 `new_session` / `switch_session`。列表、消息树、草稿和工作目录写入 `~/.pi-light/sessions.json`（`PI_LIGHT_STATE_PATH` 可改路径），下次启动读回；对应 Pi 会话文件仍在则切回。
- **同会话消息树**：编辑用户消息或重新生成助手会在同一会话内分出兄弟分支，可切换兄弟、展开消息树导航。树由壳维护（Pi v1.4.0 无同会话 `navigate_tree`）；改写或切换后对 Pi 回放当前路径。
- **工具卡片**：名称、状态、耗时、参数预览（`tool_execution_*`）。
- **`#` 引用**：输入 `#` 引用当前会话消息或本机历史会话文件，chip 拼入 prompt。
- **工作目录**：会话条菜单查看 / 切换 cwd，校验后重启 sidecar。
- **用量**：底栏展示 Pi 上报的 Token、工具 Trace 与吞吐（`message_update.usage` / `get_session_stats`）。
- **中文**：Windows 用系统微软雅黑等 CJK 字体作为 egui 回退；可用 `PI_LIGHT_CJK_FONT` 指定字体文件。
- **外观**：深色壳、自绘闪电底纹、Windows 嵌入品牌图标。

### 设置、模型与扩展

侧栏底部「设置」（输入区齿轮、顶栏账户按钮也可）打开**右侧抽屉**，七个 Tab：

| Tab | 做什么 |
|-----|--------|
| 模型 | 内置厂商卡片 +「自定义模型 API」（Provider ID / Base URL / 协议 / 模型目录）。密钥写入 Pi `auth.json`，自定义端点另写 `models.json`。保存后输入框清空，已存密钥只显示掩码 `••••••••`，不回显明文，也不写入错误信息。OAuth 登录不会被这个表单覆盖。环境变量和 Pi `/login` 仍可用。Unix 上 `auth.json` 权限 `0600`。 |
| Skills | 卡片列表；开关写回 `settings.json` 的 `skills` 数组；改后重启 sidecar。页内可列出并移除已安装 Pi 包。 |
| MCP | 卡片列表；读写官方 `enabled`（兼容旧 `disabled`）；新增写入 `mcp.json`；改后重启 sidecar。 |
| 扩展市场 | 浏览 / 搜索 / Extension·Skill·Prompt·Theme 滤镜；详情卡展示许可证、关键词、install 脚本等注册表元数据；确认后 sidecar 执行 `pi install npm:…`；已安装可 list / remove。目录来源与 [pi.dev/packages](https://pi.dev/packages) 相同（npm `pi-package`）。 |
| 记忆 | Project / User / Session 三层；内联编辑写回文件。 |
| 远程访问 | 允许手机远程、配对码 / 二维码、Cloudflare Tunnel 启停、已授权设备可撤销。浏览 / 触控 / 全屏远程桌面走同一中继。 |
| 通用 | Pi 程序位置等高级项（环境变量、sidecar 目录、PATH）收在折叠区。 |

侧栏另有「插件 / 知识库 / 数据」快捷入口，分别打开扩展市场、记忆、Skills。顶栏与作曲框都有模型下拉，选项来自 Pi 上报的可用模型。

**Slim 预置**（正式便携包 `sidecar/pi-packages/`，首次打开写入本地 path）：

- 默认启用：`pi-marketplace` 0.1.3、`pi-mcp-adapter` 7.0.0、`pi-web-access` 0.46.0、`pi-memory` 0.4.2、`pi-subagents` 0.84.0
- 预置文件、默认关闭，市场页可一键启用：`pi-browser-use` 0.12.15、`pi-chrome-connector` 0.2.2、`@injaneity/pi-computer-use` 0.5.1（启用前先走 Windows 权限向导：辅助功能 / 截屏 / 模拟输入）

钉版本与 tarball SHA-256 见 [`NOTICE`](./NOTICE) 与 [`docs/preset-packages.md`](./docs/preset-packages.md)。`PI_LIGHT_PACKAGES_DIR` 可覆盖预置树路径。

### 远程访问与 companion

桌面「远程访问」打开本机 loopback 网关（默认 `127.0.0.1:17831`），再用 **Cloudflare Tunnel** 引出公网 HTTPS。默认临时 trycloudflare，无需 Cloudflare 账号；命名隧道、Tailscale、自建入口可手填。

- 6 位配对码，TTL 10 分钟；二维码携带配对码与中继 URL。
- 手机扫码，或手输配对码 **和** 中继 URL；`POST /v1/pair/claim` 成功后进入已授权设备，可撤销。
- 已授权设备 `POST /v1/chat`（`Authorization: Bearer <token>`）写入**当前**桌面 Pi 会话。
- 已授权设备可进入远程桌面：浏览只看画面，触控 / 全屏把点按与滑动映射到桌面键鼠；画面走 `/v1/desktop/*`。
- 官方 Windows 包打入校验过 SHA 的 `cloudflared`；源码构建可在本页按钉版本下载。

Android companion 不内嵌 Pi，完整会话仍在桌面。配对、聊天与远程桌面协议见 [`docs/companion-protocol.md`](./docs/companion-protocol.md)，公网剧本见 [`docs/remote-acceptance.md`](./docs/remote-acceptance.md)。

---

## 3. 安装与使用

### Windows 便携包

从 [Releases](https://github.com/ningyanhui/pi-light/releases) 下载 `pi-light-windows-x64.zip`，解压后运行 `pi-light.exe`。不必再装 Node.js，也不必手敲 `pi install`。

```
pi-light-windows-x64/
├── pi-light.exe              # 唯一用户入口
├── NOTICE                    # Pi / MinGit / 预置包 / cloudflared 的 SHA-256 与版权
├── LICENSE                   # Apache-2.0
└── sidecar/
    ├── pi/pi.exe             # 官方 Pi v1.4.0（打包前校验 SHA-256）
    ├── bash/                 # MinGit（sh.exe；打包复制为 bash.exe）
    ├── pi-packages/          # Slim 五包（默认启用）+ 操控三包（默认关闭）
    └── relay/cloudflared.exe # Cloudflare Tunnel 客户端（打包前校验 SHA-256）
```

SHA-256 以仓库根目录 [`NOTICE`](./NOTICE) 与 [`docs/preset-packages.md`](./docs/preset-packages.md) 为准。Release 下载后核对摘要，失败则拒绝打包。签名与分发说明见 [`docs/signing.md`](./docs/signing.md)。

### 第一次打开

官方便携包已经带好 Pi。第一次运行的主界面是**配置模型 / API 密钥**：

1. 程序在 `pi-light.exe` 旁边找到 `sidecar/pi/pi.exe` 后，左侧欢迎「配置模型」，右侧是添加提供商表单。
2. 从内置厂商或「自定义模型 API」填写密钥与端点，保存后点「进入对话」。
3. 之后用侧栏「设置」打开抽屉，继续改模型、Skills、MCP、市场或远程访问。
4. 若这次没找到 Pi：主界面说明便携包应已包含它，并提供「重新探测」。自己指定 Pi 程序收在折叠高级区。

### 日常使用

1. 在输入框写消息，Enter 或发送按钮提交；生成中点「停止」，空闲时点「重试」。消息上可编辑 / 重新生成，并在同会话内切换分支。
2. 输入 `#` 挑选本会话或历史消息，chip 会跟 prompt 一起发出。
3. 侧栏或会话条新建 / 切换会话；会话条「⋯」改工作目录。
4. 顶栏或输入区切换模型。
5. 需要装包时打开「扩展市场」，在详情卡确认后再安装。

### 手机远程聊天与桌面

1. 桌面设置抽屉 → **远程访问** → 打开「允许手机远程」，等到中继状态为已连接（默认出现 `https://….trycloudflare.com`）。
2. 手机使用能访问该公网主机名的网络（公网验收用蜂窝数据）。
3. 打开 companion，扫描桌面二维码，或手输 6 位配对码和中继 URL。
4. 进入会话发一条消息；桌面同一 Pi 会话应出现来回。
5. 首页「远程桌面」进入浏览 / 触控 / 全屏，查看并（在后两档）操控桌面。

临时 trycloudflare 每次启动主机名会变，重新扫码即可。命名隧道可固定主机名。配置落在 `~/.pi-light/remote.json`（`PI_LIGHT_REMOTE_PATH` 可覆盖）。

Android APK 由 [android-apk](https://github.com/ningyanhui/pi-light/actions/workflows/android-apk.yml) 工作流构建 `assembleDebug` 产物。本地步骤见 [`android/README.md`](./android/README.md)。

### Pi 程序位置（高级）

只有便携包里没有 Pi，或你要改用别的程序时才需要：

1. 环境变量 `PI_LIGHT_PI_BIN`
2. `{exe_dir}/sidecar/pi/pi.exe`（Windows）或 `{exe_dir}/sidecar/pi/pi`（其他）
3. 本机 `PATH` 中的 `pi` / `pi.exe`

cloudflared 探测顺序：`PI_LIGHT_CLOUDFLARED` → `{exe_dir}/sidecar/relay/` → `~/.pi-light/relay/` → `PATH`。

### 开发调试

```bash
git clone https://github.com/ningyanhui/pi-light.git
cd pi-light/desktop-rust
export PI_LIGHT_PI_BIN=/path/to/pi   # 或 Windows 上的 pi.exe
cargo run
```

---

## 4. 架构

主路径是 `desktop-rust/`（egui / eframe）。壳 spawn Pi，JSONL 走管道，后台线程读 stdout。

```
┌─────────────────────────────────────────────────────────────┐
│  desktop-rust/   egui + eframe                               │
│  侧栏：品牌 / 会话 / 插件·知识库·数据 / 设置                    │
│  顶栏：模型下拉 · 重新探测 · 账户                                │
│  主区：流式聊天 + 工具卡片 + Tokens / Trace                    │
│  抽屉：模型 / Skills / MCP / 扩展市场 / 记忆 / 远程访问 / 通用   │
└────────────────────────────┬────────────────────────────────┘
                             │ Command + JSONL stdin/stdout
                             ▼
┌─────────────────────────────────────────────────────────────┐
│  Pi sidecar（官方 MIT，不改源码）                              │
│  pi --mode rpc   可选 cwd；已有会话文件则 switch_session       │
└─────────────────────────────────────────────────────────────┘

android/   Kotlin Compose companion（配对 + /v1/chat + /v1/desktop）
```

```
desktop-rust/src/
├── main.rs              # egui App、侧栏 / 顶栏 / 会话条 / 输入区
├── theme.rs             # 深色壳与共用控件
├── app_icon.rs          # 窗口与 Windows exe 品牌图标
├── fonts.rs             # 系统 CJK 字体回退
├── pi_path.rs           # Pi 三态路径探测
├── bash_path.rs         # 便携 bash 探测与 PATH 注入
├── pi_rpc.rs            # JSONL RPC（spawn 可带 cwd）
├── chat.rs              # 聊天状态机 + SessionHub
├── message_tree.rs      # 同会话消息树（壳侧）
├── session_store.rs     # 会话历史落盘与启动恢复
├── session_insights.rs  # Token / Trace
├── admin_panel.rs       # 设置抽屉（七 Tab）
├── ext_market.rs        # 扩展市场浏览 / 详情 / 确认安装 / 一键启用
├── pi_pkg.rs            # sidecar `pi install` / `list` / `remove`
├── preset_packages.rs   # Slim 预置同步、操控包启用
├── api_keys.rs          # 应用内 API 密钥（掩码，不回显明文）
├── provider_catalog.rs  # 内置厂商 + 自定义 API / models.json
├── onboarding.rs        # 首次打开：引导配置模型与密钥
├── session_ref.rs       # # 引用
├── remote_access.rs     # 远程访问页：配对码 / 二维码 / 设备
├── desktop_stream.rs    # JPEG 编码、Windows 主屏捕获、键鼠注入
├── remote_gateway.rs    # 本机 loopback 控制面（claim + /v1/chat + /v1/desktop）
├── relay_mgr.rs         # cloudflared 探测、下载、quick tunnel
└── bin/mock_pi.rs       # 测试替身
```

完整层次与模块职责见 [`docs/architecture.md`](./docs/architecture.md)。

---

## 5. 从源码构建

要求：Rust stable（edition 2021）；Linux 构建需系统 OpenGL / X11 头文件（见 CI）。

```bash
cd desktop-rust
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

集成测试使用 `mock-pi` 替身，不依赖真实 Pi 二进制。

Android companion（JDK 17+、Android SDK；`testDebugUnitTest` + `assembleDebug`）：

```bash
cd android
./gradlew assembleDebug
# 产物: app/build/outputs/apk/debug/app-debug.apk
```

贡献约定见 [`CONTRIBUTING.md`](./CONTRIBUTING.md) 与 [`AGENTS.md`](./AGENTS.md)。

---

## 6. 许可

本项目以 **Apache-2.0** 发布。

- Pi sidecar：[earendil-works/pi](https://github.com/earendil-works/pi)（MIT），嵌入不改源码
- egui / eframe：[emilk/egui](https://github.com/emilk/egui)（MIT OR Apache-2.0，本项目用 Apache-2.0 条款）
- MinGit for Windows：便携 bash 环境，原许可见包内与 [`NOTICE`](./NOTICE)

致谢见 [`ACKNOWLEDGEMENTS.md`](./ACKNOWLEDGEMENTS.md)。

---

## 7. 文档索引

| 文档 | 说明 |
|------|------|
| [`docs/architecture.md`](./docs/architecture.md) | 架构、模块、里程碑 |
| [`docs/remote-acceptance.md`](./docs/remote-acceptance.md) | 公网远程配对、中继启停与验收剧本 |
| [`docs/companion-protocol.md`](./docs/companion-protocol.md) | companion 配对 / claim / `/v1/chat` / `/v1/desktop` |
| [`docs/preset-packages.md`](./docs/preset-packages.md) | Slim / 操控预置包钉版本与 tarball SHA-256 |
| [`docs/signing.md`](./docs/signing.md) | Windows / Android 签名与分发策略 |
| [`android/README.md`](./android/README.md) | companion 构建与配对步骤 |
| [`AGENTS.md`](./AGENTS.md) | 洁净室规则（给贡献者与编码代理） |
| [`CONTRIBUTING.md`](./CONTRIBUTING.md) | 贡献指南 |
| [`ACKNOWLEDGEMENTS.md`](./ACKNOWLEDGEMENTS.md) | 第三方致谢 |
| [`NOTICE`](./NOTICE) | Pi / MinGit / 预置包 SHA-256 与版权 |
| [`THIRD-PARTY-NOTICES`](./THIRD-PARTY-NOTICES) | 依赖许可清单 |
