# ai-dock — design roll-up

A macOS menu-bar app that surfaces Codex rate-limit status and OpenRouter credit balance as a glanceable status item. This document summarizes the resolved decisions from the grilling session; authoritative reasoning lives in the ADRs and `CONTEXT.md`.

## Product

- **Menu-bar status item only.** No dock icon, no main window, `LSUIElement = true` / accessory activation policy. Click tray icon → faux-popover (hidden borderless window repositioned under the icon) opens with the detail view.
- **Static tray icon** — no ambient signal, no dynamic fill/ring. All detail lives in the popover.
- **Popover hides on:** window blur, Escape key, tray-icon toggle. No auto-timeout. Just-toggled flag prevents close-then-reopen flicker.
- **No auto-launch at login.** No `tauri-plugin-autostart`.

## Popover content

1. **Codex Primary window** (5h rolling): filled bar of `used_percent`, `reset_at` timestamp. Bar color escalates near the cap.
2. **Codex Secondary window** (7d rolling): filled bar of `used_percent`, `reset_at` timestamp.
3. **OpenRouter credits**: a bar in a **distinct treatment** from the Codex bars (same shape family, visually distinguishable — outline/ghost or neutral color, TBD at implementation), plus literal `$used / $total` text. Fill ratio = `total_usage / total_credits`.
4. **DeepSeek balance**: remaining balance text only; no bar because DeepSeek does not provide a total-limit denominator.
5. **Claude Code Primary window** (5h rolling): filled bar of `five_hour.utilization`, `resets_at` timestamp.
6. **Claude Code Secondary window** (7d rolling): filled bar of `seven_day.utilization`, `resets_at` timestamp.

Plus: a **manual refresh button**; a **settings affordance** for the OpenRouter key (config-file path, see below).

## Data sources

- **Codex:** call `GET https://chatgpt.com/backend-api/wham/usage` with the Codex CLI ChatGPT OAuth token. Structured JSON: `plan_type`, `rate_limit.{allowed, limit_reached, primary_window, secondary_window}` each with `{used_percent, limit_window_seconds, reset_after_seconds, reset_at}`. See ADR-0005.
- **OpenRouter:** `GET https://openrouter.ai/api/v1/credits`, Bearer auth with a **management key** (not a chat key — the docs flag this endpoint as management-key-gated). Response: `{ data: { total_credits, total_usage } }`. Remaining is derived as `total_credits - total_usage`.
- **DeepSeek:** account balance API using the user's DeepSeek API key from config. Displays remaining balance only.
- **Claude Code:** `GET https://api.anthropic.com/api/oauth/usage` with Claude Code OAuth credentials and `anthropic-beta: oauth-2025-04-20`. Displays real `five_hour` and `seven_day` utilization and reset timestamps. This API is reverse-engineered and undocumented; see ADR-0004.

## Credential storage

- Plaintext config file at `~/.config/ai-dock/config.json`, shape `{ "openrouter_key": "sk-or-..." }`. JSON (serde_json is already a Tauri dep). No Keychain, no env var, no LaunchAgent. Hand-editable.

## Architecture

- **Rust owns the poll loop.** A background `tokio` task polls every **5 min** (same interval for all sources), fetches provider usage/balance data, and emits a `status-update` Tauri event with a combined payload.
- **Frontend is a pure renderer.** Subscribes to `status-update`, renders the three rows, calls `invoke("refresh_now")` on the manual button. The OpenRouter key never crosses to the webview.
- **Error/stale policy:** never render a number we don't have. Transient failures (network blip, provider rate limit) → retry on the next poll. Persistent/act-required failures (wrong key, missing auth, schema changed) → error caption, no bar. Zero is never shown for a failed fetch.

## Stack

- Tauri 2, React 19, TypeScript, Vite 7 — kept as-is from the boilerplate.
- New Rust crates: `tokio` (runtime + interval), `reqwest` (provider HTTP calls), `serde`/`serde_json` (already present).
- Cut from boilerplate: the `greet` command, `src/assets/react.svg`, the logo markup in `App.tsx`, `public/vite.svg`, `public/tauri.svg`, `tauri-plugin-opener` (we're not opening URLs).

## Proposed file layout

```
src-tauri/src/
  main.rs              # entry, unchanged
  lib.rs               # tauri::Builder with tray + background poll task
  config.rs            # read ~/.config/ai-dock/config.json
  codex.rs             # read Codex auth and fetch ChatGPT wham usage
  openrouter.rs        # GET /api/v1/credits
  status.rs            # combined Status payload type + poll loop + emit status-update
  claude.rs            # read Claude Code OAuth credentials + GET /api/oauth/usage
src/
  main.tsx             # unchanged
  App.tsx              # replace boilerplate with popover renderer
  App.css              # replace boilerplate with popover styles
  components/
    Bar.tsx            # the progress bar (filled for Codex, differentiated for OpenRouter)
    Row.tsx            # one row: label + bar + reset/dollar text + stale/error caption
docs/
  adr/
    0001-codex-signal-from-logs-db.md
    0002-openrouter-bar-parallel-to-codex-bars.md   # superseded
    0003-openrouter-bar-differentiated-from-codex-bars.md
CONTEXT.md
```

## Open implementation details (decide at the keyboard, not now)

- Exact OpenRouter bar differentiator (outline vs. neutral color vs. glyph) — pick whichever reads cleanest at popover size.
- Codex bar color escalation thresholds (amber at 80%? red at 95%?).
- Exact Codex reset-credit expiry and local spend tile behavior. Option 1 only surfaces live Session/Weekly usage plus reset-credit count when present.

## Icon asset

Three ascending-length rounded bars on a dark squircle, in the same palette as the popover (`--bg` gradient for the squircle, `--accent` gradient for the bars). The three bars mirror the popover's three rows (Codex 5h, Codex 7d, OpenRouter credits), so the glyph reads as a glanceable "status panel" at every scale.

Two assets, distinct purposes — do not collapse them:

- `src-tauri/icons/icon.png` and the platform bundles generated by `pnpm exec tauri icon` (`icon.icns`, `icon.ico`, all `Square*Logo.png`/`StoreLogo.png`/`@2x` variants, `ios/`, `android/`) are **full-color** and used by the bundler for Finder / Dock / About panel / Windows resources / app stores.
- `src-tauri/icons/tray.png` is the **monochrome template** loaded by `lib.rs` (`include_bytes!`, with `icon_as_template(true)` on the `TrayIconBuilder`). It is pure black with alpha; macOS tints it white on dark menu bars and black on light menu bars. Keep it pixel-snapped and free of colour — anything else gets distorted by the template mask.

The split is deliberate: collapsing the tray onto `32x32.png` would either show a colourful glyph in the menu bar (rejected by `icon_as_template(true)`) or hand the bundler a monochrome file (wrong for Finder/Dock). The sources live in `src-tauri/icon-source/` as `bundle.svg` and `tray.svg`; re-render after edits with `rsvg-convert` and `pnpm exec tauri icon`:

```sh
# from project root
rsvg-convert -w 1024 -h 1024 src-tauri/icon-source/bundle.svg -o src-tauri/icons/icon.png
pnpm exec tauri icon src-tauri/icons/icon.png -o src-tauri/icons
rsvg-convert -w 64 -h 64 src-tauri/icon-source/tray.svg  -o src-tauri/icons/tray.png
rsvg-convert -w 32 -h 32 src-tauri/icon-source/tray.svg  -o src-tauri/icons/tray@1x.png
```

## Resolved decision log

| # | Decision | Source |
|---|----------|--------|
| Q3-Q4 | Codex signal = ChatGPT plan rate-limits, not API spend | session |
| Q5 | Codex source = ChatGPT wham API | ADR-0005 |
| Q6 | Auth: read Codex CLI ChatGPT OAuth and refresh once on 401/403 | ADR-0005 |
| Q7 | Show both Primary + Secondary windows | CONTEXT.md |
| Q8 | OpenRouter bar differentiated from Codex bars | ADR-0003 (supersedes 0002) |
| Q9 | Static tray icon, no ambient signal | session |
| Q10 | OpenRouter key in plaintext config file | session |
| Q11 | 5-min poll interval for both + manual refresh | session |
| Q12 | Stale-dim + error caption; never show 0 for a failed fetch | session |
| Q13 | Popover hides on blur + Esc + tray-toggle | session |
| Q14 | No auto-launch at login | session |
| Q15 | Rust owns poll loop, frontend pure renderer | session |
| Q16 | Claude Code usage source = reverse-engineered OAuth usage API | ADR-0004 |
