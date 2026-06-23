//! Tests for `deepseek` — kept separate from the production source file via
//! `#[path = "deepseek_tests.rs"]` on a `mod tests` declaration in deepseek.rs.

use super::*;

#[test]
fn parses_real_deepseek_response() {
    // Shape from the user's docs.
    let body = r#"{
        "is_available": true,
        "balance_infos": [
            {
                "currency": "USD",
                "total_balance": "4.12",
                "granted_balance": "0.00",
                "topped_up_balance": "4.12"
            }
        ]
    }"#;
    let parsed: DeepSeekBalanceRaw = serde_json::from_str(body).unwrap();
    assert!(parsed.is_available);
    assert_eq!(parsed.balance_infos.len(), 1);
    let info = &parsed.balance_infos[0];
    assert_eq!(info.currency, "USD");
    assert_eq!(info.total_balance.as_deref(), Some("4.12"));
    // The string-to-f64 parse happens in `fetch`; verify the conversion.
    let total: f64 = info.total_balance.as_deref().unwrap().parse().unwrap();
    assert!((total - 4.12).abs() < 1e-9);
}

#[test]
fn parses_when_balance_field_missing() {
    // Some accounts return a balance_infos entry with no total_balance.
    let body = r#"{"is_available":true,"balance_infos":[{"currency":"USD"}]}"#;
    let parsed: DeepSeekBalanceRaw = serde_json::from_str(body).unwrap();
    let info = &parsed.balance_infos[0];
    assert!(info.total_balance.is_none());
    // Summing missing balances must contribute 0, not panic.
    let sum: f64 = parsed
        .balance_infos
        .iter()
        .filter_map(|i| {
            i.total_balance
                .as_deref()
                .and_then(|s| s.parse::<f64>().ok())
        })
        .sum();
    assert_eq!(sum, 0.0);
}

#[test]
fn parses_unavailable_account() {
    let body = r#"{"is_available":false,"balance_infos":[]}"#;
    let parsed: DeepSeekBalanceRaw = serde_json::from_str(body).unwrap();
    assert!(!parsed.is_available);
    assert!(parsed.balance_infos.is_empty());
}