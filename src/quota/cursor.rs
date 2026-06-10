use super::util::format_duration_short;
use super::{QuotaError, QuotaInfo, QuotaWindow};
use serde_json::Value;
use std::time::{Duration, Instant};

fn read_api_key() -> Option<String> {
    if let Ok(key) = std::env::var("CURSOR_API_KEY") {
        if !key.is_empty() {
            return Some(key);
        }
    }
    None
}

fn format_reset_time(epoch_ms: i64) -> Option<String> {
    if epoch_ms <= 0 {
        return None;
    }
    let reset_secs = epoch_ms / 1000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    let mut diff = reset_secs - now;
    // The subscription cycle start is in the past — compute time until the
    // *next* cycle start. Cursor cycles are monthly; if the start was more
    // than a month ago, align to a standard 30-day window.
    let cycle_days = 30i64;
    let cycle_secs = cycle_days * 86_400;
    while diff <= 0 {
        diff += cycle_secs;
    }
    format_duration_short(diff)
}

fn fetch_spend_json(api_key: &str) -> Option<Value> {
    let auth = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        format!("{api_key}:"),
    );
    ureq::post("https://api.cursor.com/teams/spend")
        .set("Authorization", &format!("Basic {auth}"))
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(5))
        .call()
        .ok()
        .and_then(|resp| resp.into_json().ok())
}

fn parse_spend_response(data: &Value) -> Option<QuotaInfo> {
    let members = data.get("teamMemberSpend")?.as_array()?;
    if members.is_empty() {
        return None;
    }

    // Find self by matching to the first member with spend data
    let member = &members[0];

    let email = member
        .get("email")
        .and_then(|v| v.as_str())
        .map(String::from);

    // spendCents is on-demand spend, overallSpendCents is total (including included usage)
    let spent_cents = member
        .get("overallSpendCents")
        .and_then(|v| v.as_f64())
        .or_else(|| member.get("spendCents").and_then(|v| v.as_f64()))?;

    let limit_dollars = member
        .get("monthlyLimitDollars")
        .and_then(|v| v.as_f64());

    let cycle_start = data
        .get("subscriptionCycleStart")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let mut windows = Vec::new();

    if let Some(limit) = limit_dollars {
        if limit > 0.0 {
            let limit_cents = limit * 100.0;
            let remaining = ((limit_cents - spent_cents) / limit_cents).max(0.0);
            windows.push(QuotaWindow {
                label: "monthly".to_string(),
                remaining_percent: Some(remaining),
                resets_at: if cycle_start > 0 {
                    Some(format!("cycle start: {}", cycle_start))
                } else {
                    None
                },
                reset_in: format_reset_time(cycle_start),
            });
        }
    }

    // Always show spend as at least one window even if no limit
    if windows.is_empty() {
        windows.push(QuotaWindow {
            label: "spent".to_string(),
            remaining_percent: None,
            resets_at: Some(format!("${:.2}", spent_cents / 100.0)),
            reset_in: format_reset_time(cycle_start),
        });
    }

    let error = if windows.is_empty() {
        data.get("error").filter(|e| !e.is_null()).map(|e| {
            let message = e
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            QuotaError::Parse(message)
        })
    } else {
        None
    };

    Some(QuotaInfo {
        tool_name: "Cursor CLI".to_string(),
        email,
        account_id: None,
        windows,
        fetched_at: Instant::now(),
        error,
    })
}

pub fn fetch_quota() -> Option<QuotaInfo> {
    let api_key = read_api_key()?;
    let data = fetch_spend_json(&api_key)?;
    parse_spend_response(&data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_spend_with_limit() {
        let cycle_start = serde_json::json!(1708992000000i64);
        let data = json!({
            "teamMemberSpend": [{
                "userId": 1,
                "spendCents": 1900.0,
                "overallSpendCents": 3200.0,
                "name": "Alice",
                "email": "alice@example.com",
                "role": "member",
                "monthlyLimitDollars": 50.0
            }],
            "subscriptionCycleStart": cycle_start,
            "totalMembers": 1,
            "totalPages": 1
        });
        let q = parse_spend_response(&data).unwrap();
        assert_eq!(q.email.as_deref(), Some("alice@example.com"));
        assert_eq!(q.windows.len(), 1);
        assert_eq!(q.windows[0].label, "monthly");
        let r = q.windows[0].remaining_percent.unwrap();
        // spent 3200 cents of 5000 cents => 36% remaining
        assert!(r > 0.3 && r < 0.4, "got {r}");
    }

    #[test]
    fn no_limit_shows_spent() {
        let data = json!({
            "teamMemberSpend": [{
                "userId": 1,
                "spendCents": 1900.0,
                "overallSpendCents": 1900.0,
                "name": "Bob",
                "email": "bob@example.com",
                "role": "member"
            }],
            "subscriptionCycleStart": 0,
            "totalMembers": 1,
            "totalPages": 1
        });
        let q = parse_spend_response(&data).unwrap();
        assert_eq!(q.windows.len(), 1);
        assert_eq!(q.windows[0].label, "spent");
        assert_eq!(q.windows[0].resets_at.as_deref(), Some("$19.00"));
    }

    #[test]
    fn empty_members_is_none() {
        let data = json!({
            "teamMemberSpend": [],
            "subscriptionCycleStart": 0,
            "totalMembers": 0,
            "totalPages": 0
        });
        assert!(parse_spend_response(&data).is_none());
    }
}
