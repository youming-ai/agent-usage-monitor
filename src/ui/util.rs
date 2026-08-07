use chrono::{DateTime, Utc};

/// Compact token count, e.g. `1.2M`, `340.0k`, `512`.
pub fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Relative age from `ts` to now: `just now`, `5m`, `3h`, `2d`.
pub fn format_age(ts: DateTime<Utc>) -> String {
    let secs = (Utc::now() - ts).num_seconds().max(0);
    if secs < 60 {
        "just now".into()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn tokens_scale_with_magnitude() {
        assert_eq!(format_tokens(512), "512");
        assert_eq!(format_tokens(1_200), "1.2k");
        assert_eq!(format_tokens(1_200_000), "1.2M");
    }

    #[test]
    fn age_formats_relative() {
        let now = Utc::now();
        assert_eq!(format_age(now), "just now");
        assert_eq!(format_age(now - chrono::Duration::minutes(5)), "5m");
        assert_eq!(format_age(now - chrono::Duration::hours(3)), "3h");
        assert_eq!(format_age(now - chrono::Duration::days(2)), "2d");
        let _ = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0);
    }
}
