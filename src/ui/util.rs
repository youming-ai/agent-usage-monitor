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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_scale_with_magnitude() {
        assert_eq!(format_tokens(512), "512");
        assert_eq!(format_tokens(1_200), "1.2k");
        assert_eq!(format_tokens(1_200_000), "1.2M");
    }
}
