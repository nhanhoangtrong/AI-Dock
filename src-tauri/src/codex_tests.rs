use super::*;

#[test]
fn parses_wham_usage_windows_and_optional_counts() {
    let body = r#"{
        "plan_type": "plus",
        "rate_limit": {
            "allowed": true,
            "limit_reached": false,
            "primary_window": {
                "used_percent": 12,
                "limit_window_seconds": 18000,
                "reset_after_seconds": 17496,
                "reset_at": 1782520373
            },
            "secondary_window": {
                "used_percent": 24,
                "limit_window_seconds": 604800,
                "reset_after_seconds": 488519,
                "reset_at": 1782991396
            }
        },
        "credits": {
            "has_credits": false,
            "unlimited": false,
            "overage_limit_reached": false,
            "balance": "0",
            "approx_local_messages": [0, 0],
            "approx_cloud_messages": [0, 0]
        },
        "rate_limit_reset_credits": {
            "available_count": 2
        }
    }"#;

    let status = parse_usage_response(body).expect("usage response should parse");

    match status {
        CodexStatus::Ok {
            plan_type,
            primary,
            secondary,
            reset_credits_available,
            extra_usage,
            ..
        } => {
            assert_eq!(plan_type, "plus");
            assert_eq!(primary.used_percent, 12);
            assert_eq!(primary.window_minutes, 300);
            assert_eq!(primary.reset_after_seconds, 17496);
            assert_eq!(primary.reset_at, 1782520373);
            assert_eq!(secondary.used_percent, 24);
            assert_eq!(secondary.window_minutes, 10080);
            assert_eq!(reset_credits_available, Some(2));
            assert_eq!(extra_usage, None);
        }
        other => panic!("unexpected status: {other:?}"),
    }
}

#[test]
fn fresh_unused_session_window_normalizes_floor_to_unused() {
    let window = WhamUsageWindow {
        used_percent: 1,
        limit_window_seconds: 18_000,
        reset_after_seconds: 17_980,
        reset_at: 1_782_520_373,
    };

    let normalized = window.into_window();

    assert_eq!(normalized.used_percent, 0);
    assert_eq!(normalized.window_minutes, 300);
}

#[test]
fn parses_chatgpt_auth_file() {
    let json = r#"{
        "auth_mode": "chatgpt",
        "tokens": {
            "id_token": "id",
            "access_token": "access",
            "refresh_token": "refresh",
            "account_id": "acct"
        },
        "last_refresh": "2026-06-17T06:09:23.316958Z"
    }"#;

    let auth = parse_auth_file(json).expect("auth should parse");

    let tokens = auth.tokens.expect("tokens should be present");
    assert_eq!(tokens.access_token, "access");
    assert_eq!(tokens.refresh_token, "refresh");
}

#[test]
fn rejects_api_key_only_auth_file() {
    let json = r#"{
        "auth_mode": "apikey",
        "OPENAI_API_KEY": "sk-test"
    }"#;

    let err = parse_auth_file(json).expect_err("api-key auth must be rejected");

    assert_eq!(err, "Codex: ChatGPT login required.");
}
