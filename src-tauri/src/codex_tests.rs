//! Tests for `codex` — kept separate from the production source file via
//! `#[path = "codex_tests.rs"]` on a `mod tests` declaration in codex.rs.
//!
//! Living in the same module (via `super`) means private helpers like
//! `find_matching_brace`, `extract_json_object`, `MARKER`, and
//! `normalize_window` stay accessible without becoming part of the public API.

use super::*;

#[test]
fn brace_walker_finds_simple_close() {
    let s = r#"{"a":1}"#;
    let open = s.find('{').unwrap();
    assert_eq!(find_matching_brace(s, open), Some(s.len()));
}

#[test]
fn brace_walker_handles_nesting() {
    let s = r#"{"a":{"b":{"c":1}},"d":2}"#;
    let open = s.find('{').unwrap();
    let end = find_matching_brace(s, open).unwrap();
    // The closing brace at index `end - 1` belongs to the outermost `{`.
    assert_eq!(&s[open..end], r#"{"a":{"b":{"c":1}},"d":2}"#);
}

#[test]
fn brace_walker_skips_braces_inside_strings() {
    let s = r#"{"k":"} { nested }","x":1}"#;
    let open = s.find('{').unwrap();
    let end = find_matching_brace(s, open).unwrap();
    assert_eq!(&s[open..end], r#"{"k":"} { nested }","x":1}"#);
}

#[test]
fn brace_walker_handles_escaped_quotes_in_strings() {
    // The string contains an escaped quote followed by `{` and `}` chars
    // — the brace walker must NOT count them.
    let s = r#"{"k":"a\"{b}c\"","x":1}"#;
    let open = s.find('{').unwrap();
    let end = find_matching_brace(s, open).unwrap();
    assert_eq!(&s[open..end], r#"{"k":"a\"{b}c\"","x":1}"#);
}

#[test]
fn brace_walker_returns_none_when_unbalanced() {
    let s = r#"{"a":1"#; // missing close
    let open = s.find('{').unwrap();
    assert_eq!(find_matching_brace(s, open), None);
}

#[test]
fn extract_pulls_out_the_marker_object() {
    // Real-world shape: tracing span prefix, then `websocket event: `,
    // then the JSON object.
    let body = r#"session_loop{...}: websocket event: {"type":"codex.rate_limits","plan_type":"plus","rate_limits":{"primary":{"used_percent":1,"window_minutes":300,"reset_after_seconds":18000,"reset_at":1782162131},"secondary":{"used_percent":28,"window_minutes":10080,"reset_after_seconds":233556,"reset_at":1782385000}},"credits":null}"#;
    let json = extract_json_object(body, MARKER).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(parsed["type"], "codex.rate_limits");
    assert_eq!(parsed["plan_type"], "plus");
    assert_eq!(parsed["rate_limits"]["primary"]["used_percent"], 1);
}

#[test]
fn extract_returns_none_when_marker_absent() {
    let body = "no rate limits here";
    assert!(extract_json_object(body, MARKER).is_none());
}

fn w(used: u32, reset_at: i64) -> CodexWindow {
    CodexWindow {
        used_percent: used,
        window_minutes: 300,
        reset_after_seconds: 100,
        reset_at,
    }
}

#[test]
fn normalize_zeros_window_when_reset_in_the_past() {
    let now = 1_000_000;
    let n = normalize_window(w(88, now - 1), now);
    assert_eq!(n.used_percent, 0);
    assert_eq!(n.reset_after_seconds, 0);
    // window_minutes and reset_at preserved.
    assert_eq!(n.window_minutes, 300);
    assert_eq!(n.reset_at, now - 1);
}

#[test]
fn normalize_zeros_window_when_reset_exactly_now() {
    // The rule is `<= now`, so the boundary resets too.
    let now = 1_000_000;
    let n = normalize_window(w(88, now), now);
    assert_eq!(n.used_percent, 0);
}

#[test]
fn normalize_leaves_window_alone_when_reset_in_future() {
    let now = 1_000_000;
    let n = normalize_window(w(73, now + 1800), now);
    assert_eq!(n.used_percent, 73);
    assert_eq!(n.reset_after_seconds, 100);
}

// Regression: this used to be a flat struct expecting top-level
// `primary`/`secondary`, which failed with `missing field primary` against
// the real nested `rate_limits` wrapper. The shape below mirrors what the
// CLI actually logs.

const REAL_EVENT: &str = r#"{
    "type": "codex.rate_limits",
    "plan_type": "plus",
    "rate_limits": {
        "allowed": true,
        "limit_reached": false,
        "primary": {"used_percent": 88, "window_minutes": 300, "reset_after_seconds": 1848, "reset_at": 1782162140},
        "secondary": {"used_percent": 41, "window_minutes": 10080, "reset_after_seconds": 233556, "reset_at": 1782385000}
    },
    "code_review_rate_limits": null,
    "additional_rate_limits": null,
    "credits": null,
    "promo": null
}"#;

#[test]
fn parses_real_rate_limit_event_shape() {
    let parsed: CodexRateLimitEvent =
        serde_json::from_str(REAL_EVENT).expect("real event shape must parse");
    assert_eq!(parsed.plan_type, "plus");
    assert_eq!(parsed.rate_limits.primary.used_percent, 88);
    assert_eq!(parsed.rate_limits.primary.reset_at, 1782162140);
    assert_eq!(parsed.rate_limits.secondary.used_percent, 41);
}

#[test]
fn normalizes_both_windows_in_real_event() {
    let parsed: CodexRateLimitEvent = serde_json::from_str(REAL_EVENT).unwrap();
    // `now` is far past both reset_at timestamps.
    let now = 1_900_000_000;
    let p = normalize_window(parsed.rate_limits.primary.clone(), now);
    let s = normalize_window(parsed.rate_limits.secondary.clone(), now);
    assert_eq!(p.used_percent, 0);
    assert_eq!(s.used_percent, 0);
}
