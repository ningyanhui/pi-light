# AGENTS.md — pi-light 洁净室规则与 AI 协作指南

本文件适用于所有向 pi-light 提交代码的 AI 编码代理（Cursor、Copilot 等）及人类贡献者。

---

## 产品身份（不可偏航）

**pi-light = 内嵌 Pi 的轻量 Rust Shell（egui/eframe，Apache-2.0 洁净室）**

- 嵌入并管理官方 MIT 许可的 Pi sidecar（`earendil-works/pi`）
- 行为与信息架构参考 pi-flash，**代码洁净室自写**
- Harness/dsh 仅 UX 借鉴，**不是第二后端**

---

## 洁净室禁令（硬性，不可绕过）

以下操作在任何 PR、commit 或代码生成中**严格禁止**：

- [ ] **禁止引入 GPUI / Zed vendor 代码**：任何来自 Zed 编辑器生态或 GPUI 的源码、
  vendor 目录、crate 依赖均不得进入仓库或产物。
- [ ] **禁止使用 pi-flash crates 作为依赖或复制其实现**：包括但不限于
  `pi-link`、任何从 `danielbill/pi-flash` 复制的 Rust 类型定义或 wire 协议实现。
- [ ] **禁止摘抄他仓 README / 封面图 / 发版文运营段落**：致谢可点名引用，
  不可大段复制 pi-flash、pi-mobile-android、deepseek-harness 等仓库的 README 正文。
- [ ] **禁止将未校验 SHA-256 的上游二进制打入正式 Release**：
  Release 流水线必须验证 `pi-windows-x64.zip` 的 SHA-256 与 NOTICE 中记录值一致后才能打包。
- [ ] **禁止双后端切换 UI**：不得出现「Backend: Pi | Harness」设置项，
  不得将 dsh/deepseek-harness 作为一等公民后端。
- [ ] **禁止捆绑 dsh / `npx @deepseek-ai/dsh`**：Harness 只作 UX 学习笔记。
- [ ] **禁止将 Tauri `desktop/` 作为文档或实现主路径**：旧树保留仅为历史，
  实现与文档主干 = `desktop-rust/`（egui/eframe）。
- [ ] **禁止 force-push 或删除仓库**。
- [ ] **禁止将 GPL 源码（pi-flash / Zed / GPUI 派生）链入产物或进仓**。

---

## PR 洁净室自检清单（每个 PR 必须勾选）

提交 PR 时，描述中必须包含以下自检项：

```
### 洁净室自检

- [ ] 未引入 GPUI / Zed vendor 代码
- [ ] 未使用 pi-flash crates（含 pi-link）作依赖或拷贝
- [ ] 未摘抄他仓 README / 封面图 / 发版文运营段落
- [ ] 未将未校验 SHA-256 的二进制打入 Release（如涉及）
- [ ] 未添加双后端切换 UI 或捆绑 dsh
- [ ] 新依赖已检查许可证兼容性（Apache-2.0 洁净室）
```

---

## P0 上游钉定（不可更改，除非用户书面否决）

| 项 | 钉定值 |
|----|--------|
| 上游 | `earendil-works/pi`（MIT） |
| 版本 | v1.4.0 |
| 发行物 | `pi-windows-x64.zip` |
| 下载 URL | `https://github.com/earendil-works/pi/releases/download/v1.4.0/pi-windows-x64.zip` |
| SHA-256 | `0f7256ea8bdac3af20012b94fdc1252a31cc6dc0d2f9b9eb539555d22e3e321b` |
| sidecar 路径 | `{exe_dir}/sidecar/pi/` |
| 环境覆盖 | `PI_LIGHT_PI_BIN` |
| 启动命令 | `pi`/`pi.exe` + `--mode rpc` |

---

## 路径探测顺序（M1′ 契约，不可随意更改）

1. 环境变量 `PI_LIGHT_PI_BIN`
2. 捆绑相对路径 `{exe_dir}/sidecar/pi/pi.exe`（Windows）/ `{exe_dir}/sidecar/pi/pi`（其他平台）
3. （可选，M1′+）探测本机 PATH 中的 `pi`/`pi.exe`

---

## 依赖原则

- UI 框架：**egui + eframe**（MIT OR Apache-2.0，选 Apache-2.0 条款）
- 进程通信：`std::process::Command`，stdin/stdout/stderr piped，后台线程读 stdout
- 禁止引入 GPL 系 crate
- 新增依赖须在 PR 描述中说明许可证

---

## 里程碑范围提醒

| 里程碑 | 范围 |
|--------|------|
| **M0′（本 PR）** | README 换轨 + 骨架 + `cargo check` |
| **M1′** | 路径约定 + 三态 UI（捆绑就绪 / 探测到本机 / 未就绪） |
| **M2′** | 官方 JSONL RPC 真连聊天 |
| **M3′–M5′** | Trace/Token、管理面、`#` 引用 |
| **M6′** | Android：**维持 Option C**（Host 未长出 A 所需共享面；不升真 Host） |

**OUT of M0′**：真 RPC 聊天、三态 UI 完整、zip 捆绑进 git、双后端、Harness/dsh。

---

## 合并策略

每个 PR 合入 `main` 前须用户明确确认。AI 代理不得自行合并 PR。

---

*本文件权威来源：`AGENTS.md`；覆盖旧版本中任何双后端、Tauri 主路径、外挂 Pi 描述。*
