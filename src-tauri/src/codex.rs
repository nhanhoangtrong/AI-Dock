//! Codex signal source: latest `codex.rate_limits` event from the local logs DB.
//!
//! Path: `~/.codex/logs_2.sqlite` (resolved via `dirs::home_dir()`).
//! Read-only. Open, query, close — never hold the connection across polls.
//!
//! The JSON event is embedded in `feedback_log_body`, prefixed by tracing span
//! text. We locate the `{"type":"codex.rate_limits"` substring and walk braces
//! forward to the matching `}` before calling `serde_json`.

use std::path::PathBuf;

use rusqlite::OpenFlags;
use serde::{Deserialize, Serialize};

/// Path to the Codex logs DB. None if home dir can't be resolved.
pub fn db_path() -> Option<PathBuf> {
    let mut p = dirs::home_dir()?;
    p.push(".codex");
    p.push("logs_2.sqlite");
    Some(p)
}

/// A single rolling window in the Codex rate-limit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexWindow {
    pub used_percent: u32,
    pub window_minutes: u32,
    pub reset_after_seconds: u64,
    pub reset_at: i64,
}

/// Top-level shape of the `codex.rate_limits` event we consume.
/// `primary` and `secondary` live nested under `rate_limits` in the real
/// event; `type`/`credits`/`promo`/etc. are ignored by serde by default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexRateLimitEvent {
    pub plan_type: String,
    pub rate_limits: RateLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimits {
    pub primary: CodexWindow,
    pub secondary: CodexWindow,
}

/// What the frontend receives for the Codex row.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CodexStatus {
    Ok {
        plan_type: String,
        primary: CodexWindow,
        secondary: CodexWindow,
        /// Unix epoch seconds of the `logs.ts` row we read.
        event_ts: i64,
    },
    #[allow(dead_code)] // reserved for §4 stale-with-last-known payload
    Stale {
        plan_type: String,
        primary: CodexWindow,
        secondary: CodexWindow,
        event_ts: i64,
        message: String,
    },
    Error {
        message: String,
    },
}

/// Walk balanced braces from `start` (the index of `{`) to the matching `}`.
/// Returns the index *after* the closing brace, or `None` if unbalanced.
fn find_matching_brace(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    let mut i = start;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            if escape {
                escape = false;
            } else if c == b'\\' {
                escape = true;
            } else if c == b'"' {
                in_string = false;
            }
        } else {
            match c {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// Extract the first balanced JSON object starting with the marker substring.
fn extract_json_object<'a>(body: &'a str, marker: &str) -> Option<&'a str> {
    let start = body.find(marker)?;
    // Marker is `{"type":"codex.rate_limits"` — the `{` is at `start`.
    let open = body[start..].find('{').map(|rel| start + rel)?;
    let end = find_matching_brace(body, open)?;
    Some(&body[open..end])
}

const MARKER: &str = "{\"type\":\"codex.rate_limits\"";

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// If the window's reset time has passed, the window has rolled — the stale
/// `used_percent` from the last Codex turn no longer applies. Zero it out so
/// we never show a limit that's already reset.
fn normalize_window(mut w: CodexWindow, now: i64) -> CodexWindow {
    if w.reset_at <= now {
        w.used_percent = 0;
        w.reset_after_seconds = 0;
    }
    w
}

/// Read the latest `codex.rate_limits` event.
///
/// Returns a fully-typed `CodexStatus` mapping every failure mode from §4.1
/// of the spec to its display message.
pub fn read_latest() -> CodexStatus {
    let Some(path) = db_path() else {
        return CodexStatus::Error {
            message: "Codex: could not resolve home dir".to_string(),
        };
    };
    if !path.exists() {
        return CodexStatus::Error {
            message: "Codex: no logs found — run `codex` once.".to_string(),
        };
    }

    let conn = match rusqlite::Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            // Locked is the common transient case; treat anything else as
            // "no logs" since we can't tell the user more specifically
            // without more error taxonomy work.
            let msg = e.to_string().to_lowercase();
            if msg.contains("locked") {
                return CodexStatus::Error {
                    message: "Codex: updating…".to_string(),
                };
            }
            return CodexStatus::Error {
                message: "Codex: no logs found — run `codex` once.".to_string(),
            };
        }
    };

    let row: Result<(String, i64), rusqlite::Error> = conn.query_row(
        "SELECT feedback_log_body, ts \
         FROM logs \
         WHERE feedback_log_body LIKE '%codex.rate_limits%' \
         ORDER BY ts DESC, ts_nanos DESC, id DESC \
         LIMIT 1",
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    );

    let (body, event_ts) = match row {
        Ok(r) => r,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return CodexStatus::Error {
                message: "Codex: no rate-limit data yet.".to_string(),
            };
        }
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            if msg.contains("locked") || msg.contains("busy") {
                return CodexStatus::Error {
                    message: "Codex: updating…".to_string(),
                };
            }
            return CodexStatus::Error {
                message: format!("Codex: log read failed: {e}"),
            };
        }
    };

    let Some(json) = extract_json_object(&body, MARKER) else {
        return CodexStatus::Error {
            message: "Codex: log format changed — check for app update.".to_string(),
        };
    };

    match serde_json::from_str::<CodexRateLimitEvent>(json) {
        Ok(parsed) => {
            let now = now_unix_secs();
            CodexStatus::Ok {
                plan_type: parsed.plan_type,
                primary: normalize_window(parsed.rate_limits.primary, now),
                secondary: normalize_window(parsed.rate_limits.secondary, now),
                event_ts,
            }
        }
        Err(err) => CodexStatus::Error {
            message: format!("Codex: error parsing rate-limit data: {err}"),
        },
    }
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
