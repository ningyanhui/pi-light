# pi-light

**高性能通用桌面 Agent：egui 原生壳 + 内嵌官方 Pi sidecar。**

目标是替代 ChatGPT Desktop / Claude Desktop 一类对话壳——安装即用、本地进程、工具可见、多会话，而不是再套一层浏览器。

[![CI](https://github.com/ningyanhui/pi-light/actions/workflows/ci.yml/badge.svg)](https://github.com/ningyanhui/pi-light/actions/workflows/ci.yml)
[![Release](https://github.com/ningyanhui/pi-light/actions/workflows/release.yml/badge.svg)](https://github.com/ningyanhui/pi-light/actions/workflows/release.yml)
[![Android APK](https://github.com/ningyanhui/pi-light/actions/workflows/android-apk.yml/badge.svg)](https://github.com/ningyanhui/pi-light/actions/workflows/android-apk.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](./LICENSE)

当前版本：**v0.2.1**（Windows 便携包为主路径）。

---

## 目录

1. [它是什么](#1-它是什么)
2. [安装与运行](#2-安装与运行)
3. [已交付能力](#3-已交付能力)
4. [从源码构建](#4-从源码构建)
5. [架构要点](#5-架构要点)
6. [诚实说明](#6-诚实说明)
7. [许可](#7-许可)
8. [文档索引](#8-文档索引)

---

## 1. 它是什么

pi-light 把官方 [Pi coding agent](https://github.com/earendil-works/pi)（MIT，钉定 v1.4.0）以 sidecar 方式嵌进 [egui](https://github.com/emilk/egui) / eframe 桌面窗：

- 用户入口是一个 `pi-light.exe`，不是 Node 项目，也不是浏览器壳
- 启动命令：`pi` / `pi.exe --mode rpc`，经官方 JSONL 协议流式对话
- 便携包同时带上 Pi sidecar 与 MinGit bash 环境，Windows 上解压即可跑
- 许可：**Apache-2.0**（洁净室自写；Pi 仍为 MIT，MinGit 仍为其原许可）

平台：Windows x64 为正式发行目标；Linux / macOS 可开发调试。

---

## 2. 安装与运行

### 推荐：Windows 便携包

从 [Releases](https://github.com/ningyanhui/pi-light/releases) 下载 `pi-light-windows-x64.zip`，解压后运行 `pi-light.exe`。

包内就是全部运行时，无需再装 Node.js：

```
pi-light-windows-x64/
├── pi-light.exe              # 唯一用户入口
├── NOTICE                    # Pi / MinGit 的 SHA-256 与版权
├── LICENSE                   # Apache-2.0
└── sidecar/
    ├── pi/pi.exe             # 官方 Pi v1.4.0（打包前校验 SHA-256）
    └── bash/                 # MinGit（sh.exe；打包复制为 bash.exe；SHA 见 NOTICE）
```

SHA-256 以仓库根目录 [`NOTICE`](./NOTICE) 为准。Release 流水线校验失败则拒绝打包。签名策略见 [`docs/signing.md`](./docs/signing.md)（当前无 Authenticode）。

### Pi 路径探测（开发或自备二进制时）

1. 环境变量 `PI_LIGHT_PI_BIN`
2. `{exe_dir}/sidecar/pi/pi.exe`（Windows）或 `{exe_dir}/sidecar/pi/pi`（其他）
3. 本机 `PATH` 中的 `pi` / `pi.exe`

### 开发调试

```bash
git clone https://github.com/ningyanhui/pi-light.git
cd pi-light/desktop-rust
export PI_LIGHT_PI_BIN=/path/to/pi   # 或 Windows 上的 pi.exe
cargo run
```

---

## 3. 已交付能力

相对 v0.1.0，v0.2.0 覆盖 M1′–M5′、便携 bash，以及 Claude Desktop 对标缺口（多会话 / 工具卡 / 重试 / cwd）。v0.2.1 在此之上补上系统 CJK 字体、应用内 API 密钥和会话恢复。

| 能力 | 说明 |
|------|------|
| 三态启动 | Bundled Ready / Detected Local / Not Ready；可重新探测 |
| 流式聊天 | 官方 JSONL RPC 真连 Pi；停止生成 |
| 多会话 | 左侧列表：新建 / 切换；Pi `new_session` / `switch_session` |
| 工具卡片 | 名称 / 状态 / 耗时 / 参数预览（`tool_execution_*`） |
| 停止 / 重试 | 生成中停止；空闲时可重发上一条 prompt |
| `#` 引用 | 输入 `#` 引用当前会话消息或本机历史会话文件，chip 拼入 prompt |
| 设置 / 模型 | 标题栏「⚙ 设置」：密钥、模型、Skills、MCP、记忆 |
| API 密钥 | 「密钥」页写入 Pi `auth.json`；保存后输入框清空，已存密钥只显示掩码 |
| 会话恢复 | 会话列表与消息写入 `~/.pi-light/sessions.json`，下次启动读回；Pi 会话文件仍在则 `switch_session` |
| 中文显示 | Windows 用系统微软雅黑等 CJK 字体作为 egui 回退，避免方框；不捆绑字体文件 |
| Skills | 本机发现；读写 `settings.json` 的 `skills` 数组；改后重启 sidecar |
| MCP | 发现 `mcp.json`；读写官方 `enabled`（兼容旧 `disabled`）；改后重启 sidecar |
| 记忆 | Project / User / Session 三层；内联编辑写回文件 |
| 工作目录 | 会话栏查看 / 切换 cwd；入口单次校验后 spawn Pi |
| Trace / Token | 底部用量条；工具 Trace 列表 |
| 便携 bash | 便携包注入 MinGit `PATH`，Pi 跑 shell 命令时使用 |

Android 见第 6 节：维持 Option C 占位 APK。

---

## 4. 从源码构建

要求：Rust stable（edition 2021）；Linux 构建需系统 OpenGL / X11 头文件（见 CI）。

```bash
cd desktop-rust
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
```

集成测试使用 `mock-pi` 替身，不依赖真实 Pi 二进制。

Android 占位壳（仅 `assembleDebug`）：

```bash
cd android
./gradlew assembleDebug
# 产物: app/build/outputs/apk/debug/app-debug.apk
```

---

## 5. 架构要点

主路径是 `desktop-rust/`（egui/eframe）。壳用 `std::process::Command` 拉起 Pi，stdin/stdout 走 JSONL，后台线程读 stdout，UI 每帧 drain。

```
desktop-rust/src/
├── main.rs              # egui App、事件循环
├── fonts.rs             # 系统 CJK 字体回退
├── pi_path.rs           # Pi 三态路径探测
├── bash_path.rs         # 便携 bash 探测与 PATH 注入
├── pi_rpc.rs            # JSONL RPC（spawn 可带 cwd）
├── chat.rs              # 聊天状态机 + SessionHub
├── session_store.rs     # 会话历史落盘与启动恢复
├── session_insights.rs  # Token / Trace
├── admin_panel.rs       # 设置面
├── api_keys.rs          # 应用内 API 密钥（掩码，不回显明文）
├── session_ref.rs       # # 引用
└── bin/mock_pi.rs       # 测试替身
```

完整层次与里程碑见 [`docs/architecture.md`](./docs/architecture.md)。

---

## 6. 诚实说明

- **API 密钥**：设置里的「密钥」页可以把提供商 API 密钥写入 Pi 的 `auth.json`（`type: api_key`）。保存后输入框清空，已存密钥只显示固定掩码 `••••••••`，界面不回显明文，也不在错误信息里打印密钥。OAuth 登录不会被这个表单覆盖。环境变量和 Pi 的 `/login` 仍然可用。文件权限在 Unix 上为 `0600`。
- **会话持久化**：壳把会话列表、消息、输入草稿、工作目录和 Pi 会话文件路径写到 `~/.pi-light/sessions.json`（可用 `PI_LIGHT_STATE_PATH` 改位置）。下次启动读回。若对应会话文件还在，则对 Pi 发 `switch_session`；否则保留壳内记录并 `new_session`。没有会话树 / fork。`#` 引用仍然可用。
- **中文**：不捆绑字体。Windows 优先加载系统微软雅黑（`msyh.ttc` / `msyh.ttf`）作为 egui 回退。可用 `PI_LIGHT_CJK_FONT` 指定字体文件。
- **Android = Option C**：桌面 Host 没有长出 Android 真 Host 所需的共享面（仍是本机 stdin/stdout spawn，无 LAN / HTTP 配对 API）。M6′ **不升 Option A**，Android 继续是 debug 占位 APK（`assembleDebug`，不内嵌 Pi）。决策记录见 [`docs/architecture.md`](./docs/architecture.md)。
- **Token 用量**：依赖 Pi 上报 `message_update.usage` 或 `get_session_stats`；未上报时显示「用量未知 / 不支持」。
- **不做**：双后端切换、捆绑 dsh / Node runtime、CDP、Taskboard、把 Tauri `desktop/` 当主路径。

---

## 7. 许可

本项目以 **Apache-2.0** 发布。

- Pi sidecar：[earendil-works/pi](https://github.com/earendil-works/pi)（MIT），嵌入不改源码
- egui / eframe：[emilk/egui](https://github.com/emilk/egui)（MIT OR Apache-2.0，本项目用 Apache-2.0 条款）
- MinGit for Windows：便携 bash 环境，原许可见包内与 [`NOTICE`](./NOTICE)

致谢见 [`ACKNOWLEDGEMENTS.md`](./ACKNOWLEDGEMENTS.md)。

---

## 8. 文档索引

| 文档 | 说明 |
|------|------|
| [`docs/architecture.md`](./docs/architecture.md) | 架构、模块、里程碑、M6′ 决策 |
| [`docs/signing.md`](./docs/signing.md) | Windows / Android 签名与分发策略 |
| [`AGENTS.md`](./AGENTS.md) | 洁净室规则（给贡献者与编码代理） |
| [`ACKNOWLEDGEMENTS.md`](./ACKNOWLEDGEMENTS.md) | 第三方致谢 |
| [`NOTICE`](./NOTICE) | Pi / MinGit SHA-256 与版权 |
| [`THIRD-PARTY-NOTICES`](./THIRD-PARTY-NOTICES) | 依赖许可清单 |
