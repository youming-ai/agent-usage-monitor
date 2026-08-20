//! Token activity heatmap: one column per week, one row per weekday
//! (Mon…Sun), and month labels on top. Each week is one square plus a
//! one-column spacer, and the grid stretches to fill the terminal width.

use crate::state::{CompactDate, DayTotals};
use chrono::Utc;
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

/// Minimum horizontal footprint of a week: one square and one spacer.
const CELL_W: usize = 2;
/// Width of the weekday-label gutter on the left. `pub` so `ui::mod` can keep
/// its grid-vs-strip width gate in lockstep with the render gate here.
pub const GUTTER: u16 = 4;
const DIM: Style = Style::new().fg(Color::DarkGray);
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
/// Sparse weekday labels keep the grid visually close to Claude Code.
const ROW_LABELS: [(usize, &str); 3] = [(0, "Mon"), (2, "Wed"), (4, "Fri")];

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

/// Number of week columns that fit in a heatmap of `width` terminal cells.
pub fn visible_weeks(width: u16) -> usize {
    if width <= GUTTER {
        return 0;
    }
    let gutter = if width > GUTTER + 10 { GUTTER } else { 0 };
    let avail = width.saturating_sub(gutter) as usize;
    (avail / CELL_W).max(1)
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
        // Keep a fixed one-column spacer between squares. The caller gives the
        // widget the full platform width, so the grid stretches to fill it.
        let weeks = visible_weeks(area.width);
        let pad = (avail - weeks * CELL_W).min(CELL_W - 1) as u16;
        let col_x = |col: usize| area.x + gutter + pad + (col * CELL_W) as u16;

        let today = CompactDate::from_datetime(Utc::now());
        // Last column is the week containing today; walk back whole weeks.
        let back = today.weekday_mon0() as i64 + ((weeks - 1) * 7) as i64;
        let start = today.checked_add_days(-back).unwrap_or(today);

        // Column-major: each column is one week, rows are Mon…Sun. The rest of
        // the current week is drawn as empty days so the grid stays a
        // rectangle instead of jutting out on the last column.
        let mut metrics = vec![0u64; weeks * 7];
        let mut d = start;
        for slot in metrics.iter_mut() {
            *slot = self.daily.get(&d).map(day_metric).unwrap_or(0);
            d = d.checked_add_days(1).unwrap_or(d);
        }
        let max = metrics.iter().copied().max().unwrap_or(0);

        if has_header {
            let mut prev_month = None;
            let mut next_x = area.x;
            for col in 0..weeks {
                let monday = start.checked_add_days((col * 7) as i64).unwrap_or(start);
                if prev_month == Some(monday.month()) {
                    continue;
                }
                prev_month = Some(monday.month());
                let x = col_x(col);
                if x >= next_x && x + 3 <= area.x + area.width {
                    put_str(buf, x, area.y, MONTHS[monday.month() as usize - 1], DIM);
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
                let Some(cell) = buf.cell_mut((x0, y)) else {
                    continue;
                };
                if level == 0 {
                    cell.set_symbol("·");
                    cell.set_style(DIM);
                } else {
                    cell.set_symbol("■");
                    cell.set_style(Style::default().fg(levels[level]));
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
                let d = month
                    .first_day
                    .checked_add_days(offset as i64)
                    .unwrap_or(month.first_day);
                self.daily.get(&d).map(day_metric).unwrap_or(0)
            })
            .collect();
        let max = days.iter().copied().max().unwrap_or(0);
        for (i, metric) in days.into_iter().enumerate() {
            let level = intensity(metric, max);
            let x = area.x + i as u16;
            if let Some(cell) = buf.cell_mut((x, area.y)) {
                if level == 0 {
                    cell.set_symbol("■");
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
    d.tokens
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
        Color::DarkGray,
        mix_rgb(r, g, b, 0.22),
        mix_rgb(r, g, b, 0.42),
        mix_rgb(r, g, b, 0.68),
        Color::Rgb(r, g, b),
    ]
}

fn mix_rgb(r: u8, g: u8, b: u8, t: f64) -> Color {
    let br = 190.0;
    let bg = 190.0;
    let bb = 190.0;
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
    let grid_end = last_day.checked_add_days(trailing_days).unwrap_or(last_day);
    let weeks = grid_start.days_between(grid_end) as usize / 7 + 1;

    MonthGrid {
        first_day,
        last_day,
        grid_start,
        weeks,
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
    fn token_metric_uses_tokens_not_cost() {
        let day = DayTotals {
            cost_usd: 99.0,
            tokens: 7,
            calls: 1,
        };
        assert_eq!(day_metric(&day), 7);
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
            ["Mon", "Wed", "Fri"]
                .into_iter()
                .all(|label| out.contains(label)),
            "{out}"
        );
        let today = CompactDate::from_datetime(Utc::now());
        assert!(out.contains(MONTHS[today.month() as usize - 1]), "{out}");
        // Seven weekday rows of empty-day dots.
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
        // Fixed-width columns leave the final spacer at the right edge.
        let x = area.width - CELL_W as u16;
        for row in 0..7u16 {
            let cell = buffer.cell((x, 1 + row)).unwrap();
            if row == u16::from(today.weekday_mon0()) {
                assert_ne!(cell.style().fg, DIM.fg, "today unpainted");
            } else {
                // Empty and not-yet-happened days keep the grid rectangular.
                assert_eq!(cell.symbol(), "·", "row {row} of the last column");
            }
        }
    }

    #[test]
    fn week_columns_stretch_to_the_right_edge() {
        let area = Rect::new(0, 0, 100, 8);
        let mut buffer = Buffer::empty(area);
        contribution_heatmap(&BTreeMap::new(), Color::Blue).render(area, &mut buffer);
        // The final square is followed by one spacer column.
        for row in 0..7u16 {
            assert_eq!(
                buffer
                    .cell((area.width - CELL_W as u16, 1 + row))
                    .unwrap()
                    .symbol(),
                "·"
            );
            assert_eq!(buffer.cell((99, 1 + row)).unwrap().symbol(), " ");
        }
    }

    #[test]
    fn week_squares_keep_a_blank_column_between_neighbors() {
        let area = Rect::new(0, 0, 100, 8);
        let mut buffer = Buffer::empty(area);
        let today = CompactDate::from_datetime(Utc::now());
        let previous_week = today.checked_add_days(-7).unwrap();
        let mut daily = BTreeMap::new();
        for day in [previous_week, today] {
            daily.insert(
                day,
                DayTotals {
                    cost_usd: 0.0,
                    tokens: 1,
                    calls: 1,
                },
            );
        }
        contribution_heatmap(&daily, Color::Blue).render(area, &mut buffer);

        let y = 1 + u16::from(today.weekday_mon0());
        let positions: Vec<u16> = (GUTTER..area.width)
            .filter(|&x| buffer.cell((x, y)).unwrap().symbol() == "■")
            .collect();
        assert_eq!(positions.len(), 2);
        assert_eq!(positions[1] - positions[0], CELL_W as u16);
        assert_eq!(positions.last().copied(), Some(area.width - CELL_W as u16));
    }

    #[test]
    fn visible_week_count_matches_available_width() {
        assert_eq!(visible_weeks(50), 23);
        assert_eq!(visible_weeks(110), 53);
        // No hard cap: a wide terminal shows more than twelve months so the
        // grid keeps filling the available width.
        assert_eq!(visible_weeks(200), 98);
        assert_eq!(visible_weeks(GUTTER), 0);
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
            day = day.checked_add_days(1).unwrap();
        }
        assert_eq!(current, 31);
        assert_eq!(outside, 11);
    }
}
