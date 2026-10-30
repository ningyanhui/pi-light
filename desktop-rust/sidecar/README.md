# Sidecar 目录

pi-light 通过 **sidecar** 启动官方 Pi 进程（`pi.exe --mode rpc`）。本目录树仅描述布局；**不要把 `pi.exe` 或 `pi-windows-x64.zip` 提交进 git**（见根 `.gitignore`）。

## 运行时布局

正式 Release 与开发「捆绑就绪」态使用相同相对路径：

```
{exe_dir}/
├── pi-light.exe
└── sidecar/
    ├── pi/
    │   ├── pi.exe          ← spawn 入口
    │   └── …               ← Pi 发行包内其它文件（theme、examples 等）
    └── pi-packages/        ← M2 预置 npm 包（见 pi-packages/README.md）
```

路径探测顺序见 [AGENTS.md](../../AGENTS.md) 与 `desktop-rust/src/pi_path.rs`：环境变量 `PI_LIGHT_PI_BIN` → 内嵌 `sidecar/pi/` →（可选）PATH。

## 开发环境：手动放入 Pi

1. 从官方 Release 下载 [pi-windows-x64.zip @ v1.4.0](https://github.com/earendil-works/pi/releases/download/v1.4.0/pi-windows-x64.zip)
2. **必须**校验 SHA-256（与 [NOTICE](../../NOTICE) 一致）  
   `0f7256ea8bdac3af20012b94fdc1252a31cc6dc0d2f9b9eb539555d22e3e321b`
3. 解压到 `desktop-rust/sidecar/pi/`（使 `pi.exe` 位于该目录下）

可复用仓库脚本（校验失败则非零退出）：

```bash
# Linux / macOS / Git Bash
bash scripts/fetch-pi-sidecar.sh desktop-rust/sidecar/pi
```

```powershell
# Windows PowerShell
.\scripts\fetch-pi-sidecar.ps1 -DestDir desktop-rust\sidecar\pi
```

或使用环境变量指向任意 `pi.exe`（无需复制到 sidecar）：

```powershell
$env:PI_LIGHT_PI_BIN = "C:\path\to\pi.exe"
```

子目录 [pi/README.md](./pi/README.md) 说明 `sidecar/pi/` 占位与版权引用。

## 正式 Release：流水线打入 Pi

GitHub Actions [Release 工作流](../../.github/workflows/release.yml) 在 **Windows** job 中：

1. `cargo build --release` 生成 `pi-light.exe`
2. 运行 `scripts/fetch-pi-sidecar.ps1`，将校验通过的 Pi 解压到打包目录的 `sidecar/pi/`
3. 运行 `scripts/fetch-pi-packages.py --install-deps --check-docs`，将校验通过的预置包解压到 `sidecar/pi-packages/`
4. 复制根目录 `NOTICE`、`LICENSE`、`docs/preset-packages.md`
5. 打 zip：`pi-light-windows-x64.zip`（**不** commit 进 main）

校验失败时 workflow **失败**，不会上传含 Pi 的 zip。

## 版权

Pi 为 MIT 许可，v1.4.0 钉定信息见 [NOTICE](../../NOTICE) 与 [ACKNOWLEDGEMENTS.md](../../ACKNOWLEDGEMENTS.md)。
