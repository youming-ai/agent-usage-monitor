use super::util::format_tokens;
use crate::state::UsageRecord;
use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Row, Table},
};
use std::collections::{HashMap, VecDeque};

struct SessionAgg {
    tokens: u64,
    requests: u64,
}

/// Per-session usage: total tokens and request count for each conversation,
/// aggregated from the recent records. Highest usage first.
pub fn session_table(records: &VecDeque<UsageRecord>) -> Table<'static> {
    let mut by_session: HashMap<&str, SessionAgg> = HashMap::new();
    for r in records {
        let agg = by_session
            .entry(r.session.as_str())
            .or_insert(SessionAgg { tokens: 0, requests: 0 });
        agg.tokens +=
            r.input_tokens + r.output_tokens + r.cache_read_tokens + r.cache_creation_tokens;
        agg.requests += 1;
    }

    let mut sessions: Vec<(String, SessionAgg)> = by_session
        .into_iter()
        .map(|(name, agg)| (name.to_string(), agg))
        .collect();
    sessions.sort_by(|a, b| b.1.tokens.cmp(&a.1.tokens));

    let rows: Vec<Row> = sessions
        .iter()
        .map(|(name, agg)| {
            Row::new(vec![
                name.clone(),
                format_tokens(agg.tokens),
                agg.requests.to_string(),
            ])
        })
        .collect();

    let header = Row::new(vec!["SESSION", "TOKENS", "REQUESTS"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    Table::new(
        rows,
        [
            Constraint::Percentage(56),
            Constraint::Percentage(22),
            Constraint::Percentage(22),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(" sessions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    )
}
