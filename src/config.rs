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

    /// Path to Grok data directory
    #[serde(default = "default_grok_path")]
    pub grok_path: PathBuf,

    /// Fallback polling interval in seconds
    #[serde(default = "default_refresh")]
    pub refresh: u64,

    /// Per-platform sliding window: how many of the most recent records the
    /// TUI keeps, and therefore what its totals and tables cover.
    #[serde(default = "default_max_records")]
    pub max_records: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            claude_path: default_claude_path(),
            codex_path: default_codex_path(),
            grok_path: default_grok_path(),
            refresh: default_refresh(),
            max_records: default_max_records(),
        }
    }
}

// Every `default_*_path` below is a thin `fn() -> PathBuf` shim around
// `Platform::default_path` — required because `#[serde(default = "...")]`
// needs a zero-arg free function. The platform registry is the single source
// of truth; `Platform::default_path` delegates to it.
use crate::state::Platform;

fn default_claude_path() -> PathBuf {
    Platform::ClaudeCode.default_path()
}

fn default_codex_path() -> PathBuf {
    Platform::Codex.default_path()
}

fn default_grok_path() -> PathBuf {
    Platform::Grok.default_path()
}

fn default_refresh() -> u64 {
    5
}

/// The TUI's totals cover exactly this window, so it has to be large enough
/// that the headline cost doesn't visibly shrink while you work. One assistant
/// message is one record, so 100 (the old default) was minutes of use; 20k is
/// weeks of it, at roughly 100 bytes per record.
fn default_max_records() -> usize {
    20_000
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
        assert_eq!(config.max_records, 20_000);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let content = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&content).unwrap();
        assert_eq!(config.refresh, deserialized.refresh);
        assert_eq!(config.max_records, deserialized.max_records);
    }

    #[test]
    fn unknown_path_keys_in_toml_are_ignored() {
        // Old configs may still list cursor_path / grok_path; serde should not fail.
        let raw = r#"
claude_path = "/tmp/c"
codex_path = "/tmp/x"
cursor_path = "/tmp/old"
grok_path = "/tmp/old2"
refresh = 5
max_records = 1000
"#;
        let config: Config = toml::from_str(raw).unwrap();
        assert_eq!(config.claude_path, PathBuf::from("/tmp/c"));
        assert_eq!(config.refresh, 5);
    }
}
