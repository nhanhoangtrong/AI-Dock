//! Tests for `config` — kept separate from the production source file via
//! `#[path = "config_tests.rs"]` on a `mod tests` declaration in config.rs.

use super::*;

#[test]
fn provider_visibility_defaults_to_showing_all_known_providers() {
    let cfg = Config::default();

    assert_eq!(cfg.provider_visibility("codex"), true);
    assert_eq!(cfg.provider_visibility("claude"), true);
    assert_eq!(cfg.provider_visibility("openrouter"), true);
    assert_eq!(cfg.provider_visibility("deepseek"), true);
}

#[test]
fn provider_visibility_uses_explicit_saved_override() {
    let mut cfg = Config::default();
    cfg.set_provider_visibility("openrouter", false);

    assert_eq!(cfg.provider_visibility("openrouter"), false);
    assert_eq!(cfg.provider_visibility("codex"), true);
}

#[test]
fn serializes_both_keys() {
    let cfg = Config {
        openrouter_key: Some("sk-or-test".to_string()),
        deepseek_key: Some("sk-test".to_string()),
        provider_visibility: BTreeMap::new(),
    };
    let json = serde_json::to_string(&cfg).unwrap();
    // Both keys present.
    assert!(json.contains("\"openrouter_key\":\"sk-or-test\""));
    assert!(json.contains("\"deepseek_key\":\"sk-test\""));
}

#[test]
fn deserializes_with_missing_keys() {
    // An external hand-edited file might omit fields — both should be
    // optional.
    let json = r#"{}"#;
    let cfg: Config = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.openrouter_key, None);
    assert_eq!(cfg.deepseek_key, None);
}

#[test]
fn round_trips_through_json() {
    let original = Config {
        openrouter_key: Some("sk-or-round".to_string()),
        deepseek_key: Some("sk-round".to_string()),
        provider_visibility: BTreeMap::new(),
    };
    let json = serde_json::to_string(&original).unwrap();
    let parsed: Config = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.openrouter_key, original.openrouter_key);
    assert_eq!(parsed.deepseek_key, original.deepseek_key);
}