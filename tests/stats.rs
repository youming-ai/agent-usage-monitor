//! Black-box integration tests for the `aum stats` subcommand.
//!
//! Spawn the compiled binary as a subprocess. Requires `cargo build` to have
//! been run first (CI does this; locally it's a no-op if already built).

use std::process::Command;

fn aum_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_aum"));
    cmd.arg("stats");
    cmd.env("NO_COLOR", "1");
    cmd
}

#[test]
fn stats_default_produces_valid_json_with_all_platforms() {
    let output = aum_bin().arg("--compact").output().expect("run aum stats");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert!(json.get("generated_at").is_some());
    assert!(json.get("totals").is_some());
    let platforms = json.get("platforms").unwrap().as_object().unwrap();
    assert_eq!(platforms.len(), 13, "all 13 platforms should be present");
}

#[test]
fn stats_platform_filter_returns_only_matching() {
    let output = aum_bin()
        .args(["--platform", "claude_code", "--compact"])
        .output()
        .expect("run aum stats");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let platforms = json.get("platforms").unwrap().as_object().unwrap();
    assert_eq!(platforms.len(), 1, "platform filter should return 1 entry");
    assert!(platforms.contains_key("claude_code"));
}

#[test]
fn stats_unavailable_platform_has_available_field() {
    let output = aum_bin()
        .args(["--platform", "codex", "--compact"])
        .output()
        .expect("run aum stats");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let codex = json.get("platforms").unwrap().get("codex").unwrap();
    let available = codex.get("available").unwrap().as_bool().unwrap();
    let _ = available;
}

#[test]
fn stats_json_keys_are_stably_ordered() {
    let output = aum_bin().arg("--compact").output().expect("run aum stats");
    let s = String::from_utf8(output.stdout).unwrap();
    let g = s.find("\"generated_at\"").expect("generated_at present");
    let p = s.find("\"platforms\"").expect("platforms present");
    let t = s.find("\"totals\"").expect("totals present");
    assert!(g < p && p < t, "top-level keys must be ordered: generated_at < platforms < totals");
}

#[test]
fn stats_quota_field_absent_without_flag() {
    let output = aum_bin().arg("--compact").output().expect("run aum stats");
    let s = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&s).unwrap();
    let platforms = json.get("platforms").unwrap().as_object().unwrap();
    for (name, pr) in platforms {
        if name == "claude_code" || name == "codex" {
            assert!(
                pr.get("quota").is_none(),
                "{name} should not have quota when --include-quota is not set"
            );
        }
    }
}

#[test]
fn stats_unknown_platform_errors() {
    let output = aum_bin()
        .args(["--platform", "nonexistent_agent", "--compact"])
        .output()
        .expect("run aum stats");
    assert!(!output.status.success(), "unknown platform should exit non-zero");
}

#[test]
fn stats_invalid_date_errors() {
    let output = aum_bin()
        .args(["--since", "not-a-date", "--compact"])
        .output()
        .expect("run aum stats");
    assert!(!output.status.success(), "invalid date should exit non-zero");
}
