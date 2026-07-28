use crate::cli::Cli;
use crate::config::Config;
use crate::reader::UsageSource;
use crate::reader::claude::ClaudeReader;
use crate::reader::codex::CodexReader;
use crate::reader::cursor::CursorReader;
use crate::state::{AgentPaths, Platform};
use std::path::PathBuf;

/// Platform-specific process invocation for resuming a session. The launcher
/// supplies the working directory and terminal-window policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResumeCommand {
    pub(crate) program: &'static str,
    pub(crate) args: Vec<String>,
}

/// Metadata and wiring for one supported agent platform. New platforms add a
/// single `RegistryEntry` here instead of touching `main.rs` match arms.
pub struct RegistryEntry {
    pub platform: Platform,
    pub config_key: &'static str,
    pub log_name: &'static str,
    config_path: fn(&Config) -> PathBuf,
    cli_path: fn(&Cli) -> Option<PathBuf>,
    set_config_path: fn(&mut Config, PathBuf),
    create_reader: fn(PathBuf) -> Box<dyn UsageSource>,
    /// Builds the platform-specific command to resume a session by id. The
    /// launcher runs it in the selected session's working directory.
    resume: fn(&str) -> ResumeCommand,
}

static REGISTRY: &[RegistryEntry] = &[
    RegistryEntry {
        platform: Platform::ClaudeCode,
        config_key: "claude_path",
        log_name: "Claude Code",
        config_path: |c| c.claude_path.clone(),
        cli_path: |cli| cli.claude_path.clone(),
        set_config_path: |c, p| c.claude_path = p,
        create_reader: |p| Box::new(ClaudeReader::new(p)),
        resume: |id| ResumeCommand {
            program: "claude",
            args: vec!["--resume".into(), id.into()],
        },
    },
    RegistryEntry {
        platform: Platform::Codex,
        config_key: "codex_path",
        log_name: "Codex",
        config_path: |c| c.codex_path.clone(),
        cli_path: |cli| cli.codex_path.clone(),
        set_config_path: |c, p| c.codex_path = p,
        create_reader: |p| Box::new(CodexReader::new(p)),
        resume: |id| ResumeCommand {
            program: "codex",
            args: vec!["resume".into(), id.into()],
        },
    },
    RegistryEntry {
        platform: Platform::Cursor,
        config_key: "cursor_path",
        log_name: "Cursor CLI",
        config_path: |c| c.cursor_path.clone(),
        cli_path: |cli| cli.cursor_path.clone(),
        set_config_path: |c, p| c.cursor_path = p,
        create_reader: |p| Box::new(CursorReader::new(p)),
        // ponytail: best-effort — cursor-agent's resume flag is unverified
        // against a live binary; adjust here if the real flag differs.
        resume: |id| ResumeCommand {
            program: "cursor-agent",
            args: vec![format!("--resume={id}")],
        },
    },
];

/// All registered platforms in UI order.
pub fn entries() -> &'static [RegistryEntry] {
    REGISTRY
}

/// Look up the registry row for a platform.
pub fn entry_for_platform(platform: Platform) -> &'static RegistryEntry {
    REGISTRY
        .iter()
        .find(|e| e.platform == platform)
        .expect("every Platform must have a RegistryEntry")
}

impl RegistryEntry {
    /// Construct a usage reader for this platform at `path`.
    pub fn build_reader(&self, path: PathBuf) -> Box<dyn UsageSource> {
        (self.create_reader)(path)
    }

    /// Platform-specific command to resume `session_id`.
    pub(crate) fn resume_command(&self, session_id: &str) -> ResumeCommand {
        (self.resume)(session_id)
    }
}

/// Merge CLI overrides (when set) with config file values into resolved paths.
pub fn resolve_paths(cli: &Cli, config: &Config) -> AgentPaths {
    let paths = REGISTRY
        .iter()
        .map(|entry| {
            let path = (entry.cli_path)(cli).unwrap_or_else(|| (entry.config_path)(config));
            (entry.platform, path)
        })
        .collect();
    AgentPaths::new(paths)
}

/// Apply a `aum config set <key> <value>` update. Path keys are driven by the
/// registry; fallback `refresh` and `max_records` are handled separately.
pub fn apply_config_key(config: &mut Config, key: &str, value: &str) -> Result<(), String> {
    for entry in REGISTRY {
        if entry.config_key == key {
            (entry.set_config_path)(config, PathBuf::from(value));
            return Ok(());
        }
    }

    match key {
        "refresh" => {
            let refresh = value
                .parse()
                .map_err(|e: std::num::ParseIntError| e.to_string())?;
            if refresh == 0 {
                return Err("refresh must be at least 1 second".into());
            }
            config.refresh = refresh;
        }
        "max_records" => {
            config.max_records = value
                .parse()
                .map_err(|e: std::num::ParseIntError| e.to_string())?;
        }
        _ => {
            return Err(format_unknown_key(key));
        }
    }
    Ok(())
}

/// Comma-separated path keys plus global settings, for error messages.
pub fn config_key_list() -> String {
    let path_keys: Vec<_> = REGISTRY.iter().map(|e| e.config_key).collect();
    format!("{}, refresh, max_records", path_keys.join(", "))
}

fn format_unknown_key(key: &str) -> String {
    format!(
        "Unknown configuration key: {key}\nAvailable keys: {}",
        config_key_list()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_platform_has_registry_entry() {
        for platform in Platform::all() {
            let entry = entry_for_platform(*platform);
            assert_eq!(entry.platform, *platform);
        }
    }

    #[test]
    fn config_keys_are_unique() {
        let keys: Vec<_> = REGISTRY.iter().map(|e| e.config_key).collect();
        let mut deduped = keys.clone();
        deduped.sort_unstable();
        deduped.dedup();
        assert_eq!(keys.len(), deduped.len(), "duplicate config keys");
    }

    #[test]
    fn apply_config_key_sets_path() {
        let mut config = Config::default();
        apply_config_key(&mut config, "cursor_path", "/tmp/cursor").unwrap();
        assert_eq!(config.cursor_path, PathBuf::from("/tmp/cursor"));
    }

    #[test]
    fn resume_commands_match_each_cli() {
        let claude = entry_for_platform(Platform::ClaudeCode).resume_command("SID");
        assert_eq!(claude.program, "claude");
        assert_eq!(claude.args, vec!["--resume", "SID"]);

        let codex = entry_for_platform(Platform::Codex).resume_command("SID");
        assert_eq!(codex.program, "codex");
        assert_eq!(codex.args, vec!["resume", "SID"]);

        let cursor = entry_for_platform(Platform::Cursor).resume_command("SID");
        assert_eq!(cursor.program, "cursor-agent");
        assert_eq!(cursor.args, vec!["--resume=SID"]);
    }

    #[test]
    fn apply_config_key_rejects_unknown_or_zero_refresh() {
        let mut config = Config::default();
        assert!(apply_config_key(&mut config, "nope", "x").is_err());
        assert!(apply_config_key(&mut config, "refresh", "0").is_err());
    }

    #[test]
    fn resolve_paths_cli_overrides_config() {
        let config = Config::default();
        let cli = Cli {
            command: None,
            claude_path: Some(PathBuf::from("/cli/claude")),
            codex_path: None,
            cursor_path: None,
            refresh: None,
        };
        let paths = resolve_paths(&cli, &config);
        assert_eq!(
            paths.path_for(Platform::ClaudeCode),
            PathBuf::from("/cli/claude")
        );
        assert_eq!(paths.path_for(Platform::Codex), config.codex_path);
    }
}
