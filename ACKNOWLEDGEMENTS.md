# 致谢 / Acknowledgements

pi-light 在 Apache-2.0 许可下洁净室编写，未复制任何第三方代码。以下项目提供了技术或设计参考，在此诚挚致谢。

---

## Pi — 核心嵌入的 Runtime

- **仓库**：[earendil-works/pi](https://github.com/earendil-works/pi)（原 `badlogic/pi-mono`，由 Mario Zechner 迁移维护）
- **许可**：MIT
- **版本**：v1.4.0
- **发行物**：`pi-windows-x64.zip`
- **下载 URL**：`https://github.com/earendil-works/pi/releases/download/v1.4.0/pi-windows-x64.zip`
- **SHA-256**：`0f7256ea8bdac3af20012b94fdc1252a31cc6dc0d2f9b9eb539555d22e3e321b`
- **用途**：pi-light 作为壳嵌入并管理 Pi sidecar（`pi --mode rpc`），经官方 JSONL RPC 协议与其通信。

---

## egui / eframe — UI 框架

- **仓库**：[emilk/egui](https://github.com/emilk/egui)
- **许可**：MIT OR Apache-2.0（pi-light 选用 Apache-2.0 条款）
- **作者**：Emil Ernerfeldt 及社区贡献者
- **用途**：pi-light 桌面客户端的全部 UI 构建于 egui/eframe 之上。

---

## Agent Skills 规范 — Skills 发现路径约定

- **规范站点**：[agentskills.io](https://agentskills.io)
- **用途**：pi-light M4′ 管理面的 Skills 发现路径（`~/.agents/skills/`、`.agents/skills/`、`<name>/SKILL.md` 目录约定）遵循此规范。

---

## pi-flash — 洁净室灵感（一句点名）

- **仓库**：[danielbill/pi-flash](https://github.com/danielbill/pi-flash)（GPL-3.0-or-later）
- **用途**：pi-light 的桌面 Agent Shell 定位受其启发。pi-light 不包含 pi-flash 任何代码，不链接其 crate，许可证完全独立。

---

## bwoc-chat — egui + 子进程先例

- **仓库**：[bemindlabs/bwoc-chat](https://github.com/bemindlabs/bwoc-chat)
- **用途**：egui 窗口 + 子进程 stdin/stdout 通信模式的工程参考（仅学机制，不拷代码）。

---

*完整第三方依赖许可清单见 [THIRD-PARTY-NOTICES](./THIRD-PARTY-NOTICES)。*
*Pi 版权声明与 MIT 全文见 [NOTICE](./NOTICE)。*
