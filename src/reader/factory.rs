use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::PathBuf;

use super::UsageSource;
use super::find_recursive;
use super::is_uuid_jsonl;
use super::session_jsonl::{SessionFileState, read_jsonl_from_offset};
use super::FileScanner;

pub struct FactoryReader {
    data_dir: PathBuf,
    scanner: FileScanner<SessionFileState>,
}

impl FactoryReader {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            scanner: FileScanner::new(),
        }
    }

    fn find_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.data_dir.exists() {
            find_recursive(&self.data_dir, &mut files, &|p| {
                p.extension().map(|e| e == "jsonl").unwrap_or(false) && is_uuid_jsonl(p)
            });
        }
        files
    }

    fn scan(&mut self) -> Vec<UsageRecord> {
        let files = self.find_files();
        self.scanner.scan(
            files,
            |_| SessionFileState::with_dir("unknown", ""),
            |file, offset, st| read_jsonl_from_offset(file, offset, st, parse_factory_line),
        )
    }
}

impl UsageSource for FactoryReader {
    fn platform(&self) -> Platform {
        Platform::Factory
    }

    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.scanner.reset();
        self.scan()
    }

    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        self.scan()
    }
}

fn parse_factory_line(line: &str, st: &SessionFileState) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;

    if v.get("type")?.as_str()? != "message" {
        return None;
    }

    let message = v.get("message")?;
    if message.get("role")?.as_str()? != "assistant" {
        return None;
    }

    let usage = message.get("usage")?;

    let input = usage.get("input_tokens")?.as_u64()?;
    let output = usage.get("output_tokens")?.as_u64().unwrap_or(0);
    let cache_read = usage
        .get("cache_read_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_write = usage
        .get("cache_write_tokens")
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

    let dir = v
        .get("cwd")
        .and_then(|c| c.as_str())
        .map(crate::reader::basename)
        .unwrap_or_else(|| st.dir.clone());
    let session_id = v
        .get("sessionId")
        .and_then(|s| s.as_str())
        .unwrap_or(st.sid.as_str());
    let session = crate::reader::session_label(&dir, session_id);

    // `cost` is a sibling of `usage` inside `message`, not nested inside it.
    let cost_usd = message
        .get("cost")
        .and_then(|c| c.get("total"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let record_id = v.get("id").and_then(|i| i.as_str()).unwrap_or(line);

    Some(UsageRecord {
        timestamp,
        platform: Platform::Factory,
        model: crate::state::intern(&model),
        session: crate::state::intern(&session),
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
            r#"{"type":"session","id":"abc123","timestamp":"2026-06-05T10:00:00Z","cwd":"/Users/me/project"}"#,
            r#"{"type":"message","id":"msg1","timestamp":"2026-06-05T10:00:01Z","message":{"role":"assistant","model":"claude-sonnet-4-5","usage":{"input_tokens":1200,"output_tokens":450,"cache_read_tokens":800,"cache_write_tokens":150},"cost":{"total":0.02}}}"#,
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
        write_file(
            dir.path(),
            "a3f2c1d8-10e5-4b2a-9c1d-ef0123456789.jsonl",
            &sample_jsonl(),
        );

        let mut reader = FactoryReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(crate::state::resolve(records[0].model), "claude-sonnet-4-5");
        assert_eq!(records[0].input_tokens, 1200);
        assert_eq!(records[0].output_tokens, 450);
        assert_eq!(records[0].cache_read_tokens, 800);
        assert_eq!(records[0].cache_creation_tokens, 150);
        assert_eq!(records[0].platform, Platform::Factory);
        assert_eq!(crate::state::resolve(records[0].session), "project abc123");
        assert_eq!(
            records[0].cost_usd, 0.02,
            "cost is a sibling of usage inside message, not nested in it"
        );
    }

    #[test]
    fn ignores_non_uuid_jsonl_files() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "session.jsonl", &sample_jsonl());

        let mut reader = FactoryReader::new(dir.path().to_path_buf());
        assert!(reader.scan_all().is_empty());
    }

    #[test]
    fn poll_delta_returns_nothing_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        write_file(
            dir.path(),
            "a3f2c1d8-10e5-4b2a-9c1d-ef0123456789.jsonl",
            &sample_jsonl(),
        );

        let mut reader = FactoryReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 1);
        assert_eq!(reader.poll_delta().len(), 0);
    }

    #[test]
    fn missing_directory_yields_empty() {
        let mut reader = FactoryReader::new(PathBuf::from("/nonexistent/factory"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }
}
