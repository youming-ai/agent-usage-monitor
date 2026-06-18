//! `aum stats --json` subcommand implementation.
//!
//! Produces a JSON usage report by driving each registered reader's
//! `scan_all()` and aggregating into a `StatsReport`. Independent from
//! the TUI: no event loop, no ratatui, no background tasks.

use crate::quota::{QuotaInfo, QuotaWindow};
use crate::state::UsageRecord;
use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct StatsReport {
    pub generated_at: DateTime<Utc>,
    pub platforms: BTreeMap<String, PlatformReport>,
    pub totals: Totals,
}

#[derive(Serialize, Default)]
pub struct Totals {
    pub total_calls: u64,
    pub total_cost_usd: f64,
    pub platforms_with_data: u32,
}

#[derive(Serialize, Default)]
pub struct PlatformTotals {
    pub calls: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

#[derive(Serialize, Default)]
pub struct ModelSummary {
    pub calls: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub sessions: u32,
}

#[derive(Serialize, Default)]
pub struct SessionSummaryView {
    pub session: String,
    pub calls: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub models: Vec<String>,
}

#[derive(Serialize, Default)]
pub struct DateBucket {
    pub calls: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub models: BTreeMap<String, u64>,
}

#[derive(Serialize)]
pub struct PlatformReport {
    pub available: bool,
    pub data_path: PathBuf,
    pub totals: PlatformTotals,
    pub models: BTreeMap<String, ModelSummary>,
    pub sessions: Vec<SessionSummaryView>,
    pub dates: BTreeMap<String, DateBucket>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<QuotaView>,
}

#[derive(Serialize)]
pub struct QuotaView {
    pub tool_name: String,
    pub email: Option<String>,
    pub windows: Vec<QuotaWindow>,
    pub fetched_at: String,
    pub error: Option<String>,
}

#[derive(Default, Debug)]
pub struct Filters {
    pub platforms: Option<std::collections::BTreeSet<String>>,
    pub since: Option<NaiveDate>,
    pub until: Option<NaiveDate>,
}

pub struct CollectOptions {
    pub include_quota: bool,
    pub filters: Filters,
}
