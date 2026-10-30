# sidecar/pi-packages

本目录是 **Pi 预置包** 的运行时根（正式便携 zip 为 `{exe_dir}/sidecar/pi-packages/`）。

- 钉版本与 tarball SHA-256 见仓库 [`scripts/preset-packages.lock.json`](../../../scripts/preset-packages.lock.json) 与 [`docs/preset-packages.md`](../../../docs/preset-packages.md)
- **不要**把解压树或 `node_modules` 提交进 git
- 开发机预览：

```bash
python3 scripts/fetch-pi-packages.py --dest desktop-rust/sidecar/pi-packages
```

或设置 `PI_LIGHT_PACKAGES_DIR` 指向已解压根目录。
