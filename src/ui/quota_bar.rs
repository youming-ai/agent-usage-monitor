use crate::quota::QuotaInfo;
use crate::state::Tab;
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

const BAR_WIDTH: usize = 12;

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

/// Build one quota-window line: `✓ 5h ▓▓▓▓▓▓▓▓░░ 82%  resets 2h30m`.
fn window_line(
    accent: Color,
    label: &str,
    remaining: Option<f64>,
    reset: Option<&str>,
) -> Line<'static> {
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

    if let Some(r) = reset {
        spans.push(Span::styled(
            format!("  resets {r}"),
            Style::default().fg(Color::DarkGray),
        ));
    }

    Line::from(spans)
}

/// Render the quota windows as one bar per line (replaces the old single
/// gauge). The active platform color is the single accent used for the bars.
pub fn quota_panel(active_tab: Tab, quota: Option<&QuotaInfo>) -> Paragraph<'static> {
    let accent = active_tab.primary_color();

    let lines: Vec<Line> = match quota {
        None => vec![Line::from(Span::styled(
            " loading…",
            Style::default().fg(Color::DarkGray),
        ))],
        Some(q) if q.error.is_some() => vec![Line::from(Span::styled(
            format!(" ✗ {}", q.error.as_ref().unwrap().display()),
            Style::default().fg(Color::DarkGray),
        ))],
        Some(q) if q.windows.is_empty() => vec![Line::from(Span::styled(
            " no quota data",
            Style::default().fg(Color::DarkGray),
        ))],
        Some(q) => q
            .windows
            .iter()
            .map(|w| window_line(accent, &w.label, w.remaining_percent, w.reset_in.as_deref()))
            .collect(),
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

    #[test]
    fn status_glyph_reflects_remaining() {
        assert_eq!(status_glyph(Some(0.9)), "✓");
        assert_eq!(status_glyph(Some(0.3)), "⚠");
        assert_eq!(status_glyph(Some(0.1)), "✗");
        assert_eq!(status_glyph(None), "·");
    }
}
