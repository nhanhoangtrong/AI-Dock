//! Tests for `openrouter` — kept separate from the production source file via
//! `#[path = "openrouter_tests.rs"]` on a `mod tests` declaration in openrouter.rs.

use super::*;

#[test]
fn parses_real_openrouter_response() {
    // Shape confirmed against the OpenRouter docs.
    let body = r#"{"data":{"total_credits":100.5,"total_usage":25.75}}"#;
    let parsed: ApiResponse = serde_json::from_str(body).unwrap();
    assert_eq!(parsed.data.total_credits, 100.5);
    assert_eq!(parsed.data.total_usage, 25.75);
    // `remaining` is derived in `fetch`, not in the response.
    let remaining = parsed.data.total_credits - parsed.data.total_usage;
    assert!((remaining - 74.75).abs() < 1e-9);
}

#[test]
fn rejects_unexpected_response_shape() {
    // Missing `data` wrapper — should fail.
    let body = r#"{"total_credits":100.5,"total_usage":25.75}"#;
    let parsed: Result<ApiResponse, _> = serde_json::from_str(body);
    assert!(parsed.is_err());
}