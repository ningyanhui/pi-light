# pi-light

**pi-light — 内嵌官方 Pi 的轻量 Rust 桌面 Agent Shell**

[![CI](https://github.com/ningyanhui/pi-light/actions/workflows/ci.yml/badge.svg)](https://github.com/ningyanhui/pi-light/actions/workflows/ci.yml)
[![Release](https://github.com/ningyanhui/pi-light/actions/workflows/release.yml/badge.svg)](https://github.com/ningyanhui/pi-light/actions/workflows/release.yml)
[![Android APK](https://github.com/ningyanhui/pi-light/actions/workflows/android-apk.yml/badge.svg)](https://github.com/ningyanhui/pi-light/actions/workflows/android-apk.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](./LICENSE)

---

## 目录

1. [定位](#1-定位)
2. [能做什么 / 不能做什么](#2-能做什么--不能做什么)
3. [环境与系统要求](#3-环境与系统要求)
4. [开始使用](#4-开始使用)
5. [构建](#5-构建)
6. [架构](#6-架构)
7. [安全边界](#7-安全边界)
8. [文档索引](#8-文档索引)
9. [许可与致谢](#9-许可与致谢)

---

## 1. 定位

pi-light 是一个**通用桌面 Agent Shell**：把官方 [Pi coding agent](https://github.com/earendil-works/pi)（MIT）以 sidecar 方式嵌入 egui/eframe 壳中，安装即用，无需手动配置 Node.js runtime。

- **UI 框架**：[egui](https://github.com/emilk/egui) / eframe（MIT OR Apache-2.0，选 Apache-2.0 条款）
- **内嵌 sidecar**：Pi v1.4.0，以 `pi --mode rpc` 启动，通过官方 JSONL 协议通信
- **许可**：Apache-2.0（洁净室）
- **平台**：Windows x64（主），Linux / macOS 可开发调试

---

## 2. 能做什么 / 不能做什么

### 当前可用（M1′–M4′）

| 功能 | 说明 |
|------|------|
| **三态启动屏** | Bundled Ready / Detected Local / Not Ready，引导用户放置 Pi 二进制 |
| **流式聊天** | 官方 JSONL RPC 真连 Pi，流式文字 + 停止按钮 + 进程生命周期管理 |
| **Token / Trace** | 底部 Token 摘要条（M3′）；可折叠 Trace 列表来自 `tool_execution_*` 事件 |
| **管理面（M4′）** | 右侧面板：五个 tab，各有诚实的空态降级文案 |
| ↳ Skills | 本机发现（`~/.pi/agent/skills/`、`.pi/skills/`、`~/.agents/skills/`）；列表只读展示 |
| ↳ MCP | 发现 `mcp.json`（`~/.pi/agent/mcp.json`、`.pi/mcp.json`）；支持 UI 开关并写回文件 |
| ↳ 插件 | 诚实空态：Pi v1.4.0 暂无独立插件体系 |
| ↳ 记忆 | Project / User / Session 三层；读取 CLAUDE.md / AGENTS.md / `~/.pi/agent/AGENTS.md`；只读展示 |
| ↳ 模型 | 读取 `settings.json`；显示默认提供商/模型；支持写入 settings.json 或通过 RPC `set_model` 切换（需 Pi 就绪）；API 密钥状态（环境变量或 auth.json 存在性，不展示明文） |
| **Android APK** | `android/` debug 占位壳（Compose；首页 + 设置 stub；不内嵌 Pi） |

### 诚实限制

- **Skills 开关**：Pi v1.4.0 无对应 RPC；UI 标注「只读 / 暂不支持开关」，请手动编辑配置文件
- **Skills 启用状态**：SKILL.md 无 `enabled` 字段，列表展示为「状态未知」
- **会话持久化**：使用 `--no-session`，每次启动为全新会话；多会话为后续里程碑
- **Token 用量**：依赖 Pi 上报 `message_update.usage` 或 `get_session_stats`；Pi 未上报时显示「用量未知 / 不支持」
- **模型 RPC 切换**：依赖 Pi v1.4.0 的 `set_model` 命令；Pi 未就绪时仅可写入 settings.json（下次启动生效）
- **API Key 编辑**：不在 UI 中存储或显示明文 key；请使用环境变量或 Pi 的 `/login` 命令
- **插件**：Pi v1.4.0 无插件扩展体系，此 tab 保留待后续版本

---

## 3. 环境与系统要求

| 项 | 要求 |
|----|------|
| Rust | stable ≥ 1.85（edition2024 支持） |
| 平台（构建） | Windows x64 / Linux / macOS |
| Pi sidecar | v1.4.0（由 Release 流水线捆绑；开发时手动提供） |
| egui/eframe | 0.29（Apache-2.0） |

### Pi 路径探测顺序

1. 环境变量 `PI_LIGHT_PI_BIN`
2. 相对路径 `{exe_dir}/sidecar/pi/pi.exe`（Windows）/ `{exe_dir}/sidecar/pi/pi`（其他）
3. 本机 `PATH` 中的 `pi` / `pi.exe`

---

## 4. 开始使用

### 方式 A — 下载发行包（推荐）

从 [Releases](https://github.com/ningyanhui/pi-light/releases) 下载最新 Windows 便携包，解压运行 `pi-light.exe`。
Pi sidecar 已捆绑在 `sidecar/pi/` 目录中，SHA-256 已由 CI 校验。

### 方式 B — 开发调试

```bash
git clone https://github.com/ningyanhui/pi-light.git
cd pi-light/desktop-rust

# 提供 Pi 二进制（任选其一）
export PI_LIGHT_PI_BIN=/path/to/pi   # 或 pi.exe

cargo run
```

---

## 5. 构建

```bash
cd desktop-rust

# 检查
cargo check --all-targets

# 测试（含集成测试，使用 mock-pi 自动替代真实 Pi）
cargo test --all-targets

# 格式检查
cargo fmt --check

# Lint（-D warnings）
cargo clippy --all-targets -- -D warnings

# 发行构建
cargo build --release
```

---

## 6. 架构

详见 [`docs/architecture.md`](./docs/architecture.md)。

```
desktop-rust/
├── src/
│   ├── main.rs           # egui/eframe App、事件循环
│   ├── lib.rs            # 库入口（供集成测试用）
│   ├── pi_path.rs        # Pi 二进制路径探测（三态）
│   ├── pi_rpc.rs         # JSONL RPC 客户端（M2′+M4′ 模型命令）
│   ├── chat.rs           # 聊天状态机（M2′）
│   ├── session_insights.rs  # Token / Trace（M3′）
│   ├── admin_panel.rs    # 管理面（M4′）
│   └── bin/mock_pi.rs    # 集成测试用 Pi 替身
└── tests/rpc_smoke.rs    # 集成烟测
```

---

## 7. 安全边界

- **API Key**：不在 UI 中存储或展示明文 key。管理面仅检测环境变量是否存在、auth.json 是否存在，展示"已配置/未配置"。
- **Pi SHA-256**：Release 流水线校验 Pi sidecar 的 SHA-256（`scripts/verify_pi_sha256.sh`），不合格不打包。
- **洁净室**：不引入 GPUI / Zed vendor 代码；不使用 pi-flash crate；不捆绑 dsh；不做双后端切换 UI。

---

## 8. 文档索引

| 文档 | 说明 |
|------|------|
| [`docs/architecture.md`](./docs/architecture.md) | 宏观架构、模块职责、里程碑 |
| [`docs/signing.md`](./docs/signing.md) | Windows 签名策略 |
| [`AGENTS.md`](./AGENTS.md) | AI 编码代理洁净室规则 |
| [`ACKNOWLEDGEMENTS.md`](./ACKNOWLEDGEMENTS.md) | 第三方致谢 |
| [`NOTICE`](./NOTICE) | Pi MIT 版权声明 |
| [`THIRD-PARTY-NOTICES`](./THIRD-PARTY-NOTICES) | 依赖许可清单 |

---

## 9. 许可与致谢

本项目以 **Apache-2.0** 许可发布。

- **Pi sidecar**：[earendil-works/pi](https://github.com/earendil-works/pi)（MIT），以 sidecar 方式嵌入，不修改源码
- **egui / eframe**：[emilk/egui](https://github.com/emilk/egui)（MIT OR Apache-2.0）
- 完整致谢见 [`ACKNOWLEDGEMENTS.md`](./ACKNOWLEDGEMENTS.md)
