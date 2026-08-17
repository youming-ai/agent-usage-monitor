//! Grok (Build / SuperGrok) quota fetcher.
//!
//! xAI has no public quota API. The official Grok Build CLI pulls subscription
//! usage from an undocumented billing endpoint on the CLI chat proxy:
//! `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits`, authed with
//! the session token from `~/.grok/auth.json` plus CLI-specific headers
//! (`X-XAI-Token-Auth: xai-grok-cli`, `x-userid`).
//!
//! The endpoint reports the shared weekly usage pool as a percentage
//! (`creditUsagePercent`) with the current period's reset time — exactly what
//! the quota panel needs. Since June 2026 paid Grok plans share one weekly
//! pool across Chat/Imagine/Voice/Build/API, so this is a pool %, not tokens.
//!
//! Session tokens expire (~7 days); like the CLI itself, we silently refresh
//! via the stored OIDC `refresh_token` when the JWT is near/at expiry. The
//! refreshed token is used in memory only — `auth.json` is left untouched
//! (the CLI rewrites it on its own next run).

use super::{QuotaError, QuotaInfo, QuotaWindow};
use crate::quota::util::{decode_jwt_payload, format_duration_short};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const BILLING_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";

/// OIDC refresh tokens rotate on every use (each refresh mints a new pair and
/// revokes the old one). We keep the refreshed pair in memory so a second
/// refresh later in this process doesn't replay a revoked token; `auth.json`
/// is never written (the CLI owns that file, with its own lock) — on the next
/// process start we re-read whatever the CLI last persisted.
static REFRESHED: Mutex<Option<(String, String)>> = Mutex::new(None); // (access, refresh)

/// Session-scoped OIDC credential from `~/.grok/auth.json`.
struct GrokAuth {
    key: String,
    user_id: String,
    email: Option<String>,
    refresh_token: String,
    oidc_issuer: String,
    oidc_client_id: String,
}

fn grok_auth_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".grok")
        .join("auth.json")
}

/// Read the first usable OIDC session entry from `~/.grok/auth.json`.
///
/// The file is keyed by `"<issuer>::<client_id>"`; session entries carry a
/// `key` (access token), `refresh_token`, `user_id`, and `email`. API-key
/// entries (no `refresh_token`) are skipped — the billing proxy needs a CLI
/// session token.
fn read_auth() -> Option<GrokAuth> {
    let raw = std::fs::read_to_string(grok_auth_path()).ok()?;
    let data: Value = serde_json::from_str(&raw).ok()?;
    let obj = data.as_object()?;

    obj.values().find_map(|v| {
        let e = v.as_object()?;
        let key = e.get("key")?.as_str()?.to_string();
        let refresh_token = e.get("refresh_token")?.as_str()?.to_string();
        let oidc_client_id = e.get("oidc_client_id")?.as_str()?.to_string();
        Some(GrokAuth {
            user_id: e
                .get("user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            email: e.get("email").and_then(|v| v.as_str()).map(String::from),
            key,
            refresh_token,
            oidc_issuer: e
                .get("oidc_issuer")
                .and_then(|v| v.as_str())
                .unwrap_or("https://auth.x.ai")
                .to_string(),
            oidc_client_id,
        })
    })
}

/// True if the access token's JWT `exp` is within 5 minutes of now.
fn is_token_expiring(access_token: &str) -> bool {
    let exp = decode_jwt_payload(access_token)
        .and_then(|p| p.get("exp").and_then(|v| v.as_i64()))
        .unwrap_or(0);
    let now = Utc::now().timestamp();
    exp - now < 300
}

/// Exchange the refresh token for a fresh (access, refresh) pair via OIDC.
/// Returns the rotated pair so the caller can cache it — the old refresh
/// token is already revoked server-side.
fn refresh_access_token(
    oidc_issuer: &str,
    oidc_client_id: &str,
    refresh_token: &str,
) -> Option<(String, String)> {
    let resp = ureq::post(&format!("{oidc_issuer}/oauth2/token"))
        .set("Accept", "application/json")
        .timeout(Duration::from_secs(10))
        .send_form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", oidc_client_id),
            (
                "scope",
                "openid profile email offline_access grok-cli:access",
            ),
        ])
        .ok()?;
    let data: Value = resp.into_json().ok()?;
    let access = data.get("access_token")?.as_str()?.to_string();
    let refresh = data
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .unwrap_or(refresh_token)
        .to_string();
    Some((access, refresh))
}

/// Fetch the billing config. Returns `Ok(Some(json))` on success,
/// `Ok(None)` when the token was rejected (caller should report re-auth),
/// and `Err(())` on transport failures (treated as transient).
fn fetch_billing(token: &str, user_id: &str) -> Result<Option<Value>, ()> {
    let result = ureq::get(BILLING_URL)
        .set("Authorization", &format!("Bearer {token}"))
        .set("X-XAI-Token-Auth", "xai-grok-cli")
        .set("x-userid", user_id)
        .set("x-grok-client-version", "0.1.210")
        .set("x-grok-client-mode", "headless")
        .set("Accept", "application/json")
        .timeout(Duration::from_secs(8))
        .call();
    match result {
        Ok(resp) => Ok(resp.into_json().ok()),
        Err(ureq::Error::Status(code, _)) if code == 401 || code == 403 => Ok(None),
        Err(_) => Err(()),
    }
}

fn parse_rfc3339_end(s: &str) -> Option<String> {
    let end: DateTime<Utc> = DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))?;
    let now = Utc::now().timestamp();
    format_duration_short(end.timestamp() - now)
}

/// Parse the credits billing response into `QuotaInfo`.
fn parse_billing_response(data: &Value, email: Option<String>) -> Option<QuotaInfo> {
    let config = data.get("config")?;
    let credit_pct = config.get("creditUsagePercent").and_then(|v| v.as_f64());
    let period_end = config
        .get("currentPeriod")
        .and_then(|p| p.get("end"))
        .and_then(|v| v.as_str())
        .or_else(|| config.get("billingPeriodEnd").and_then(|v| v.as_str()))
        .map(String::from);

    let mut windows = Vec::new();
    if let Some(pct) = credit_pct {
        windows.push(QuotaWindow {
            label: "weekly".into(),
            remaining_percent: Some(((100.0 - pct) / 100.0).clamp(0.0, 1.0)),
            resets_at: period_end.clone(),
            reset_in: period_end.as_deref().and_then(parse_rfc3339_end),
        });
    }

    let mut summary_bits = Vec::new();
    if let Some(pct) = credit_pct {
        summary_bits.push(format!("{pct:.0}% used"));
    }
    let on_demand_used = config
        .get("onDemandUsed")
        .and_then(|v| v.get("val"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let on_demand_cap = config
        .get("onDemandCap")
        .and_then(|v| v.get("val"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    if on_demand_cap > 0 {
        summary_bits.push(format!("on-demand {on_demand_used}/{on_demand_cap}"));
    } else if on_demand_used > 0 {
        summary_bits.push("on-demand on".into());
    }
    if let Some(val) = config
        .get("prepaidBalance")
        .and_then(|v| v.get("val"))
        .and_then(|v| v.as_i64())
        .filter(|&val| val > 0)
    {
        summary_bits.push(format!("prepaid ${:.0}", val as f64 / 100.0));
    }

    let live_summary = if summary_bits.is_empty() {
        None
    } else {
        Some(summary_bits.join(" · "))
    };

    Some(QuotaInfo {
        tool_name: "Grok".to_string(),
        email,
        account_id: None,
        plan: None,
        org: None,
        windows,
        live_summary,
        fetched_at: Instant::now(),
        error: None,
    })
}

/// Main entry point: read credentials (refreshing the token if needed) and
/// fetch Grok subscription quota.
pub fn fetch_quota() -> Option<QuotaInfo> {
    let auth = read_auth()?;

    // Prefer a pair refreshed earlier in this process; else fall back to the
    // file's stored tokens.
    let (mut access, mut refresh) = REFRESHED
        .lock()
        .ok()
        .and_then(|c| c.clone())
        .unwrap_or_else(|| (auth.key.clone(), auth.refresh_token.clone()));

    if is_token_expiring(&access)
        && let Some((new_access, new_refresh)) =
            refresh_access_token(&auth.oidc_issuer, &auth.oidc_client_id, &refresh)
    {
        access = new_access;
        refresh = new_refresh;
        if let Ok(mut cache) = REFRESHED.lock() {
            *cache = Some((access.clone(), refresh.clone()));
        }
    }

    match fetch_billing(&access, &auth.user_id) {
        Ok(Some(data)) => parse_billing_response(&data, auth.email),
        Ok(None) => Some(QuotaInfo {
            tool_name: "Grok".to_string(),
            email: auth.email,
            account_id: None,
            plan: None,
            org: None,
            windows: Vec::new(),
            live_summary: None,
            fetched_at: Instant::now(),
            error: Some(QuotaError::Auth(
                "session rejected — run `grok login`".into(),
            )),
        }),
        Err(()) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(data: Value) -> QuotaInfo {
        parse_billing_response(&data, Some("a@b.c".into())).unwrap()
    }

    #[test]
    fn parses_weekly_pool_percent_and_reset() {
        let q = parse(json!({
            "config": {
                "creditUsagePercent": 42.5,
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "start": "2026-08-14T00:40:46.251743+00:00",
                    "end": "2026-08-21T00:40:46.251743+00:00"
                },
                "onDemandCap": { "val": 0 },
                "onDemandUsed": { "val": 0 },
                "prepaidBalance": { "val": 0 }
            }
        }));
        assert_eq!(q.windows.len(), 1);
        assert_eq!(q.windows[0].label, "weekly");
        assert!((q.windows[0].remaining_percent.unwrap() - 0.575).abs() < 0.01);
        assert_eq!(
            q.windows[0].resets_at.as_deref(),
            Some("2026-08-21T00:40:46.251743+00:00")
        );
        assert_eq!(q.live_summary.as_deref(), Some("42% used"));
        assert_eq!(q.email.as_deref(), Some("a@b.c"));
        assert!(q.error.is_none());
    }

    #[test]
    fn clamps_remaining_to_zero() {
        let q = parse(json!({
            "config": { "creditUsagePercent": 120.0,
                        "currentPeriod": { "end": "2026-08-21T00:40:46.251743+00:00" } }
        }));
        assert_eq!(q.windows[0].remaining_percent.unwrap(), 0.0);
    }

    #[test]
    fn legacy_billing_period_fields_are_a_fallback() {
        let q = parse(json!({
            "config": {
                "monthlyLimit": { "val": 2000 },
                "used": { "val": 500 },
                "billingPeriodEnd": "2026-09-01T00:00:00Z"
            }
        }));
        // No creditUsagePercent in the legacy shape -> no window, no summary.
        assert!(q.windows.is_empty());
        assert!(q.live_summary.is_none());
        assert!(q.error.is_none());
    }

    #[test]
    fn missing_config_is_unparseable() {
        assert!(parse_billing_response(&json!({}), None).is_none());
    }
}
