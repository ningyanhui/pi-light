# sidecar/pi — Pi 二进制放置目录

此目录用于存放 Pi v1.4.0 的官方 Windows 独立可执行文件。

**此目录不在 git 中追踪实际二进制**（`.gitignore` 已排除 `*.exe`、`*.zip` 等）。

## 如何准备

### 正式 Release（推荐）

Release CI 流水线会自动：

1. 从官方 Release 下载 `pi-windows-x64.zip`
2. 校验 SHA-256：`0f7256ea8bdac3af20012b94fdc1252a31cc6dc0d2f9b9eb539555d22e3e321b`
3. 解压到此目录

### 开发环境手动准备

```powershell
# 下载
$url = "https://github.com/earendil-works/pi/releases/download/v1.4.0/pi-windows-x64.zip"
Invoke-WebRequest -Uri $url -OutFile "$env:TEMP\pi-windows-x64.zip"

# 校验 SHA-256（必须与上方记录一致）
(Get-FileHash "$env:TEMP\pi-windows-x64.zip" -Algorithm SHA256).Hash

# 解压到此目录
Expand-Archive -Path "$env:TEMP\pi-windows-x64.zip" -DestinationPath "."
```

或使用环境变量覆盖（无需放置到此目录）：

```powershell
$env:PI_LIGHT_PI_BIN = "C:\path\to\pi.exe"
```

## 目录结构（解压后）

```
desktop-rust/sidecar/pi/
├── pi.exe         ← Pi 可执行文件（spawn: pi.exe --mode rpc）
├── README.md      ← 本文件
└── .gitkeep       ← 目录占位符
```

## Pi 版权信息

- **仓库**：https://github.com/earendil-works/pi
- **版本**：v1.4.0
- **许可**：MIT — Copyright (c) Mario Zechner and contributors
- **完整声明**：见根目录 [NOTICE](../../NOTICE) 和 [ACKNOWLEDGEMENTS.md](../../ACKNOWLEDGEMENTS.md)
