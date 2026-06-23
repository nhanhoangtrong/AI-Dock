//! Credential storage for ai-dock.
//!
//! Reads/writes `~/.config/ai-dock/config.json` (resolved via `dirs::config_dir()`).
//! Shape: `{ "openrouter_key": "sk-or-..." }`.
//!
//! Created lazily on first write; never created at launch if absent.
//! Hand-editable; re-read on each poll so external edits take effect.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openrouter_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deepseek_key: Option<String>,
}

/// Path to the config file. Does not guarantee the file exists.
pub fn config_path() -> Option<PathBuf> {
    let mut p = dirs::config_dir()?;
    p.push("ai-dock");
    p.push("config.json");
    Some(p)
}

/// Read the config from disk. Missing file or missing field → empty `Config`.
///
/// We never fail loud here: an absent config is a normal first-launch state
/// and must surface as a *persistent* error in the popover, not a panic.
pub fn read() -> Config {
    let Some(path) = config_path() else {
        return Config::default();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return Config::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Write the config to disk. Creates `~/.config/ai-dock/` if needed.
///
/// On success returns the path written.
pub fn write(cfg: &Config) -> Result<PathBuf, String> {
    let path = config_path().ok_or_else(|| "could not resolve config dir".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create config dir {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_vec_pretty(cfg)
        .map_err(|e| format!("serialize config: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write config {}: {e}", path.display()))?;
    Ok(path)
}
