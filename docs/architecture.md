# pi-light 架构说明

> **当前版本**：M2′（真连 Pi RPC 聊天）  
> **主路径**：`desktop-rust/`（egui/eframe）  
> **许可**：Apache-2.0 洁净室

---

## 一句话定位

**pi-light = 内嵌官方 Pi sidecar 的轻量 Rust Shell（egui/eframe，Windows 优先）**

行为与信息架构参考 [pi-flash](https://github.com/danielbill/pi-flash)，代码洁净室自写；  
Harness/dsh 仅 UX 借鉴，不作第二后端。

---

## 宏观层次图

```
┌─────────────────────────────────────────────────────────────────────┐
│  desktop-rust/   egui + eframe（主桌面客户端）                        │
│                                                                       │
│  ┌──────────┐  ┌──────────────────────────────┐  ┌───────────────┐  │
│  │  侧栏     │  │  主区                         │  │  设置面板      │  │
│  │  会话列表  │  │  消息流（流式 + 工具卡）         │  │  M4′+        │  │
│  │  搜索/置顶 │  │  输入框 + 停止               │  │               │  │
│  │  Pi就绪灯  │  │  Token/上下文条（M3′+）       │  │               │  │
│  └──────────┘  └──────────────────────────────┘  └───────────────┘  │
│                                                                       │
│  pi_path 模块：路径解析（env → sidecar → PATH）                        │
│  pi_rpc 模块（M2′）：JSONL over stdin/stdout                          │
└────────────────────────────┬────────────────────────────────────────┘
                             │ std::process::Command（stdin/stdout piped）
                             │ 后台线程读 stdout → mpsc channel
                             │ UI 每帧 drain channel
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│  Pi sidecar（官方 MIT，不修改）                                        │
│  spawn: pi --mode rpc                                                │
│  协议: JSONL（官方 rpc.md / rpc-commands.md / json.md）               │
│  布局: {exe_dir}/sidecar/pi/pi.exe                                   │
│  覆盖: PI_LIGHT_PI_BIN                                               │
└─────────────────────────────────────────────────────────────────────┘

android/       Kotlin Compose（Option C：debug 占位 APK，不内嵌 Pi / 不连 Host）
host/          可选本机桥（stretch；Option C 不依赖）
desktop/       旧 Tauri 树（已标 deprecated，不是文档/实现主路径）
```

---

## 目录结构（M2′ + Android Option C）

```
pi-light/
├── Cargo.toml                    # workspace
├── desktop-rust/                 # 主桌面客户端（egui/eframe）
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   ├── main.rs
│   │   ├── pi_path.rs
│   │   ├── pi_rpc.rs
│   │   ├── chat.rs
│   │   └── bin/mock_pi.rs
│   ├── tests/rpc_smoke.rs
│   └── sidecar/pi/
├── android/                      # Option C 占位壳（Compose）
│   ├── app/
│   └── README.md
├── docs/architecture.md
├── .github/workflows/
│   ├── ci.yml
│   └── android-apk.yml
├── LICENSE
├── NOTICE
├── ACKNOWLEDGEMENTS.md
├── AGENTS.md
├── CONTRIBUTING.md
├── THIRD-PARTY-NOTICES
└── README.md
```

---

## Pi sidecar 集成（协议摘要）

### 版本钉定

| 项 | 值 |
|----|----|
| 上游 | `earendil-works/pi`（MIT） |
| 版本 | v1.4.0 |
| 发行物 | `pi-windows-x64.zip` |
| SHA-256 | `0f7256ea8bdac3af20012b94fdc1252a31cc6dc0d2f9b9eb539555d22e3e321b` |

### 进程约定

```
启动: pi.exe --mode rpc [--session <id>]
协议: JSONL；stdin ← 命令；stdout → 响应 + 事件；stderr ← 非协议日志
轮次: 命令携带 id；等待 agent_settled（非仅 agent_end）
关闭: 关 stdin → wait timeout → kill（防孤儿进程）
```

### 路径探测顺序

1. 环境变量 `PI_LIGHT_PI_BIN`（开发优先）
2. 捆绑相对路径 `{exe_dir}/sidecar/pi/pi.exe`
3. （M1′+）探测本机 PATH 中的 `pi.exe`

---

## 里程碑与主要模块

| 里程碑 | 新增模块 / 功能 | 状态 |
|--------|-----------------|------|
| **M0′** | 仓骨架、README 换轨、egui 窗、pi_path stub | ✅ 已合入 |
| **M1′** | 三态 UI（捆绑就绪 / 探测到本机 / 未就绪）、进程契约完整 | ✅ 已合入 |
| **M2′** | pi_rpc 模块（JSONL RPC 真连）、流式聊天、停止、进程生命周期、mock-pi 测试 | ✅ 本 PR |
| **M3′** | Trace/Token on Pi（`get_session_stats` + usage 事件） | **当前** |
| **M4′** | 管理面（skill/MCP/记忆列表开关） | 后续 |
| **M5′** | `#` 会话引用 + 可选 Taskboard 布局 | 后续 |
| **M6′** | Android Host 真连或同构嵌入（Option A stretch） | 后续 |
| **Android C** | `android/` debug 占位 APK（首页 + 设置 stub，CI artifact） | ✅ 本里程碑 |
| **R0** | Release（Win 便携 zip + 签名策略文档） | 后续 |

---

## 洁净室边界

- 禁止引入 GPUI / Zed vendor 代码
- 禁止使用 pi-flash `pi-link` crate
- 禁止双后端切换 UI
- 完整禁令见 [AGENTS.md](../AGENTS.md)

---

## 测试策略（M2′）

| 类型 | 文件 | 说明 |
|------|------|------|
| 单元测试 | `pi_rpc::tests` | JSONL 解析逻辑（text_delta、agent_settled、response 错误等） |
| 单元测试 | `pi_path::tests` | 路径探测三态逻辑 |
| 单元测试 | `chat::tests` | 会话状态机（完整轮次、中止、错误） |
| 集成测试 | `tests/rpc_smoke.rs` | spawn → prompt → 流式 → settled → shutdown；mock-pi 作 Pi 替身 |
| 真实测试 | `PI_LIGHT_PI_BIN=/path/to/pi cargo test` | 对真实 Pi 的端到端验证 |

---

*最后更新：M2′ Pi RPC 聊天阶段*
