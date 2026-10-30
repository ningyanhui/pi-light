# 远程访问验收（v3 M5 + M6-lite）

桌面设置抽屉 **远程访问** 页对照原型 `acceptance/04-desktop-remote`。Android companion 对照 `acceptance/03-mobile-pair`、`acceptance/05-mobile-chat`、`mobile-v2/01-home`、`mobile-v2/05-chat`（只学信息架构，不拷贝他仓实现）。

本里程碑交付：**公网配对 + 一通远程聊天**。远程桌面三档（浏览 / 触控 / 全屏）不在范围。

## 配对流程

1. 打开 pi-light → 左下角齿轮 → **远程访问**。
2. 打开 **允许手机远程**。壳会在 `127.0.0.1` 绑定本机网关（默认端口 `17831`，被占用则换临时端口）。
3. 默认中继 **Cloudflare Tunnel（临时 trycloudflare）**：
   - 探测 `PI_LIGHT_CLOUDFLARED` → `{exe_dir}/sidecar/relay/cloudflared[.exe]` → `~/.pi-light/relay/` → `PATH`。
   - 找不到则点 **下载 cloudflared**（钉版本 + SHA-256，见 `scripts/cloudflared.lock.json` / `NOTICE`）。官方 Windows 便携包在 Release 流水线里已打入校验过的二进制。
   - 启动：`cloudflared tunnel --no-autoupdate --url http://127.0.0.1:<port>`。
   - 从 stdout/stderr 解析 `https://*.trycloudflare.com` 后状态灯为 **已连接**。
4. 中继有公网 URL 后显示 **配对二维码** 与 **6 位配对码**（TTL 10 分钟，可复制 / 刷新）。二维码 JSON 含 `code`、`relay`、`claim`、`exp`。
5. 手机打开 companion：扫码，或手输配对码 **以及** 桌面上的中继 URL。向 `{relay}/v1/pair/claim` POST `{"code","device_name","platform"}`。校验通过后桌面「已授权设备」出现该设备，可 **撤销**。
6. 手机在「会话」发一条消息：`POST {relay}/v1/chat`（`Authorization: Bearer <token>`）。桌面当前 Pi 会话出现该用户消息，回复后手机显示助手文本。

不要把局域网发现当成已完成的公网能力。

## 中继如何启动 / 看状态

| 方式 | 启动 | 状态 |
|------|------|------|
| Cloudflare 临时隧道（默认） | 开关打开且找到 cloudflared 后自动 `tunnel --url`；也可点「启动中继 / 停止中继」 | 未找到 / 下载中 / 建立中 / 已连接（显示 trycloudflare URL）/ 出错 |
| Cloudflare 命名隧道 | 填写 token 与 https 主机名，走 `tunnel run --token` | 进程在跑且填了主机名视为已连接 |
| Tailscale | 不代为安装。用户自装并填写 tailnet 上的网关地址 | 「已填写入口」 |
| 自建 | 用户反代到本机网关并填写公网 URL | 「已填写入口」 |

临时 trycloudflare **不需要** Cloudflare 账号，主机名每次启动都会变。命名隧道与 Tailscale 需要各自账号。中继依赖网络；未连上中继时页面不会假装「同一 Wi‑Fi 就能给蜂窝网手机用」。

## 公网剧本（蜂窝网手机 ↔ 家宽桌面）

这条才是 M6-lite 验收，不是「两台设备连同一个 Wi‑Fi」。

1. **电脑在家宽**，打开 pi-light，配好模型密钥，确认桌面能完成本地一通聊天。
2. 设置 → 远程访问 → 打开「允许手机远程」，等到 Cloudflare Tunnel **已连接** 且出现 `https://….trycloudflare.com`。
3. **手机开蜂窝数据**（关闭该电脑所在 Wi‑Fi，或用另一条蜂窝链路）。
4. 打开 companion：扫描桌面二维码，**或** 输入 6 位配对码 + 上面的 trycloudflare URL。
5. 桌面「已授权设备」出现该手机；手机首页显示「我的电脑 / 已连接 / via 中继」。
6. 点「进入会话」，发一条短消息（例如「回复 ping」）。
7. 桌面同一 Pi 会话出现这条用户消息；助手回复后，手机气泡里能看到回复。

需要的条件（诚实）：运行中的桌面、已配置的模型、本机网关、公网中继、手机能访问该 HTTPS 主机名。任一步失败都不能声称「公网远程已通」。临时隧道主机名在下次启动中继后会变，必须重新扫码或改填 URL。

本机冒烟（**不能**代替上面的公网剧本）：

```bash
# 隧道起来之前也可以打 loopback
curl -s http://127.0.0.1:17831/health
curl -s http://127.0.0.1:17831/v1/pair/info   # 不含配对码明文
TOKEN=$(curl -s -X POST http://127.0.0.1:17831/v1/pair/claim \
  -H 'content-type: application/json' \
  -d '{"code":"<当前配对码>","device_name":"Pixel","platform":"Android"}' \
  | python -c "import sys,json; print(json.load(sys.stdin)['token'])")
curl -s http://127.0.0.1:17831/v1/chat -H "Authorization: Bearer $TOKEN"
curl -s -X POST http://127.0.0.1:17831/v1/chat \
  -H "Authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' \
  -d '{"text":"ping from curl"}'
```

Android 模拟器打宿主机网关可用 `http://10.0.2.2:17831`（debug 允许该 cleartext）。这只证明协议，**不是**蜂窝↔家宽验收。

配置落在 `~/.pi-light/remote.json`（可用 `PI_LIGHT_REMOTE_PATH` 覆盖）。Unix 权限 `0600`。日志与错误不回显配对码或 token。

## 诚实限制

- 这不是局域网产品。未建立公网中继就不能声称「手机在外面能连」。
- 远程桌面三档（浏览 / 触控 / 全屏）不在本里程碑；companion 上的「远程桌面」入口会说明即将推出。
- 电脑操控扩展不会因为打开远程而自动启用。
- 聊天默认可写同一 Pi 会话；只读观察不是默认，本里程碑不做该设置项。
- cloudflared 体积较大；未过 SHA 的文件不得留在 sidecar/relay，也不得打进 Release。
- 临时隧道、模型 API、家宽上行都可能失败；失败时 UI 应说中继/网络问题，而不是假装已经连上。
