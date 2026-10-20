use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

struct Sandbox {
    home: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aum"));
        command
            .env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path())
            .env("RUST_LOG", "info")
            .env("NO_COLOR", "1")
            .stdin(Stdio::null());
        command
    }

    fn config_path(&self) -> PathBuf {
        let base = if cfg!(target_os = "macos") {
            self.home.path().join("Library/Application Support")
        } else {
            self.home.path().to_path_buf()
        };
        base.join("aum/config.toml")
    }

    fn stats(&self, args: &[&str]) -> Output {
        self.command()
            .args(["--claude-path", "tests/fixtures/claude"])
            .args(["--codex-path", "tests/fixtures/codex"])
            .arg("stats")
            .args(args)
            .output()
            .unwrap()
    }
}

#[test]
fn stats_logs_go_to_stderr_and_reads_do_not_create_config() {
    let sandbox = Sandbox::new();
    let output = sandbox.stats(&["--compact"]);
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["totals"]["total_calls"], 3);
    assert!(String::from_utf8_lossy(&output.stderr).contains("Starting"));
    assert!(!sandbox.config_path().exists());
}

#[test]
fn broken_config_is_preserved_until_explicit_reset() {
    let sandbox = Sandbox::new();
    let path = sandbox.config_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let broken = "claude_path = [";
    std::fs::write(&path, broken).unwrap();
    for args in [
        vec!["config", "show"],
        vec!["config"],
        vec!["config", "set", "refresh", "2"],
        vec!["stats"],
        vec!["mcp"],
    ] {
        let output = sandbox.command().args(&args).output().unwrap();
        assert!(!output.status.success(), "{args:?} must reject bad config");
        assert!(output.stdout.is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
    }
    assert!(
        sandbox
            .command()
            .args(["config", "reset"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_ne!(std::fs::read_to_string(path).unwrap(), broken);
}

#[test]
fn config_show_is_read_only_and_set_preserves_existing_settings() {
    let sandbox = Sandbox::new();
    assert!(
        sandbox
            .command()
            .args(["config", "show"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(!sandbox.config_path().exists());
    for args in [
        ["config", "set", "codex_path", "/tmp/custom-codex"],
        ["config", "set", "refresh", "2"],
    ] {
        assert!(
            sandbox
                .command()
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    let config: agent_usage_monitor::config::Config =
        toml::from_str(&std::fs::read_to_string(sandbox.config_path()).unwrap()).unwrap();
    assert_eq!(config.codex_path, PathBuf::from("/tmp/custom-codex"));
    assert_eq!(config.refresh, 2);
}

#[test]
fn date_range_filters_tool_operations_together_with_usage() {
    let sandbox = Sandbox::new();
    let logs = sandbox.home.path().join("logs");
    std::fs::create_dir(&logs).unwrap();
    let rows = [
        ("2026-05-29T23:59:59Z", "before"),
        ("2026-05-30T00:00:00Z", "start"),
        ("2026-05-30T23:59:59Z", "end"),
        ("2026-05-31T00:00:00Z", "after"),
    ]
    .map(|(timestamp, id)| {
        serde_json::json!({
            "type": "assistant", "timestamp": timestamp, "sessionId": "fixture",
            "message": {"id": id, "model": "claude-opus-4",
                "content": [{"type": "tool_use", "id": id, "name": "Read",
                    "input": {"file_path": "/tmp/fixture"}}],
                "usage": {"input_tokens": 1, "output_tokens": 1}}
        })
        .to_string()
    });
    std::fs::write(logs.join("fixture.jsonl"), rows.join("\n") + "\n").unwrap();
    for (dates, expected) in [
        (vec![], 4),
        (vec!["--since", "2026-05-30"], 3),
        (vec!["--until", "2026-05-30"], 3),
        (vec!["--since", "2026-05-30", "--until", "2026-05-30"], 2),
        (vec!["--since", "2026-06-01", "--until", "2026-06-30"], 0),
    ] {
        let output = sandbox
            .command()
            .args(["stats", "--platform", "claude_code", "--claude-path"])
            .arg(&logs)
            .args(dates)
            .output()
            .unwrap();
        assert!(output.status.success());
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let platform = &report["platforms"]["claude_code"];
        assert_eq!(platform["totals"]["calls"], expected);
        assert_eq!(platform["tool_ops"]["files_read"], expected);
    }
}

#[test]
fn reversed_dates_are_a_usage_error() {
    let output = Sandbox::new().stats(&["--since", "2026-06-30", "--until", "2026-06-01"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--since"));
    assert!(output.stdout.is_empty());
}

#[test]
fn platform_errors_list_supported_names() {
    let sandbox = Sandbox::new();
    let output = sandbox.stats(&["--platform", "nonexistent"]);
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("claude"));
    assert!(error.contains("codex"));
    assert!(!error.contains("config set"));
    let output = sandbox.stats(&["--platform", "claude"]);
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["platforms"].as_object().unwrap().len(), 1);
    assert!(report["platforms"].get("claude_code").is_some());
}

#[test]
fn refresh_is_rejected_outside_monitoring() {
    let output = Sandbox::new()
        .command()
        .args(["--refresh", "2", "stats"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--refresh"));
}
