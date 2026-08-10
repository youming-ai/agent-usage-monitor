//! GitHub-style contribution heatmap: one column per week, one row per weekday
//! (Sun…Sat), month labels on top. Cells are square, so the terminal width
//! decides how many weeks of history fit.

use crate::state::{CompactDate, DayTotals};
use chrono::{Datelike, Utc};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use std::collections::BTreeMap;

/// Preferred height: month labels + the seven weekday rows.
pub const HEATMAP_FULL_HEIGHT: u16 = 8;

/// Minimum height the grid needs (seven weekday rows, no month labels).
pub const HEATMAP_MIN_HEIGHT: u16 = 7;

/// Two terminal columns per cell — roughly square in a monospace font.
const CELL_W: usize = 2;
/// Cap on history so an ultra-wide terminal does not scroll back forever.
const MAX_WEEKS: usize = 105;
/// Width of the Mon/Wed/Fri gutter on the left. `pub` so `ui::mod` can keep its
/// grid-vs-strip width gate in lockstep with the render gate here.
pub const GUTTER: u16 = 4;
const DIM: Style = Style::new().fg(Color::DarkGray);
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
/// Row index (Sun = 0) → weekday label, GitHub-style.
const ROW_LABELS: [(usize, &str); 3] = [(1, "Mon"), (3, "Wed"), (5, "Fri")];

/// Contribution heatmap: columns = weeks (newest last), rows = weekdays.
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

impl Widget for Heatmap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < HEATMAP_MIN_HEIGHT || area.width <= GUTTER {
            return;
        }

        // ratatui reuses the frame buffer between draws; clear first so week
        // columns dropped on a resize leave nothing behind.
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(" ");
                    cell.set_style(Style::default());
                }
            }
        }

        let levels = accent_levels(self.accent);
        let has_header = area.height >= HEATMAP_FULL_HEIGHT;
        let grid_top = area.y + u16::from(has_header);
        let gutter = if area.width > GUTTER + 10 { GUTTER } else { 0 };
        let avail = (area.width - gutter) as usize;
        // Square cells of a fixed width; the width decides how many weeks of
        // history fit. An odd leftover column pads the gutter so the grid ends
        // flush with the right edge.
        let weeks = (avail / CELL_W).clamp(1, MAX_WEEKS);
        let pad = (avail - weeks * CELL_W).min(CELL_W - 1) as u16;
        let col_x = |col: usize| area.x + gutter + pad + (col * CELL_W) as u16;

        let today = CompactDate::from_datetime(Utc::now());
        // Last column is the week containing today; walk back whole weeks.
        let back = today.weekday_sun0() as i64 + ((weeks - 1) * 7) as i64;
        let start = add_days(today, -back).unwrap_or(today);

        // Column-major: each column is one week, rows are Sun…Sat. The rest of
        // the current week is drawn as empty days so the grid stays a
        // rectangle instead of jutting out on the last column.
        let mut metrics = vec![0u64; weeks * 7];
        let mut d = start;
        for slot in metrics.iter_mut() {
            *slot = self.daily.get(&d).map(day_metric).unwrap_or(0);
            d = add_days(d, 1).unwrap_or(d);
        }
        let max = metrics.iter().copied().max().unwrap_or(0);

        if has_header {
            let mut prev_month = None;
            let mut next_x = area.x;
            for col in 0..weeks {
                let sunday = add_days(start, (col * 7) as i64).unwrap_or(start);
                if prev_month == Some(sunday.month()) {
                    continue;
                }
                prev_month = Some(sunday.month());
                let x = col_x(col);
                if x >= next_x && x + 3 <= area.x + area.width {
                    put_str(buf, x, area.y, MONTHS[sunday.month() as usize - 1], DIM);
                    next_x = x + 4;
                }
            }
        }

        if gutter > 0 {
            for (row, label) in ROW_LABELS {
                put_str(buf, area.x, grid_top + row as u16, label, DIM);
            }
        }

        for col in 0..weeks {
            let x0 = col_x(col);
            for row in 0..7 {
                let y = grid_top + row as u16;
                let level = intensity(metrics[col * 7 + row], max);
                for dx in 0..CELL_W as u16 {
                    let Some(cell) = buf.cell_mut((x0 + dx, y)) else {
                        continue;
                    };
                    if level == 0 {
                        cell.set_symbol(if dx == 0 { "·" } else { " " });
                        cell.set_style(DIM);
                    } else {
                        cell.set_symbol(" ");
                        cell.set_style(Style::default().bg(levels[level]));
                    }
                }
            }
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
                    cell.set_style(DIM);
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

// ponytail: month_grid / MonthGrid / days_between / weekday_mon0 are now
// carried only by contribution_strip (the short-terminal fallback). If the
// strip is ever simplified, all four can go with it.
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

    fn dump(w: u16, h: u16, daily: &BTreeMap<CompactDate, DayTotals>) -> String {
        let area = Rect::new(0, 0, w, h);
        let mut buffer = Buffer::empty(area);
        contribution_heatmap(daily, Color::Blue).render(area, &mut buffer);
        let mut out = String::new();
        for y in 0..h {
            for x in 0..w {
                out.push_str(buffer.cell((x, y)).unwrap().symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn year_grid_has_weekday_and_month_labels() {
        let out = dump(80, 8, &BTreeMap::new());
        assert!(
            out.contains("Mon") && out.contains("Wed") && out.contains("Fri"),
            "{out}"
        );
        let today = CompactDate::from_datetime(Utc::now());
        assert!(out.contains(MONTHS[today.month() as usize - 1]), "{out}");
        // Seven weekday rows of cells, today's week in the last column.
        assert_eq!(out.lines().filter(|l| l.contains('·')).count(), 7);
    }

    #[test]
    fn today_sits_in_the_last_column_and_the_grid_stays_a_rectangle() {
        let today = CompactDate::from_datetime(Utc::now());
        let mut daily = BTreeMap::new();
        daily.insert(
            today,
            DayTotals {
                cost_usd: 1.0,
                tokens: 1,
                calls: 1,
            },
        );
        let area = Rect::new(0, 0, 100, 8);
        let mut buffer = Buffer::empty(area);
        contribution_heatmap(&daily, Color::Blue).render(area, &mut buffer);
        // Cells are CELL_W wide and flush right, so the last one starts here.
        let x = area.width - CELL_W as u16;
        for row in 0..7u16 {
            let cell = buffer.cell((x, 1 + row)).unwrap();
            if row == u16::from(today.weekday_sun0()) {
                assert_ne!(cell.style().bg, Some(Color::Reset), "today unpainted");
            } else {
                // Empty and not-yet-happened days alike keep the grid square.
                assert_eq!(cell.symbol(), "·", "row {row} of the last column");
            }
        }
    }

    #[test]
    fn week_columns_stretch_to_the_right_edge() {
        let area = Rect::new(0, 0, 100, 8);
        let mut buffer = Buffer::empty(area);
        contribution_heatmap(&BTreeMap::new(), Color::Blue).render(area, &mut buffer);
        // Painted cells carry a style; the cleared background does not. The
        // rightmost column of every weekday row must belong to a week cell.
        for row in 0..7u16 {
            assert_eq!(buffer.cell((99, 1 + row)).unwrap().style().fg, DIM.fg);
        }
    }

    #[test]
    fn too_short_area_renders_nothing() {
        let out = dump(80, 6, &BTreeMap::new());
        assert!(out.trim().is_empty());
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
