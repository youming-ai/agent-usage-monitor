//! Claude Code–style contribution heatmap (Mon-first week, month labels,
//! accent color ramp, Less/More legend).

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

/// Build a contribution heatmap ending today, tinted with `accent`.
pub fn contribution_heatmap<'a>(
    daily: &'a BTreeMap<CompactDate, DayTotals>,
    weeks: u16,
    accent: Color,
) -> Heatmap<'a> {
    Heatmap {
        daily,
        weeks: weeks.max(1),
        accent,
    }
}

pub struct Heatmap<'a> {
    daily: &'a BTreeMap<CompactDate, DayTotals>,
    weeks: u16,
    accent: Color,
}

/// Preferred height: 1 month row + 7 day rows + 1 legend = 9.
pub const HEATMAP_FULL_HEIGHT: u16 = 9;

impl Widget for Heatmap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let levels = accent_levels(self.accent);
        let has_month = area.height >= 9;
        let has_legend = area.height >= 9;
        let grid_top = area.y + u16::from(has_month);
        let grid_h = if has_legend {
            area.height.saturating_sub(2)
        } else {
            area.height.saturating_sub(u16::from(has_month))
        }
        .min(7) as usize;

        if grid_h == 0 {
            return;
        }

        let gutter = if area.width >= 14 { 4u16 } else { 0 };
        let weeks = self.weeks.min(area.width.saturating_sub(gutter)).max(1) as usize;

        let today = CompactDate::from_datetime(Utc::now());
        let today_wd = today.weekday_mon0() as usize; // Mon=0 … Sun=6

        // Grid: weeks columns × 7 rows (Mon…Sun). Today at (weeks-1, today_wd).
        let total_cells = weeks * 7;
        let days_from_start_monday = ((weeks - 1) * 7 + today_wd) as i64;
        let start = today
            .checked_sub_days(days_from_start_monday)
            .unwrap_or_else(|| CompactDate::new(1970, 1, 1));

        let mut cells: Vec<(CompactDate, u64)> = Vec::with_capacity(total_cells);
        let mut d = start;
        for _ in 0..total_cells {
            let metric = self.daily.get(&d).map(day_metric).unwrap_or(0);
            cells.push((d, metric));
            d = add_one_day(d).unwrap_or(d);
        }
        let max = cells.iter().map(|(_, m)| *m).max().unwrap_or(0);

        // Month labels on the first week of each month.
        if has_month {
            let mut last_month = 0u8;
            for col in 0..weeks {
                let idx = col * 7;
                if idx >= cells.len() {
                    break;
                }
                let month = cells[idx].0.month();
                // Label when this week contains the 1st, or month changes on Mon.
                let week_has_first = (0..7).any(|r| {
                    let i = col * 7 + r;
                    i < cells.len() && cells[i].0.day() == 1
                });
                if week_has_first || (col == 0 && month != last_month) {
                    let label = month_abbr(month);
                    let x = area.x + gutter + col as u16;
                    put_str(buf, x, area.y, label, Style::default().fg(Color::DarkGray));
                    last_month = month;
                } else if month != last_month {
                    last_month = month;
                }
            }
        }

        // Weekday gutter: Mon / Wed / Fri only (like Claude Code).
        let day_labels = [
            Some("Mon"),
            None,
            Some("Wed"),
            None,
            Some("Fri"),
            None,
            None,
        ];

        for row in 0..grid_h {
            if gutter > 0
                && let Some(label) = day_labels[row]
            {
                put_str(
                    buf,
                    area.x,
                    grid_top + row as u16,
                    label,
                    Style::default().fg(Color::DarkGray),
                );
            }
            for col in 0..weeks {
                let idx = col * 7 + row;
                if idx >= cells.len() {
                    break;
                }
                let metric = cells[idx].1;
                let level = intensity(metric, max);
                let x = area.x + gutter + col as u16;
                let y = grid_top + row as u16;
                if x >= area.x + area.width || y >= area.y + area.height {
                    continue;
                }
                if let Some(cell) = buf.cell_mut((x, y)) {
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

        // Less ■■■■ More legend.
        if has_legend {
            let y = area.y + area.height - 1;
            let mut x = area.x + gutter;
            put_str(buf, x, y, "Less ", Style::default().fg(Color::DarkGray));
            x += 5;
            for level in 1..=4 {
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

/// Compact 1-row strip of the last `n` days (short terminals).
pub fn contribution_strip<'a>(
    daily: &'a BTreeMap<CompactDate, DayTotals>,
    n: u16,
    accent: Color,
) -> StripHeatmap<'a> {
    StripHeatmap {
        daily,
        n: n.max(1),
        accent,
    }
}

pub struct StripHeatmap<'a> {
    daily: &'a BTreeMap<CompactDate, DayTotals>,
    n: u16,
    accent: Color,
}

impl Widget for StripHeatmap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let levels = accent_levels(self.accent);
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

/// Five colors: empty placeholder + 4 ramp steps from dark accent → bright accent.
fn accent_levels(accent: Color) -> [Color; 5] {
    let (r, g, b) = match accent {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (217, 119, 87), // Claude orange fallback
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
    // Blend toward near-black background.
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
