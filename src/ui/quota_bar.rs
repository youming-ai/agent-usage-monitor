use crate::quota::QuotaInfo;
use crate::state::Platform;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
const BAR_WIDTH: usize = 6;
const BAR_MAX_WIDTH: usize = 80;
const BAR_MIN_WIDTH: usize = 10;

/// Compute a bar width that stretches to fill `available_width` while
/// leaving room for the fixed glyph/label/pct/reset text. `reset_len` is the
/// longest reset string in the panel so every row gets the same bar width and
/// the right edges stay aligned.
fn bar_width_for(available_width: u16, reset_len: Option<usize>) -> usize {
    if available_width == 0 {
        return BAR_WIDTH;
    }
    // fixed overhead per window row:
    //   prefix " live "/"      " (6) + glyph " x " (3) + label "xxx " (4)
    //   + pct "  82%" (5) + optional " resets 2h30m" (8+len) + bar + 2 breathing cols.
    let reset_len = reset_len.map(|l| 8 + l).unwrap_or(0);
    let fixed = 6 + 3 + 4 + 5 + reset_len + 2;
    let avail = available_width as usize;
    let w = avail.saturating_sub(fixed);
    w.clamp(BAR_MIN_WIDTH, BAR_MAX_WIDTH).min(avail)
}

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

/// Build one compact quota-window segment: `▓▓▓▓░░  ✓ 5h  82% resets 2h30m`.
/// `bar_width` is computed from the available panel width so the bar stretches
/// to fill the row. The bar is leading and left-aligned; the whole
/// `✓ 5h 96% resets…` block trails on the right.
fn mini_window_spans(
    accent: Color,
    label: &str,
    remaining: Option<f64>,
    reset_in: Option<&str>,
    bar_width: usize,
) -> Vec<Span<'static>> {
    let bw = bar_width.max(1);
    let filled = remaining.map(|r| filled_cells(r, bw)).unwrap_or(0);
    let empty = bw - filled;

    let pct = remaining
        .map(|r| format!("{}%", (r * 100.0).round() as u64))
        .unwrap_or_else(|| "--".to_string());

    let mut spans = vec![
        Span::styled("▓".repeat(filled), Style::default().fg(accent)),
        Span::styled("░".repeat(empty), Style::default().fg(Color::DarkGray)),
        Span::raw(format!("  {} {label:<3} {pct:>4}", status_glyph(remaining))),
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
/// `width` is the panel's available width and controls how long the bar
/// stretches — wider terminals get a longer bar that fills the row.
pub fn quota_panel(
    platform: Platform,
    quota: Option<&QuotaInfo>,
    width: u16,
) -> Paragraph<'static> {
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
            // One bar width for the whole panel: per-window reset strings have
            // different lengths, which would make the bars misalign on the right.
            let bw = bar_width_for(
                width,
                q.windows
                    .iter()
                    .filter_map(|w| w.reset_in.as_deref())
                    .map(str::len)
                    .max(),
            );
            let mut lines = Vec::new();
            for (i, w) in q.windows.iter().enumerate() {
                let mut spans = vec![Span::styled(if i == 0 { " live " } else { "      " }, dim)];
                spans.extend(mini_window_spans(
                    accent,
                    &w.label,
                    w.remaining_percent,
                    w.reset_in.as_deref(),
                    bw,
                ));
                lines.push(Line::from(spans));
            }
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
        let spans = mini_window_spans(Color::Reset, "5h", Some(0.67), Some("1h13m"), 12);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("67%"));
        assert!(text.contains("resets 1h13m"));
    }

    #[test]
    fn window_without_a_reset_time_omits_the_suffix() {
        let text: String = mini_window_spans(Color::Reset, "7d", Some(0.5), None, 12)
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(!text.contains("resets"));
        assert!(text.contains("50%"));
    }

    #[test]
    fn bars_align_across_windows_with_different_reset_lengths() {
        use std::time::Instant;
        let q = QuotaInfo {
            tool_name: "Codex".into(),
            email: None,
            account_id: None,
            plan: None,
            org: None,
            windows: [
                ("5h", Some(1.0), Some("4h58m")),
                ("7d", Some(0.65), Some("6d0h")),
            ]
            .into_iter()
            .map(|(label, remaining, reset)| crate::quota::QuotaWindow {
                label: label.into(),
                remaining_percent: remaining,
                resets_at: None,
                reset_in: reset.map(str::to_string),
            })
            .collect(),
            live_summary: None,
            fetched_at: Instant::now(),
            error: None,
        };
        let area = ratatui::layout::Rect::new(0, 0, 60, 2);
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        use ratatui::widgets::Widget;
        quota_panel(Platform::Codex, Some(&q), area.width).render(area, &mut buffer);
        // Right edge of each bar row (last '░' or '▓') must be the same column.
        let bar_end = |y: u16| {
            (0..area.width)
                .filter(|&x| {
                    matches!(
                        buffer
                            .cell((x, y))
                            .map(|c| c.symbol().to_string())
                            .as_deref(),
                        Some("▓" | "░")
                    )
                })
                .max()
                .unwrap()
        };
        assert_eq!(bar_end(0), bar_end(1));
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
        quota_panel(Platform::ClaudeCode, Some(&q), area.width).render(area, &mut buffer);
        let rendered: String = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter_map(|pos| buffer.cell(pos).map(|cell| cell.symbol()))
            .collect();
        assert!(rendered.contains("5h"));
        assert!(rendered.contains("7d"));
        assert!(rendered.contains("Opus"));
    }
}
