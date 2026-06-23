//! ai-dock — macOS menu-bar popover that surfaces Codex rate-limit status and
//! OpenRouter credit balance.
//!
//! Architecture (per `docs/spec/2026-06-22-initial.md` §5):
//!   - Rust owns the poll loop; the frontend never polls, fetches, or reads files.
//!   - A tokio task wakes every `status::POLL_INTERVAL_SECS` and emits
//!     `status-update` to the webview.
//!   - Manual refresh (`refresh_now`) and tray-click both kick the loop via a
//!     shared `Notify`.
//!   - The tray icon is static; the popover toggles open on tray-click and
//!     closes on blur, Escape, or tray-click-while-open. A short just-toggled
//!     flag suppresses the blur that fires as part of the click itself, so the
//!     popover doesn't close-then-immediately-reopen (§1.3 flicker guard).

mod codex;
mod claude;
mod config;
mod deepseek;
mod openrouter;
mod status;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{
    image::Image,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Position, RunEvent, Size as DpiSize,
    WindowEvent,
};
use tokio::sync::Notify;

const POPOVER_LABEL: &str = "popover";
const TRAY_ID: &str = "main";
/// How long the just-toggled flag suppresses the blur that fires as part of
/// the tray click itself. Short enough to feel instant; long enough to span
/// the click → focus-loss round-trip on macOS.
const JUST_TOGGLED_CLEAR_MS: u64 = 200;

/// Tray icon baked into the binary at compile time.
///
/// A dedicated monochrome template (black with alpha) so macOS can tint it
/// to match the active menu-bar appearance (white on dark menu bars, black
/// on light). Distinct from `icons/32x32.png`, which stays a colourful bundle
/// asset for the Finder/Dock.
const TRAY_ICON_PNG: &[u8] = include_bytes!("../icons/tray.png");

// ---------- Tauri commands ----------

/// Force an immediate poll cycle. The frontend invokes this from the refresh
/// button. The actual update flows back through the `status-update` event;
/// the 5-minute timer is *not* reset (§5.2).
#[tauri::command]
async fn refresh_now(
    kick: tauri::State<'_, Arc<Notify>>,
) -> Result<(), String> {
    kick.notify_one();
    Ok(())
}

/// Persist a new OpenRouter management key. The key never crosses back to the
/// frontend — Rust re-reads the config file on every poll (§3 in the spec).
#[tauri::command]
fn set_openrouter_key(key: String) -> Result<(), String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("OpenRouter key cannot be empty".into());
    }
    let mut cfg = config::read();
    cfg.openrouter_key = Some(trimmed.to_string());
    config::write(&cfg)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Persist a new DeepSeek API key.
#[tauri::command]
fn set_deepseek_key(key: String) -> Result<(), String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err("DeepSeek key cannot be empty".into());
    }
    let mut cfg = config::read();
    cfg.deepseek_key = Some(trimmed.to_string());
    config::write(&cfg)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Read the provider visibility map for the settings UI and row filtering.
/// Missing entries are expanded with show-by-default values for known providers.
#[tauri::command]
fn get_provider_visibility() -> BTreeMap<String, bool> {
    config::read().provider_visibility_map()
}

/// Persist a provider visibility override. Provider ids are intentionally
/// generic so future providers can reuse the same config field and command.
#[tauri::command]
fn set_provider_visibility(provider: String, visible: bool) -> Result<(), String> {
    let provider = provider.trim();
    if provider.is_empty() {
        return Err("Provider id cannot be empty".into());
    }
    let mut cfg = config::read();
    cfg.set_provider_visibility(provider, visible);
    config::write(&cfg)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Quit the application entirely.
#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}
#[tauri::command]
fn hide_popover(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window(POPOVER_LABEL) {
        w.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------- App entry ----------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Shared between the poll loop (consumer) and the refresh_now command /
    // tray click (producers).
    let kick = Arc::new(Notify::new());
    // Set just before a tray-click-driven show/hide; cleared after a short
    // delay. While set, WindowEvent::Focused(false) is ignored, which
    // prevents the click → blur → close → reopen flicker (§1.3).
    let just_toggled = Arc::new(AtomicBool::new(false));

    tauri::Builder::default()
        .manage(kick.clone())
        .invoke_handler(tauri::generate_handler![
            refresh_now,
            set_openrouter_key,
            set_deepseek_key,
            get_provider_visibility,
            set_provider_visibility,
            quit_app,
            hide_popover,
        ])
        .setup({
            let kick = kick.clone();
            let just_toggled = just_toggled.clone();
            move |app| {
                // LSUIElement equivalent: no dock icon, no Cmd-Tab entry (§5.4).
                #[cfg(target_os = "macos")]
                {
                    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                }

                // 1. Static tray icon (no ambient signal — §1.2).
                let icon = Image::from_bytes(TRAY_ICON_PNG)?;
                let tray_jt = just_toggled.clone();
                TrayIconBuilder::with_id(TRAY_ID)
                    .icon(icon)
                    .icon_as_template(true) // macOS treats it as a monochrome glyph
                    .show_menu_on_left_click(false) // route left-click to on_tray_icon_event
                    .on_tray_icon_event(move |tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            rect,
                            ..
                        } = event
                        {
                            toggle_popover(
                                tray.app_handle(),
                                &tray_jt,
                                Some(rect),
                            );
                        }
                    })
                    .build(app)?;

                // 2. Background poll loop. `kick` is shared with refresh_now.
                status::spawn_loop(app.handle().clone(), kick.clone());

                Ok(())
            }
        })
        .on_window_event({
            let just_toggled = just_toggled.clone();
            move |window, event| {
                // Only the popover window participates in the blur-hide behavior.
                if window.label() != POPOVER_LABEL {
                    return;
                }
                if let WindowEvent::Focused(false) = event {
                    // The tray-click itself causes a blur; the flag swallows that
                    // one so we don't immediately hide what we just (re)opened.
                    if just_toggled.swap(false, Ordering::SeqCst) {
                        return;
                    }
                    if let Err(e) = window.hide() {
                        eprintln!("ai-dock: hide on blur failed: {e}");
                    }
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(handle_run_event);
}

fn handle_run_event(_app: &AppHandle, event: RunEvent) {
    // Currently no-op: the spec keeps the app alive without auto-launch (§1.5)
    // and we don't intercept exit. Hook reserved for future graceful shutdown
    // (e.g. flushing in-flight fetch state).
    if let RunEvent::ExitRequested { .. } = event {
        // let it exit; nothing to clean up.
    }
}

// ---------- Popover lifecycle ----------

fn toggle_popover(
    app: &AppHandle,
    just_toggled: &Arc<AtomicBool>,
    rect: Option<tauri::Rect>,
) {
    let Some(window) = app.get_webview_window(POPOVER_LABEL) else {
        eprintln!("ai-dock: popover window '{POPOVER_LABEL}' not found");
        return;
    };

    let visible = window.is_visible().unwrap_or(false);

    // Raise the flicker-guard flag *before* any show/hide, so the blur that
    // fires as part of the click is swallowed by on_window_event.
    just_toggled.store(true, Ordering::SeqCst);

    if visible {
        if let Err(e) = window.hide() {
            eprintln!("ai-dock: hide failed: {e}");
        }
    } else {
        // macOS: tell the NSWindow to follow the active Space so clicking the
        // tray icon from any desktop shows the popover there, not the last one.
        #[cfg(target_os = "macos")]
        move_window_to_active_space(&window);

        if let Some(r) = rect {
            position_under_tray(&window, &r);
        }
        if let Err(e) = window.show() {
            eprintln!("ai-dock: show failed: {e}");
        }
        if let Err(e) = window.set_focus() {
            eprintln!("ai-dock: set_focus failed: {e}");
        }
    }

    // Clear the flag after a short delay. The blur event from the tray click
    // lands well within this window on macOS; longer than that and we'd
    // suppress legitimate blur-driven hides.
    let jt = just_toggled.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(JUST_TOGGLED_CLEAR_MS)).await;
        jt.store(false, Ordering::SeqCst);
    });
}

/// Place the popover centered horizontally under the tray icon, just below it.
/// Falls back to the current position if anything goes wrong.
///
/// Multi-monitor aware: finds the monitor containing the tray icon and clamps
/// the popover within that monitor's bounds (handles displays to the
/// left/right/above/below the primary, which have negative coordinates).
fn position_under_tray(window: &tauri::WebviewWindow, rect: &tauri::Rect) {
    // TrayIconEvent::Click.rect carries physical pixels on macOS.
    let (icon_x, icon_y, icon_w, icon_h) = match (rect.position, rect.size) {
        (Position::Physical(p), DpiSize::Physical(s)) => (p.x, p.y, s.width, s.height),
        (Position::Logical(p), DpiSize::Logical(s)) => (p.x as i32, p.y as i32, s.width as u32, s.height as u32),
        // Mixed variants — coerce to physical via window scale factor.
        _ => return,
    };

    let win_size = window
        .outer_size()
        .unwrap_or(PhysicalSize::new(320, 240));

    let pop_w = win_size.width as i32;
    let pop_h = win_size.height as i32;

    let icon_center_x = icon_x + (icon_w as i32) / 2;
    let icon_bottom_y = icon_y + icon_h as i32;

    let mut x = icon_center_x - pop_w / 2;
    let mut y = icon_bottom_y + 4;

    // Find the monitor that contains the tray icon and clamp within it.
    // This is critical for multi-monitor setups where a secondary display
    // sits to the left or above the primary (negative global coordinates);
    // the old origin-based clamp forced the popover back onto the primary.
    let monitors = window.available_monitors().unwrap_or_default();
    eprintln!(
        "ai-dock: tray icon at ({icon_x},{icon_y}) {icon_w}x{icon_h}; popover {pop_w}x{pop_h}; {} monitor(s)",
        monitors.len()
    );
    for m in &monitors {
        let mp = m.position();
        let ms = m.size();
        let (mx, my, mw, mh) = (mp.x, mp.y, ms.width as i32, ms.height as i32);
        eprintln!(
            "ai-dock:   monitor at ({mx},{my}) {mw}x{mh} name={:?}",
            m.name()
        );
        if icon_center_x >= mx && icon_center_x < mx + mw
            && icon_bottom_y >= my && icon_bottom_y < my + mh
        {
            // Clamp popover within this monitor, with a small margin.
            let margin = 4;
            if x < mx + margin {
                x = mx + margin;
            }
            if x + pop_w > mx + mw - margin {
                x = mx + mw - margin - pop_w;
            }
            if y < my + margin {
                y = my + margin;
            }
            if y + pop_h > my + mh - margin {
                y = my + mh - margin - pop_h;
            }
            eprintln!("ai-dock: clamped to monitor ({mx},{my}); final pos ({x},{y})");
            break;
        }
    }

    if let Err(e) = window.set_position(Position::Physical(PhysicalPosition::new(x, y))) {
        eprintln!("ai-dock: set_position failed: {e}");
    }
}

/// macOS: set the NSWindow's collection behavior so it moves to the active
/// Space when shown, rather than remembering the Space it was last on.
#[cfg(target_os = "macos")]
fn move_window_to_active_space(window: &tauri::WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    unsafe {
        let ns_window: *mut std::ffi::c_void = match window.ns_window() {
            Ok(p) => p,
            Err(_) => return,
        };
        let ns_window = ns_window as *mut AnyObject;
        if !ns_window.is_null() {
            // NSWindowCollectionBehaviorMoveToActiveSpace = 1 << 1 = 2
            // NSWindowCollectionBehaviorManaged = 1 << 2 = 4
            let behavior: u64 = 2 | 4;
            let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
        }
    }
}
