# pi-light 架构说明

> **当前版本**：M1′（三态 UI + 路径探测契约）  
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

android/       Kotlin Compose（M6′ 移动端，两周内为弱功能占位）
host/          可选本机桥（过渡期可保留，终态以壳内直管 Pi 为准）
desktop/       旧 Tauri 树（已标 deprecated，不是文档/实现主路径）
```

---

## 目录结构（M0′）

```
pi-light/
├── Cargo.toml                    # workspace
├── desktop-rust/                 # 主桌面客户端（egui/eframe）
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs               # 入口 + eframe App
│   │   └── pi_path.rs            # Pi 路径解析 stub
│   └── sidecar/
│       └── pi/
│           ├── .gitkeep          # 目录占位
│           └── README.md         # sidecar 放置说明
├── docs/
│   └── architecture.md          # 本文件
├── .github/
│   └── workflows/
│       └── ci.yml               # cargo check CI
├── LICENSE                      # Apache-2.0
├── NOTICE                       # Pi MIT 版权声明
├── ACKNOWLEDGEMENTS.md          # 致谢（pi-flash 灵感、earendil-works/pi 等）
├── AGENTS.md                    # 洁净室规则与 AI 协作指南
├── CONTRIBUTING.md              # 贡献指南
├── THIRD-PARTY-NOTICES          # 第三方依赖许可清单（起步版）
└── README.md                    # 主 README（中文）
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
| **M0′** | 仓骨架、README 换轨、egui 窗、pi_path stub | ✅ 本 PR |
| **M1′** | 三态 UI（捆绑就绪 / 探测到本机 / 未就绪）、进程契约完整 | 待实现 |
| **M2′** | pi_rpc 模块（JSONL RPC 真连）、流式聊天、停止、进程生命周期 | 待实现 |
| **M3′** | Trace/Token on Pi（`get_session_stats` + usage 事件） | Stretch |
| **M4′** | 管理面（skill/MCP/记忆列表开关） | 后续 |
| **M5′** | `#` 会话引用 + 可选 Taskboard 布局 | 后续 |
| **M6′** | Android APK（Kotlin Compose，弱功能 → 连 Host） | 并行 |
| **R0** | Release（Win 便携 zip + 签名策略文档） | 后续 |

---

## 洁净室边界

- 禁止引入 GPUI / Zed vendor 代码
- 禁止使用 pi-flash `pi-link` crate
- 禁止双后端切换 UI
- 完整禁令见 [AGENTS.md](../AGENTS.md)

---

*最后更新：M0′ 骨架阶段*
