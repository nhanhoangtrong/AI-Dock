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
    pub openrouter: OpenRouterStatus,
    pub deepseek: DeepSeekStatus,
    pub polled_at: i64, // unix epoch seconds
}

/// Run one poll cycle: read Codex logs, fetch OpenRouter, fetch DeepSeek, emit `status-update`.
pub async fn run_cycle(app: &AppHandle) -> StatusUpdate {
    // 1. Codex — synchronous SQLite read, off the async runtime to avoid
    //    blocking the executor for the whole duration.
    let codex_status = tokio::task::spawn_blocking(codex::read_latest)
        .await
        .unwrap_or_else(|e| CodexStatus::Error {
            message: format!("Codex: poll task panicked: {e}"),
        });

    // 2. OpenRouter + DeepSeek — re-read config so hand edits take effect.
    let cfg = config::read();
    let or_fut = openrouter::fetch(cfg.openrouter_key.as_deref());
    let ds_fut = deepseek::fetch(cfg.deepseek_key.as_deref());

    // Fire both fetches concurrently.
    let (or_status, ds_status) = tokio::join!(or_fut, ds_fut);

    // 3. Build + emit.
    let update = StatusUpdate {
        codex: codex_status,
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
