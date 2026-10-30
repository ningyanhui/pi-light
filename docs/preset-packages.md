# 预置 Pi 包（M2）

发行便携包在 `sidecar/pi-packages/` 放入钉版本的 npm tarball 解压树。壳启动时把 **Slim 五包** 的本地 path 写入用户 `settings.json`（`~/.pi/agent/settings.json`，可被 `PI_CODING_AGENT_DIR` 覆盖），因此第一次打开不必手敲 `npm` 或 `pi install`。

权威钉：[`scripts/preset-packages.lock.json`](../scripts/preset-packages.lock.json)。SHA-256 是 **npm tarball** 的摘要；缺失或与下载文件不符时，`scripts/fetch-pi-packages.py` 与 Release 必须失败。

钉规格（与 lockfile / NOTICE 一致）：`pi-marketplace@0.1.3` · `pi-mcp-adapter@7.0.0` · `pi-web-access@0.46.0` · `pi-memory@0.4.2` · `pi-subagents@0.84.0` · `pi-browser-use@0.12.15` · `pi-chrome-connector@0.2.2` · `@injaneity/pi-computer-use@0.5.1`。

## Slim 默认启用（打进正式 Release）

| 包 | 版本 | 许可 | tarball SHA-256 |
|----|------|------|-----------------|
| pi-marketplace | 0.1.3 | MIT | `3c852b0d3b368fb9a5e86104fd4376a3037d3b1c1e688887e296b72025beea7b` |
| pi-mcp-adapter | 7.0.0 | MIT | `1cd1eab457c0303a4574e04731c566162ef21d98614d1341751e0ad27648ed73` |
| pi-web-access | 0.46.0 | MIT | `14f9ec9ed276d54118996788f268e537070aac05d5f0a3a9333869698d5a94ad` |
| pi-memory | 0.4.2 | MIT | `ac4826a4b16bb75e81871aa4ccaccb7701fd09cbd7ed6bf785ad99bd3ca60e3f` |
| pi-subagents | 0.84.0 | MIT | `f9c038aac95d860c7adb77f02919b5016f410f8670f41a8cb911b56598742a02` |

zip 内路径：`sidecar/pi-packages/<dir>/`，与 lock 的 `dir` 字段一致。Pi 以 **本地 path** 加载（不复制到 `~/.pi/agent/npm`）。相对 path 会相对 settings 文件解析，所以壳写入**绝对路径**，并在安装目录搬家后静默修复。

若用户已经用市场装了同名 `npm:<name>`，不重复写入本地 path。

## 操控档（预置文件、默认关闭、一键启用）

| 包 | 版本 | 许可 | tarball SHA-256 | 启用 |
|----|------|------|-----------------|------|
| pi-browser-use | 0.12.15 | MIT | `0d194a3fccace45784bc1f34f81760442beb61e99fa71415b5b0eee0d015487a` | 一键启用；装后 `/reload` 或重启 sidecar |
| pi-chrome-connector | 0.2.2 | MIT | `4e050fb8fe0baffcae601b4fd96ee08dc7f8ecbbb8fed486327f2bc167ca9065` | 同上；Chrome Companion 需用户自行装到浏览器 |
| @injaneity/pi-computer-use | 0.5.1 | MIT | `ec7fff45a3febbab2b5c302db1171f5f10d6ca85aacd892a9f422028246dcb79` | 必须先过 Win 权限向导壳（辅助功能 / 截屏 / 模拟输入 / 后台）；**不**默认自动开 |

本里程碑不跑浏览器五步真操控或电脑 observe/click/scroll 烟测（M3/M4）。

## 明确排除默认包（市场可装）

- `pi-lens`
- `billion-context`

二者体积与供应链成本高，不进 Slim，也不进操控预置。扩展市场仍可搜索、确认后 `pi install`。

## 拉取与校验

```bash
# CI：只下载 tarball 对 SHA，并核对 NOTICE / 本文
python3 scripts/fetch-pi-packages.py --verify-only --check-docs

# 本地或 Release：解压到发行树；Windows Release 另加 --install-deps
python3 scripts/fetch-pi-packages.py --dest sidecar/pi-packages --install-deps --check-docs
```

`--install-deps` 仅在包的 `package.json` 声明了 `dependencies` 时于**该包目录**执行 `npm install --omit=dev --legacy-peer-deps`。这是为了让本地 path 包带上运行时依赖（Pi 对 local 包不会再跑 npm）。**不**把 Node 运行时打进 zip。该步骤必须在目标平台（正式 Windows 便携包 = `windows-latest`）上执行，以便原生附加组件匹配 Windows。

## 开发覆盖

- `PI_LIGHT_PACKAGES_DIR`：指向已解压的 `pi-packages` 根目录（测试或本机预览）
- 未解压时，设置抽屉仍可浏览市场；已安装列表不会出现预置项，这是源码树的正常状态
