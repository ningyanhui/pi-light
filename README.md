# pi-light

**pi-light — 内嵌 Pi 的轻量 Rust Shell**

用 Rust（egui/eframe）构建的轻量桌面客户端，把官方 [Pi coding agent](https://github.com/earendil-works/pi) 直接打进壳里——安装即用，无需手动配置外部 runtime。行为与信息架构参考 [pi-flash](https://github.com/danielbill/pi-flash)，代码 Apache-2.0 洁净室自写。

[![CI](https://github.com/ningyanhui/pi-light/actions/workflows/ci.yml/badge.svg)](https://github.com/ningyanhui/pi-light/actions/workflows/ci.yml)
[![Release](https://github.com/ningyanhui/pi-light/actions/workflows/release.yml/badge.svg)](https://github.com/ningyanhui/pi-light/actions/workflows/release.yml)
[![Android APK](https://github.com/ningyanhui/pi-light/actions/workflows/android-apk.yml/badge.svg)](https://github.com/ningyanhui/pi-light/actions/workflows/android-apk.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](./LICENSE)

---

## 目录

1. [定位](#1-定位)
2. [截图](#2-截图)
3. [与 pi-flash 的关系](#3-与-pi-flash-的关系)
4. [环境与系统要求](#4-环境与系统要求)
5. [开始使用](#5-开始使用)
6. [能做什么 / 不能做什么](#6-能做什么--不能做什么)
7. [构建](#7-构建)
8. [架构](#8-架构)
9. [安全边界](#9-安全边界)
10. [文档索引](#10-文档索引)
11. [已知限制](#11-已知限制)
12. [许可与致谢](#12-许可与致谢)

---

## 1. 定位

pi-light 是**内嵌 Pi 的轻量 Rust Shell**：

- **UI 栈**：[egui](https://github.com/emilk/egui) / eframe（Apache-2.0）——原生渲染，无 Electron/WebView 开销
- **内嵌 runtime**：官方 MIT 许可的 [Pi v1.4.0](https://github.com/earendil-works/pi/releases/tag/v1.4.0)，以 sidecar 方式随壳分发
- **通信协议**：官方 JSONL RPC（`pi --mode rpc`），流式响应
- **许可**：Apache-2.0（洁净室；禁止引入 GPL 代码）
- **目标平台**：Windows x64（主），Linux/macOS 可开发调试

---

## 2. 截图

> **M2′ 阶段**：egui 窗口 + 真连 Pi RPC 聊天（流式响应、停止按钮、进程生命周期管理）。

```
╔════════════════════════════════════════════════════════════╗
║  pi-light — 内嵌 Pi 的轻量 Rust Shell  ●  [⟳ 重新探测]    ║
╠════════════════════════════════════════════════════════════╣
║                                                            ║
║  You: 请列出当前目录下的文件                                  ║
║  ─────────────────────────────────────────────────────     ║
║  Pi: 好的，我来帮您列出文件……                                 ║
║       [tool: bash] ls -la                                  ║
║       （工具执行中…）                                        ║
║  Pi: 当前目录包含以下文件：README.md …▌                      ║
║                                                            ║
╠════════════════════════════════════════════════════════════╣
║  [输入框：请输入消息…]              [发送]  [■ 停止]          ║
╚════════════════════════════════════════════════════════════╝
```

Pi 未就绪时保留 M1′ 三态指引界面（Bundled Ready / Detected Local / Not Ready）。

---

## 3. 与 pi-flash 的关系

pi-light **不是** pi-flash 的 fork，与 pi-flash 也没有任何官方关联。

| 维度 | pi-flash | pi-light |
|------|----------|----------|
| 许可 | GPL-3.0-or-later（Zed 派生） | Apache-2.0（洁净室） |
| UI 栈 | GPUI / gpui-component | egui / eframe |
| Pi 集成 | npm `@earendil-works/pi-coding-agent` + Node | 官方 Win standalone（`pi.exe --mode rpc`） |
| 代码关系 | — | 零代码共享 |

pi-light 的会话优先布局、sidecar 捆绑行为、Pi 进程生命周期管理等设计**以 pi-flash 为 UX 灵感**，在 Apache-2.0 许可下独立实现。

---

## 4. 环境与系统要求

### Windows（主要目标平台）

| 依赖 | 版本 | 说明 |
|------|------|------|
| Windows | 10 / 11 x64 | 必须 |
| Git Bash | 最新 | Pi 内部使用 bash 工具（官方要求） |
| Pi sidecar | v1.4.0 | 随 Release 捆绑，或手动放置 |

> Pi 官方文档说明 Windows 上需要 Git Bash 提供 bash 工具支持。详见 [Pi 官方文档](https://github.com/earendil-works/pi)。

### 开发环境（构建）

| 依赖 | 版本 |
|------|------|
| Rust | 1.75+ |
| cargo | 随 Rust 工具链 |

---

## 5. 开始使用

### Pi 就绪三态说明

pi-light 启动时自动探测 Pi 二进制，并在顶部状态栏显示三种状态之一。
找到 Pi 后自动启动 `pi --mode rpc --no-session` 并进入聊天模式。
点击「⟳ 重新探测」可在不重启的情况下重新扫描并重连 Pi。

| 状态 | 含义 | 来源 |
|------|------|------|
| **✓ 内嵌 sidecar（Bundled Ready）** | 在 `{exe_dir}/sidecar/pi/` 下找到 Pi | 正式 Release 默认路径 |
| **~ 本机（Detected Local）** | 通过环境变量或 PATH 找到 Pi | 开发环境 / 本机安装 |
| **⚠ 未就绪（Not Ready）** | 未找到 Pi，显示放置指引 | — |

Pi 就绪后，主区切换为**聊天界面**：输入框输入消息 → 按 Enter 或点「发送」→ Pi 流式返回 → 点「■ 停止」可中止当前生成。

### 方式一：正式 Release（推荐）

1. 打开 [Releases](https://github.com/ningyanhui/pi-light/releases)，下载 **`pi-light-windows-x64.zip`**（及可选 Android **`app-debug.apk`**）
2. 解压 zip 到任意目录（目录内应含 `pi-light.exe` 与 `sidecar/pi/pi.exe`）
3. 运行 `pi-light.exe`

> 正式 Release 已将 **Pi v1.4.0** 打入 `sidecar/pi/`；流水线在打包前校验 SHA-256（与 [NOTICE](./NOTICE) 一致），校验失败不会发布该 zip。  
> Windows / Android **均无商店级代码签名**，说明见 [docs/signing.md](./docs/signing.md)。

### 方式二：开发构建 + 手动提供 Pi

**1. 克隆仓库**

```bash
git clone https://github.com/ningyanhui/pi-light
cd pi-light
```

**2. 提供 Pi 二进制（任选其一，优先级从高到低）**

**优先级 1 — 环境变量（最简单）：**

```powershell
$env:PI_LIGHT_PI_BIN = "C:\path\to\pi.exe"
```

**优先级 2 — 捆绑 sidecar 目录（推荐，校验 SHA-256 后）：**

```powershell
# 下载
$url = "https://github.com/earendil-works/pi/releases/download/v1.4.0/pi-windows-x64.zip"
Invoke-WebRequest -Uri $url -OutFile "$env:TEMP\pi.zip"

# 校验 SHA-256（期望值如下，必须匹配）
# 0f7256ea8bdac3af20012b94fdc1252a31cc6dc0d2f9b9eb539555d22e3e321b
(Get-FileHash "$env:TEMP\pi.zip" -Algorithm SHA256).Hash

# 解压到 sidecar 目录，启动后显示「Bundled Ready」
Expand-Archive -Path "$env:TEMP\pi.zip" -DestinationPath "desktop-rust\sidecar\pi"
```

**优先级 3 — 系统 PATH（可选）：**

将 `pi.exe` 或 `pi` 加入系统 PATH，启动后显示「Detected Local」状态。

**3. 构建并运行**

```bash
cd desktop-rust
cargo run
```

Pi 就绪后，主区自动切换为聊天界面。如果 Pi 需要 API Key，请先按 Pi 官方文档配置（`pi auth login`）。

---

## 6. 能做什么 / 不能做什么

### 当前（M3′ 阶段）

| 能做 | 不能做 |
|------|--------|
| 显示三态 Pi 就绪 UI（Bundled / Detected / Not Ready） | 商店签名 / Play 正式包（见 [docs/signing.md](./docs/signing.md)） |
| 从 [Releases](https://github.com/ningyanhui/pi-light/releases) 下载 **Windows 便携 zip**（含 SHA-256 校验后的 Pi v1.4.0） | — |
| Pi 就绪时自动启动 `pi --mode rpc --no-session` | skill / MCP 管理面板（M4′） |
| 流式聊天：输入 → Enter/发送 → 流式文本 → `agent_settled` | 完整 IDE（文件树 + 多终端 + Git） |
| **■ 停止**按钮：发送 `abort` 命令，中止当前生成 | 双后端（Pi + Harness）切换 |
| **Trace / Token**：`message_update.usage`、`tool_execution_*`、`get_session_stats`（有字段则显示；缺字段三态降级） | 内嵌 Taskboard / CDP 注入 |
| 路径探测（env → sidecar → PATH），「⟳ 重新探测」无需重启 | 捆绑 dsh / deepseek-harness |
| 关窗时关闭 stdin → 等待 → kill，无孤儿/僵尸进程 | Android 上与 Pi 对话 / Host 配对 |
| `cargo check` + `cargo test` + `clippy` + `fmt` 全绿 | 把未校验 Pi 二进制打进 git |
| Android **debug 占位 APK**（Option C：首页 + 设置 stub，无 Pi） | — |

### 上一阶段（M2′，已包含在 M3′ 基线中）

| 能做 | 不能做 |
|------|--------|
| 同上聊天与三态 Pi 就绪 | Trace / Token 用量条（已在 M3′ 提供） |

### 计划中（里程碑交付后）

| 能做 | 不能做（明确不在范围） |
|------|----------------------|
| 与 Pi 流式对话（M2′，桌面） | 内嵌 Taskboard / CDP 注入 |
| skill / MCP 列表开关（M4′） | 双后端（Pi + Harness）切换 |
| Android Host 或同构真连（M6′ stretch） | 完整 IDE（文件树 + 多终端 + Git） |

---

## 7. 构建

### Windows 本地构建

```powershell
cd desktop-rust
cargo build --release
# 产物: desktop-rust\target\release\pi-light.exe
```

### Linux / macOS（开发验证）

```bash
cd desktop-rust
cargo check    # 验证编译
cargo build    # 需安装系统 UI 依赖（见 CI 配置）
```

Linux 需要以下系统库（CI 中已自动安装）：

```bash
sudo apt-get install libgtk-3-dev libglib2.0-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libegl1-mesa-dev libgl1-mesa-dev
```

### Android（Option C · debug 占位 APK）

Android 目录 [`android/`](./android/) 提供 **Kotlin + Jetpack Compose** 弱功能壳：

| 项 | 说明 |
|----|------|
| **定位** | 可安装的 debug APK；首页与设置页诚实说明限制 |
| **Pi** | **不内嵌** sidecar，不启动 `pi --mode rpc` |
| **Host** | **不实现** LAN / Host 配对（stretch Option A 留待后续） |
| **完整会话** | 请在 **Windows** 使用 `desktop-rust/`（egui + 内嵌/探测 Pi） |

**本地构建**（需 JDK 17+ 与 Android SDK）：

```bash
cd android
./gradlew assembleDebug
# 产物: app/build/outputs/apk/debug/app-debug.apk
```

**CI**：[`android-apk.yml`](./.github/workflows/android-apk.yml) 在 push/PR 时构建 debug APK 并上传 artifact `pi-light-android-debug`。

---

## 8. 架构

详细架构说明见 [docs/architecture.md](./docs/architecture.md)。

**简图**：

```
desktop-rust/ (egui/eframe)          android/ (Compose · Option C 占位壳)
      │                                      │
      │ std::process::Command                │ （无 Pi / 无 Host）
      │ JSONL over stdin/stdout              ▼
      ▼                              诚实 UI：完整会话请用 Win
Pi sidecar (pi.exe --mode rpc, MIT)
```

---

## 9. 安全边界

- Pi sidecar 在本机运行，拥有当前用户的文件系统访问权限
- **正式 Release 中的 Pi 二进制来自官方 Release，并经 SHA-256 校验**
- 私钥 / API Key 由 Pi 自身管理（存于 Pi 的配置目录），pi-light 不读取
- pi-light **不**向外网发送遥测数据
- 进程退出时关闭 Pi stdin → 等待退出 → 超时后 kill，避免孤儿进程

---

## 10. 文档索引

| 文档 | 说明 |
|------|------|
| [docs/architecture.md](./docs/architecture.md) | 架构详解 |
| [docs/signing.md](./docs/signing.md) | R0 签名与分发策略（debug / 未签名） |
| [desktop-rust/sidecar/README.md](./desktop-rust/sidecar/README.md) | Sidecar 布局、开发放置 Pi、Release 打包说明 |
| [AGENTS.md](./AGENTS.md) | 洁净室规则与 AI 协作指南 |
| [ACKNOWLEDGEMENTS.md](./ACKNOWLEDGEMENTS.md) | 致谢与灵感来源 |
| [CONTRIBUTING.md](./CONTRIBUTING.md) | 贡献指南 |
| [NOTICE](./NOTICE) | Pi MIT 版权声明 |
| [THIRD-PARTY-NOTICES](./THIRD-PARTY-NOTICES) | 第三方依赖许可清单 |
| [android/README.md](./android/README.md) | Android 占位壳构建说明 |

---

## 11. 已知限制

- **Windows 依赖 Git Bash**：Pi 官方要求在 Windows 上安装 Git Bash 以提供 bash 工具支持
- **无会话持久化**：M2′ 使用 `--no-session`，每次启动为全新会话；持久化为后续里程碑
- **Android（Option C）**：[`android/`](./android/) 提供可安装 **debug 占位 APK**（首页 + 设置 stub）；**不内嵌 Pi、不连 Host**；与 Pi 的完整流式会话请使用 Windows `desktop-rust/`
- **无官方商店分发**：R0 提供 GitHub Release 便携 zip 与 Android debug APK；签名策略见 [docs/signing.md](./docs/signing.md)
- **Trace / Token 用量（M3′）**：聊天区底部显示 Token 摘要；可折叠 Trace 列表来自 Pi 的 `tool_execution_*` 事件。用量来自 `message_update.usage` 与回合结束后的 `get_session_stats`。**无字段时不臆造数据**（显示「用量未知 / 暂无 Trace / 不支持」）。`--no-session` 下 stats 可能为空或不可用，属 Pi 行为限制。
- **CI 测试用 mock-pi**：集成烟测使用内置 `mock-pi` 二进制，无需真实 Pi；实机测试设 `PI_LIGHT_PI_BIN`

---

## 12. 许可与致谢

pi-light 以 **Apache-2.0** 许可开源。

```
Copyright 2026 pi-light contributors
Licensed under the Apache License, Version 2.0
```

本项目嵌入 **Pi v1.4.0**（MIT，Copyright © Mario Zechner 及贡献者）。  
完整致谢见 [ACKNOWLEDGEMENTS.md](./ACKNOWLEDGEMENTS.md)。  
Pi 版权声明与 MIT 全文见 [NOTICE](./NOTICE)。

---

> **关于旧 desktop/ Tauri 实现**：历史提交中存在基于 Tauri 的原型（旧 M0/M1 阶段），
> 该路径已不再是产品主路径。文档与实现主干为 `desktop-rust/`（egui/eframe）。
