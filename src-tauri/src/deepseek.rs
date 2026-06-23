//! DeepSeek signal source: `GET https://api.deepseek.com/user/balance`.
//!
//! Auth: `Authorization: Bearer <api_key>` (a regular DeepSeek API key).
//! Response shape: `{ "is_available": bool, "balance_infos": [...] }`.

use serde::{Deserialize, Serialize};

const ENDPOINT: &str = "https://api.deepseek.com/user/balance";
const TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone, Deserialize)]
pub struct DeepSeekBalanceRaw {
    pub is_available: bool,
    pub balance_infos: Vec<BalanceInfo>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct BalanceInfo {
    pub currency: String,
    pub total_balance: Option<String>, // e.g. "4.12" — might be missing if empty
    pub granted_balance: Option<String>,
    pub topped_up_balance: Option<String>,
}

/// What the frontend receives for the DeepSeek row.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum DeepSeekStatus {
    Ok {
        total_balance: f64,
        currency: String,
    },
    #[allow(dead_code)]
    Stale {
        total_balance: f64,
        currency: String,
        message: String,
    },
    Error {
        message: String,
    },
}

/// Fetch DeepSeek balance. The key is the `deepseek_key` from config.
///
/// Returns `Error` with a UI-ready message on any failure.
pub async fn fetch(key: Option<&str>) -> DeepSeekStatus {
    let Some(key) = key else {
        return DeepSeekStatus::Error {
            message: "DeepSeek: no key — add one in settings.".to_string(),
        };
    };
    if key.trim().is_empty() {
        return DeepSeekStatus::Error {
            message: "DeepSeek: no key — add one in settings.".to_string(),
        };
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .user_agent(concat!("ai-dock/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return DeepSeekStatus::Error {
                message: format!("DeepSeek: couldn't fetch ({e})."),
            };
        }
    };

    let resp = match client.get(ENDPOINT).bearer_auth(key).send().await {
        Ok(r) => r,
        Err(e) => return classify_transport(&e),
    };

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED
        || status == reqwest::StatusCode::FORBIDDEN
    {
        return DeepSeekStatus::Error {
            message: "DeepSeek: key rejected — check your API key.".to_string(),
        };
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return DeepSeekStatus::Error {
            message: "DeepSeek: rate-limited, retrying.".to_string(),
        };
    }
    if status.is_server_error() {
        return DeepSeekStatus::Error {
            message: "DeepSeek: couldn't fetch.".to_string(),
        };
    }
    if !status.is_success() {
        return DeepSeekStatus::Error {
            message: format!("DeepSeek: unexpected status {status}."),
        };
    }

    let body = match resp.json::<DeepSeekBalanceRaw>().await {
        Ok(b) => b,
        Err(_) => {
            return DeepSeekStatus::Error {
                message: "DeepSeek: unexpected response.".to_string(),
            };
        }
    };

    if !body.is_available {
        return DeepSeekStatus::Error {
            message: "DeepSeek: account unavailable.".to_string(),
        };
    }

    // Sum balances across all currencies (typically just USD).
    let total: f64 = body
        .balance_infos
        .iter()
        .filter_map(|info| {
            info.total_balance
                .as_deref()
                .and_then(|s| s.parse::<f64>().ok())
        })
        .sum();

    let currency = body
        .balance_infos
        .first()
        .map(|i| i.currency.as_str())
        .unwrap_or("USD")
        .to_string();

    // Ponytail: assume USD. If the balance is in another currency, the
    // label still shows the raw dollar amount — the user can infer.
    // Multi-currency DeepSeek accounts are extremely rare.

    DeepSeekStatus::Ok {
        total_balance: total,
        currency,
    }
}

fn classify_transport(e: &reqwest::Error) -> DeepSeekStatus {
    if e.is_timeout() || e.is_connect() || e.is_request() {
        DeepSeekStatus::Error {
            message: "DeepSeek: couldn't fetch.".to_string(),
        }
    } else {
        DeepSeekStatus::Error {
            message: "DeepSeek: couldn't fetch.".to_string(),
        }
    }
}
