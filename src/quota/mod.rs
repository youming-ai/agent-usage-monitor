pub mod claude;
pub mod codex;

use serde::{Deserialize, Serialize};
use std::time::Duration;

const CACHE_TTL: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaWindow {
    pub label: String,
    pub remaining_percent: Option<f64>,
    pub resets_at: Option<String>,
    pub reset_in: Option<String>,
}

#[derive(Debug, Clone)]
pub struct QuotaInfo {
    #[allow(dead_code)]
    pub tool_name: String,
    pub email: Option<String>,
    #[allow(dead_code)]
    pub account_id: Option<String>,
    pub windows: Vec<QuotaWindow>,
    pub fetched_at: std::time::Instant,
    pub error: Option<String>,
}

impl QuotaInfo {
    pub fn is_stale(&self) -> bool {
        self.fetched_at.elapsed() > CACHE_TTL
    }

    #[allow(dead_code)]
    pub fn summary(&self) -> String {
        if let Some(err) = &self.error {
            return format!("Error: {}", err);
        }

        let parts: Vec<String> = self
            .windows
            .iter()
            .map(|w| {
                let percent = w
                    .remaining_percent
                    .map(|p| format!("{}%", (p * 100.0).round() as u64))
                    .unwrap_or_else(|| "?%".to_string());
                let reset = w
                    .reset_in
                    .as_deref()
                    .map(|r| format!(" · reset {r}"))
                    .unwrap_or_default();
                format!("{} remain {percent}{reset}", w.label)
            })
            .collect();

        if parts.is_empty() {
            "No quota data".to_string()
        } else {
            parts.join("  |  ")
        }
    }
}
