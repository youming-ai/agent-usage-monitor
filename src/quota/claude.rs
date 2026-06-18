use super::util::format_duration_short;
use super::{QuotaError, QuotaInfo, QuotaWindow};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

/// Read Claude OAuth credentials, preferring the macOS Keychain and falling
/// back to the `~/.claude/.credentials.json` file used on Linux (and some
/// macOS setups).
fn read_oauth_credentials() -> Option<Value> {
    read_keychain_credentials().or_else(read_credentials_file)
}

/// Read Claude OAuth credentials from the macOS Keychain.
fn read_keychain_credentials() -> Option<Value> {
    let output = Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8(output.stdout).ok()?;
    serde_json::from_str(raw.trim()).ok()
}

fn credentials_file_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join(".credentials.json")
}

/// Read Claude OAuth credentials from `~/.claude/.credentials.json`.
fn read_credentials_file() -> Option<Value> {
    let raw = std::fs::read_to_string(credentials_file_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Extract access token from Keychain credentials
fn read_access_token() -> Option<String> {
    let parsed = read_oauth_credentials()?;
    parsed
        .get("claudeAiOauth")
        .and_then(|v| v.get("accessToken"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Extract email from Keychain credentials
fn read_email() -> Option<String> {
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", "Claude Code-credentials"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8(output.stdout).ok()?;
    parse_keychain_account(&raw)
}

fn parse_keychain_account(raw: &str) -> Option<String> {
    let marker = "\"acct\"<blob>=\"";
    raw.lines().find_map(|line| {
        let line = line.trim();
        let start = line.find(marker)? + marker.len();
        let end = line[start..].find('"')?;
        Some(line[start..start + end].to_string())
    })
}

/// Format reset time from ISO 8601 string
fn format_reset_time(iso_str: &str) -> Option<String> {
    let reset_at: DateTime<Utc> = iso_str.parse().ok()?;
    let now = Utc::now();
    let diff = (reset_at - now).num_seconds();
    format_duration_short(diff)
}

/// Fetch usage data from Claude API via pure-Rust HTTP (ureq).
fn fetch_usage_json(access_token: &str) -> Option<Value> {
    ureq::get("https://api.anthropic.com/api/oauth/usage")
        .set("Authorization", &format!("Bearer {access_token}"))
        .set("anthropic-beta", "oauth-2025-04-20")
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .set("User-Agent", "claude-code/2.0.27")
        .timeout(Duration::from_secs(5))
        .call()
        .ok()
        .and_then(|resp| resp.into_json().ok())
}

/// Parse usage response into QuotaInfo
fn parse_usage_response(data: &Value, email: Option<String>) -> Option<QuotaInfo> {
    let mut windows = Vec::new();

    // Parse 5-hour window
    if let Some(five_hour) = data.get("five_hour") {
        let utilization = five_hour.get("utilization").and_then(|v| v.as_f64());
        let resets_at = five_hour
            .get("resets_at")
            .and_then(|v| v.as_str())
            .map(String::from);
        let reset_in = resets_at.as_deref().and_then(format_reset_time);

        windows.push(QuotaWindow {
            label: "5h".to_string(),
            // Claude API returns utilization as percentage (0-100), convert to remaining (0-1)
            remaining_percent: utilization.map(|u| (100.0 - u) / 100.0),
            resets_at,
            reset_in,
        });
    }

    // Parse 7-day window
    if let Some(seven_day) = data.get("seven_day") {
        let utilization = seven_day.get("utilization").and_then(|v| v.as_f64());
        let resets_at = seven_day
            .get("resets_at")
            .and_then(|v| v.as_str())
            .map(String::from);
        let reset_in = resets_at.as_deref().and_then(format_reset_time);

        windows.push(QuotaWindow {
            label: "7d".to_string(),
            // Claude API returns utilization as percentage (0-100), convert to remaining (0-1)
            remaining_percent: utilization.map(|u| (100.0 - u) / 100.0),
            resets_at,
            reset_in,
        });
    }

    // Surface an API error only when we have no windows to show. A stray
    // `error` key alongside valid windows must NOT hide real quota (the UI
    // renders the error arm before the windows arm), and `.map` would set
    // `Some` for any `error` key — so gate it on the windows being empty.
    let error = if windows.is_empty() {
        // `data.get("error")` is `Some` for an explicit `"error": null` (a
        // common success sentinel) — filter that out so it isn't surfaced as a
        // bogus parse error that would also force a re-fetch every tick.
        data.get("error").filter(|e| !e.is_null()).map(|e| {
            let error_type = e.get("type").and_then(|v| v.as_str());
            let message = e
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            if error_type == Some("authentication_error") {
                QuotaError::Auth(message)
            } else {
                QuotaError::Parse(format!("{}: {message}", error_type.unwrap_or("error")))
            }
        })
    } else {
        None
    };

    Some(QuotaInfo {
        tool_name: "Claude Code".to_string(),
        email,
        account_id: None,
        windows,
        fetched_at: Instant::now(),
        error,
    })
}

/// Main function to fetch Claude quota info
pub fn fetch_quota() -> Option<QuotaInfo> {
    let access_token = read_access_token()?;
    let email = read_email();
    let data = fetch_usage_json(&access_token)?;
    parse_usage_response(&data, email)
}

/// `QuotaFetcher` implementation for the registry in `quota::fetchers()`.
pub struct ClaudeQuotaFetcher;

impl super::QuotaFetcher for ClaudeQuotaFetcher {
    fn platform(&self) -> crate::state::Platform {
        crate::state::Platform::ClaudeCode
    }
    fn fetch(&self) -> Option<QuotaInfo> {
        fetch_quota()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_windows_no_error_is_none() {
        let q = parse_usage_response(&json!({}), None).unwrap();
        assert!(q.windows.is_empty());
        assert!(q.error.is_none()); // renders as "no quota data", not a failure
    }

    #[test]
    fn explicit_null_error_is_ignored() {
        let q = parse_usage_response(&json!({ "error": null }), None).unwrap();
        assert!(q.error.is_none());
    }

    #[test]
    fn auth_error_classified_when_no_windows() {
        let data = json!({ "error": { "type": "authentication_error", "message": "expired" } });
        let q = parse_usage_response(&data, None).unwrap();
        assert_eq!(q.error, Some(QuotaError::Auth("expired".into())));
    }

    #[test]
    fn stray_error_does_not_hide_valid_windows() {
        // A present `five_hour` block yields a window; a stray error must not
        // suppress it (the UI renders the error arm before the windows arm).
        let data = json!({
            "five_hour": { "utilization": 10.0 },
            "error": { "type": "x", "message": "y" },
        });
        let q = parse_usage_response(&data, None).unwrap();
        assert!(!q.windows.is_empty());
        assert!(q.error.is_none());
    }
}
