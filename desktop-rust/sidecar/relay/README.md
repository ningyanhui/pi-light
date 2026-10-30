# sidecar/relay

本目录在运行时放置 **cloudflared**（Cloudflare Tunnel 客户端）。**不要把二进制提交进 git。**

## 布局

```
{exe_dir}/sidecar/relay/cloudflared.exe   # Windows
{exe_dir}/sidecar/relay/cloudflared       # Linux
```

探测顺序见 `desktop-rust/src/relay_mgr.rs`：

1. 环境变量 `PI_LIGHT_CLOUDFLARED`
2. 捆绑 `{exe_dir}/sidecar/relay/`
3. 用户目录 `~/.pi-light/relay/`（首次下载落点）
4. 系统 `PATH`

## 钉定

版本、下载 URL 与 SHA-256 以仓库 [`scripts/cloudflared.lock.json`](../../../scripts/cloudflared.lock.json) 与根目录 [`NOTICE`](../../../NOTICE) 为准。校验失败不得打入正式 Release，也不得留下未校验文件。

cloudflared 为 **Apache-2.0**（https://github.com/cloudflare/cloudflared）。
