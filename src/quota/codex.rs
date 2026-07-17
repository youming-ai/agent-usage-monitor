use super::util::{classify_api_error, decode_jwt_payload, format_duration_short};
use super::{QuotaInfo, QuotaWindow};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn codex_auth_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
        .join("auth.json")
}

/// Read Codex auth info from ~/.codex/auth.json: access token, account id,
/// and email — all derived from the same parsed file (and, for account id /
/// email, the same decoded access-token JWT), so this reads+parses it once
/// instead of once per field.
fn read_auth_info() -> Option<(String, String, Option<String>)> {
    let auth_path = codex_auth_path();
    let raw = std::fs::read_to_string(&auth_path).ok()?;
    let data: Value = serde_json::from_str(&raw).ok()?;

    let access_token = data
        .get("tokens")
        .and_then(|t| t.get("access_token"))
        .and_then(|v| v.as_str())
        .or_else(|| data.get("access_token").and_then(|v| v.as_str()))?
        .to_string();

    let jwt_payload = decode_jwt_payload(&access_token);

    let account_id = data
        .get("tokens")
        .and_then(|t| t.get("account_id"))
        .and_then(|v| v.as_str())
        .or_else(|| data.get("account_id").and_then(|v| v.as_str()))
        .map(String::from)
        .or_else(|| {
            jwt_payload.as_ref().and_then(|payload| {
                payload
                    .get("chatgpt_account_id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
        })?;

    let email = jwt_payload.as_ref().and_then(|payload| {
        payload
            .get("https://api.openai.com/auth")
            .and_then(|v| v.get("email"))
            .or_else(|| payload.get("email"))
            .and_then(|v| v.as_str())
            .map(String::from)
    });

    Some((access_token, account_id, email))
}

/// Format reset time from epoch seconds
fn format_reset_time(epoch_str: &str) -> Option<String> {
    let reset_at: i64 = epoch_str.parse().ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    format_duration_short(reset_at - now)
}

/// Fetch usage data from Codex API via pure-Rust HTTP (ureq).
fn fetch_usage_json(access_token: &str, account_id: &str) -> Option<Value> {
    ureq::get("https://chatgpt.com/backend-api/wham/usage")
        .set("Authorization", &format!("Bearer {access_token}"))
        .set("ChatGPT-Account-Id", account_id)
        .set("Accept", "application/json")
        .timeout(Duration::from_secs(5))
        .call()
        .ok()
        .and_then(|resp| resp.into_json().ok())
}

/// Format usage value for a window
fn format_window_value(label: &str, window: &Value) -> Option<QuotaWindow> {
    // Codex API returns used_percent as percentage (0-100), convert to remaining (0-1)
    let used_percent = window.get("used_percent").and_then(|v| v.as_f64())?;
    let remaining = (100.0 - used_percent) / 100.0;

    let resets_at = window.get("reset_at").and_then(|v| {
        v.as_str()
            .map(String::from)
            .or_else(|| v.as_i64().map(|n| n.to_string()))
    });
    let reset_in = window
        .get("reset_after_seconds")
        .and_then(|v| v.as_i64())
        .and_then(format_duration_short)
        .or_else(|| resets_at.as_deref().and_then(format_reset_time));

    Some(QuotaWindow {
        label: label.to_string(),
        remaining_percent: Some(remaining),
        resets_at,
        reset_in,
    })
}

/// Parse usage response into QuotaInfo
fn parse_usage_response(
    data: &Value,
    email: Option<String>,
    account_id: String,
) -> Option<QuotaInfo> {
    let mut windows = Vec::new();

    // Prefer email from API response over JWT
    let email = data
        .get("email")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or(email);

    if let Some(rate_limit) = data.get("rate_limit") {
        if let Some(primary) = rate_limit.get("primary_window") {
            let label = format_window_label(primary).unwrap_or_else(|| "5h".to_string());
            if let Some(window) = format_window_value(&label, primary) {
                windows.push(window);
            }
        }
        if let Some(secondary) = rate_limit.get("secondary_window") {
            let label = format_window_label(secondary).unwrap_or_else(|| "7d".to_string());
            if let Some(window) = format_window_value(&label, secondary) {
                windows.push(window);
            }
        }
    }

    let error = classify_api_error(data, windows.is_empty());

    Some(QuotaInfo {
        tool_name: "Codex".to_string(),
        email,
        account_id: Some(account_id),
        windows,
        fetched_at: Instant::now(),
        error,
    })
}

/// Format window label from limit_window_seconds
fn format_window_label(window: &Value) -> Option<String> {
    let seconds = window
        .get("limit_window_seconds")
        .and_then(|v| v.as_i64())?;
    let hours = seconds / 3600;
    let days = seconds / 86400;

    if days >= 1 {
        Some(format!("{days}d"))
    } else if hours >= 1 {
        Some(format!("{hours}h"))
    } else {
        Some(format!("{}m", seconds / 60))
    }
}

/// Main function to fetch Codex quota info
pub fn fetch_quota() -> Option<QuotaInfo> {
    let (access_token, account_id, email) = read_auth_info()?;
    let data = fetch_usage_json(&access_token, &account_id)?;
    parse_usage_response(&data, email, account_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::QuotaError;
    use serde_json::json;

    fn parse(data: serde_json::Value) -> QuotaInfo {
        parse_usage_response(&data, None, "acct".into()).unwrap()
    }

    #[test]
    fn empty_windows_no_error_is_none() {
        let q = parse(json!({}));
        assert!(q.windows.is_empty());
        assert!(q.error.is_none()); // benign empty response -> "no quota data"
    }

    #[test]
    fn explicit_null_error_is_ignored() {
        assert!(parse(json!({ "error": null })).error.is_none());
    }

    #[test]
    fn auth_error_classified() {
        let q = parse(json!({ "error": { "type": "authentication_error", "message": "expired" } }));
        assert_eq!(q.error, Some(QuotaError::Auth("expired".into())));
    }

    #[test]
    fn other_error_is_parse() {
        let q = parse(json!({ "error": { "type": "rate_limit", "message": "slow" } }));
        assert_eq!(q.error, Some(QuotaError::Parse("rate_limit: slow".into())));
    }
}
