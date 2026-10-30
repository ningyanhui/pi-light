# pi-light Android companion（M6-lite）

Kotlin + Jetpack Compose 的 **debug companion APK**：首页设备卡、扫码或手输配对、远程聊天。不内嵌 Pi runtime；完整 sidecar 会话仍在 Windows `desktop-rust/`。

对照原型只学信息架构：`acceptance/03-mobile-pair`、`acceptance/05-mobile-chat`、`mobile-v2/01-home`、`mobile-v2/05-chat`。实现是洁净室自写，不拷贝他仓源码或 README。

- **配对**：扫描桌面二维码（JSON 含 `code` + `relay`），或手输 6 位配对码 **和** 公网中继 URL
- **聊天**：`Authorization: Bearer <token>` 调用 `{relay}/v1/chat`，写入桌面当前 Pi 会话
- **不包含**：远程桌面三档（浏览 / 触控 / 全屏）、Pi sidecar、Harness / dsh
- Release 流水线跑 `testDebugUnitTest` + `assembleDebug`

## 权限

| 权限 | 用途 |
|------|------|
| `INTERNET` / `ACCESS_NETWORK_STATE` | 访问 Cloudflare Tunnel（或其它已填写的中继） |
| `CAMERA` | 扫描桌面配对二维码；可拒绝，改为手输配对码 |

相机不是强制硬件（`android.hardware.camera` required=false）。

## 本地构建 debug APK

需要 JDK 17+ 与 Android SDK（`ANDROID_HOME`）。

```bash
cd android
./gradlew testDebugUnitTest assembleDebug
# 产物: app/build/outputs/apk/debug/app-debug.apk
```

CI 见 `.github/workflows/android-apk.yml`。

## 怎么连到桌面

1. 电脑打开远程访问，等到中继显示 `https://….trycloudflare.com`。
2. 手机使用**蜂窝数据**（公网验收不要连电脑同一 Wi‑Fi）。
3. 扫描二维码，或把配对码和中继 URL 贴进配对页。
4. 「进入会话」发一条消息；桌面同一会话应出现来回。

协议见 [`docs/companion-protocol.md`](../docs/companion-protocol.md)，验收剧本见 [`docs/remote-acceptance.md`](../docs/remote-acceptance.md)。

开发时可用模拟器访问宿主机 loopback：`http://10.0.2.2:17831`。这只是协议冒烟，不能当成公网验收。

## 许可

与仓库根目录相同：**Apache-2.0**。

新增依赖（均 Apache-2.0）：OkHttp 4.12、ZXing core 3.5.3、CameraX 1.4.1。
