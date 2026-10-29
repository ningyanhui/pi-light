# 贡献指南 / Contributing

欢迎为 pi-light 提交贡献！请在开始前阅读以下简要说明。

## 洁净室原则

pi-light 是 **Apache-2.0 洁净室项目**。请在贡献前阅读 [AGENTS.md](./AGENTS.md) 中的完整禁令列表。核心要点：

- 不引入 GPL 代码（pi-flash / Zed / GPUI）
- 不复制他仓 README 正文
- 不添加双后端或捆绑 dsh
- 新依赖须许可证兼容（Apache-2.0 洁净室）

## 开发环境

- Rust 1.75+（edition 2021）
- Windows 10/11 x64（主要目标平台）
- Linux / macOS 可用于开发和 `cargo check`

```bash
# 克隆并进入 desktop-rust
git clone https://github.com/ningyanhui/pi-light
cd pi-light/desktop-rust

# 检查编译
cargo check

# 构建（需 Windows 目标或 native Windows 环境）
cargo build
```

## 提交 PR

1. 从 `main` 分支创建功能分支
2. 遵循 Conventional Commits 格式编写 commit message
3. PR 描述中包含 `AGENTS.md` 中规定的洁净室自检清单
4. 等待维护者审核后合并，**不得自行合并**

## 行为准则

尊重所有贡献者，保持建设性讨论。

---

*详见 [AGENTS.md](./AGENTS.md) 了解完整的代理协作规则与洁净室禁令。*
