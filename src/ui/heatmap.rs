//! Current-month contribution heatmap with one cell for every calendar day.
//! Weekday columns keep the month in a familiar calendar layout; days outside
//! the current month are left blank.

use crate::state::{CompactDate, DayTotals};
use chrono::{Datelike, Utc};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use std::collections::BTreeMap;

/// Preferred height: weekday header + the six rows a month can occupy + legend.
pub const HEATMAP_FULL_HEIGHT: u16 = 8;

const WEEKDAY_LABELS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// Contribution heatmap: columns = weekdays, rows = the calendar weeks that
/// contain the current month's days.
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

/// How the seven weekday columns divide the width. The first `extra` columns
/// are one cell wider, so every terminal column is used even when the width is
/// not divisible by seven.
fn layout(area: Rect) -> (u16, u16) {
    let base_w = (area.width / 7).max(1);
    let extra = area.width % 7;
    (base_w, extra)
}

fn col_width(base_w: u16, extra: u16, col: usize) -> u16 {
    base_w + u16::from((col as u16) < extra)
}

fn col_x(base_w: u16, extra: u16, col: usize) -> u16 {
    let col = col as u16;
    col * base_w + col.min(extra)
}

impl Widget for Heatmap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let levels = accent_levels(self.accent);
        let has_header = area.height >= 2;
        let has_legend = area.height >= 3;
        let (base_w, extra) = layout(area);

        let today = CompactDate::from_datetime(Utc::now());
        let month = month_grid(today);
        let rows_avail = area
            .height
            .saturating_sub(u16::from(has_header) + u16::from(has_legend))
            .max(1) as usize;
        // A month can span at most six calendar weeks. On a short terminal,
        // keep the most recent rows visible so today's activity is not lost.
        let grid_h = month.weeks.min(rows_avail);
        let first_row = month.weeks.saturating_sub(grid_h);

        let grid_top = area.y + u16::from(has_header);
        if grid_h == 0 {
            return;
        }

        let start = add_days(month.grid_start, (first_row * 7) as i64).unwrap_or(month.grid_start);

        // Build the visible calendar rows. `None` marks the leading/trailing
        // cells that belong to an adjacent month and must remain blank.
        let mut metrics = vec![None; grid_h * 7];
        let mut d = start;
        for row in 0..grid_h {
            for col in 0..7 {
                if d.year() == today.year() && d.month() == today.month() {
                    metrics[row * 7 + col] = Some(self.daily.get(&d).map(day_metric).unwrap_or(0));
                }
                d = add_days(d, 1).unwrap_or(d);
            }
        }
        let max = metrics.iter().flatten().copied().max().unwrap_or(0);

        // Header: Mon Tue Wed Thu Fri Sat Sun.
        if has_header {
            for (col, label) in WEEKDAY_LABELS.iter().enumerate() {
                let x = area.x + col_x(base_w, extra, col);
                let cell_w = col_width(base_w, extra, col);
                // Center short label in the cell when wide enough.
                if cell_w >= 3 {
                    let label_x = x + (cell_w - 3) / 2;
                    put_str(
                        buf,
                        label_x,
                        area.y,
                        label,
                        Style::default().fg(Color::DarkGray),
                    );
                } else {
                    // Narrow cells always use one letter; writing the full
                    // label here would overwrite adjacent weekday columns.
                    let ch = &label[..1];
                    put_str(buf, x, area.y, ch, Style::default().fg(Color::DarkGray));
                }
            }
        }

        // Body: row = calendar week, col = weekday. Only dates in the current
        // month receive a cell; adjacent-month padding stays empty.
        for row in 0..grid_h {
            let y = grid_top + row as u16;
            if y >= area.y + area.height {
                break;
            }
            for col in 0..7 {
                let x0 = area.x + col_x(base_w, extra, col);
                let cell_w = col_width(base_w, extra, col);
                let Some(metric) = metrics[row * 7 + col] else {
                    // Clear padding cells explicitly because ratatui reuses
                    // the frame buffer between draws (notably at month end).
                    for dx in 0..cell_w {
                        if let Some(cell) = buf.cell_mut((x0 + dx, y)) {
                            cell.set_symbol(" ");
                            cell.set_style(Style::default());
                        }
                    }
                    continue;
                };
                let level = intensity(metric, max);
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

        // Legend.
        if has_legend {
            let y = area.y + area.height - 1;
            let mut x = area.x;
            put_str(buf, x, y, "Less ", Style::default().fg(Color::DarkGray));
            x += 5;
            for color in levels.iter().skip(1) {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol("■");
                    cell.set_style(Style::default().fg(*color));
                }
                x += 1;
            }
            put_str(buf, x, y, " More", Style::default().fg(Color::DarkGray));
        }
    }
}

/// Compact strip of the current month's days for short terminals. If there
/// is not enough width for all days, keep the most recent visible days.
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
        let today = CompactDate::from_datetime(Utc::now());
        let month = month_grid(today);
        let days_in_month = month.last_day.day() as usize;
        let visible_days = days_in_month.min(area.width as usize);
        let start_day = days_in_month.saturating_sub(visible_days);
        let days: Vec<u64> = (start_day..days_in_month)
            .map(|offset| {
                let d = add_days(month.first_day, offset as i64).unwrap_or(month.first_day);
                self.daily.get(&d).map(day_metric).unwrap_or(0)
            })
            .collect();
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
        for i in visible_days..area.width as usize {
            if let Some(cell) = buf.cell_mut((area.x + i as u16, area.y)) {
                cell.set_symbol(" ");
                cell.set_style(Style::default());
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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct MonthGrid {
    first_day: CompactDate,
    last_day: CompactDate,
    grid_start: CompactDate,
    weeks: usize,
}

fn month_grid(today: CompactDate) -> MonthGrid {
    let first_day = CompactDate::new(today.year(), today.month(), 1);
    let next_month = if today.month() == 12 {
        CompactDate::new(today.year().saturating_add(1), 1, 1)
    } else {
        CompactDate::new(today.year(), today.month() + 1, 1)
    };
    let last_day = next_month.checked_sub_days(1).unwrap_or(first_day);
    let grid_start = first_day
        .checked_sub_days(first_day.weekday_mon0() as i64)
        .unwrap_or(first_day);
    let trailing_days = 6 - last_day.weekday_mon0() as i64;
    let grid_end = add_days(last_day, trailing_days).unwrap_or(last_day);
    let weeks = days_between(grid_start, grid_end) as usize / 7 + 1;

    MonthGrid {
        first_day,
        last_day,
        grid_start,
        weeks,
    }
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

fn days_between(a: CompactDate, b: CompactDate) -> u64 {
    use chrono::NaiveDate;
    let da = NaiveDate::from_ymd_opt(a.year() as i32, a.month() as u32, a.day() as u32);
    let db = NaiveDate::from_ymd_opt(b.year() as i32, b.month() as u32, b.day() as u32);
    match (da, db) {
        (Some(a), Some(b)) => (b - a).num_days().unsigned_abs(),
        _ => 0,
    }
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
        let area = Rect::new(0, 0, 80, 20);
        let (base_w, extra) = layout(area);
        assert_eq!(base_w * 7 + extra, 80);
        assert_eq!(col_x(base_w, extra, 6) + col_width(base_w, extra, 6), 80);
    }

    #[test]
    fn narrow_weekday_labels_do_not_overlap() {
        let area = Rect::new(0, 0, 7, 3);
        let mut buffer = Buffer::empty(area);
        contribution_heatmap(&BTreeMap::new(), Color::Blue).render(area, &mut buffer);
        let header: String = (0..7)
            .filter_map(|x| buffer.cell((x, 0)).map(|cell| cell.symbol()))
            .collect();
        assert_eq!(header, "MTWTFSS");
    }

    #[test]
    fn month_grid_contains_every_day_and_only_current_month() {
        let grid = month_grid(CompactDate::new(2026, 8, 7));
        assert_eq!(grid.first_day, CompactDate::new(2026, 8, 1));
        assert_eq!(grid.last_day, CompactDate::new(2026, 8, 31));
        assert_eq!(grid.grid_start, CompactDate::new(2026, 7, 27));
        assert_eq!(grid.weeks, 6);

        let mut current = 0;
        let mut outside = 0;
        let mut day = grid.grid_start;
        for _ in 0..grid.weeks * 7 {
            if day.year() == 2026 && day.month() == 8 {
                current += 1;
            } else {
                outside += 1;
            }
            day = add_days(day, 1).unwrap();
        }
        assert_eq!(current, 31);
        assert_eq!(outside, 11);
    }
}
