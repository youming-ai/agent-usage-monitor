use crate::state::{ToolOps, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::PathBuf;

use super::FileScanner;
use super::find_recursive;
use super::{ReaderResult, UsageSource, note_tool};

pub struct ClaudeReader {
    data_dir: PathBuf,
    scanner: FileScanner<()>,
    pending_ops: ToolOps,
    /// session_id -> latest title seen in this process
    titles: std::collections::HashMap<String, String>,
}

impl ClaudeReader {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            scanner: FileScanner::new(),
            pending_ops: ToolOps::default(),
            titles: std::collections::HashMap::new(),
        }
    }

    fn find_files(&self) -> ReaderResult<Vec<PathBuf>> {
        let mut files = Vec::new();
        if self.data_dir.exists() {
            find_recursive(&self.data_dir, &mut files, &|p| {
                p.extension().map(|e| e == "jsonl").unwrap_or(false)
            })?;
        }
        Ok(files)
    }

    fn scan(&mut self) -> ReaderResult<Vec<UsageRecord>> {
        let files = self.find_files()?;
        let titles = &mut self.titles;
        let ops = &mut self.pending_ops;
        self.scanner.scan(
            files,
            |_| (),
            |file, offset, _st| {
                crate::reader::read_lines_from_offset(file, offset, |line| {
                    parse_claude_line(line, titles, ops)
                })
            },
        )
    }

    fn scan_changed(&mut self, paths: &[PathBuf]) -> ReaderResult<Vec<UsageRecord>> {
        let mut files = Vec::new();
        for path in paths {
            if !path.is_file()
                || !path.starts_with(&self.data_dir)
                || path
                    .extension()
                    .is_none_or(|extension| extension != "jsonl")
            {
                return self.scan();
            }
            files.push(path.clone());
        }
        files.sort_unstable();
        files.dedup();
        let titles = &mut self.titles;
        let ops = &mut self.pending_ops;
        self.scanner.scan_changed(
            files,
            |_| (),
            |file, offset, _st| {
                crate::reader::read_lines_from_offset(file, offset, |line| {
                    parse_claude_line(line, titles, ops)
                })
            },
        )
    }
}

impl UsageSource for ClaudeReader {
    fn scan_all(&mut self) -> ReaderResult<Vec<UsageRecord>> {
        self.scanner.reset();
        self.titles.clear();
        self.pending_ops = ToolOps::default();
        self.scan()
    }

    fn poll_delta(&mut self) -> ReaderResult<Vec<UsageRecord>> {
        self.scan()
    }

    fn poll_changed(&mut self, paths: &[PathBuf]) -> ReaderResult<Vec<UsageRecord>> {
        self.scan_changed(paths)
    }

    fn take_tool_ops_delta(&mut self) -> ToolOps {
        std::mem::take(&mut self.pending_ops)
    }
}

fn parse_claude_line(
    line: &str,
    titles: &mut std::collections::HashMap<String, String>,
    ops: &mut ToolOps,
) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;

    let line_type = v.get("type")?.as_str()?;
    let session_id = v
        .get("sessionId")
        .or_else(|| v.get("session_id"))
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    if line_type == "ai-title" {
        if let Some(title) = v.get("aiTitle").and_then(|t| t.as_str())
            && !session_id.is_empty()
        {
            titles.insert(session_id, title.to_string());
        }
        return None;
    }

    // Tool uses live inside assistant message content blocks.
    if line_type == "assistant"
        && let Some(content) = v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
    {
        for block in content {
            if block.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                && let Some(name) = block.get("name").and_then(|n| n.as_str())
            {
                note_tool(ops, name);
            }
        }
    }

    if line_type != "assistant" {
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

    let cwd = v.get("cwd").and_then(|c| c.as_str()).unwrap_or("");
    let dir = if cwd.is_empty() {
        "unknown".to_string()
    } else {
        crate::reader::basename(cwd)
    };
    let session = crate::reader::session_label(&dir, &session_id);
    let title = titles.get(&session_id).map(|s| s.as_str()).unwrap_or("");

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
        model: crate::state::intern(&model),
        session: crate::state::intern(&session),
        id: crate::state::record_id(record_id),
        input_tokens,
        output_tokens,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        reasoning_tokens: 0,
        session_title: crate::state::intern(title),
        project: crate::state::intern(&dir),
        cost_usd,
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
        let initial = reader.scan_all().unwrap();
        assert_eq!(
            initial.len(),
            2,
            "scan_all should find both assistant records"
        );

        // No new lines were written, so a subsequent poll must yield nothing.
        let delta = reader.poll_delta().unwrap();
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
        assert_eq!(reader.scan_all().unwrap().len(), 2);

        // Append two more newline-terminated lines: one non-record, one record.
        let appended = format!(
            "{}\n{}\n",
            r#"{"type":"user","message":{"role":"user"}}"#,
            r#"{"type":"assistant","timestamp":"2026-05-29T10:02:00Z","requestId":"req3","message":{"id":"msg3","model":"claude-opus-4","usage":{"input_tokens":300,"output_tokens":90,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"service_tier":"standard"},"cost_usd":0.03}}"#,
        );
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(appended.as_bytes()).unwrap();

        let delta = reader.poll_delta().unwrap();
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
        let records = reader.scan_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].cost_usd, 0.05,
            "costUSD (camelCase) must be read"
        );
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
        let records = reader.scan_all().unwrap();
        assert_eq!(
            records.len(),
            1,
            "the valid record after the bad-UTF8 line must still be read"
        );
        assert_eq!(records[0].input_tokens, 100);
    }

    #[test]
    fn scan_reports_directory_read_errors() {
        let dir = tempfile::tempdir().unwrap();
        let not_a_directory = dir.path().join("claude-data");
        fs::write(&not_a_directory, b"not a directory").unwrap();
        let mut reader = ClaudeReader::new(not_a_directory);

        assert!(reader.scan_all().is_err());
    }
}
