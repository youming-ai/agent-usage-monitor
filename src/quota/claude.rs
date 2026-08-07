use super::util::{classify_api_error, format_duration_short};
use super::{QuotaInfo, QuotaWindow};
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

fn get_json(url: &str, access_token: &str) -> Option<Value> {
    ureq::get(url)
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

/// Fetch usage data from Claude API via pure-Rust HTTP (ureq).
fn fetch_usage_json(access_token: &str) -> Option<Value> {
    get_json("https://api.anthropic.com/api/oauth/usage", access_token)
}

fn fetch_profile_json(access_token: &str) -> Option<Value> {
    get_json("https://api.anthropic.com/api/oauth/profile", access_token)
}

/// One utilization block: `{ utilization, resets_at, ... }`.
fn window_from_block(label: &str, block: &Value) -> Option<QuotaWindow> {
    let utilization = block.get("utilization").and_then(|v| v.as_f64())?;
    let resets_at = block
        .get("resets_at")
        .and_then(|v| v.as_str())
        .map(String::from);
    let reset_in = resets_at.as_deref().and_then(format_reset_time);
    Some(QuotaWindow {
        label: label.to_string(),
        // Claude returns utilization as 0–100; we store remaining as 0–1.
        remaining_percent: Some((100.0 - utilization) / 100.0),
        resets_at,
        reset_in,
    })
}

/// Named utilization fields on the usage payload (beyond five_hour / seven_day).
const EXTRA_WINDOW_KEYS: &[(&str, &str)] = &[
    ("seven_day_opus", "Opus"),
    ("seven_day_sonnet", "Sonnet"),
    ("seven_day_cowork", "Cowork"),
    ("seven_day_oauth_apps", "OAuth"),
    ("seven_day_omelette", "Omelette"),
];

fn parse_extra_usage_summary(data: &Value) -> Option<String> {
    let extra = data.get("extra_usage")?;
    let enabled = extra.get("is_enabled").and_then(|v| v.as_bool())?;
    if !enabled {
        return Some("extra usage off".into());
    }
    let used = extra
        .get("used_credits")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let monthly = extra
        .get("monthly_limit")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let currency = extra
        .get("currency")
        .and_then(|v| v.as_str())
        .unwrap_or("USD");
    if monthly > 0.0 {
        Some(format!("extra {used:.0}/{monthly:.0} {currency}"))
    } else if used > 0.0 {
        Some(format!("extra used {used:.2} {currency}"))
    } else {
        Some("extra usage on".into())
    }
}

fn parse_spend_summary(data: &Value) -> Option<String> {
    let spend = data.get("spend")?;
    let used_minor = spend
        .get("used")
        .and_then(|u| u.get("amount_minor"))
        .and_then(|v| v.as_i64())?;
    let limit_minor = spend
        .get("limit")
        .and_then(|u| u.get("amount_minor"))
        .and_then(|v| v.as_i64())?;
    let exp = spend
        .get("used")
        .and_then(|u| u.get("exponent"))
        .and_then(|v| v.as_i64())
        .unwrap_or(2) as i32;
    let scale = 10f64.powi(exp);
    let used = used_minor as f64 / scale;
    let limit = limit_minor as f64 / scale;
    if limit > 0.0 || used > 0.0 {
        Some(format!("spend ${used:.2}/${limit:.2}"))
    } else {
        None
    }
}

/// Parse usage (+ optional profile) into QuotaInfo.
fn parse_usage_response(
    data: &Value,
    email: Option<String>,
    profile: Option<&Value>,
) -> Option<QuotaInfo> {
    let mut windows = Vec::new();

    if let Some(five_hour) = data.get("five_hour")
        && let Some(w) = window_from_block("5h", five_hour)
    {
        windows.push(w);
    }
    if let Some(seven_day) = data.get("seven_day")
        && let Some(w) = window_from_block("7d", seven_day)
    {
        windows.push(w);
    }
    for (key, label) in EXTRA_WINDOW_KEYS {
        if let Some(block) = data.get(*key)
            && !block.is_null()
            && let Some(w) = window_from_block(label, block)
        {
            windows.push(w);
        }
    }

    // Scoped limits (e.g. per-model weekly) that aren't already covered.
    if let Some(limits) = data.get("limits").and_then(|v| v.as_array()) {
        for lim in limits {
            if lim.get("is_active").and_then(|v| v.as_bool()) != Some(true) {
                continue;
            }
            let kind = lim.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            // Skip ones we already mapped from five_hour / seven_day.
            if matches!(kind, "session" | "weekly_all") {
                continue;
            }
            let percent = lim.get("percent").and_then(|v| v.as_f64());
            let Some(percent) = percent else { continue };
            let label = lim
                .get("scope")
                .and_then(|s| s.get("model"))
                .and_then(|m| m.get("display_name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| kind.to_string());
            // Avoid duplicates by label.
            if windows.iter().any(|w| w.label == label) {
                continue;
            }
            let resets_at = lim
                .get("resets_at")
                .and_then(|v| v.as_str())
                .map(String::from);
            let reset_in = resets_at.as_deref().and_then(format_reset_time);
            windows.push(QuotaWindow {
                label,
                remaining_percent: Some((100.0 - percent) / 100.0),
                resets_at,
                reset_in,
            });
        }
    }

    let error = classify_api_error(data, windows.is_empty());

    let (plan, org, profile_email) = profile
        .map(|p| {
            let org = p.get("organization");
            let plan = org
                .and_then(|o| o.get("rate_limit_tier"))
                .and_then(|v| v.as_str())
                .or_else(|| {
                    org.and_then(|o| o.get("organization_type"))
                        .and_then(|v| v.as_str())
                })
                .map(|s| s.trim_start_matches("default_").replace('_', " "));
            let org_name = org
                .and_then(|o| o.get("name"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let email = p
                .get("account")
                .and_then(|a| a.get("email"))
                .and_then(|v| v.as_str())
                .map(String::from);
            (plan, org_name, email)
        })
        .unwrap_or((None, None, None));

    let mut summary_bits = Vec::new();
    if let Some(s) = parse_extra_usage_summary(data) {
        summary_bits.push(s);
    }
    if let Some(s) = parse_spend_summary(data) {
        summary_bits.push(s);
    }
    let live_summary = if summary_bits.is_empty() {
        None
    } else {
        Some(summary_bits.join(" · "))
    };

    Some(QuotaInfo {
        tool_name: "Claude Code".to_string(),
        email: profile_email.or(email),
        account_id: None,
        plan,
        org,
        windows,
        live_summary,
        fetched_at: Instant::now(),
        error,
    })
}

/// Main function to fetch Claude quota info (usage + profile).
pub fn fetch_quota() -> Option<QuotaInfo> {
    let access_token = read_access_token()?;
    let email = read_email();
    let data = fetch_usage_json(&access_token)?;
    let profile = fetch_profile_json(&access_token);
    parse_usage_response(&data, email, profile.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::QuotaError;
    use serde_json::json;

    #[test]
    fn empty_windows_no_error_is_none() {
        let q = parse_usage_response(&json!({}), None, None).unwrap();
        assert!(q.windows.is_empty());
        assert!(q.error.is_none());
    }

    #[test]
    fn explicit_null_error_is_ignored() {
        let q = parse_usage_response(&json!({ "error": null }), None, None).unwrap();
        assert!(q.error.is_none());
    }

    #[test]
    fn auth_error_classified_when_no_windows() {
        let data = json!({ "error": { "type": "authentication_error", "message": "expired" } });
        let q = parse_usage_response(&data, None, None).unwrap();
        assert_eq!(q.error, Some(QuotaError::Auth("expired".into())));
    }

    #[test]
    fn stray_error_does_not_hide_valid_windows() {
        let data = json!({
            "five_hour": { "utilization": 10.0 },
            "error": { "type": "x", "message": "y" },
        });
        let q = parse_usage_response(&data, None, None).unwrap();
        assert!(!q.windows.is_empty());
        assert!(q.error.is_none());
    }

    #[test]
    fn parses_extra_windows_and_profile() {
        let usage = json!({
            "five_hour": { "utilization": 18.0, "resets_at": "2026-08-07T09:00:00Z" },
            "seven_day": { "utilization": 10.0, "resets_at": "2026-08-13T14:00:00Z" },
            "seven_day_opus": { "utilization": 5.0, "resets_at": "2026-08-13T14:00:00Z" },
            "extra_usage": {
                "is_enabled": true,
                "monthly_limit": 0,
                "used_credits": 0.0,
                "currency": "USD"
            }
        });
        let profile = json!({
            "account": { "email": "a@b.c" },
            "organization": {
                "name": "elestyle",
                "rate_limit_tier": "default_claude_max_5x",
                "organization_type": "claude_team"
            }
        });
        let q = parse_usage_response(&usage, None, Some(&profile)).unwrap();
        assert_eq!(q.windows.len(), 3);
        assert!(q.windows.iter().any(|w| w.label == "Opus"));
        assert_eq!(q.plan.as_deref(), Some("claude max 5x"));
        assert_eq!(q.org.as_deref(), Some("elestyle"));
        assert_eq!(q.email.as_deref(), Some("a@b.c"));
        assert!(q.live_summary.as_deref().unwrap_or("").contains("extra"));
    }
}
