use crate::quota::QuotaInfo;
use crate::state::Platform;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

const BAR_WIDTH: usize = 6;

/// Number of filled cells for a remaining fraction across `width` cells.
fn filled_cells(remaining: f64, width: usize) -> usize {
    (remaining.clamp(0.0, 1.0) * width as f64).round() as usize
}

/// Status glyph for a remaining fraction. Status is conveyed by the symbol,
/// not by color, to keep the palette minimal.
fn status_glyph(remaining: Option<f64>) -> &'static str {
    match remaining {
        Some(r) if r >= 0.5 => "✓",
        Some(r) if r >= 0.2 => "⚠",
        Some(_) => "✗",
        None => "·",
    }
}

/// Build one compact quota-window segment: `✓ 5h ▓▓▓▓░░ 82% resets 2h30m`.
/// The reset time is the whole point of a quota window — how long until the
/// limit lifts — so it stays even in the compact layout.
fn mini_window_spans(
    accent: Color,
    label: &str,
    remaining: Option<f64>,
    reset_in: Option<&str>,
) -> Vec<Span<'static>> {
    let filled = remaining.map(|r| filled_cells(r, BAR_WIDTH)).unwrap_or(0);
    let empty = BAR_WIDTH - filled;

    let pct = remaining
        .map(|r| format!("{}%", (r * 100.0).round() as u64))
        .unwrap_or_else(|| "--".to_string());

    let mut spans = vec![
        Span::raw(format!(" {} ", status_glyph(remaining))),
        Span::raw(format!("{label:<3} ")),
        Span::styled("▓".repeat(filled), Style::default().fg(accent)),
        Span::styled("░".repeat(empty), Style::default().fg(Color::DarkGray)),
        Span::raw(format!(" {pct:>4}")),
    ];
    if let Some(reset) = reset_in {
        spans.push(Span::styled(
            format!(" resets {reset}"),
            Style::default().fg(Color::DarkGray),
        ));
    }
    spans
}

/// Render the quota windows as one bar per line (replaces the old single
/// gauge). The active platform color is the single accent used for the bars.
pub fn quota_panel(platform: Platform, quota: Option<&QuotaInfo>) -> Paragraph<'static> {
    let accent = platform.primary_color();

    let lines: Vec<Line> = match quota {
        None => vec![Line::from(Span::styled(
            " loading…",
            Style::default().fg(Color::DarkGray),
        ))],
        Some(q) if q.error.is_some() => vec![Line::from(Span::styled(
            format!(" ✗ {}", q.error.as_ref().unwrap().display()),
            Style::default().fg(Color::DarkGray),
        ))],
        Some(q) if q.windows.is_empty() => {
            if q.email.is_some() {
                vec![Line::from(Span::styled(
                    " ✓ Signed in",
                    Style::default().fg(accent),
                ))]
            } else {
                vec![Line::from(Span::styled(
                    " no quota data",
                    Style::default().fg(Color::DarkGray),
                ))]
            }
        }
        Some(q) => {
            let mut spans = Vec::new();
            for (i, w) in q.windows.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::styled("  |  ", Style::default().fg(Color::DarkGray)));
                }
                spans.extend(mini_window_spans(
                    accent,
                    &w.label,
                    w.remaining_percent,
                    w.reset_in.as_deref(),
                ));
            }
            vec![Line::from(spans)]
        }
    };

    Paragraph::new(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filled_cells_rounds_to_nearest() {
        assert_eq!(filled_cells(0.0, 10), 0);
        assert_eq!(filled_cells(1.0, 10), 10);
        assert_eq!(filled_cells(0.82, 10), 8);
        assert_eq!(filled_cells(0.54, 10), 5);
    }

    #[test]
    fn filled_cells_clamps_out_of_range() {
        assert_eq!(filled_cells(-0.5, 10), 0);
        assert_eq!(filled_cells(1.5, 10), 10);
    }

    fn rendered(spans: &[Span<'static>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    /// The compact-layout refactor dropped the reset time; it's the one number
    /// that answers "when can I use this again", so keep a test on it.
    #[test]
    fn window_shows_percent_and_reset_time() {
        let spans = mini_window_spans(Color::Reset, "5h", Some(0.67), Some("1h13m"));
        let text = rendered(&spans);
        assert!(text.contains("5h"), "{text}");
        assert!(text.contains("67%"), "{text}");
        assert!(text.contains("resets 1h13m"), "{text}");
    }

    #[test]
    fn window_without_a_reset_time_omits_the_suffix() {
        let text = rendered(&mini_window_spans(Color::Reset, "7d", Some(0.5), None));
        assert!(!text.contains("resets"), "{text}");
    }

    #[test]
    fn status_glyph_reflects_remaining() {
        assert_eq!(status_glyph(Some(0.9)), "✓");
        assert_eq!(status_glyph(Some(0.3)), "⚠");
        assert_eq!(status_glyph(Some(0.1)), "✗");
        assert_eq!(status_glyph(None), "·");
    }
}
