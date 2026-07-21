use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::PathBuf;

use super::FileScanner;
use super::UsageSource;
use super::find_recursive;
use super::session_jsonl::{SessionFileState, read_jsonl_from_offset};

pub struct PiReader {
    data_dir: PathBuf,
    platform: Platform,
    scanner: FileScanner<SessionFileState>,
}

impl PiReader {
    pub fn new(data_dir: PathBuf, platform: Platform) -> Self {
        Self {
            data_dir,
            platform,
            scanner: FileScanner::new(),
        }
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

    fn scan(&mut self) -> Vec<UsageRecord> {
        let files = self.find_files();
        let platform = self.platform;
        self.scanner.scan(
            files,
            |_| SessionFileState::with_dir("unknown", ""),
            |file, offset, st| {
                read_jsonl_from_offset(file, offset, st, |line, st| {
                    parse_pi_line(line, st, platform)
                })
            },
        )
    }
}

impl UsageSource for PiReader {
    fn platform(&self) -> Platform {
        self.platform
    }

    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.scanner.reset();
        self.scan()
    }

    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        self.scan()
    }
}

fn parse_pi_line(line: &str, st: &SessionFileState, platform: Platform) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;

    if v.get("type")?.as_str()? != "message" {
        return None;
    }

    let message = v.get("message")?;
    if message.get("role")?.as_str()? != "assistant" {
        return None;
    }

    let usage = message.get("usage")?;

    let input = usage.get("input")?.as_u64()?;
    let output = usage.get("output")?.as_u64().unwrap_or(0);
    let cache_read = usage.get("cacheRead").and_then(|v| v.as_u64()).unwrap_or(0);
    let cache_write = usage
        .get("cacheWrite")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if input == 0 && output == 0 && cache_read == 0 && cache_write == 0 {
        return None;
    }

    let model = message
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp_str = v.get("timestamp")?.as_str()?;
    let timestamp: DateTime<Utc> = timestamp_str.parse().ok()?;

    let cost_usd = usage
        .get("cost")
        .and_then(|c| c.get("total"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let record_id = v.get("id").and_then(|i| i.as_str()).unwrap_or(line);

    Some(UsageRecord {
        timestamp,
        platform,
        model: crate::state::intern(&model),
        session: crate::state::intern(&st.session_label()),
        id: crate::state::intern(record_id),
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_write,
        cost_usd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn sample_jsonl() -> String {
        [
            r#"{"type":"session","version":3,"id":"abc123","timestamp":"2026-06-05T10:00:00Z","cwd":"/Users/me/project"}"#,
            r#"{"type":"message","id":"msg1","parentId":null,"timestamp":"2026-06-05T10:00:01Z","message":{"role":"user","content":"hello"}}"#,
            r#"{"type":"message","id":"msg2","parentId":"msg1","timestamp":"2026-06-05T10:00:02Z","message":{"role":"assistant","content":[{"type":"text","text":"Hi!"}],"provider":"anthropic","model":"claude-sonnet-4-5","usage":{"input":100,"output":50,"cacheRead":10,"cacheWrite":5,"totalTokens":165,"cost":{"input":0.015,"output":0.0075,"cacheRead":0.00015,"cacheWrite":0.0001875,"total":0.0228375}},"stopReason":"stop","timestamp":1717584002000}}"#,
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
    fn parses_assistant_messages() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "session.jsonl", &sample_jsonl());

        let mut reader = PiReader::new(dir.path().to_path_buf(), Platform::Pi);
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(crate::state::resolve(records[0].model), "claude-sonnet-4-5");
        assert_eq!(records[0].input_tokens, 100);
        assert_eq!(records[0].output_tokens, 50);
        assert_eq!(records[0].cache_read_tokens, 10);
        assert_eq!(records[0].cache_creation_tokens, 5);
        assert_eq!(records[0].platform, Platform::Pi);
        assert_eq!(crate::state::resolve(records[0].session), "project abc123");
    }

    #[test]
    fn poll_delta_returns_nothing_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "session.jsonl", &sample_jsonl());

        let mut reader = PiReader::new(dir.path().to_path_buf(), Platform::Pi);
        assert_eq!(reader.scan_all().len(), 1);
        assert_eq!(reader.poll_delta().len(), 0);
    }

    #[test]
    fn missing_directory_yields_empty() {
        let mut reader = PiReader::new(PathBuf::from("/nonexistent/pi"), Platform::Pi);
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }
}
