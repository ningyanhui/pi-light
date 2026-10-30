# pi-light Android（Option C · 占位壳）

Kotlin + Jetpack Compose 的 **debug 占位 APK**：首页说明占位与诚实限制，设置页为 stub。

桌面 **M5** 已提供本机配对网关与 Cloudflare Tunnel 钩子（见 [`docs/companion-protocol.md`](../docs/companion-protocol.md)）。本目录 APK **仍为 Option C 占位**：不扫码、不连中继、不内嵌 Pi。companion 真连聊天是后续 **M6-lite**。完整会话请用 Windows `desktop-rust/`。决策见 [`docs/architecture.md`](../docs/architecture.md)。

- **不内嵌** Pi runtime
- **不实现** 扫码配对或远程聊天（留给 M6）
- Release 流水线只跑 `assembleDebug`

## 本地构建 debug APK

需要 JDK 17+ 与 Android SDK（`ANDROID_HOME`）。

```bash
cd android
./gradlew assembleDebug
# 产物: app/build/outputs/apk/debug/app-debug.apk
```

CI 见 `.github/workflows/android-apk.yml`。

## 许可

与仓库根目录相同：**Apache-2.0**。
