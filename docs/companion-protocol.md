# Companion 协议钩子（供 M6-lite 消费）

M5 桌面侧已经长出本机控制面 + 公网中继出口。Android companion **不要另起一套配对语义**，直接消费下列字段。

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
- 用户也可只看到桌面上的配对码，此时必须同时知道 `relay`（扫码更完整）。
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
  "protocol": "pi-light-companion/1"
}
```

失败：`400` 缺字段，`401` 配对码不对（响应不含正确码），`409` 桌面未开放配对，`410` 过期。

把 `device_id` + `token` 存进手机；桌面只保留 `token` 的 SHA-256。撤销设备后 M6 必须拒绝该 token。

## 远程聊天（M6 才实现）

`GET` / `POST` `{relay}/v1/chat` 在 M5 返回 **HTTP 501**：

```json
{"ok":false,"error":"not_implemented","milestone":"M6","path":"/v1/chat"}
```

建议 M6：同一路径升级为 WebSocket 或 JSONL，带 `Authorization: Bearer <token>`，写入桌面当前 Pi 会话。不要默认改成只读观察。

## 本机绑定

网关只听 `127.0.0.1`。公网暴露必须经中继（默认 Cloudflare Tunnel）。不要把网关绑到 `0.0.0.0` 来「省掉中继」。
