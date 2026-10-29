# pi-light Android（Option C · 占位壳）

Kotlin + Jetpack Compose 的 **debug 占位 APK**：首页说明占位与诚实限制，设置页为 stub。

M6′ **维持 Option C**。桌面 Host（`desktop-rust` egui + 本机 `pi --mode rpc`）没有长出 Android 真 Host 所需的共享面（无 LAN / HTTP 配对、无 Host API），因此不升 Option A，也不做 Option B 内嵌 Pi。完整会话请用 Windows `desktop-rust/` 客户端。决策见 [`docs/architecture.md`](../docs/architecture.md)。

- **不内嵌** Pi runtime（非 Option B）
- **不实现** Host / LAN 配对（非 Option A）
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
