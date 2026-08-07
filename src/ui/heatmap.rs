//! GitHub-style contribution heatmap (7 rows × N weeks).

use crate::state::{CompactDate, DayTotals};
use chrono::{Datelike, Utc};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use std::collections::BTreeMap;

/// GitHub contribution greens (dark-theme approximate).
const LEVELS: [Color; 5] = [
    Color::Rgb(22, 27, 34),  // empty
    Color::Rgb(14, 68, 41),  // low
    Color::Rgb(0, 109, 50),  // mid-low
    Color::Rgb(38, 166, 65), // mid-high
    Color::Rgb(57, 211, 83), // high
];

/// Build a contribution heatmap ending today, spanning `weeks` columns.
pub fn contribution_heatmap<'a>(
    daily: &'a BTreeMap<CompactDate, DayTotals>,
    weeks: u16,
) -> Heatmap<'a> {
    Heatmap {
        daily,
        weeks: weeks.max(1),
    }
}

pub struct Heatmap<'a> {
    daily: &'a BTreeMap<CompactDate, DayTotals>,
    weeks: u16,
}

impl Widget for Heatmap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let weeks = self.weeks.min(area.width.saturating_sub(2)).max(1) as usize;
        let today = CompactDate::from_datetime(Utc::now());
        let today_wd = today.weekday_sun0() as usize;

        // Grid: `weeks` columns × 7 rows (Sun..Sat). Today sits at
        // (weeks-1, today_wd). Start from that week's Sunday, then fill.
        let total_cells = weeks * 7;
        let days_from_start_sunday = ((weeks - 1) * 7 + today_wd) as i64;
        let start = today
            .checked_sub_days(days_from_start_sunday)
            .unwrap_or_else(|| CompactDate::new(1970, 1, 1));

        let mut cells: Vec<u64> = Vec::with_capacity(total_cells);
        let mut d = start;
        for _ in 0..total_cells {
            cells.push(self.daily.get(&d).map(day_metric).unwrap_or(0));
            d = add_one_day(d).unwrap_or(d);
        }

        let max = cells.iter().copied().max().unwrap_or(0);

        // Optional one-letter weekday gutter when height >= 7.
        let gutter = if area.width >= 12 && area.height >= 7 {
            2
        } else {
            0
        };
        let labels = ['S', 'M', 'T', 'W', 'T', 'F', 'S'];

        let rows = area.height.min(7) as usize;
        for (row, label) in labels.iter().enumerate().take(rows) {
            if gutter > 0 {
                let x = area.x;
                let y = area.y + row as u16;
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol(&label.to_string());
                    cell.set_style(Style::default().fg(Color::DarkGray));
                }
            }
            for col in 0..weeks {
                let idx = col * 7 + row;
                if idx >= cells.len() {
                    break;
                }
                let metric = cells[idx];
                let level = intensity(metric, max);
                let x = area.x + gutter + col as u16;
                let y = area.y + row as u16;
                if x >= area.x + area.width || y >= area.y + area.height {
                    continue;
                }
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_symbol("■");
                    cell.set_style(Style::default().fg(LEVELS[level]));
                }
            }
        }
    }
}

/// Compact 1-row strip of the last `n` days (fallback for short terminals).
pub fn contribution_strip<'a>(
    daily: &'a BTreeMap<CompactDate, DayTotals>,
    n: u16,
) -> StripHeatmap<'a> {
    StripHeatmap { daily, n: n.max(1) }
}

pub struct StripHeatmap<'a> {
    daily: &'a BTreeMap<CompactDate, DayTotals>,
    n: u16,
}

impl Widget for StripHeatmap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let n = self.n.min(area.width) as usize;
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
                cell.set_symbol("■");
                cell.set_style(Style::default().fg(LEVELS[level]));
            }
        }
    }
}

fn day_metric(d: &DayTotals) -> u64 {
    // Prefer cost (micro-dollars) so free-but-busy days don't dominate once
    // pricing is known; fall back to tokens when cost is zero across the board
    // is handled by intensity against max of the same metric.
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

fn add_one_day(d: CompactDate) -> Option<CompactDate> {
    use chrono::NaiveDate;
    let date = NaiveDate::from_ymd_opt(d.year() as i32, d.month() as u32, d.day() as u32)?;
    let next = date.succ_opt()?;
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
}
