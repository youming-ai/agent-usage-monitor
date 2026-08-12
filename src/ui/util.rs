/// Compact token/count display: `512`, `1.2k`, `3.4m`, `17.1b`.
pub fn format_tokens(n: u64) -> String {
    let n = n as f64;
    if n >= 1_000_000_000.0 {
        format!("{:.1}b", n / 1_000_000_000.0)
    } else if n >= 1_000_000.0 {
        format!("{:.1}m", n / 1_000_000.0)
    } else if n >= 1_000.0 {
        format!("{:.1}k", n / 1_000.0)
    } else {
        (n as u64).to_string()
    }
}

/// Compact token display for the activity summary: `4.77B`, `159M`, `12.5K`.
pub fn format_activity_tokens(n: u64) -> String {
    let (value, suffix) = if n >= 1_000_000_000 {
        (n as f64 / 1_000_000_000.0, "B")
    } else if n >= 1_000_000 {
        (n as f64 / 1_000_000.0, "M")
    } else if n >= 1_000 {
        (n as f64 / 1_000.0, "K")
    } else {
        return n.to_string();
    };

    let value = format!("{value:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string();
    format!("{value}{suffix}")
}

/// Compact duration from seconds: `45m`, `3h12m`, `9d4h`.
pub fn format_duration_secs(secs: i64) -> String {
    let secs = secs.max(0);
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let minutes = (secs % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Short month name for heatmap labels.
pub fn month_abbr(month: u8) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}

/// `Aug 6` style day label.
pub fn format_month_day(month: u8, day: u8) -> String {
    format!("{} {day}", month_abbr(month))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_scale() {
        assert_eq!(format_tokens(512), "512");
        assert_eq!(format_tokens(1_200), "1.2k");
        assert_eq!(format_tokens(1_200_000), "1.2m");
        assert_eq!(format_tokens(17_100_000_000), "17.1b");
    }

    #[test]
    fn activity_tokens_scale() {
        assert_eq!(format_activity_tokens(512), "512");
        assert_eq!(format_activity_tokens(12_500), "12.5K");
        assert_eq!(format_activity_tokens(159_000_000), "159M");
        assert_eq!(format_activity_tokens(4_770_000_000), "4.77B");
    }

    #[test]
    fn duration_formats() {
        assert_eq!(format_duration_secs(45 * 60), "45m");
        assert_eq!(format_duration_secs(3 * 3600 + 12 * 60), "3h 12m");
        assert_eq!(format_duration_secs(9 * 86400 + 4 * 3600), "9d 4h");
    }
}
