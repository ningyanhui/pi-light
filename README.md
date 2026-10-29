# pi-light

**pi-light — 内嵌 Pi 的轻量 Rust Shell**

用 Rust（egui/eframe）构建的轻量桌面客户端，把官方 [Pi coding agent](https://github.com/earendil-works/pi) 直接打进壳里——安装即用，无需手动配置外部 runtime。行为与信息架构参考 [pi-flash](https://github.com/danielbill/pi-flash)，代码 Apache-2.0 洁净室自写。

[![CI](https://github.com/ningyanhui/pi-light/actions/workflows/ci.yml/badge.svg)](https://github.com/ningyanhui/pi-light/actions/workflows/ci.yml)
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

> **M1′ 阶段**：egui 窗口可运行，三态 Pi 就绪 UI 已实现；会话聊天功能在 M2′ 推出。

```
╔═══════════════════════════════════════════════════════════╗
║  pi-light — 内嵌 Pi 的轻量 Rust Shell  ●  [⟳ 重新探测]   ║
╠═══════════════════════════════════════════════════════════╣
║                                                           ║
║  [状态 1 — 内嵌 sidecar 就绪]                              ║
║  ✓ Pi 已就绪（内嵌 sidecar / Bundled Ready）               ║
║  Pi 路径：{exe_dir}/sidecar/pi/pi.exe                     ║
║                                                           ║
║  [状态 2 — 探测到本机安装]                                  ║
║  ~ Pi 就绪（本机 / Detected Local — 环境变量）              ║
║  Pi 路径：/path/to/pi.exe                                  ║
║                                                           ║
║  [状态 3 — 未就绪]                                         ║
║  ⚠ Pi 尚未就绪（Not Ready）                                ║
║    方式 1 — PI_LIGHT_PI_BIN=<路径>                         ║
║    方式 2 — {exe_dir}/sidecar/pi/pi.exe                   ║
║    方式 3 — 本机 PATH                                      ║
║    下载：pi-windows-x64.zip @ v1.4.0                      ║
║    SHA-256: 0f7256ea8bdac3af...                           ║
║                                                           ║
║  会话聊天功能将在 M2′ 推出                                    ║
╚═══════════════════════════════════════════════════════════╝
```

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
点击「⟳ 重新探测」可在不重启的情况下重新扫描。

| 状态 | 含义 | 来源 |
|------|------|------|
| **✓ 内嵌 sidecar（Bundled Ready）** | 在 `{exe_dir}/sidecar/pi/` 下找到 Pi | 正式 Release 默认路径 |
| **~ 本机（Detected Local）** | 通过环境变量或 PATH 找到 Pi | 开发环境 / 本机安装 |
| **⚠ 未就绪（Not Ready）** | 未找到 Pi，显示放置指引 | — |

> **会话聊天功能将在 M2′ 推出**。M1′ 阶段实现三态 UI 与路径探测契约，无需真实 Pi 对话即可演示。

### 方式一：正式 Release（推荐，待 Release 发布）

1. 从 [Releases](https://github.com/ningyanhui/pi-light/releases) 下载最新 Windows 便携包
2. 解压到任意目录
3. 直接运行 `pi-light.exe`

> 正式 Release 已将 Pi v1.4.0 打包入 `sidecar/pi/`，启动即显示「Bundled Ready」状态。  
> Release 流水线会在打包前校验 Pi 的 SHA-256，确保来源可信。

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

---

## 6. 能做什么 / 不能做什么

### 当前（M1′ 阶段）

| 能做 | 不能做 |
|------|--------|
| 显示三态 Pi 就绪 UI（Bundled / Detected / Not Ready） | 实际与 Pi 对话（M2′） |
| 路径探测（env → sidecar → PATH，按优先级顺序） | 流式会话、停止按钮（M2′） |
| 「⟳ 重新探测」按钮（无需重启即更新状态） | 发布可安装包（Release 待定） |
| `cargo check` + `cargo test` 通过（7 个路径探测单元测试） | Trace / Token 用量（M3′） |

### 计划中（里程碑交付后）

| 能做 | 不能做（明确不在范围） |
|------|----------------------|
| 与 Pi 流式对话（M2′） | 内嵌 Taskboard / CDP 注入 |
| 显示 Trace / Token 用量（M3′） | 双后端（Pi + Harness）切换 |
| skill / MCP 列表开关（M4′） | 捆绑 dsh / deepseek-harness |
| Android 轻量客户端（M6′） | 完整 IDE（文件树 + 多终端 + Git） |
| 便携 Release zip | 商店签名分发（本轮不做） |

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

### Android（M6′，两周内为弱功能占位）

Android 当前为**壳/占位阶段**，完整会话能力以 Windows 为主路径。  
详见里程碑计划。

---

## 8. 架构

详细架构说明见 [docs/architecture.md](./docs/architecture.md)。

**简图**：

```
desktop-rust/ (egui/eframe)
      │
      │ std::process::Command
      │ JSONL over stdin/stdout
      ▼
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
| [AGENTS.md](./AGENTS.md) | 洁净室规则与 AI 协作指南 |
| [ACKNOWLEDGEMENTS.md](./ACKNOWLEDGEMENTS.md) | 致谢与灵感来源 |
| [CONTRIBUTING.md](./CONTRIBUTING.md) | 贡献指南 |
| [NOTICE](./NOTICE) | Pi MIT 版权声明 |
| [THIRD-PARTY-NOTICES](./THIRD-PARTY-NOTICES) | 第三方依赖许可清单 |

---

## 11. 已知限制

- **M1′ 阶段**：三态 UI 与路径探测契约已实现，无实际 Pi RPC 会话能力
- **会话聊天**：`pi --mode rpc` JSONL 流式对话在 **M2′** 推出，M1′ 不包含
- **Windows 依赖 Git Bash**：Pi 官方要求在 Windows 上安装 Git Bash 以提供 bash 工具支持
- **Android 为弱功能占位**：完整移动端会话能力为后续里程碑（M6′）
- **进程生命周期管理**：M2′ 起加入 spawn / stdin 关闭 / wait / kill 全流程
- **无官方商店分发**：当前仅提供便携 zip，签名策略见 R0 里程碑

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
