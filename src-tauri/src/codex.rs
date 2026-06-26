//! Codex subscription usage source.
//!
//! This uses the same ChatGPT OAuth credentials as the Codex CLI and calls the
//! reverse-engineered ChatGPT wham usage endpoint. Keep errors isolated to this
//! provider: the API, token refresh shape, and credential storage may change.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

const USAGE_ENDPOINT: &str = "https://chatgpt.com/backend-api/wham/usage";
const TOKEN_ENDPOINT: &str = "https://auth.openai.com/oauth/token";
const CLIENT_ID: &str = "6160ae70-bcfd-4ca8-a99b-40f73b3b072e";
const TIMEOUT_SECS: u64 = 10;
const KEYCHAIN_ACCOUNT: &str = "Codex";
const KEYCHAIN_SERVICES: &[&str] = &["Codex", "Codex CLI", "codex"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexWindow {
    pub used_percent: u32,
    pub window_minutes: u32,
    pub reset_after_seconds: u64,
    pub reset_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CodexStatus {
    Ok {
        plan_type: String,
        primary: CodexWindow,
        secondary: CodexWindow,
        event_ts: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        reset_credits_available: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        extra_usage: Option<String>,
    },
    #[allow(dead_code)]
    Stale {
        plan_type: String,
        primary: CodexWindow,
        secondary: CodexWindow,
        event_ts: i64,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        reset_credits_available: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        extra_usage: Option<String>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone)]
enum CredentialSource {
    File { path: PathBuf },
    Keychain { service: String },
}

#[derive(Debug, Clone)]
struct LoadedCredentials {
    source: CredentialSource,
    file: CodexAuthFile,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CodexAuthFile {
    auth_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tokens: Option<CodexTokens>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_refresh: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct CodexTokens {
    access_token: String,
    refresh_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WhamUsageResponse {
    plan_type: String,
    rate_limit: WhamRateLimit,
    #[serde(default)]
    credits: Option<WhamCredits>,
    #[serde(default)]
    rate_limit_reset_credits: Option<WhamResetCredits>,
}

#[derive(Debug, Deserialize)]
struct WhamRateLimit {
    primary_window: WhamUsageWindow,
    secondary_window: WhamUsageWindow,
}

#[derive(Debug, Deserialize)]
struct WhamUsageWindow {
    used_percent: u32,
    limit_window_seconds: u64,
    reset_after_seconds: u64,
    reset_at: i64,
}

#[derive(Debug, Deserialize)]
struct WhamCredits {
    #[serde(default)]
    balance: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WhamResetCredits {
    available_count: u32,
}

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

pub async fn fetch() -> CodexStatus {
    let mut loaded = match load_credentials() {
        Ok(credentials) => credentials,
        Err(message) => {
            return CodexStatus::Error { message };
        }
    };

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECS))
        .user_agent(concat!("ai-dock/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            return CodexStatus::Error {
                message: format!("Codex: couldn't fetch ({e})."),
            };
        }
    };

    let access_token = loaded
        .file
        .tokens
        .as_ref()
        .map(|tokens| tokens.access_token.as_str())
        .unwrap_or("");
    match request_usage(&client, access_token).await {
        Ok(status) => status,
        Err(CodexFetchError::Auth) => {
            if let Err(message) = refresh_credentials(&client, &mut loaded).await {
                return CodexStatus::Error { message };
            }
            let access_token = loaded
                .file
                .tokens
                .as_ref()
                .map(|tokens| tokens.access_token.as_str())
                .unwrap_or("");
            match request_usage(&client, access_token).await {
                Ok(status) => status,
                Err(CodexFetchError::Auth) => CodexStatus::Error {
                    message: "Codex: sign in again.".to_string(),
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
) -> Result<CodexStatus, CodexFetchError> {
    let resp = client
        .get(USAGE_ENDPOINT)
        .bearer_auth(access_token)
        .header("accept", "application/json")
        .send()
        .await
        .map_err(CodexFetchError::Transport)?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(CodexFetchError::Auth);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(CodexFetchError::Message(
            "Codex: rate-limited, retrying.".to_string(),
        ));
    }
    if status.is_server_error() {
        return Err(CodexFetchError::Message(
            "Codex: couldn't fetch.".to_string(),
        ));
    }
    if !status.is_success() {
        return Err(CodexFetchError::Message(format!(
            "Codex: unexpected status {status}."
        )));
    }

    let text = resp
        .text()
        .await
        .map_err(|_| CodexFetchError::Message("Codex: couldn't fetch.".to_string()))?;
    parse_usage_response(&text).map_err(|_| {
        CodexFetchError::Message("Codex: usage format changed — check for app update.".to_string())
    })
}

enum CodexFetchError {
    Auth,
    Transport(reqwest::Error),
    Message(String),
}

impl CodexFetchError {
    fn into_status(self) -> CodexStatus {
        match self {
            CodexFetchError::Auth => CodexStatus::Error {
                message: "Codex: sign in again.".to_string(),
            },
            CodexFetchError::Transport(e) => {
                let message = if e.is_timeout() || e.is_connect() || e.is_request() {
                    "Codex: couldn't fetch.".to_string()
                } else {
                    format!("Codex: couldn't fetch ({e}).")
                };
                CodexStatus::Error { message }
            }
            CodexFetchError::Message(message) => CodexStatus::Error { message },
        }
    }
}

fn parse_usage_response(body: &str) -> Result<CodexStatus, String> {
    let raw: WhamUsageResponse =
        serde_json::from_str(body).map_err(|e| format!("parse usage response: {e}"))?;
    let extra_usage = raw.credits.and_then(extra_usage_label);
    Ok(CodexStatus::Ok {
        plan_type: raw.plan_type,
        primary: raw.rate_limit.primary_window.into_window(),
        secondary: raw.rate_limit.secondary_window.into_window(),
        event_ts: now_unix_secs(),
        reset_credits_available: raw
            .rate_limit_reset_credits
            .map(|credits| credits.available_count),
        extra_usage,
    })
}

impl WhamUsageWindow {
    fn into_window(self) -> CodexWindow {
        let used_percent = if self.used_percent <= 1
            && self.reset_after_seconds.saturating_add(60) >= self.limit_window_seconds
        {
            0
        } else {
            self.used_percent.min(100)
        };
        CodexWindow {
            used_percent,
            window_minutes: (self.limit_window_seconds / 60) as u32,
            reset_after_seconds: self.reset_after_seconds,
            reset_at: self.reset_at,
        }
    }
}

fn extra_usage_label(credits: WhamCredits) -> Option<String> {
    let balance = credits.balance?;
    let trimmed = balance.trim();
    if trimmed.is_empty() || trimmed == "0" || trimmed == "0.0" || trimmed == "0.00" {
        None
    } else {
        Some(format!("${trimmed}"))
    }
}

fn load_credentials() -> Result<LoadedCredentials, String> {
    if let Some(path) = auth_file_path() {
        if let Ok(json) = std::fs::read_to_string(&path) {
            let file = parse_auth_file(&json)?;
            return Ok(LoadedCredentials {
                source: CredentialSource::File { path },
                file,
            });
        }
    }

    for service in KEYCHAIN_SERVICES {
        if let Ok(json) = read_keychain_password(service) {
            if let Ok(file) = parse_auth_file(&json) {
                return Ok(LoadedCredentials {
                    source: CredentialSource::Keychain {
                        service: (*service).to_string(),
                    },
                    file,
                });
            }
        }
    }

    Err("Codex: not logged in — run `codex` once.".to_string())
}

fn auth_file_path() -> Option<PathBuf> {
    let mut path = if let Ok(home) = std::env::var("CODEX_HOME") {
        if home.trim().is_empty() {
            default_codex_home()?
        } else {
            PathBuf::from(home)
        }
    } else {
        default_codex_home()?
    };
    path.push("auth.json");
    Some(path)
}

fn default_codex_home() -> Option<PathBuf> {
    let mut path = dirs::home_dir()?;
    path.push(".codex");
    Some(path)
}

fn parse_auth_file(json: &str) -> Result<CodexAuthFile, String> {
    let file: CodexAuthFile =
        serde_json::from_str(json).map_err(|_| "Codex: could not read credentials.".to_string())?;
    if file.auth_mode != "chatgpt" {
        return Err("Codex: ChatGPT login required.".to_string());
    }
    let Some(tokens) = &file.tokens else {
        return Err("Codex: not logged in — run `codex` once.".to_string());
    };
    if tokens.access_token.trim().is_empty() || tokens.refresh_token.trim().is_empty() {
        return Err("Codex: not logged in — run `codex` once.".to_string());
    }
    Ok(file)
}

async fn refresh_credentials(
    client: &reqwest::Client,
    loaded: &mut LoadedCredentials,
) -> Result<(), String> {
    let refresh_token = loaded
        .file
        .tokens
        .as_ref()
        .map(|tokens| tokens.refresh_token.clone())
        .ok_or_else(|| "Codex: sign in again.".to_string())?;

    let resp = client
        .post(TOKEN_ENDPOINT)
        .header("content-type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
            ("client_id", CLIENT_ID),
        ])
        .send()
        .await
        .map_err(|_| "Codex: sign in again.".to_string())?;

    if !resp.status().is_success() {
        return Err("Codex: sign in again.".to_string());
    }

    let body = resp
        .json::<RefreshResponse>()
        .await
        .map_err(|_| "Codex: sign in again.".to_string())?;

    let Some(tokens) = &mut loaded.file.tokens else {
        return Err("Codex: sign in again.".to_string());
    };
    tokens.access_token = body.access_token;
    if let Some(refresh_token) = body.refresh_token {
        tokens.refresh_token = refresh_token;
    }
    if let Some(id_token) = body.id_token {
        tokens.id_token = Some(id_token);
    }

    persist_credentials(loaded)
}

fn persist_credentials(loaded: &LoadedCredentials) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&loaded.file)
        .map_err(|_| "Codex: could not read credentials.".to_string())?;
    match &loaded.source {
        CredentialSource::File { path } => {
            std::fs::write(path, json).map_err(|_| "Codex: could not read credentials.".to_string())
        }
        CredentialSource::Keychain { service } => write_keychain_password(service, &json),
    }
}

fn read_keychain_password(service: &str) -> Result<String, String> {
    let output = Command::new("security")
        .args(["find-generic-password", "-s", service, "-w"])
        .output()
        .map_err(|_| "Codex: could not read credentials.".to_string())?;
    if !output.status.success() {
        return Err("Codex: could not read credentials.".to_string());
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim_end_matches('\n').to_string())
        .map_err(|_| "Codex: could not read credentials.".to_string())
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
        .map_err(|_| "Codex: could not read credentials.".to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Codex: could not read credentials.".to_string())
    }
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
