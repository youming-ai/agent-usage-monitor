use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

use super::find_recursive;
use super::jsonl_reader::JsonlReader;

pub struct ClaudeReader {
    pub(crate) data_dir: PathBuf,
    file_positions: HashMap<PathBuf, u64>,
}

impl ClaudeReader {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            file_positions: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude/projects")
    }
}

impl JsonlReader for ClaudeReader {
    fn file_positions(&mut self) -> &mut HashMap<PathBuf, u64> {
        &mut self.file_positions
    }

    fn find_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.data_dir.exists() {
            find_recursive(&self.data_dir, &mut files, &|p| {
                p.extension().map(|e| e == "jsonl").unwrap_or(false)
            });
        }
        files
    }

    fn parse_line(&self, line: &str) -> Option<UsageRecord> {
        parse_claude_line(line)
    }
}

fn parse_claude_line(line: &str) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;

    if v.get("type")?.as_str()? != "assistant" {
        return None;
    }

    let message = v.get("message")?;
    let usage = message.get("usage")?;

    let input_tokens = usage.get("input_tokens")?.as_u64()?;
    let output_tokens = usage.get("output_tokens")?.as_u64().unwrap_or(0);
    let cache_read = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_creation = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if input_tokens == 0 && output_tokens == 0 && cache_read == 0 && cache_creation == 0 {
        return None;
    }

    let model = message
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp_str = v.get("timestamp")?.as_str()?;
    let timestamp: DateTime<Utc> = timestamp_str.parse().ok()?;

    let dir = v
        .get("cwd")
        .and_then(|c| c.as_str())
        .map(crate::reader::basename)
        .unwrap_or_else(|| "unknown".to_string());
    let session_id = v.get("sessionId").and_then(|s| s.as_str()).unwrap_or("");
    let session = crate::reader::session_label(&dir, session_id);

    // Cost: read from JSONL only (comes from Anthropic API response).
    // `costUSD` is the historical camelCase key some Claude Code versions
    // logged; keep it as a fallback alongside the newer snake_case keys.
    let cost_usd = v
        .get("cost_usd")
        .and_then(|v| v.as_f64())
        .or_else(|| v.get("costUSD").and_then(|v| v.as_f64()))
        .or_else(|| v.get("cost").and_then(|v| v.as_f64()))
        .unwrap_or(0.0);

    // message.id is unique per API response; requestId is the next best
    // identifier. Fall back to the raw line if neither is present so a
    // truncation-triggered re-read still dedups instead of double-counting.
    let record_id = message
        .get("id")
        .and_then(|v| v.as_str())
        .or_else(|| v.get("requestId").and_then(|v| v.as_str()))
        .unwrap_or(line);

    Some(UsageRecord {
        timestamp,
        platform: Platform::ClaudeCode,
        model: crate::state::intern(&model),
        session: crate::state::intern(&session),
        id: crate::state::intern(record_id),
        input_tokens,
        output_tokens,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        cost_usd,
        files_read: 0,
        files_edited: 0,
        files_added: 0,
        files_deleted: 0,
        terminal_commands: 0,
        lines_read: 0,
        lines_edited: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    /// A Claude JSONL file mixes non-record lines (user, summary) with
    /// `assistant` records carrying usage. Only some lines produce records.
    fn sample_jsonl() -> String {
        [
            r#"{"type":"user","message":{"role":"user"}}"#,
            r#"{"type":"summary","summary":"x"}"#,
            r#"{"type":"assistant","timestamp":"2026-05-29T10:00:00Z","requestId":"req1","message":{"id":"msg1","model":"claude-opus-4","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"service_tier":"standard"},"cost_usd":0.01}}"#,
            r#"{"type":"user","message":{"role":"user"}}"#,
            r#"{"type":"assistant","timestamp":"2026-05-29T10:01:00Z","requestId":"req2","message":{"id":"msg2","model":"claude-opus-4","usage":{"input_tokens":200,"output_tokens":80,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"service_tier":"standard"},"cost_usd":0.02}}"#,
            // Each committed line is newline-terminated in real logs.
            "",
        ]
        .join("\n")
    }

    fn write_file(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn poll_delta_returns_nothing_when_file_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "a.jsonl", &sample_jsonl());

        let mut reader = ClaudeReader::new(dir.path().to_path_buf());
        let initial = reader.scan_all();
        assert_eq!(
            initial.len(),
            2,
            "scan_all should find both assistant records"
        );

        // No new lines were written, so a subsequent poll must yield nothing.
        let delta = reader.poll_delta();
        assert!(
            delta.is_empty(),
            "poll_delta re-emitted {} already-seen records",
            delta.len()
        );
    }

    #[test]
    fn poll_delta_returns_only_appended_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(dir.path(), "a.jsonl", &sample_jsonl());

        let mut reader = ClaudeReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 2);

        // Append two more newline-terminated lines: one non-record, one record.
        let appended = format!(
            "{}\n{}\n",
            r#"{"type":"user","message":{"role":"user"}}"#,
            r#"{"type":"assistant","timestamp":"2026-05-29T10:02:00Z","requestId":"req3","message":{"id":"msg3","model":"claude-opus-4","usage":{"input_tokens":300,"output_tokens":90,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"service_tier":"standard"},"cost_usd":0.03}}"#,
        );
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(appended.as_bytes()).unwrap();

        let delta = reader.poll_delta();
        assert_eq!(
            delta.len(),
            1,
            "only the one appended record should be returned"
        );
        assert_eq!(delta[0].input_tokens, 300);
    }

    #[test]
    fn cost_usd_falls_back_to_camelcase_costusd() {
        // C9 regression: some Claude Code versions historically logged
        // camelCase `costUSD` instead of `cost_usd`/`cost`.
        let dir = tempfile::tempdir().unwrap();
        let line = r#"{"type":"assistant","timestamp":"2026-05-29T10:00:00Z","requestId":"req1","message":{"id":"msg1","model":"claude-opus-4","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}},"costUSD":0.05}"#;
        write_file(dir.path(), "a.jsonl", &format!("{line}\n"));

        let mut reader = ClaudeReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].cost_usd, 0.05, "costUSD (camelCase) must be read");
    }

    #[test]
    fn invalid_utf8_line_is_skipped_not_stuck() {
        // C8 regression: a single non-UTF-8 byte anywhere in a line used to
        // wedge the reader forever — read_line's error left the offset
        // unadvanced, so every subsequent poll re-read the same bad byte and
        // stopped there, silently dropping every later record.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.jsonl");
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(b"{\"type\":\"user\",\"message\":{\"role\":\"user\"}}\n")
            .unwrap();
        // A line with an invalid UTF-8 byte in the middle.
        f.write_all(b"{\"type\":\"user\",\"bad\":\"").unwrap();
        f.write_all(&[0xFF]).unwrap();
        f.write_all(b"\"}\n").unwrap();
        // A valid, well-formed record after the bad line.
        f.write_all(
            r#"{"type":"assistant","timestamp":"2026-05-29T10:00:00Z","requestId":"req1","message":{"id":"msg1","model":"claude-opus-4","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":0,"cache_creation_input_tokens":0},"cost_usd":0.01}}
"#
            .as_bytes(),
        )
        .unwrap();

        let mut reader = ClaudeReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(
            records.len(),
            1,
            "the valid record after the bad-UTF8 line must still be read"
        );
        assert_eq!(records[0].input_tokens, 100);
    }
}
