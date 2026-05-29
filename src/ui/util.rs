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

/// Cost formatted with enough precision to stay meaningful for small
/// per-call amounts while staying compact for large totals.
pub fn format_cost(c: f64) -> String {
    if c >= 1.0 {
        format!("${c:.2}")
    } else if c >= 0.001 {
        format!("${c:.3}")
    } else if c > 0.0 {
        "<$0.001".to_string()
    } else {
        "$0.00".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_scale_with_magnitude() {
        assert_eq!(format_tokens(512), "512");
        assert_eq!(format_tokens(1_200), "1.2k");
        assert_eq!(format_tokens(1_200_000), "1.2M");
    }

    #[test]
    fn cost_precision_adapts() {
        assert_eq!(format_cost(0.0), "$0.00");
        assert_eq!(format_cost(0.0005), "<$0.001");
        assert_eq!(format_cost(0.012), "$0.012");
        assert_eq!(format_cost(12.345), "$12.35");
    }
}
