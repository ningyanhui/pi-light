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

返回桌面**当前活动会话**快照（最多约 50 条）：

```json
{
  "ok": true,
  "writable": true,
  "streaming": false,
  "partial": "",
  "host": "desktop-name",
  "session_title": "日常咨询",
  "messages": [{"role":"user","text":"…"},{"role":"assistant","text":"…"}],
  "protocol": "pi-light-companion/1",
  "relay": "https://…"
}
```

### `POST {relay}/v1/chat`

```json
{"text":"帮我看一下桌面"}
```

也接受 `message` / `prompt` 字段。成功 `200`：

```json
{
  "ok": true,
  "reply": "助手完整回复",
  "aborted": false,
  "writable": true,
  "protocol": "pi-light-companion/1"
}
```

桌面把这条用户消息写入**当前 Pi 会话**并 `prompt` 给 sidecar，手机一直等到本轮 `agent_settled`（最长约 120 秒）。桌面聊天区会看到同一条来回。

失败：

| HTTP | error | 含义 |
|------|--------|------|
| 401 | unauthorized | 无 token / token 已撤销 |
| 400 | text_required / invalid_json | 请求体不完整 |
| 409 | busy | 桌面正在生成，或已有一轮 companion 请求 |
| 503 | pi_unavailable | 桌面没有跑着 Pi sidecar |
| 504 | timeout | 本轮未在时限内结束 |

`GET /health` 的 `chat` 为 `true`。不要把局域网发现当成已完成的公网能力。

## 本机绑定

网关只听 `127.0.0.1`。公网暴露必须经中继（默认 Cloudflare Tunnel）。不要把网关绑到 `0.0.0.0` 来「省掉中继」。
