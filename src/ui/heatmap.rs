//! Week-aggregated contribution heatmap.
//!
//! One column per ISO week (Mon–Sun), intensity = sum of that week's daily
//! metrics. Fills the area width by growing cell width; optional vertical
//! bars when height allows.

use super::util::month_abbr;
use crate::state::{CompactDate, DayTotals};
use chrono::{Datelike, Utc};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use std::collections::BTreeMap;

/// ~1 year of weeks; beyond this we widen cells instead of adding empty years.
const TARGET_WEEKS: u16 = 53;

/// Preferred height: month labels + 4 bar rows + legend.
pub const HEATMAP_FULL_HEIGHT: u16 = 6;

/// Build a week-aggregated heatmap ending this week, tinted with `accent`.
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

/// `(gutter, weeks, base_cell_w, extra)` — first `extra` cols are base+1 wide.
fn layout_grid(area_width: u16) -> (u16, u16, u16, u16) {
    // No weekday gutter for weekly view; keep a small left pad for legend align.
    let gutter = if area_width >= 8 { 1u16 } else { 0 };
    let avail = area_width.saturating_sub(gutter).max(1);
    if avail <= TARGET_WEEKS {
        return (gutter, avail, 1, 0);
    }
    let weeks = TARGET_WEEKS;
    let base = avail / weeks;
    let extra = avail % weeks;
    (gutter, weeks, base.max(1), extra)
}

fn col_width(base: u16, extra: u16, col: usize) -> u16 {
    if (col as u16) < extra { base + 1 } else { base }
}

fn col_x(gutter: u16, base: u16, extra: u16, col: usize) -> u16 {
    let col = col as u16;
    if col <= extra {
        gutter + col * (base + 1)
    } else {
        gutter + extra * (base + 1) + (col - extra) * base
    }
}

/// Sum metrics for Mon–Sun week starting at `week_start` (must be a Monday).
fn week_metric(daily: &BTreeMap<CompactDate, DayTotals>, week_start: CompactDate) -> u64 {
    let mut total = 0u64;
    let mut d = week_start;
    for _ in 0..7 {
        if let Some(day) = daily.get(&d) {
            total = total.saturating_add(day_metric(day));
        }
        d = add_one_day(d).unwrap_or(d);
    }
    total
}

/// Monday of the calendar week containing `day`.
fn monday_of(day: CompactDate) -> CompactDate {
    let wd = day.weekday_mon0() as i64;
    day.checked_sub_days(wd)
        .unwrap_or_else(|| CompactDate::new(1970, 1, 5)) // 1970-01-05 was a Monday
}

impl Widget for Heatmap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let levels = accent_levels(self.accent);
        let has_month = area.height >= 3;
        let has_legend = area.height >= 3;
        let grid_top = area.y + u16::from(has_month);
        let grid_bottom = if has_legend {
            area.y + area.height - 1
        } else {
            area.y + area.height
        };
        let bar_h = grid_bottom.saturating_sub(grid_top).max(1) as usize;

        let (gutter, weeks_u, base_w, extra) = layout_grid(area.width);
        let weeks = weeks_u as usize;

        let today = CompactDate::from_datetime(Utc::now());
        let this_monday = monday_of(today);
        // Oldest week Monday: this_monday - (weeks-1)*7
        let start_monday = this_monday
            .checked_sub_days(((weeks.saturating_sub(1)) * 7) as i64)
            .unwrap_or_else(|| CompactDate::new(1970, 1, 5));

        let mut week_starts = Vec::with_capacity(weeks);
        let mut week_metrics = Vec::with_capacity(weeks);
        let mut d = start_monday;
        for _ in 0..weeks {
            week_starts.push(d);
            week_metrics.push(week_metric(self.daily, d));
            d = add_days(d, 7).unwrap_or(d);
        }

        let max = week_metrics.iter().copied().max().unwrap_or(0);

        // Month labels above weeks that contain the 1st of a month (or col 0).
        if has_month {
            let mut next_free_x = area.x + gutter;
            for (col, &ws) in week_starts.iter().enumerate() {
                let mut month_to_label: Option<u8> = None;
                let mut day = ws;
                for _ in 0..7 {
                    if day.day() == 1 {
                        month_to_label = Some(day.month());
                        break;
                    }
                    day = add_one_day(day).unwrap_or(day);
                }
                if month_to_label.is_none() && col == 0 {
                    month_to_label = Some(ws.month());
                }
                let Some(month) = month_to_label else {
                    continue;
                };
                let label = month_abbr(month);
                let x = area.x + col_x(gutter, base_w, extra, col);
                if x < next_free_x || x + 3 > area.x + area.width {
                    continue;
                }
                put_str(buf, x, area.y, label, Style::default().fg(Color::DarkGray));
                next_free_x = x + 4;
            }
        }

        // Vertical bars: fill bottom `level/4 * bar_h` rows.
        for (col, &metric) in week_metrics.iter().enumerate() {
            let level = intensity(metric, max);
            let filled = if level == 0 {
                0
            } else {
                // level 1..4 → at least 1 row, up to bar_h
                ((level * bar_h) + 3) / 4
            };
            let x0 = area.x + col_x(gutter, base_w, extra, col);
            let cw = col_width(base_w, extra, col);
            for row in 0..bar_h {
                // row 0 is top of bar area; fill from bottom
                let from_bottom = bar_h - 1 - row;
                let y = grid_top + row as u16;
                if y >= grid_bottom {
                    break;
                }
                let on = from_bottom < filled;
                for dx in 0..cw {
                    let x = x0 + dx;
                    if x >= area.x + area.width {
                        break;
                    }
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        if on {
                            cell.set_symbol("■");
                            cell.set_style(Style::default().fg(levels[level.max(1)]));
                        } else if bar_h == 1 {
                            // Single-row mode: empty weeks as dots
                            if dx == 0 {
                                cell.set_symbol("·");
                            } else {
                                cell.set_symbol(" ");
                            }
                            cell.set_style(Style::default().fg(Color::DarkGray));
                        } else {
                            cell.set_symbol(" ");
                        }
                    }
                }
            }
        }

        if has_legend {
            let y = area.y + area.height - 1;
            let mut x = area.x + gutter;
            put_str(buf, x, y, "Less ", Style::default().fg(Color::DarkGray));
            x += 5;
            for level in 1..=4usize {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol("■");
                    cell.set_style(Style::default().fg(levels[level]));
                }
                x += 1;
            }
            put_str(
                buf,
                x,
                y,
                " More · per week",
                Style::default().fg(Color::DarkGray),
            );
        }
    }
}

/// Compact 1-row weekly strip for short terminals.
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
        let this_monday = monday_of(today);
        let mut metrics = Vec::with_capacity(n);
        for i in (0..n).rev() {
            let start = this_monday
                .checked_sub_days((i * 7) as i64)
                .unwrap_or_else(|| CompactDate::new(1970, 1, 5));
            metrics.push(week_metric(self.daily, start));
        }
        let max = metrics.iter().copied().max().unwrap_or(0);
        for (i, metric) in metrics.into_iter().enumerate() {
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

fn add_one_day(d: CompactDate) -> Option<CompactDate> {
    add_days(d, 1)
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
    fn layout_fills_wide_terminal() {
        let (gutter, weeks, base, extra) = layout_grid(200);
        assert_eq!(gutter, 1);
        assert_eq!(weeks, TARGET_WEEKS);
        assert!(base >= 3, "wide terminal should widen cells, got {base}");
        assert_eq!(weeks * base + extra, 199, "grid must span full avail width");
    }

    #[test]
    fn layout_narrow_uses_unit_cells() {
        let (gutter, weeks, base, extra) = layout_grid(40);
        assert_eq!(gutter, 1);
        assert_eq!(base, 1);
        assert_eq!(extra, 0);
        assert_eq!(weeks, 39);
    }

    #[test]
    fn week_metric_sums_seven_days() {
        let mut daily = BTreeMap::new();
        // 2026-08-03 is a Monday
        let mon = CompactDate::new(2026, 8, 3);
        for i in 0..7 {
            let d = add_days(mon, i).unwrap();
            daily.insert(
                d,
                DayTotals {
                    cost_usd: 1.0,
                    tokens: 0,
                    calls: 1,
                },
            );
        }
        // 1.0 * 1e6 * 7
        assert_eq!(week_metric(&daily, mon), 7_000_000);
    }

    #[test]
    fn monday_of_aligns() {
        // 2026-08-07 is Friday → Monday is 2026-08-03
        let fri = CompactDate::new(2026, 8, 7);
        assert_eq!(monday_of(fri), CompactDate::new(2026, 8, 3));
        assert_eq!(monday_of(CompactDate::new(2026, 8, 3)).weekday_mon0(), 0);
    }
}
