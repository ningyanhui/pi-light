# Companion 协议（M6-lite）

桌面本机控制面 + 公网中继出口。Android companion **不要另起一套配对语义**，直接消费下列字段。聊天默认**写入桌面当前 Pi 会话**（可写，不是只读观察）。

## 二维码 / 配对码

二维码内容是 UTF-8 JSON（不是自定义 URL scheme）：

```json
{
  "v": 1,
  "app": "pi-light",
  "code": "AB12CD",
  "claim": "/v1/pair/claim",
  "chat": "/v1/chat",
  "protocol": "pi-light-companion/1",
  "exp": 1730000000,
  "relay": "https://random-words.trycloudflare.com"
}
```

- `code`：6 位，字母数字（不含 I/O/1/0 易混字符），TTL ≤ 10 分钟。
- 扫码最完整：JSON 同时带 `code` 与 `relay`。
- 手输配对码时，手机**必须**同时填写桌面显示的公网 `relay` URL。临时 trycloudflare 主机名每次启动都会变。
- `GET {relay}/v1/pair/info` **不会**返回 `code`，避免「猜到隧道 URL 就能配对」。

## Claim

`POST {relay}/v1/pair/claim`

```json
{"code":"AB12CD","device_name":"Pixel","platform":"Android"}
```

成功 `200`：

```json
{
  "ok": true,
  "device_id": "dev_…",
  "token": "<hex，只此一次>",
  "relay_url": "https://…",
  "chat_path": "/v1/chat",
  "protocol": "pi-light-companion/1",
  "writable": true
}
```

失败：`400` 缺字段，`401` 配对码不对（响应不含正确码），`409` 桌面未开放配对，`410` 过期。

把 `device_id` + `token` + `relay_url` 存进手机；桌面只保留 `token` 的 SHA-256。撤销设备后同一 token 访问 `/v1/chat` 必须 `401`。

## 远程聊天

所有聊天请求带 `Authorization: Bearer <token>`。

### `GET {relay}/v1/chat`

返回桌面**当前活动会话**快照（最多约 50 条）。`tools` 是当前会话最近的工具调用（最多 12 条），供手机画工具卡；旧客户端可忽略新字段。

```json
{
  "ok": true,
  "writable": true,
  "streaming": false,
  "can_stop": false,
  "partial": "",
  "host": "desktop-name",
  "session_title": "日常咨询",
  "messages": [{"role":"user","text":"…"},{"role":"assistant","text":"…"}],
  "tools": [
    {
      "id": "call-1",
      "name": "bash",
      "status": "running",
      "args": "{\"command\":\"ls\"}",
      "duration_ms": 99
    }
  ],
  "protocol": "pi-light-companion/1",
  "relay": "https://…"
}
```

`tools[].status`：`running` / `done` / `error`。`can_stop` 为 true 时手机可 `POST /v1/chat/stop`（含桌面本机正在生成的一轮）。

### `POST {relay}/v1/chat`

```json
{"text":"帮我看一下桌面"}
```

也接受 `message` / `prompt` 字段。默认仍是整轮 JSON（兼容旧客户端）：成功 `200`：

```json
{
  "ok": true,
  "reply": "助手完整回复",
  "aborted": false,
  "writable": true,
  "protocol": "pi-light-companion/1"
}
```

桌面把这条用户消息写入**当前 Pi 会话**并 `prompt` 给 sidecar。非流式请求等到本轮 `agent_settled`（最长约 120 秒）。桌面聊天区会看到同一条来回。

#### 流式（增量，可选）

请求带 `"stream": true`，或 `Accept: text/event-stream`。响应 `Content-Type: text/event-stream`，每条事件一块 JSON：

| event | data | 含义 |
|-------|------|------|
| `delta` | `{"text":"…"}` | 助手增量文本 |
| `tool` | 与快照 `tools[]` 同形 | 工具开始/结束 |
| `settled` | 与整轮 JSON 成功体同形 | 本轮结束 |
| `error` | `{"ok":false,"error":"…","message":"…"}` | 失败后关闭连接 |

旧客户端不带 `stream`、不声明 SSE 时，行为与原来一致。

### `POST {relay}/v1/chat/stop`

`Authorization: Bearer <token>`。桌面若正在生成（companion 这一轮或桌面本机一轮），排队发送 Pi `abort`。

成功 `200`：`{"ok":true,"accepted":true,"protocol":"pi-light-companion/1"}`。

失败：`401` unauthorized；`409` `not_streaming`（当前没有可停的一轮）。

`GET /health` 的 `chat` / `stream` / `stop` / `desktop` 为 `true`，并带 `stop_path`。不要把局域网发现当成已完成的公网能力。

失败（`POST /v1/chat`）：

| HTTP | error | 含义 |
|------|--------|------|
| 401 | unauthorized | 无 token / token 已撤销 |
| 400 | text_required / invalid_json | 请求体不完整 |
| 409 | busy | 桌面正在生成，或已有一轮 companion 请求 |
| 503 | pi_unavailable | 桌面没有跑着 Pi sidecar |
| 504 | timeout | 本轮未在时限内结束 |

## 远程桌面（JPEG 轮询 + JSON 输入）

这不是 WebRTC。Host 捕获 **Windows 主屏**，缩放到最长边 1280，JPEG 质量约 58，约 150ms 一帧（5–8 fps）。Companion 轮询取帧，并把触控/全屏手势打成 JSON。公网中继延迟通常明显高于局域网。

配对码 JSON、`GET /v1/pair/info`、claim 响应会带：

- `desktop`: `/v1/desktop/session`
- `desktop_frame`: `/v1/desktop/frame`
- `desktop_input`: `/v1/desktop/input`

所有桌面请求带 `Authorization: Bearer <token>`。

### `POST {relay}/v1/desktop/session`

```json
{"action":"start","mode":"browse"}
```

`action`：`start` / `mode` / `stop`。`mode`：`browse`（浏览）/ `touch`（触控）/ `fullscreen`（全屏）。

### `GET {relay}/v1/desktop/session`

```json
{
  "ok": true,
  "active": true,
  "mode": "browse",
  "mode_zh": "浏览",
  "width": 1280,
  "height": 720,
  "seq": 12,
  "capture": "ok",
  "input": "view_only",
  "allow_preview": true,
  "allow_touch": true
}
```

`capture`：`ok` / `permission_required` / `unavailable`。`input`：`ok` / `view_only` / `denied` / `unavailable`。

### `GET {relay}/v1/desktop/frame`

成功 `200`，`Content-Type: image/jpeg`。响应头：

- `ETag` / `X-Pi-Light-Seq`
- `X-Pi-Light-Width` / `X-Pi-Light-Height`
- `X-Pi-Light-Mode` / `X-Pi-Light-Capture`

`If-None-Match: <seq>` 时若无新帧返回 `304`。尚无帧 `204`。桌面关闭预览 `403 preview_disabled`。未开始会话 `409 no_session`。

### `POST {relay}/v1/desktop/input`

```json
{"type":"pointer","x":0.42,"y":0.31,"action":"click","button":"left"}
```

也接受 `scroll`（`dx`/`dy`）、`key`（`key`）、`text`（`text`）。坐标是主屏归一化 0–1。

| 模式 | 输入 |
|------|------|
| 浏览 `browse` | `403 view_only`，不注入 |
| 触控 `touch` / 全屏 `fullscreen` | 注入鼠标键盘（桌面仍可关「允许触控操控」→ `403 touch_disabled`） |

Windows 上截屏走 GDI BitBlt，注入走 `SendInput`。非 Windows Host 诚实返回 `capture=unavailable`，并可能下发占位 JPEG 以打通协议。屏幕捕获 / 模拟输入可能被系统或安全软件拦截。

## 本机绑定

网关只听 `127.0.0.1`。公网暴露必须经中继（默认 Cloudflare Tunnel）。不要把网关绑到 `0.0.0.0` 来「省掉中继」。
