# pi-light UI system

This document records the shared visual rules for the native desktop shell and Android companion. It is intentionally a compact implementation guide, not a marketing design description.

## Design intent

- **Work-first, not decoration-first.** The interface should feel like a focused coding workspace. Content, tool activity, connection state and next actions take priority over ornament.
- **Layered near-black surfaces.** Use opaque, layered surfaces rather than translucent glass cards stacked over a decorative wallpaper.
- **Indigo as the single primary accent.** Reserve `#7C8CFF` for selected navigation, primary actions, focus and links. Do not use it as a large background fill everywhere.
- **Semantic status colors.** Green means ready/success, amber means attention/waiting, red means failure/destructive. Provider and package identity colors may remain distinct.
- **Consistent density.** Keep control heights, spacing, corner radii and text hierarchy predictable across desktop and mobile.

## Shared palette

| Token | Desktop RGB | Purpose |
|---|---:|---|
| App background | `#0A0C10` | Main window / chat canvas |
| Rail | `#0D1015` | Frameless title bar, icon rail |
| Sidebar / panel | `#11151B` | Dock, dropdowns, settings chrome |
| Card | `#161B23` | Panels, message support surfaces |
| Card raised | `#1B212B` | Hover rows, keycaps |
| Border | `#232A35` | Quiet separators and outlines |
| Border strong | `#2E3746` | Floating surfaces, window edge |
| Text primary | `#E8EDF5` | Main content |
| Text secondary | `#A3ADBD` | Metadata and supporting copy |
| Text tertiary | `#6B7688` | Hints and low-priority labels |
| Accent | `#7C8CFF` | Primary action / selected state |
| Accent bright | `#9AA6FF` | Hover, link, icon emphasis |
| Accent cyan | `#22D3EE` | Secondary accent, live indicators |
| Success | `#34D399` | Connected / enabled |
| Warning | `#FBBF24` | Waiting / degraded |
| Danger | `#F87171` | Error / destructive |

Android uses the matching Compose tokens in `ui/theme/Theme.kt`. Legacy symbol names such as `PurplePrimary` are retained for source compatibility; their values now represent the shared indigo palette.

## Desktop composition

1. **Title bar (frameless):** the window has no OS decorations (`with_decorations(false)`); `show_title_bar` paints brand, the workspace switcher, 打开文件夹, remote / reconnect / settings icons and custom minimize / maximize / close. Empty bar space drags the window, double-click toggles maximize, and `handle_window_frame` provides 5px resize edges plus a 1px window border.
2. **Workspace switching:** never ask for a typed path. The switcher dropdown offers the native folder picker (`rfd`), recent folders, and a drag-and-drop hint; Ctrl+O and dropping a folder onto the window do the same. All routes end in `apply_workdir_path`.
3. **Icon rail + side panel:** a 48px rail (`show_icon_rail`) switches the 248px side panel between 会话 / 文件 / Git; clicking the active icon collapses the panel. The rail also holds 扩展市场 and 设置. Sessions are grouped into 最近 (24h) / 更早, with one 新建会话 entry in the panel header.
4. **Session strip + inspector:** the strip shows title, id / message count (or an Agent 运行中 pill), the current model · thinking, and the inspector toggle. The right inspector (`timeline_ui::show_inspector`, default on at ≥1280px) shows context usage, files read / changed this turn and run stats.
5. **Conversation canvas:** content is centred at ≤820px. Tool calls render as a step timeline (`timeline_ui::show_timeline`): 读取 / 执行 / 修改 / 写入 / 搜索 rows with status, duration and an inline diff for edit / write steps (`tool_step::summarize` derives target and diff from the tool `args`).
6. **Composer:** a floating card centred under the canvas: reference chips, the input, then 引用文件 / model / thinking on the left and retry + send (or 停止 while streaming) on the right.
7. **Settings panel:** a centered modal (max 1040×780) over the shell with a 184px left category nav (`theme::nav_row`, painter icons) and a scrollable content pane; 模型提供商 dialogs open at a fixed 520px width with a viewport-bounded, scrolling body.
   - 记忆：左侧按 项目指令 / 项目记忆 / 会话记忆 / 用户指令 / 全局记忆 分组的文件列表，右侧 markdown 全文查看，可切换编辑并在文件管理器中显示。
   - Skills：卡片显示 SKILL.md 描述与来源（项目/用户），点击在右侧查看全文。MCP：卡片显示 command / args / env，支持编辑、二次确认删除、查看 mcp.json。

## Rendering rules (desktop)

- The theme forces egui dark visuals for both OS themes (`theme::apply_theme`); never rely on `ctx.set_visuals` alone, or light-mode Windows renders white widgets on graphite panels.
- Only use glyphs that the bundled egui fonts, the system CJK font or the symbol fallback (`fonts.rs`) cover. For icons use the painter-drawn line set in `icons.rs` (`icons::paint`, `icons::icon_button`) instead of symbols.
- Secondary per-message actions (copy, regenerate, edit) are quiet text buttons; reserve filled buttons for primary actions.

## Android companion composition

- Home prioritizes desktop connection state, relay/latency information and the two main tasks: chat and remote desktop.
- Pairing prioritizes QR scanning, a legible one-time code and a clear pending/connected state.
- Chat mirrors the desktop's current session. Do not show a fake session switcher or a new-session action that cannot be completed on the phone.
- Remote desktop uses the same graphite surfaces around the actual screen image. Browse, touch and fullscreen modes must remain visually distinct and explain whether input is enabled.
- Settings expose only controls that the companion can actually enforce. Host-owned routing and model configuration should be described as host-owned rather than represented as phone-side selectors.

## Interaction and copy rules

- Every visible action should work, or be visibly disabled with a reason. Avoid inert icons that look like attachment, refresh or create actions.
- Display status from real protocol state. Never show “connected” just because a pairing code or relay URL exists.
- Empty states explain the next useful action. Error states say what failed and provide retry or recovery when available.
- Keep keyboard focus, selected states and error contrast visible. Validate keyboard navigation, screen-reader labels and touch targets in runtime testing; source styling alone does not prove accessibility.

## Functional boundaries to keep visible

- Android is a companion, not an independent Pi client: chat writes to the desktop's current Pi session.
- The Android companion does not currently provide independent session creation/switching, model/API-key management or file attachments.
- Remote desktop uses JPEG polling (documented target around 5–8 fps), not WebRTC. Actual capture and input injection depend on a supported Windows host and its permissions.
- The official release path is Windows x64 portable zip. Android currently builds a debug APK; production signing and store distribution are separate work.

## Source of truth

- Desktop palette and shared controls: `desktop-rust/src/theme.rs`
- Desktop composition: `desktop-rust/src/main.rs`, `desktop-rust/src/workspace.rs`, `desktop-rust/src/admin_panel.rs`
- Android palette: `android/app/src/main/java/com/pilight/android/ui/theme/Theme.kt`
- Android screens: `HomeScreen.kt`, `PairScreen.kt`, `ChatScreen.kt`, `DesktopScreen.kt`, `SettingsScreen.kt`
