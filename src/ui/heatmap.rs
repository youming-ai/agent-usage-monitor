//! Day contribution heatmap with **weekday columns** (Mon…Sun) and **one row
//! per week** — no month labels. Intensity is per-day; fills the area by
//! growing cell width.

use crate::state::{CompactDate, DayTotals};
use chrono::{Datelike, Utc};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use std::collections::BTreeMap;

/// ~1 year of weeks as rows when height allows; otherwise as many as fit.
const TARGET_WEEKS: u16 = 53;

/// Preferred height: weekday header + many week rows + legend.
/// Caller may give less; we adapt.
pub const HEATMAP_FULL_HEIGHT: u16 = 16;

const WEEKDAY_LABELS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// Contribution heatmap: columns = weekdays, rows = weeks (oldest → newest).
pub fn contribution_heatmap<'a>(
    daily: &'a BTreeMap<CompactDate, DayTotals>,
    accent: Color,
) -> Heatmap<'a> {
    Heatmap { daily, accent }
}

pub struct Heatmap<'a> {
    daily: &'a BTreeMap<CompactDate, DayTotals>,
    accent: Color,
}

/// How many week-rows fit, and cell width for the 7 weekday columns.
/// Returns `(gutter, weeks, cell_w, pad_left)` where pad centers the 7 cols.
fn layout(area: Rect) -> (u16, usize, u16, u16) {
    let has_header = area.height >= 2;
    let has_legend = area.height >= 3;
    let rows_avail = area
        .height
        .saturating_sub(u16::from(has_header) + u16::from(has_legend))
        .max(1) as usize;

    let weeks = (TARGET_WEEKS as usize).min(rows_avail).max(1);

    // 7 weekday columns fill the width (optional 1-col left pad).
    let gutter = 0u16;
    let avail = area.width.saturating_sub(gutter).max(7);
    let cell_w = (avail / 7).max(1);
    let used = cell_w * 7;
    let pad_left = gutter + (avail - used) / 2;
    (gutter, weeks, cell_w, pad_left)
}

impl Widget for Heatmap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let levels = accent_levels(self.accent);
        let has_header = area.height >= 2;
        let has_legend = area.height >= 3;
        let (_, weeks, cell_w, pad_left) = layout(area);

        let grid_top = area.y + u16::from(has_header);
        let grid_h = if has_legend {
            area.height.saturating_sub(1 + u16::from(has_header))
        } else {
            area.height.saturating_sub(u16::from(has_header))
        }
        .min(weeks as u16) as usize;

        if grid_h == 0 {
            return;
        }

        let today = CompactDate::from_datetime(Utc::now());
        let this_monday = monday_of(today);
        // Oldest week at top: this_monday - (grid_h-1)*7
        let start_monday = this_monday
            .checked_sub_days(((grid_h.saturating_sub(1)) * 7) as i64)
            .unwrap_or_else(|| CompactDate::new(1970, 1, 5));

        // Build grid_h weeks × 7 days metrics.
        let mut metrics = vec![0u64; grid_h * 7];
        let mut d = start_monday;
        for row in 0..grid_h {
            for col in 0..7 {
                metrics[row * 7 + col] = self.daily.get(&d).map(day_metric).unwrap_or(0);
                d = add_days(d, 1).unwrap_or(d);
            }
        }
        let max = metrics.iter().copied().max().unwrap_or(0);

        // Header: Mon Tue Wed Thu Fri Sat Sun
        if has_header {
            for (col, label) in WEEKDAY_LABELS.iter().enumerate() {
                let x = area.x + pad_left + (col as u16) * cell_w;
                // Center short label in the cell when wide enough.
                let label_x = if cell_w >= 3 {
                    x + (cell_w.saturating_sub(3)) / 2
                } else {
                    x
                };
                if label_x + 3 <= area.x + area.width {
                    put_str(
                        buf,
                        label_x,
                        area.y,
                        label,
                        Style::default().fg(Color::DarkGray),
                    );
                } else if cell_w >= 1 {
                    // Single letter fallback
                    let ch = &label[..1];
                    put_str(buf, x, area.y, ch, Style::default().fg(Color::DarkGray));
                }
            }
        }

        // Body: row = week, col = weekday
        for row in 0..grid_h {
            let y = grid_top + row as u16;
            if y >= area.y + area.height {
                break;
            }
            for col in 0..7 {
                let metric = metrics[row * 7 + col];
                let level = intensity(metric, max);
                let x0 = area.x + pad_left + (col as u16) * cell_w;
                for dx in 0..cell_w {
                    let x = x0 + dx;
                    if x >= area.x + area.width {
                        break;
                    }
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        if level == 0 {
                            if dx == 0 {
                                cell.set_symbol("·");
                            } else {
                                cell.set_symbol(" ");
                            }
                            cell.set_style(Style::default().fg(Color::DarkGray));
                        } else {
                            cell.set_symbol("■");
                            cell.set_style(Style::default().fg(levels[level]));
                        }
                    }
                }
            }
        }

        // Legend
        if has_legend {
            let y = area.y + area.height - 1;
            let mut x = area.x + pad_left;
            put_str(buf, x, y, "Less ", Style::default().fg(Color::DarkGray));
            x += 5;
            for level in 1..=4usize {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol("■");
                    cell.set_style(Style::default().fg(levels[level]));
                }
                x += 1;
            }
            put_str(buf, x, y, " More", Style::default().fg(Color::DarkGray));
        }
    }
}

/// Compact strip: last N days left→right (short terminals).
pub fn contribution_strip<'a>(
    daily: &'a BTreeMap<CompactDate, DayTotals>,
    accent: Color,
) -> StripHeatmap<'a> {
    StripHeatmap { daily, accent }
}

pub struct StripHeatmap<'a> {
    daily: &'a BTreeMap<CompactDate, DayTotals>,
    accent: Color,
}

impl Widget for StripHeatmap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let levels = accent_levels(self.accent);
        let n = area.width as usize;
        let today = CompactDate::from_datetime(Utc::now());
        let mut days = Vec::with_capacity(n);
        for i in (0..n).rev() {
            let d = today
                .checked_sub_days(i as i64)
                .unwrap_or_else(|| CompactDate::new(1970, 1, 1));
            days.push(self.daily.get(&d).map(day_metric).unwrap_or(0));
        }
        let max = days.iter().copied().max().unwrap_or(0);
        for (i, metric) in days.into_iter().enumerate() {
            let level = intensity(metric, max);
            let x = area.x + i as u16;
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                if level == 0 {
                    cell.set_symbol("·");
                    cell.set_style(Style::default().fg(Color::DarkGray));
                } else {
                    cell.set_symbol("■");
                    cell.set_style(Style::default().fg(levels[level]));
                }
            }
        }
    }
}

pub fn day_metric(d: &DayTotals) -> u64 {
    if d.cost_usd > 0.0 {
        (d.cost_usd * 1_000_000.0) as u64
    } else {
        d.tokens
    }
}

fn intensity(metric: u64, max: u64) -> usize {
    if metric == 0 || max == 0 {
        return 0;
    }
    let ratio = metric as f64 / max as f64;
    if ratio > 0.75 {
        4
    } else if ratio > 0.5 {
        3
    } else if ratio > 0.25 {
        2
    } else {
        1
    }
}

fn accent_levels(accent: Color) -> [Color; 5] {
    let (r, g, b) = match accent {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (217, 119, 87),
    };
    [
        Color::Rgb(40, 40, 44),
        mix_rgb(r, g, b, 0.22),
        mix_rgb(r, g, b, 0.42),
        mix_rgb(r, g, b, 0.68),
        Color::Rgb(r, g, b),
    ]
}

fn mix_rgb(r: u8, g: u8, b: u8, t: f64) -> Color {
    let br = 22.0;
    let bg = 27.0;
    let bb = 34.0;
    Color::Rgb(
        (br + (r as f64 - br) * t).round() as u8,
        (bg + (g as f64 - bg) * t).round() as u8,
        (bb + (b as f64 - bb) * t).round() as u8,
    )
}

fn put_str(buf: &mut Buffer, x: u16, y: u16, s: &str, style: Style) {
    for (i, ch) in s.chars().enumerate() {
        let cx = x + i as u16;
        if let Some(cell) = buf.cell_mut((cx, y)) {
            cell.set_symbol(&ch.to_string());
            cell.set_style(style);
        }
    }
}

fn monday_of(day: CompactDate) -> CompactDate {
    let wd = day.weekday_mon0() as i64;
    day.checked_sub_days(wd)
        .unwrap_or_else(|| CompactDate::new(1970, 1, 5))
}

fn add_days(d: CompactDate, days: i64) -> Option<CompactDate> {
    use chrono::NaiveDate;
    let date = NaiveDate::from_ymd_opt(d.year() as i32, d.month() as u32, d.day() as u32)?;
    let next = date.checked_add_signed(chrono::Duration::days(days))?;
    Some(CompactDate::new(
        next.year() as u16,
        next.month() as u8,
        next.day() as u8,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intensity_scales_with_max() {
        assert_eq!(intensity(0, 100), 0);
        assert_eq!(intensity(10, 100), 1);
        assert_eq!(intensity(30, 100), 2);
        assert_eq!(intensity(60, 100), 3);
        assert_eq!(intensity(90, 100), 4);
    }

    #[test]
    fn layout_seven_columns_fill_width() {
        let area = Rect::new(0, 0, 70, 20);
        let (_, weeks, cell_w, pad) = layout(area);
        assert!(weeks >= 1);
        assert_eq!(cell_w * 7 + pad * 2 <= 70 || cell_w * 7 <= 70, true);
        assert_eq!(cell_w, 10); // 70/7
    }

    #[test]
    fn monday_of_aligns() {
        // 2026-08-07 Friday → Monday 2026-08-03
        assert_eq!(
            monday_of(CompactDate::new(2026, 8, 7)),
            CompactDate::new(2026, 8, 3)
        );
    }
}
