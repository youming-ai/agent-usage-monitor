use super::{QuotaInfo, QuotaWindow};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

fn codex_auth_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
        .join("auth.json")
}

/// Read Codex auth info from ~/.codex/auth.json
fn read_auth_info() -> Option<(String, String)> {
    let auth_path = codex_auth_path();
    let raw = std::fs::read_to_string(&auth_path).ok()?;
    let data: Value = serde_json::from_str(&raw).ok()?;

    let access_token = data
        .get("tokens")
        .and_then(|t| t.get("access_token"))
        .and_then(|v| v.as_str())
        .or_else(|| data.get("access_token").and_then(|v| v.as_str()))?
        .to_string();

    let account_id = data
        .get("tokens")
        .and_then(|t| t.get("account_id"))
        .and_then(|v| v.as_str())
        .or_else(|| data.get("account_id").and_then(|v| v.as_str()))
        .map(String::from)
        .or_else(|| {
            decode_jwt_payload(&access_token).and_then(|payload| {
                payload
                    .get("chatgpt_account_id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
        })?;

    Some((access_token, account_id))
}

/// Extract email from auth info
fn read_email() -> Option<String> {
    let auth_path = codex_auth_path();
    let raw = std::fs::read_to_string(&auth_path).ok()?;
    let data: Value = serde_json::from_str(&raw).ok()?;

    let access_token = data
        .get("tokens")
        .and_then(|t| t.get("access_token"))
        .and_then(|v| v.as_str())
        .or_else(|| data.get("access_token").and_then(|v| v.as_str()))?;

    decode_jwt_payload(access_token).and_then(|payload| {
        payload
            .get("https://api.openai.com/auth")
            .and_then(|v| v.get("email"))
            .or_else(|| payload.get("email"))
            .and_then(|v| v.as_str())
            .map(String::from)
    })
}

/// Decode JWT payload without verification
fn decode_jwt_payload(token: &str) -> Option<Value> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return None;
    }

    let payload = parts[1];
    let decoded = base64_decode(payload)?;
    serde_json::from_str(&decoded).ok()
}

fn base64_decode(input: &str) -> Option<String> {
    use base64::Engine;
    let padded = match input.len() % 4 {
        0 => input.to_string(),
        n => format!("{}{}", input, "=".repeat(4 - n)),
    };
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&padded)
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(&padded))
        .ok()?;
    String::from_utf8(bytes).ok()
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

fn format_duration_short(total_seconds: i64) -> Option<String> {
    if total_seconds <= 0 {
        return None;
    }

    let days = total_seconds / 86_400;
    let hours = (total_seconds % 86_400) / 3_600;
    let minutes = (total_seconds % 3_600) / 60;

    if days > 0 {
        Some(format!("{days}d{hours}h"))
    } else if hours > 0 {
        Some(format!("{hours}h{minutes}m"))
    } else {
        Some(format!("{minutes}m"))
    }
}

/// Fetch usage data from Codex API
fn fetch_usage_json(access_token: &str, account_id: &str) -> Option<Value> {
    let output = Command::new("/usr/bin/curl")
        .args([
            "-sS",
            "--max-time",
            "3",
            "-H",
            &format!("Authorization: Bearer {access_token}"),
            "-H",
            &format!("ChatGPT-Account-Id: {account_id}"),
            "-H",
            "Accept: application/json",
            "https://chatgpt.com/backend-api/wham/usage",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8(output.stdout).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Format usage value for a window
fn format_window_value(label: &str, window: &Value) -> Option<QuotaWindow> {
    // Codex API returns used_percent as percentage (0-100), convert to remaining (0-1)
    let used_percent = window.get("used_percent").and_then(|v| v.as_f64())?;
    let remaining = (100.0 - used_percent) / 100.0;
    
    let resets_at = window
        .get("reset_at")
        .and_then(|v| v.as_str().map(String::from).or_else(|| v.as_i64().map(|n| n.to_string())));
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
fn parse_usage_response(data: &Value, email: Option<String>, account_id: String) -> Option<QuotaInfo> {
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

    Some(QuotaInfo {
        tool_name: "Codex".to_string(),
        email,
        account_id: Some(account_id),
        windows,
        fetched_at: Instant::now(),
        error: None,
    })
}

/// Format window label from limit_window_seconds
fn format_window_label(window: &Value) -> Option<String> {
    let seconds = window.get("limit_window_seconds").and_then(|v| v.as_i64())?;
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
    let (access_token, account_id) = read_auth_info()?;
    let email = read_email();
    let data = fetch_usage_json(&access_token, &account_id)?;
    parse_usage_response(&data, email, account_id)
}
