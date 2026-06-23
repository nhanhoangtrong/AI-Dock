//! Claude Code subscription usage source.
//!
//! This uses a reverse-engineered, undocumented Claude Code OAuth endpoint.
//! Keep errors isolated to this provider: the API, beta header, credential
//! shape, and Keychain service names may change without notice.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const USAGE_ENDPOINT: &str = "https://api.anthropic.com/api/oauth/usage";
const TOKEN_ENDPOINT: &str = "https://platform.claude.com/v1/oauth/token";
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const OAUTH_SCOPE: &str =
    "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";
const BETA_HEADER: &str = "oauth-2025-04-20";
const DEFAULT_KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const KEYCHAIN_ACCOUNT: &str = "Claude Code";
const TIMEOUT_SECS: u64 = 10;
const REFRESH_SKEW_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Serialize)]
pub struct ClaudeWindow {
    pub used_percent: u32,
    pub window_minutes: u32,
    pub reset_after_seconds: u64,
    pub reset_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ClaudeStatus {
    Ok {
        five_hour: ClaudeWindow,
        seven_day: ClaudeWindow,
        #[serde(skip_serializing_if = "Option::is_none")]
        seven_day_opus: Option<ClaudeWindow>,
        #[serde(skip_serializing_if = "Option::is_none")]
        seven_day_omelette: Option<ClaudeWindow>,
    },
    #[allow(dead_code)]
    Stale {
        five_hour: ClaudeWindow,
        seven_day: ClaudeWindow,
        #[serde(skip_serializing_if = "Option::is_none")]
        seven_day_opus: Option<ClaudeWindow>,
        #[serde(skip_serializing_if = "Option::is_none")]
        seven_day_omelette: Option<ClaudeWindow>,
        message: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ClaudeCredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    oauth: ClaudeOauth,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeOauth {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    subscription_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rate_limit_tier: Option<String>,
}

#[derive(Debug, Clone)]
enum CredentialSource {
    Keychain { service: String },
    File { path: PathBuf },
}

#[derive(Debug, Clone)]
struct LoadedCredentials {
    source: CredentialSource,
    file: ClaudeCredentialsFile,
}

#[derive(Debug, Deserialize)]
struct UsageResponse {
    five_hour: UsageWindow,
    seven_day: UsageWindow,
    seven_day_opus: Option<UsageWindow>,
    seven_day_omelette: Option<UsageWindow>,
}

#[derive(Debug, Deserialize)]
struct UsageWindow {
    utilization: u32,
    resets_at: String,
}

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

/// Fetch real Claude Code subscription utilization from the unofficial OAuth
/// usage endpoint documented in ADR-0004.
pub async fn fetch() -> ClaudeStatus {
    let mut loaded = match load_credentials() {
        Ok(credentials) => credentials,
        Err(message) => {
            return ClaudeStatus::Error { message };
        }
    };

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .user_agent(concat!("ai-dock/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            return ClaudeStatus::Error {
                message: format!("Claude Code: couldn't fetch ({e})."),
            };
        }
    };

    if should_refresh(loaded.file.oauth.expires_at) {
        if let Err(message) = refresh_credentials(&client, &mut loaded).await {
            return ClaudeStatus::Error { message };
        }
    }

    match request_usage(&client, &loaded.file.oauth.access_token).await {
        Ok(status) => status,
        Err(UsageFetchError::Auth) => {
            if let Err(message) = refresh_credentials(&client, &mut loaded).await {
                return ClaudeStatus::Error { message };
            }
            match request_usage(&client, &loaded.file.oauth.access_token).await {
                Ok(status) => status,
                Err(UsageFetchError::Auth) => ClaudeStatus::Error {
                    message: "Claude Code: sign in again.".to_string(),
                },
                Err(err) => err.into_status(),
            }
        }
        Err(err) => err.into_status(),
    }
}

async fn request_usage(
    client: &reqwest::Client,
    access_token: &str,
) -> Result<ClaudeStatus, UsageFetchError> {
    let resp = client
        .get(USAGE_ENDPOINT)
        .bearer_auth(access_token)
        .header("accept", "application/json")
        .header("content-type", "application/json")
        .header("anthropic-beta", BETA_HEADER)
        .send()
        .await
        .map_err(UsageFetchError::Transport)?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(UsageFetchError::Auth);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(UsageFetchError::Message(
            "Claude Code: rate-limited, retrying.".to_string(),
        ));
    }
    if status.is_server_error() {
        return Err(UsageFetchError::Message(
            "Claude Code: couldn't fetch.".to_string(),
        ));
    }
    if !status.is_success() {
        return Err(UsageFetchError::Message(format!(
            "Claude Code: unexpected status {status}."
        )));
    }

    let text = resp
        .text()
        .await
        .map_err(|_| UsageFetchError::Message("Claude Code: couldn't fetch.".to_string()))?;
    parse_usage_response(&text).map_err(|_| {
        UsageFetchError::Message(
            "Claude Code: usage format changed — check for app update.".to_string(),
        )
    })
}

enum UsageFetchError {
    Auth,
    Transport(reqwest::Error),
    Message(String),
}

impl UsageFetchError {
    fn into_status(self) -> ClaudeStatus {
        match self {
            UsageFetchError::Auth => ClaudeStatus::Error {
                message: "Claude Code: sign in again.".to_string(),
            },
            UsageFetchError::Transport(e) => {
                let message = if e.is_timeout() || e.is_connect() || e.is_request() {
                    "Claude Code: couldn't fetch.".to_string()
                } else {
                    format!("Claude Code: couldn't fetch ({e}).")
                };
                ClaudeStatus::Error { message }
            }
            UsageFetchError::Message(message) => ClaudeStatus::Error { message },
        }
    }
}

fn parse_usage_response(body: &str) -> Result<ClaudeStatus, String> {
    let raw: UsageResponse =
        serde_json::from_str(body).map_err(|e| format!("parse usage response: {e}"))?;
    Ok(ClaudeStatus::Ok {
        five_hour: raw.five_hour.into_window(5 * 60)?,
        seven_day: raw.seven_day.into_window(7 * 24 * 60)?,
        seven_day_opus: raw
            .seven_day_opus
            .map(|w| w.into_window(7 * 24 * 60))
            .transpose()?,
        seven_day_omelette: raw
            .seven_day_omelette
            .map(|w| w.into_window(7 * 24 * 60))
            .transpose()?,
    })
}

impl UsageWindow {
    fn into_window(self, window_minutes: u32) -> Result<ClaudeWindow, String> {
        let reset_at = parse_iso8601_z(&self.resets_at)?;
        let now = now_unix_secs();
        Ok(ClaudeWindow {
            used_percent: self.utilization.min(100),
            window_minutes,
            reset_after_seconds: reset_at.saturating_sub(now) as u64,
            reset_at,
        })
    }
}

fn load_credentials() -> Result<LoadedCredentials, String> {
    let mut services = Vec::new();
    if let Ok(config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        if !config_dir.trim().is_empty() {
            services.push(config_specific_service_name(&config_dir));
        }
    }
    services.push(DEFAULT_KEYCHAIN_SERVICE.to_string());

    for service in services {
        match read_keychain_password(&service) {
            Ok(json) => {
                let file = parse_credentials(&json)?;
                return Ok(LoadedCredentials {
                    source: CredentialSource::Keychain { service },
                    file,
                });
            }
            Err(_) => continue,
        }
    }

    let path = credentials_file_path()
        .ok_or_else(|| "Claude Code: no credentials found — run Claude Code once.".to_string())?;
    let bytes = std::fs::read_to_string(&path)
        .map_err(|_| "Claude Code: no credentials found — run Claude Code once.".to_string())?;
    let file = parse_credentials(&bytes)?;
    Ok(LoadedCredentials {
        source: CredentialSource::File { path },
        file,
    })
}

fn parse_credentials(json: &str) -> Result<ClaudeCredentialsFile, String> {
    serde_json::from_str(json).map_err(|_| "Claude Code: could not read credentials.".to_string())
}

async fn refresh_credentials(
    client: &reqwest::Client,
    loaded: &mut LoadedCredentials,
) -> Result<(), String> {
    let resp = client
        .post(TOKEN_ENDPOINT)
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": loaded.file.oauth.refresh_token,
            "client_id": CLIENT_ID,
            "scope": OAUTH_SCOPE,
        }))
        .send()
        .await
        .map_err(|_| "Claude Code: sign in again.".to_string())?;

    if !resp.status().is_success() {
        return Err("Claude Code: sign in again.".to_string());
    }

    let body = resp
        .json::<RefreshResponse>()
        .await
        .map_err(|_| "Claude Code: sign in again.".to_string())?;

    loaded.file.oauth.access_token = body.access_token;
    if let Some(refresh_token) = body.refresh_token {
        loaded.file.oauth.refresh_token = refresh_token;
    }
    if let Some(expires_in) = body.expires_in {
        loaded.file.oauth.expires_at = now_unix_millis() + expires_in * 1000;
    }

    persist_credentials(loaded)
}

fn persist_credentials(loaded: &LoadedCredentials) -> Result<(), String> {
    let json = serde_json::to_string(&loaded.file)
        .map_err(|_| "Claude Code: could not read credentials.".to_string())?;
    match &loaded.source {
        CredentialSource::Keychain { service } => write_keychain_password(service, &json),
        CredentialSource::File { path } => std::fs::write(path, json)
            .map_err(|_| "Claude Code: could not read credentials.".to_string()),
    }
}

fn credentials_file_path() -> Option<PathBuf> {
    let mut path = dirs::home_dir()?;
    path.push(".claude");
    path.push(".credentials.json");
    Some(path)
}

fn read_keychain_password(service: &str) -> Result<String, String> {
    let output = Command::new("security")
        .args(["find-generic-password", "-s", service, "-w"])
        .output()
        .map_err(|_| "Claude Code: could not read credentials.".to_string())?;
    if !output.status.success() {
        return Err("Claude Code: could not read credentials.".to_string());
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim_end_matches('\n').to_string())
        .map_err(|_| "Claude Code: could not read credentials.".to_string())
}

fn write_keychain_password(service: &str, value: &str) -> Result<(), String> {
    let status = Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-s",
            service,
            "-a",
            KEYCHAIN_ACCOUNT,
            "-w",
            value,
        ])
        .status()
        .map_err(|_| "Claude Code: could not read credentials.".to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Claude Code: could not read credentials.".to_string())
    }
}

fn should_refresh(expires_at_ms: i64) -> bool {
    expires_at_ms <= now_unix_millis() + REFRESH_SKEW_MS
}

fn config_specific_service_name(config_dir: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(config_dir.as_bytes());
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{DEFAULT_KEYCHAIN_SERVICE}-{}", &hex[..8])
}

fn now_unix_secs() -> i64 {
    now_unix_millis() / 1000
}

fn now_unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn parse_iso8601_z(input: &str) -> Result<i64, String> {
    if input.len() != 20 || !input.ends_with('Z') {
        return Err("invalid timestamp".to_string());
    }
    let year = parse_i32(&input[0..4])?;
    let month = parse_i32(&input[5..7])?;
    let day = parse_i32(&input[8..10])?;
    let hour = parse_i32(&input[11..13])?;
    let minute = parse_i32(&input[14..16])?;
    let second = parse_i32(&input[17..19])?;
    if &input[4..5] != "-"
        || &input[7..8] != "-"
        || &input[10..11] != "T"
        || &input[13..14] != ":"
        || &input[16..17] != ":"
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return Err("invalid timestamp".to_string());
    }

    let days = days_from_civil(year, month, day);
    Ok(days * 86_400 + (hour as i64) * 3_600 + (minute as i64) * 60 + second as i64)
}

fn parse_i32(s: &str) -> Result<i32, String> {
    s.parse::<i32>()
        .map_err(|_| "invalid timestamp".to_string())
}

fn days_from_civil(year: i32, month: i32, day: i32) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usage_windows_from_oauth_usage_response() {
        let body = r#"{
          "five_hour": {
            "utilization": 25,
            "resets_at": "2026-01-28T15:00:00Z"
          },
          "seven_day": {
            "utilization": 40,
            "resets_at": "2026-02-01T00:00:00Z"
          },
          "seven_day_opus": {
            "utilization": 7,
            "resets_at": "2026-02-01T00:00:00Z"
          }
        }"#;

        let status = parse_usage_response(body).expect("usage should parse");

        match status {
            ClaudeStatus::Ok {
                five_hour,
                seven_day,
                seven_day_opus,
                ..
            } => {
                assert_eq!(five_hour.used_percent, 25);
                assert_eq!(five_hour.reset_at, 1_769_612_400);
                assert_eq!(seven_day.used_percent, 40);
                assert_eq!(seven_day.reset_at, 1_769_904_000);
                assert_eq!(seven_day_opus.unwrap().used_percent, 7);
            }
            ClaudeStatus::Error { message } => panic!("unexpected error: {message}"),
            ClaudeStatus::Stale { .. } => panic!("unexpected stale status"),
        }
    }

    #[test]
    fn config_specific_keychain_service_uses_sha256_prefix() {
        let service = config_specific_service_name("/tmp/claude-alt");

        assert_eq!(service, "Claude Code-credentials-04923786");
    }
}
