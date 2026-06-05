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
            refresh: default_refresh(),
            max_records: default_max_records(),
        }
    }
}

fn default_claude_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude/projects")
}

fn default_codex_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
}

fn default_opencode_path() -> PathBuf {
    // opencode uses the XDG data dir on every platform (NOT macOS's
    // ~/Library/Application Support), so we resolve it ourselves rather than
    // via dirs::data_dir().
    std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local/share")
        })
        .join("opencode")
}

fn default_kimi_code_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".kimi-code")
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
