//! Request/Response types for the 6 MCP tools.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct GetDailyStatsRequest {
    /// Optional analyzer filter (e.g. "claude_code").
    pub analyzer: Option<String>,
    /// Optional date filter (YYYY-MM-DD).
    pub date: Option<String>,
    /// Optional limit on number of results (default 30).
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DailyStat {
    pub date: String,
    pub calls: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub models: std::collections::BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct DailyStatsResponse {
    pub results: Vec<DailyStat>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct GetModelUsageRequest {
    pub analyzer: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModelCount {
    pub model: String,
    pub message_count: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ModelUsageResponse {
    pub models: Vec<ModelCount>,
    pub total_messages: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct GetCostBreakRequest {
    pub analyzer: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DailyCost {
    pub date: String,
    pub cost: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct CostBreakdownResponse {
    pub total_cost: f64,
    pub daily_costs: Vec<DailyCost>,
    pub average_daily_cost: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct GetFileOpsRequest {
    pub analyzer: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct FileOpsResponse {
    pub files_read: u64,
    pub files_edited: u64,
    pub files_added: u64,
    pub files_deleted: u64,
    pub terminal_commands: u64,
    pub lines_read: u64,
    pub lines_edited: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct GetSessionStatsRequest {
    pub analyzer: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionEntry {
    pub session: String,
    pub analyzer: String,
    pub calls: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SessionStatsResponse {
    pub sessions: Vec<SessionEntry>,
    pub total_sessions: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct QuotaEntry {
    pub platform: String,
    pub tool_name: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub org: Option<String>,
    pub windows: Vec<crate::quota::QuotaWindow>,
    pub live_summary: Option<String>,
    pub fetched_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct QuotaResponse {
    pub quota: Vec<QuotaEntry>,
}
