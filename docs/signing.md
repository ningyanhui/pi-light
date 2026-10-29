# 签名与分发策略

pi-light 的正式 GitHub Release 不做商业代码签名（本文件说明原因、缓解措施及后续路线）。

---

## Windows 便携 zip（`pi-light-windows-x64.zip`）

### 内容布局

```
pi-light-windows-x64/
├── pi-light.exe              # egui/eframe Release 构建（Apache-2.0）
├── NOTICE                    # Pi + MinGit SHA-256 与版权声明
├── LICENSE                   # Apache-2.0
└── sidecar/
    ├── pi/
    │   └── pi.exe            # Pi sidecar v1.4.0（MIT）
    └── bash/                 # MinGit v2.47.1.2 便携 bash 环境（GPL-2.0 + 例外）
        ├── usr/bin/          # bash.exe, sh.exe, 及 MSYS 工具
        ├── mingw64/bin/      # 工具链
        └── cmd/              # git.exe
```

### 签名状态

| 项 | 说明 |
|----|------|
| `pi-light.exe` | **无** Authenticode 签名；SmartScreen 可能提示「未知发布者」 |
| `pi.exe`（sidecar） | 由 earendil-works/pi 原始发布，未经 pi-light 重签 |
| MinGit 内容 | 由 Git for Windows 原始发布；pi-light CI 仅校验 zip SHA-256 |
| 类型 | 解压即用目录，**非** MSI / MSIX / 商店包 |
| 平台要求 | Windows 10 x64（1903+）；不支持 Windows 7/8 |

### SHA-256 校验（CI 强制）

Release 流水线执行 `scripts/fetch-pi-sidecar.ps1` 和 `scripts/fetch-bash-sidecar.ps1`，
均在打包前以硬编码 SHA-256 校验下载产物。不匹配时 **流水线立即失败，拒绝打包**。
校验值记录在根目录 [NOTICE](../NOTICE)。

### SmartScreen 缓解措施

对最终用户：

1. 仅从本仓库 [Releases](https://github.com/ningyanhui/pi-light/releases) 下载
2. 核对 tag 与 Release notes；不接受非官方渠道的重打包
3. 如提示「未知发布者」，可在文件属性中点击「解除锁定（Unblock）」
4. 企业环境请走内部软件白名单流程

---

## 本地签名（开发者备忘）

若需本地测试 Authenticode 签名，典型流程如下（需自签名测试证书或有效 EV 证书）：

```powershell
# 1. 生成自签名测试证书（仅开发，不信任链）
$cert = New-SelfSignedCertificate `
    -Subject "CN=pi-light Dev,O=dev,C=CN" `
    -Type CodeSigningCert `
    -CertStoreLocation "Cert:\CurrentUser\My" `
    -NotAfter (Get-Date).AddYears(1)

# 2. 对构建产物签名
$sigParams = @{
    Certificate       = $cert
    TimestampServer   = "http://timestamp.digicert.com"
    HashAlgorithm     = "SHA256"
    FilePath          = "target\release\pi-light.exe"
}
Set-AuthenticodeSignature @sigParams

# 3. 验证
Get-AuthenticodeSignature "target\release\pi-light.exe" | Select-Object Status, SignerCertificate
```

> **注意**：自签名证书不会消除 SmartScreen 警告，仅用于本地测试签名流程。

---

## CI Authenticode 占位

当前 Release 流水线（`.github/workflows/release.yml`）**未配置** Authenticode 签名步骤。
若后续引入 EV 证书签名，在 `build-windows` job 的 `Create Windows zip` 步骤之前插入：

```yaml
- name: Sign pi-light.exe (Authenticode / EV)
  # 仅在 secrets 存在时运行（本地 workflow_dispatch 可跳过）
  if: ${{ secrets.CODESIGN_CERT_BASE64 != '' }}
  shell: pwsh
  run: |
    $cert = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new(
        [Convert]::FromBase64String("$env:CODESIGN_CERT_BASE64"),
        "$env:CODESIGN_CERT_PASSWORD",
        "Exportable,PersistKeySet"
    )
    Set-AuthenticodeSignature `
        -FilePath "pi-light-windows-x64\pi-light.exe" `
        -Certificate $cert `
        -TimestampServer "http://timestamp.digicert.com" `
        -HashAlgorithm SHA256
  env:
    CODESIGN_CERT_BASE64: ${{ secrets.CODESIGN_CERT_BASE64 }}
    CODESIGN_CERT_PASSWORD: ${{ secrets.CODESIGN_CERT_PASSWORD }}
```

Secrets 在 GitHub 仓库 Settings → Secrets and variables → Actions 中配置；
CI 未注入时步骤自动跳过，**不影响无签名发布**。

---

## Android debug APK（`app-debug.apk`）

| 项 | 说明 |
|----|------|
| 构建 | `./gradlew assembleDebug`（debug 签名） |
| 能力 | Option C 占位壳：不含 Pi sidecar、不含 Host RPC |
| 签名 | Android debug keystore（Gradle 默认）；**非** release / Play App Signing |
| 安装 | 需允许「未知来源」；无法作为商店正式版分发 |

---

## 明确不上架商店（本策略约束）

- **Windows Store（MSIX）**：本里程碑范围外；不打包 MSIX，不提交 Partner Center
- **Google Play**：Android 侧为 debug 占位壳；release 签名与 Play 分发属 M6′ 及以后
- **Apple Mac App Store**：macOS 目前为可开发调试平台，未列入正式发布计划

---

## 相关文档

| 文档 | 说明 |
|------|------|
| [NOTICE](../NOTICE) | Pi sidecar + MinGit SHA-256 版权声明 |
| [scripts/fetch-pi-sidecar.ps1](../scripts/fetch-pi-sidecar.ps1) | Pi sidecar 下载/校验脚本 |
| [scripts/fetch-bash-sidecar.ps1](../scripts/fetch-bash-sidecar.ps1) | MinGit 下载/校验脚本 |
| [.github/workflows/release.yml](../.github/workflows/release.yml) | Release 流水线 |
| [ACKNOWLEDGEMENTS.md](../ACKNOWLEDGEMENTS.md) | 第三方致谢 |
