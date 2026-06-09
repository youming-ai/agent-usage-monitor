pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod copilot;
pub mod kimi_code;
pub mod opencode;
pub(crate) mod util;

use serde::{Deserialize, Serialize};
use std::time::Duration;

const CACHE_TTL: Duration = Duration::from_secs(120);

/// Reason a quota fetch failed. Surfaced verbatim in the UI and used to decide
/// whether the cached result should be re-tried sooner than the regular TTL.
#[allow(dead_code)] // NoCredentials/Network are reserved for future use
                    // (today fetch_quota returns None on those paths).
#[derive(Debug, Clone, PartialEq)]
pub enum QuotaError {
    /// No local credentials found (Keychain, credentials.json, or auth.json).
    NoCredentials,
    /// Network call failed (curl exit, timeout, unreachable host).
    Network(String),
    /// API returned an authentication error — user must re-login.
    Auth(String),
    /// Response body could not be parsed or carried an unexpected error shape.
    Parse(String),
}

impl QuotaError {
    /// Short human-readable label for the UI.
    pub fn display(&self) -> String {
        match self {
            QuotaError::NoCredentials => "no credentials".into(),
            QuotaError::Network(msg) => format!("network: {msg}"),
            QuotaError::Auth(msg) => format!("re-auth required: {msg}"),
            QuotaError::Parse(msg) => format!("parse: {msg}"),
        }
    }
}

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
    pub error: Option<QuotaError>,
}

impl QuotaInfo {
    /// True if the cached data is older than the TTL — and also true if the
    /// last fetch was an error (so we don't keep showing a stale failure).
    pub fn is_stale(&self) -> bool {
        if self.error.is_some() {
            return true;
        }
        self.fetched_at.elapsed() > CACHE_TTL
    }

    #[allow(dead_code)]
    pub fn summary(&self) -> String {
        if let Some(err) = &self.error {
            return format!("Error: {}", err.display());
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
