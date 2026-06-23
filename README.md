# ai-dock

A macOS menu-bar app that surfaces **Codex rate-limit status** and
**OpenRouter credit balance** as a glanceable popover. Click the tray icon,
see your usage, move on.

- **Tray icon** in the macOS menu bar. Static; the data lives in the popover.
- **Popover** shows both Codex windows (5h + weekly) and your OpenRouter
  credits, each with a usage bar and a "resets in …" hint.
- **Manual refresh** forces an immediate poll of both sources.
- **Settings** lives inside the popover — paste your OpenRouter management
  key, it's stored in `~/.config/ai-dock/config.json`.
- **No auto-launch.** Launch it manually when you want it.

The popover is built from the spec in [`docs/spec/2026-06-22-initial.md`](docs/spec/2026-06-22-initial.md).
Product reasoning and decision history live in [`DESIGN.md`](DESIGN.md),
[`CONTEXT.md`](CONTEXT.md), and [`docs/adr/`](docs/adr/).

## Requirements

- macOS (uses `LSUIElement` and the menu-bar tray APIs)
- Rust toolchain (for `src-tauri`)
- Node + pnpm (for the Vite frontend)

## Development

```bash
pnpm install
pnpm tauri dev
```

## Build

```bash
pnpm tauri build
```

The bundled `.app` is in `src-tauri/target/release/bundle/macos/`.
