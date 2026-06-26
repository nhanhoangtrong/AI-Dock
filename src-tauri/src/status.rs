//! Combined `Status` payload + poll loop.
//!
//! The frontend is a pure renderer; it never polls, never fetches, never reads
//! files. It subscribes to the `status-update` event emitted by the poll loop.
//!
//! Polling cadence: every 5 minutes (same interval for both sources).
//! Manual refresh (`refresh_now` command) triggers an immediate cycle on the
//! same code path; the 5-min timer is *not* reset (§5.2).

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

use crate::claude::{self, ClaudeStatus};
use crate::codex::{self, CodexStatus};
use crate::config;
use crate::deepseek::{self, DeepSeekStatus};
use crate::openrouter::{self, OpenRouterStatus};

pub const EVENT: &str = "status-update";
pub const POLL_INTERVAL_SECS: u64 = 300;

/// What the frontend receives on every poll cycle.
#[derive(Debug, Clone, Serialize)]
pub struct StatusUpdate {
    pub codex: CodexStatus,
    pub claude: ClaudeStatus,
    pub openrouter: OpenRouterStatus,
    pub deepseek: DeepSeekStatus,
    pub polled_at: i64, // unix epoch seconds
}

/// Run one poll cycle: fetch provider statuses and emit `status-update`.
pub async fn run_cycle(app: &AppHandle) -> StatusUpdate {
    // Re-read config so hand edits take effect. Codex and Claude Code use
    // their own OAuth credential sources.
    let cfg = config::read();
    let codex_fut = codex::fetch();
    let or_fut = openrouter::fetch(cfg.openrouter_key.as_deref());
    let ds_fut = deepseek::fetch(cfg.deepseek_key.as_deref());
    let claude_fut = claude::fetch();

    // Fire network fetches concurrently.
    let (codex_status, or_status, ds_status, claude_status) =
        tokio::join!(codex_fut, or_fut, ds_fut, claude_fut);

    // 3. Build + emit.
    let update = StatusUpdate {
        codex: codex_status,
        claude: claude_status,
        openrouter: or_status,
        deepseek: ds_status,
        polled_at: now_unix_secs(),
    };

    if let Err(e) = app.emit(EVENT, &update) {
        eprintln!("ai-dock: emit {EVENT} failed: {e}");
    }

    update
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Spawn the background poll loop.
///
/// `kick` is a `Notify` the frontend (or manual refresh) can signal to run an
/// immediate cycle without waiting for the next timer tick.
pub fn spawn_loop(app: AppHandle, kick: Arc<Notify>) {
    tauri::async_runtime::spawn(async move {
        // Initial poll so the popover shows real data the first time it opens.
        run_cycle(&app).await;

        let mut ticker = tokio::time::interval(Duration::from_secs(POLL_INTERVAL_SECS));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    run_cycle(&app).await;
                }
                _ = kick.notified() => {
                    run_cycle(&app).await;
                }
            }
        }
    });
}
