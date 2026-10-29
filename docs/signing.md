# 签名与分发策略（R0）

pi-light **R0** 正式 GitHub Release 提供的安装介质如下，均为**未做商业代码签名**的构建产物。使用前请理解下列限制。

## Windows 便携 zip（`pi-light-windows-x64.zip`）

| 项 | 说明 |
|----|------|
| 内容 | `pi-light.exe`（egui/eframe Release 构建）+ `sidecar/pi/`（官方 Pi v1.4.0，流水线 SHA-256 校验后打入）+ `NOTICE` / `LICENSE` |
| 签名 | **无** Authenticode / EV 代码签名；SmartScreen 可能提示「未知发布者」 |
| 类型 | 解压即用目录，非 MSI/商店包 |
| Pi 来源 | [earendil-works/pi v1.4.0](https://github.com/earendil-works/pi/releases/tag/v1.4.0)，校验值见根目录 [NOTICE](../NOTICE) |

缓解 SmartScreen 的常见做法：仅从本仓库 [Releases](https://github.com/ningyanhui/pi-light/releases) 下载；核对 tag 与 Release 说明；必要时在属性中「解除锁定」或加入本机信任策略（企业环境请走内部软件白名单流程）。

## Android debug APK（`app-debug.apk`）

| 项 | 说明 |
|----|------|
| 构建 | `./gradlew assembleDebug`（**debug** 签名，非 Play 上架包） |
| 能力 | Option C 占位壳：**不含 Pi sidecar、不含 Host RPC**；用于 UI/导航演示 |
| 签名 | Android **debug keystore**（Gradle 默认），**非** release / Play App Signing |
| 安装 | 需允许「未知来源」；无法作为商店正式版分发 |

## 后续（非 R0 范围）

- Windows：计划单独里程碑引入 Authenticode（或企业内部分发通道）
- Android：release 签名、Play 分发与完整 Host 能力属 M6′ 及以后

## 相关文档

- Sidecar 放置与 Release 打包：[desktop-rust/sidecar/README.md](../desktop-rust/sidecar/README.md)
- 第三方与 Pi 版权：[NOTICE](../NOTICE)、[ACKNOWLEDGEMENTS.md](../ACKNOWLEDGEMENTS.md)
