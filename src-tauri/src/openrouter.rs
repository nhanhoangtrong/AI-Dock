//! OpenRouter signal source: `GET /api/v1/credits`.
//!
//! Auth: `Authorization: Bearer <management_key>`. Standard chat `sk-or-...`
//! keys may be rejected (401/403). We classify errors per §4.2 so the popover
//! can show the right caption.
//!
//! Derived value: `remaining = total_credits - total_usage` (the API does not
//! return a "remaining" field).

use serde::{Deserialize, Serialize};

const ENDPOINT: &str = "https://openrouter.ai/api/v1/credits";
const TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterCreditsRaw {
    pub total_credits: f64,
    pub total_usage: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiResponse {
    data: OpenRouterCreditsRaw,
}

/// What the frontend receives for the OpenRouter row.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum OpenRouterStatus {
    Ok {
        total_credits: f64,
        total_usage: f64,
        remaining: f64,
    },
    #[allow(dead_code)] // reserved for §4 stale-with-last-known payload
    Stale {
        total_credits: f64,
        total_usage: f64,
        remaining: f64,
        message: String,
    },
    Error {
        message: String,
    },
}

/// Fetch OpenRouter credits. The key is the `openrouter_key` from config.
///
/// Returns `Error` with a UI-ready message on any failure (auth, network,
/// timeout, malformed payload, missing key). The error mapping is the
/// authoritative source for §4.2 captions — keep them stable.
pub async fn fetch(key: Option<&str>) -> OpenRouterStatus {
    let Some(key) = key else {
        return OpenRouterStatus::Error {
            message: "OpenRouter: no key — add one in settings.".to_string(),
        };
    };
    if key.trim().is_empty() {
        return OpenRouterStatus::Error {
            message: "OpenRouter: no key — add one in settings.".to_string(),
        };
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .user_agent(concat!("ai-dock/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return OpenRouterStatus::Error {
                message: format!("OpenRouter: couldn't fetch ({e})."),
            };
        }
    };

    let resp = match client
        .get(ENDPOINT)
        .bearer_auth(key)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return classify_transport(&e);
        }
    };

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return OpenRouterStatus::Error {
            message: "OpenRouter: key rejected — is it a management key?".to_string(),
        };
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return OpenRouterStatus::Error {
            message: "OpenRouter: rate-limited, retrying.".to_string(),
        };
    }
    if status.is_server_error() {
        return OpenRouterStatus::Error {
            message: "OpenRouter: couldn't fetch.".to_string(),
        };
    }
    if !status.is_success() {
        return OpenRouterStatus::Error {
            message: format!("OpenRouter: unexpected status {status}."),
        };
    }

    let body = match resp.json::<ApiResponse>().await {
        Ok(b) => b,
        Err(_) => {
            return OpenRouterStatus::Error {
                message: "OpenRouter: unexpected response.".to_string(),
            };
        }
    };

    let total_credits = body.data.total_credits;
    let total_usage = body.data.total_usage;
    let remaining = (total_credits - total_usage).max(0.0);

    OpenRouterStatus::Ok {
        total_credits,
        total_usage,
        remaining,
    }
}

fn classify_transport(e: &reqwest::Error) -> OpenRouterStatus {
    if e.is_timeout() {
        OpenRouterStatus::Error {
            message: "OpenRouter: couldn't fetch.".to_string(),
        }
    } else if e.is_connect() || e.is_request() {
        OpenRouterStatus::Error {
            message: "OpenRouter: couldn't fetch.".to_string(),
        }
    } else {
        OpenRouterStatus::Error {
            message: "OpenRouter: couldn't fetch.".to_string(),
        }
    }
}
