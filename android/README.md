# pi-light Android（Option C · 占位壳）

Kotlin + Jetpack Compose 的**弱功能 debug APK**：首页说明占位与诚实限制，设置页为 stub。

- **不内嵌** Pi runtime（非 Option B）
- **不实现** Host / LAN 配对（stretch Option A 留待 M6′+）
- 完整 Pi 会话请使用 Windows `desktop-rust/` 客户端

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
