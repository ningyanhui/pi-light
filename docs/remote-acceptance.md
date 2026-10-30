# 远程访问验收（v3 M5）

桌面设置抽屉 **远程访问** 页对照原型 `acceptance/04-desktop-remote`。本里程碑交付桌面侧公网配对与中继；手机 companion 真聊是 **M6**。

## 配对流程

1. 打开 pi-light → 左下角齿轮 → **远程访问**。
2. 打开 **允许手机远程**。壳会在 `127.0.0.1` 绑定本机网关（默认端口 `17831`，被占用则换临时端口）。
3. 默认中继 **Cloudflare Tunnel（临时 trycloudflare）**：
   - 探测 `PI_LIGHT_CLOUDFLARED` → `{exe_dir}/sidecar/relay/cloudflared[.exe]` → `~/.pi-light/relay/` → `PATH`。
   - 找不到则点 **下载 cloudflared**（钉版本 + SHA-256，见 `scripts/cloudflared.lock.json` / `NOTICE`）。官方 Windows 便携包在 Release 流水线里已打入校验过的二进制。
   - 启动：`cloudflared tunnel --no-autoupdate --url http://127.0.0.1:<port>`。
   - 从 stdout/stderr 解析 `https://*.trycloudflare.com` 后状态灯为 **已连接**。
4. 中继有公网 URL 后显示 **配对二维码** 与 **6 位配对码**（TTL 10 分钟，可复制 / 刷新）。二维码 JSON 含 `code`、`relay`、`claim`、`exp`。
5. 手机（M6）扫码或手输配对码，向 `{relay}/v1/pair/claim` POST `{"code","device_name","platform"}`。校验通过后桌面「已授权设备」出现该设备，可 **撤销**。
6. `GET /v1/chat` 目前返回 **501**（明确留给 M6）。不要把局域网发现当成已完成的公网能力。

## 中继如何启动 / 看状态

| 方式 | 启动 | 状态 |
|------|------|------|
| Cloudflare 临时隧道（默认） | 开关打开且找到 cloudflared 后自动 `tunnel --url`；也可点「启动中继 / 停止中继」 | 未找到 / 下载中 / 建立中 / 已连接（显示 trycloudflare URL）/ 出错 |
| Cloudflare 命名隧道 | 填写 token 与 https 主机名，走 `tunnel run --token` | 进程在跑且填了主机名视为已连接 |
| Tailscale | 不代为安装。用户自装并填写 tailnet 上的网关地址 | 「已填写入口」 |
| 自建 | 用户反代到本机网关并填写公网 URL | 「已填写入口」 |

临时 trycloudflare **不需要** Cloudflare 账号，主机名每次启动都会变。命名隧道与 Tailscale 需要各自账号。中继依赖网络；未连上中继时页面不会假装「同一 Wi‑Fi 就能给蜂窝网手机用」。

## 公网剧本（演示）

1. 电脑在家宽，打开远程访问，等到 Cloudflare Tunnel **已连接** 且出现 `https://….trycloudflare.com`。
2. 手机开 **蜂窝数据**（不要连电脑同一 Wi‑Fi）。
3. 扫码或输入配对码；桌面设备列表出现该手机。
4. M6 完成后：在手机发一条消息，桌面同一 Pi 会话应能看见。M5 只验收到第 3 步（claim 可用 `curl` 代替手机）。

```bash
# 本机冒烟（隧道起来之前也可以打 loopback）
curl -s http://127.0.0.1:17831/health
curl -s http://127.0.0.1:17831/v1/pair/info   # 不含配对码明文
curl -s -X POST http://127.0.0.1:17831/v1/pair/claim \
  -H 'content-type: application/json' \
  -d '{"code":"<当前配对码>","device_name":"Pixel","platform":"Android"}'
```

配置落在 `~/.pi-light/remote.json`（可用 `PI_LIGHT_REMOTE_PATH` 覆盖）。Unix 权限 `0600`。日志与错误不回显配对码。

## 诚实限制

- 这不是局域网产品。未建立公网中继就不能声称「手机在外面能连」。
- 远程聊天、远程桌面三档（浏览 / 触控 / 全屏）不在本里程碑。
- 电脑操控扩展不会因为打开远程而自动启用。
- cloudflared 体积较大；未过 SHA 的文件不得留在 sidecar/relay，也不得打进 Release。
