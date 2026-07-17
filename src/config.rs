use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to Claude Code data directory
    #[serde(default = "default_claude_path")]
    pub claude_path: PathBuf,

    /// Path to Codex data directory
    #[serde(default = "default_codex_path")]
    pub codex_path: PathBuf,

    /// Path to opencode data directory (contains opencode.db)
    #[serde(default = "default_opencode_path")]
    pub opencode_path: PathBuf,

    /// Path to Kimi Code data directory
    #[serde(default = "default_kimi_code_path")]
    pub kimi_code_path: PathBuf,

    /// Path to pi data directory
    #[serde(default = "default_pi_path")]
    pub pi_path: PathBuf,

    /// Path to openclaw data directory
    #[serde(default = "default_openclaw_path")]
    pub openclaw_path: PathBuf,

    /// Path to hermes-agent data directory
    #[serde(default = "default_hermes_path")]
    pub hermes_path: PathBuf,

    /// Path to Factory AI data directory
    #[serde(default = "default_factory_path")]
    pub factory_path: PathBuf,

    /// Path to Grok Build data directory (~/.grok)
    #[serde(default = "default_grok_path")]
    pub grok_path: PathBuf,

    /// Path to Cursor CLI data directory (~/.cursor)
    #[serde(default = "default_cursor_path")]
    pub cursor_path: PathBuf,

    /// Path to Copilot CLI data directory (~/.copilot)
    #[serde(default = "default_copilot_path")]
    pub copilot_path: PathBuf,

    /// Path to Antigravity CLI data directory (~/.gemini/antigravity-cli)
    #[serde(default = "default_antigravity_path")]
    pub antigravity_path: PathBuf,

    /// Path to MiMo Code data directory
    #[serde(default = "default_mimo_code_path")]
    pub mimo_code_path: PathBuf,

    /// Polling interval in seconds
    #[serde(default = "default_refresh")]
    pub refresh: u64,

    /// Maximum number of records to keep in memory
    #[serde(default = "default_max_records")]
    pub max_records: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            claude_path: default_claude_path(),
            codex_path: default_codex_path(),
            opencode_path: default_opencode_path(),
            kimi_code_path: default_kimi_code_path(),
            pi_path: default_pi_path(),
            openclaw_path: default_openclaw_path(),
            hermes_path: default_hermes_path(),
            factory_path: default_factory_path(),
            grok_path: default_grok_path(),
            cursor_path: default_cursor_path(),
            copilot_path: default_copilot_path(),
            antigravity_path: default_antigravity_path(),
            mimo_code_path: default_mimo_code_path(),
            refresh: default_refresh(),
            max_records: default_max_records(),
        }
    }
}

// Every `default_*_path` below is a thin `fn() -> PathBuf` shim around
// `Tab::default_path` — required because `#[serde(default = "...")]` needs a
// zero-arg free function, but `Tab::default_path` takes `self`. `Tab` is the
// single source of truth for these paths; keeping them here too (rather than
// inlining the join logic per-platform, as before) means there is exactly one
// place that knows where each agent's data lives.
use crate::state::Tab;

fn default_claude_path() -> PathBuf {
    Tab::ClaudeCode.default_path()
}

fn default_codex_path() -> PathBuf {
    Tab::Codex.default_path()
}

fn default_opencode_path() -> PathBuf {
    Tab::OpenCode.default_path()
}

/// Resolves the XDG data directory for agents that follow XDG on every
/// platform (notably opencode, which does NOT use macOS's
/// `~/Library/Application Support`). Honors `$XDG_DATA_HOME` if set and
/// non-empty; otherwise falls back to `~/.local/share`. Centralized so the
/// config default and the tab-detection path in `app_state.rs` can never drift.
pub(crate) fn xdg_data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local/share")
        })
}

fn default_kimi_code_path() -> PathBuf {
    Tab::KimiCode.default_path()
}

fn default_pi_path() -> PathBuf {
    Tab::Pi.default_path()
}

fn default_openclaw_path() -> PathBuf {
    Tab::OpenClaw.default_path()
}

fn default_hermes_path() -> PathBuf {
    Tab::Hermes.default_path()
}

fn default_factory_path() -> PathBuf {
    Tab::Factory.default_path()
}

fn default_grok_path() -> PathBuf {
    Tab::Grok.default_path()
}

fn default_cursor_path() -> PathBuf {
    Tab::Cursor.default_path()
}

fn default_copilot_path() -> PathBuf {
    Tab::Copilot.default_path()
}

fn default_antigravity_path() -> PathBuf {
    Tab::Antigravity.default_path()
}

fn default_mimo_code_path() -> PathBuf {
    Tab::MimoCode.default_path()
}

fn default_refresh() -> u64 {
    5
}

fn default_max_records() -> usize {
    100
}

/// Get the path to the configuration file
pub fn config_file_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("aum")
        .join("config.toml")
}

/// Load configuration from file, or create default if not exists
pub fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let config_path = config_file_path();

    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    } else {
        // Create default config file
        let config = Config::default();
        save_config(&config)?;
        Ok(config)
    }
}

/// Save configuration to file
pub fn save_config(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = config_file_path();

    // Create parent directory if it doesn't exist
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = toml::to_string_pretty(config)?;
    std::fs::write(&config_path, content)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.refresh, 5);
        assert_eq!(config.max_records, 100);
    }

    #[test]
    fn default_opencode_path_ends_with_opencode() {
        let p = default_opencode_path();
        assert!(p.ends_with("opencode"), "got {p:?}");
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let content = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&content).unwrap();
        assert_eq!(config.refresh, deserialized.refresh);
        assert_eq!(config.max_records, deserialized.max_records);
    }
}
