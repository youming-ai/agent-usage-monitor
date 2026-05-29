use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ClaudeReader {
    data_dir: PathBuf,
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

    fn find_jsonl_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.data_dir.exists() {
            find_jsonl_recursive(&self.data_dir, &mut files);
        }
        files
    }

    pub fn scan_all(&mut self) -> Vec<UsageRecord> {
        let files = self.find_jsonl_files();
        let mut records = Vec::new();
        for file in files {
            let (entries, lines_read) = self.read_file_from(&file, 0);
            self.file_positions.insert(file, lines_read);
            records.extend(entries);
        }
        records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        records
    }

    pub fn poll_delta(&mut self) -> Vec<UsageRecord> {
        let files = self.find_jsonl_files();
        let mut new_records = Vec::new();
        for file in files {
            let offset = self.file_positions.get(&file).copied().unwrap_or(0);
            let (entries, lines_read) = self.read_file_from(&file, offset);
            // Always advance the cursor past consumed lines, even when none of
            // them produced a record, so non-record lines are not re-scanned.
            self.file_positions.insert(file, lines_read);
            new_records.extend(entries);
        }
        new_records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        new_records
    }

    /// Parse records from `path`, skipping the first `skip_lines` lines.
    /// Returns the parsed records and the number of *complete* lines in the
    /// file (the cursor to store). A trailing line without a newline is
    /// treated as still being written and is left for the next poll.
    fn read_file_from(&self, path: &Path, skip_lines: u64) -> (Vec<UsageRecord>, u64) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            // Keep the existing cursor on a transient read error.
            Err(_) => return (Vec::new(), skip_lines),
        };

        let project = extract_project_name(path, &self.data_dir);

        let complete_lines = if content.ends_with('\n') {
            content.lines().count()
        } else {
            content.lines().count().saturating_sub(1)
        };

        let records = content
            .lines()
            .take(complete_lines)
            .skip(skip_lines as usize)
            .filter_map(|line| parse_claude_line(line, &project))
            .collect();

        (records, complete_lines as u64)
    }
}

fn parse_claude_line(line: &str, project: &str) -> Option<UsageRecord> {
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

    if input_tokens == 0 && output_tokens == 0 {
        return None;
    }

    let model = message
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp_str = v.get("timestamp")?.as_str()?;
    let timestamp: DateTime<Utc> = timestamp_str.parse().ok()?;

    let request_id = v
        .get("requestId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let message_id = message
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let service_tier = usage
        .get("service_tier")
        .and_then(|v| v.as_str())
        .unwrap_or("standard")
        .to_string();

    // Cost: read from JSONL only (comes from Anthropic API response)
    let cost_usd = v
        .get("cost_usd")
        .and_then(|v| v.as_f64())
        .or_else(|| v.get("cost").and_then(|v| v.as_f64()))
        .unwrap_or(0.0);

    Some(UsageRecord {
        timestamp,
        platform: Platform::ClaudeCode,
        model,
        project: project.to_string(),
        input_tokens,
        output_tokens,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        cost_usd,
        service_tier,
        message_id,
        request_id,
    })
}

fn extract_project_name(path: &Path, data_dir: &Path) -> String {
    path.parent()
        .and_then(|p| p.strip_prefix(data_dir).ok())
        .and_then(|p| p.to_str())
        .map(|s| {
            s.trim_start_matches('-')
                .split('-')
                .next_back()
                .unwrap_or(s)
                .to_string()
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn find_jsonl_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                find_jsonl_recursive(&path, files);
            } else if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                files.push(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
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
        assert_eq!(initial.len(), 2, "scan_all should find both assistant records");

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
        assert_eq!(delta.len(), 1, "only the one appended record should be returned");
        assert_eq!(delta[0].input_tokens, 300);
    }
}
