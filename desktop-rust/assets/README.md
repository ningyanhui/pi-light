# pi-light 品牌图标

洁净室自制：紫色圆底 + 几何 π。用于 Windows `pi-light.exe` 的文件/任务栏图标。

| 文件 | 用途 |
|------|------|
| `pi-light.ico` | 多尺寸 ICO（16/24/32/48/64/128/256），由 `build.rs` + `winres` 嵌入 exe |
| `pi-light-256.png` | 运行时窗口图标（`eframe` `ViewportBuilder::with_icon`） |
| `pi-light.svg` | 矢量源，便于以后重导出 |

不要使用 eframe 默认的「E」占位图标。

## 本地验证（Windows）

1. `cargo build --release -p pi-light --bin pi-light`
2. 资源管理器中查看 `target/release/pi-light.exe`：文件图标应为紫色 π，而不是大写 E
3. 右键 → 属性 → 详细信息：产品名称为 pi-light
4. 运行 exe，任务栏图标应为同一品牌标（不是 E）
