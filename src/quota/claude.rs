use super::{QuotaInfo, QuotaWindow};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::process::Command;
use std::time::Instant;

/// Read Claude OAuth credentials from macOS Keychain
fn read_oauth_credentials() -> Option<Value> {
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8(output.stdout).ok()?;
    serde_json::from_str(raw.trim()).ok()
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

/// Fetch usage data from Claude API
fn fetch_usage_json(access_token: &str) -> Option<Value> {
    let output = Command::new("/usr/bin/curl")
        .args([
            "-sS",
            "--max-time",
            "3",
            "-H",
            &format!("Authorization: Bearer {access_token}"),
            "-H",
            "anthropic-beta: oauth-2025-04-20",
            "-H",
            "Accept: application/json",
            "-H",
            "Content-Type: application/json",
            "-H",
            "User-Agent: claude-code/2.0.27",
            "https://api.anthropic.com/api/oauth/usage",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8(output.stdout).ok()?;
    serde_json::from_str(&raw).ok()
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

    // Check for error
    let error = data.get("error").and_then(|e| {
        let error_type = e.get("type").and_then(|v| v.as_str());
        if error_type == Some("authentication_error") {
            Some("Re-auth required".to_string())
        } else {
            e.get("message").and_then(|v| v.as_str()).map(String::from)
        }
    });

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
