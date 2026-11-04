# pi-light UI system

This document records the shared visual rules for the native desktop shell and Android companion. It is intentionally a compact implementation guide, not a marketing design description.

## Design intent

- **Work-first, not decoration-first.** The interface should feel like a focused coding workspace. Content, tool activity, connection state and next actions take priority over ornament.
- **Graphite surfaces.** Use opaque, layered surfaces rather than translucent glass cards stacked over a decorative wallpaper.
- **Cobalt as the single primary accent.** Reserve accent blue for selected navigation, primary actions, focus and links. Do not use it as a large background fill everywhere.
- **Semantic status colors.** Green means ready/success, amber means attention/waiting, red means failure/destructive. Provider and package identity colors may remain distinct.
- **Consistent density.** Keep control heights, spacing, corner radii and text hierarchy predictable across desktop and mobile.

## Shared palette

| Token | Desktop RGB | Purpose |
|---|---:|---|
| App background | `#0F131A` | Main window / chat canvas |
| Sidebar | `#12171F` | Dock, settings drawer chrome |
| Card | `#1B222D` | Panels, message support surfaces |
| Border | `#2B3544` | Quiet separators and outlines |
| Text primary | `#E7ECF4` | Main content |
| Text secondary | `#9DA8B7` | Metadata and supporting copy |
| Text tertiary | `#707D8F` | Hints and low-priority labels |
| Accent | `#5B89FF` | Primary action / selected state |
| Accent bright | `#7EA4FF` | Hover, link, provider status |
| Success | `#3EB98B` | Connected / enabled |
| Warning | `#DCA953` | Waiting / degraded |
| Danger | `#E16969` | Error / destructive |

Android uses the matching Compose tokens in `ui/theme/Theme.kt`. Legacy symbol names such as `PurplePrimary` are retained for source compatibility; their values now represent the cobalt palette.

## Desktop composition

1. **Workspace dock:** persistent left navigation for Sessions, Files and Git. The selected state uses a muted cobalt-tinted surface and a fine accent outline, not a full-bright neon block.
2. **Top bar:** brand mark, current working directory and actual runtime state. Keep the path switch discoverable without competing with the conversation; model and thinking level live in the composer, not here.
3. **Session strip:** current session identity only (title, id, message count). Creating sessions lives in the dock so there is one entry point.
4. **Conversation canvas:** assistant content is capped at a readable measure; user messages remain right-aligned and narrower. Streaming and completed messages share the same typographic rhythm.
5. **Composer:** the strongest interactive surface: input + send on the first row, model / thinking / settings chips and stop / retry on the second. Context-reference affordances must describe what they really do.
6. **Settings drawer:** one consistent panel surface with a clear title, tabs, scrollable content and explicit error feedback.

## Rendering rules (desktop)

- The theme forces egui dark visuals for both OS themes (`theme::apply_theme`); never rely on `ctx.set_visuals` alone, or light-mode Windows renders white widgets on graphite panels.
- Only use glyphs that the bundled egui fonts, the system CJK font or the symbol fallback (`fonts.rs`) cover. Prefer short text labels or painter-drawn marks (`theme::pi_avatar`, `theme::send_button`) over decorative symbols.
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
