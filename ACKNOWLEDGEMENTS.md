# 致谢 / Acknowledgements

pi-light 在 Apache-2.0 许可下洁净室编写，未复制任何第三方代码。以下仓库提供了行为、信息架构或技术参考，在此诚挚致谢。

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

## pi-flash — 行为与信息架构灵感

- **仓库**：[danielbill/pi-flash](https://github.com/danielbill/pi-flash)
- **许可**：GPL-3.0-or-later（**pi-light 未使用其代码，仅参考行为**）
- **用途**：会话优先布局、sidecar 捆绑行为、Pi 进程生命周期管理的 **UX 灵感来源**。
  pi-light 是独立的洁净室重写，不是 pi-flash 的 fork，亦无官方关联。
  pi-flash 使用 GPUI/Zed 技术栈（GPL），pi-light 使用 egui/eframe（Apache-2.0），
  两者在代码层面没有任何共享。

---

## egui / eframe — UI 框架

- **仓库**：[emilk/egui](https://github.com/emilk/egui)
- **许可**：MIT OR Apache-2.0（pi-light 选用 Apache-2.0 条款）
- **作者**：Emil Ernerfeldt 及社区贡献者
- **用途**：pi-light 桌面客户端的全部 UI 构建于 egui/eframe 之上。

---

## bwoc-chat — egui + 子进程先例

- **仓库**：[bemindlabs/bwoc-chat](https://github.com/bemindlabs/bwoc-chat)
- **用途**：egui 窗口 + 子进程 stdin/stdout 通信模式的工程参考（仅学机制，不拷代码）。

---

## pi-mobile-android — Android 端 README 风格灵感

- **仓库**：[LiuChen19960902/pi-mobile-android](https://github.com/LiuChen19960902/pi-mobile-android)
- **用途**：Android 移动端客户端设计与 README 文档风格参考（定位清晰、诚实限制）。
  pi-light 未使用其代码，未摘抄其 README 正文。

---

## deepseek-harness / dsh-mobile — UX 借鉴

- **仓库**：[deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)、[Clarklevis1995/dsh-mobile](https://github.com/Clarklevis1995/dsh-mobile)
- **用途**：Trace/Token 展示、插件/技能管理面板的 **视觉呈现思路借鉴**（映射到 Pi 事件与
  `get_session_stats`）。pi-light 不捆绑 dsh，不做双后端切换。

---

## 旧 desktop/ Tauri 实现（已标 deprecated）

本仓库历史提交中曾存在基于 Tauri 的 `desktop/` 原型（旧 M0/M1 阶段）。
该路径已不再是产品主路径；文档与实现主干为 `desktop-rust/`（egui/eframe）。
旧树保留仅作历史记录，不作宣传。

---

*完整第三方依赖许可清单见 [THIRD-PARTY-NOTICES](./THIRD-PARTY-NOTICES)。*
*Pi 版权声明与 MIT 全文见 [NOTICE](./NOTICE)。*
