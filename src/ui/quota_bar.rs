use crate::quota::QuotaInfo;
use crate::state::Platform;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

const BAR_WIDTH: usize = 6;

/// Number of filled cells for a remaining fraction across `width` cells.
fn filled_cells(remaining: f64, width: usize) -> usize {
    (remaining.clamp(0.0, 1.0) * width as f64).round() as usize
}

/// Status glyph for a remaining fraction.
fn status_glyph(remaining: Option<f64>) -> &'static str {
    match remaining {
        Some(r) if r >= 0.5 => "✓",
        Some(r) if r >= 0.2 => "⚠",
        Some(_) => "✗",
        None => "·",
    }
}

/// Build one compact quota-window segment: `✓ 5h ▓▓▓▓░░  82% resets 2h30m`.
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

/// How many terminal rows the live quota panel needs for this payload.
pub fn quota_panel_height(quota: Option<&QuotaInfo>) -> u16 {
    match quota {
        Some(q) if !q.windows.is_empty() => {
            let meta = u16::from(q.plan.is_some() || q.org.is_some() || q.live_summary.is_some());
            q.windows.len() as u16 + meta
        }
        _ => 1,
    }
}

/// Live (network) quota: windows row + optional plan/summary row.
pub fn quota_panel(platform: Platform, quota: Option<&QuotaInfo>) -> Paragraph<'static> {
    let accent = platform.primary_color();
    let dim = Style::default().fg(Color::DarkGray);
    let accent_bold = Style::default().fg(accent).add_modifier(Modifier::BOLD);

    let lines: Vec<Line> = match quota {
        None => vec![Line::from(Span::styled(" live · loading…", dim))],
        Some(q) if q.error.is_some() => vec![Line::from(Span::styled(
            format!(" live · ✗ {}", q.error.as_ref().unwrap().display()),
            dim,
        ))],
        Some(q) if q.windows.is_empty() => {
            let mut spans = vec![Span::styled(" live · ", dim)];
            if let Some(plan) = q.plan.as_deref() {
                spans.push(Span::styled(plan.to_string(), accent_bold));
                spans.push(Span::raw(" · "));
            }
            if q.email.is_some() {
                spans.push(Span::styled("signed in", Style::default().fg(accent)));
            } else {
                spans.push(Span::styled("no quota data", dim));
            }
            vec![Line::from(spans)]
        }
        Some(q) => {
            let mut lines = Vec::new();
            // One window per row prevents later model-scoped limits from
            // being silently clipped at ordinary terminal widths.
            for (i, w) in q.windows.iter().enumerate() {
                let mut spans = vec![Span::styled(if i == 0 { " live " } else { "      " }, dim)];
                spans.extend(mini_window_spans(
                    accent,
                    &w.label,
                    w.remaining_percent,
                    w.reset_in.as_deref(),
                ));
                lines.push(Line::from(spans));
            }

            // Final row: plan / org / credits when present.
            if q.plan.is_some() || q.org.is_some() || q.live_summary.is_some() {
                let mut meta = vec![Span::styled("      ", dim)];
                let mut first = true;
                let mut push_sep = |meta: &mut Vec<Span<'static>>| {
                    if !first {
                        meta.push(Span::styled(" · ", dim));
                    }
                    first = false;
                };
                if let Some(plan) = q.plan.as_deref() {
                    push_sep(&mut meta);
                    meta.push(Span::styled(plan.to_string(), accent_bold));
                }
                if let Some(org) = q.org.as_deref() {
                    push_sep(&mut meta);
                    meta.push(Span::styled(org.to_string(), dim));
                }
                if let Some(sum) = q.live_summary.as_deref() {
                    push_sep(&mut meta);
                    meta.push(Span::styled(sum.to_string(), dim));
                }
                lines.push(Line::from(meta));
            }
            lines
        }
    };

    Paragraph::new(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filled_cells_rounds_to_nearest() {
        assert_eq!(filled_cells(0.5, 6), 3);
        assert_eq!(filled_cells(0.0, 6), 0);
        assert_eq!(filled_cells(1.0, 6), 6);
    }

    #[test]
    fn filled_cells_clamps_out_of_range() {
        assert_eq!(filled_cells(-1.0, 6), 0);
        assert_eq!(filled_cells(2.0, 6), 6);
    }

    #[test]
    fn status_glyph_reflects_remaining() {
        assert_eq!(status_glyph(Some(0.8)), "✓");
        assert_eq!(status_glyph(Some(0.3)), "⚠");
        assert_eq!(status_glyph(Some(0.1)), "✗");
        assert_eq!(status_glyph(None), "·");
    }

    #[test]
    fn window_shows_percent_and_reset_time() {
        let spans = mini_window_spans(Color::Reset, "5h", Some(0.67), Some("1h13m"));
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("67%"));
        assert!(text.contains("resets 1h13m"));
    }

    #[test]
    fn window_without_a_reset_time_omits_the_suffix() {
        let text: String = mini_window_spans(Color::Reset, "7d", Some(0.5), None)
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(!text.contains("resets"));
        assert!(text.contains("50%"));
    }

    #[test]
    fn extra_windows_get_their_own_rows() {
        use std::time::Instant;
        let q = QuotaInfo {
            tool_name: "Claude".into(),
            email: None,
            account_id: None,
            plan: Some("team".into()),
            org: None,
            windows: ["5h", "7d", "Opus"]
                .into_iter()
                .map(|label| crate::quota::QuotaWindow {
                    label: label.into(),
                    remaining_percent: Some(0.5),
                    resets_at: None,
                    reset_in: None,
                })
                .collect(),
            live_summary: None,
            fetched_at: Instant::now(),
            error: None,
        };
        assert_eq!(quota_panel_height(Some(&q)), 4);
        assert_eq!(quota_panel_height(None), 1);

        use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
        let area = Rect::new(0, 0, 60, 4);
        let mut buffer = Buffer::empty(area);
        quota_panel(Platform::ClaudeCode, Some(&q)).render(area, &mut buffer);
        let rendered: String = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter_map(|pos| buffer.cell(pos).map(|cell| cell.symbol()))
            .collect();
        assert!(rendered.contains("5h"));
        assert!(rendered.contains("7d"));
        assert!(rendered.contains("Opus"));
    }
}
