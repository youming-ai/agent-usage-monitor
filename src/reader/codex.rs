use crate::reader::pricing;
use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct CodexReader {
    sessions_dir: PathBuf,
    file_positions: HashMap<PathBuf, u64>,
    // The "current model" is per rollout file: a session sets its model via a
    // `turn_context`/`task_started` event at the top, and that line is skipped
    // on subsequent polls — so the model must persist per file rather than in a
    // single shared field (which would leak between sessions).
    file_models: HashMap<PathBuf, String>,
}

impl CodexReader {
    pub fn new(codex_dir: PathBuf) -> Self {
        let sessions_dir = codex_dir.join("sessions");
        Self {
            sessions_dir,
            file_positions: HashMap::new(),
            file_models: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".codex")
    }

    fn find_rollout_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.sessions_dir.exists() {
            find_rollout_recursive(&self.sessions_dir, &mut files);
        }
        files
    }

    pub fn scan_all(&mut self) -> Vec<UsageRecord> {
        let files = self.find_rollout_files();
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
        let files = self.find_rollout_files();
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
    fn read_file_from(&mut self, path: &Path, skip_lines: u64) -> (Vec<UsageRecord>, u64) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            // Keep the existing cursor on a transient read error.
            Err(_) => return (Vec::new(), skip_lines),
        };

        let project = extract_codex_project(path);

        let complete_lines = if content.ends_with('\n') {
            content.lines().count()
        } else {
            content.lines().count().saturating_sub(1)
        };

        // Resume this file's running model (persists across polls).
        let mut model = self
            .file_models
            .get(path)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let records = content
            .lines()
            .take(complete_lines)
            .skip(skip_lines as usize)
            .filter_map(|line| parse_codex_line(line, &project, &mut model))
            .collect();

        self.file_models.insert(path.to_path_buf(), model);

        (records, complete_lines as u64)
    }
}

/// Parse one rollout line. `model` carries the session's running model across
/// lines and polls; `turn_context`/`task_started` events update it in place.
fn parse_codex_line(line: &str, project: &str, model: &mut String) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;
    let event_type = v.get("type")?.as_str()?;

    // turn_context is a top-level event (not wrapped in event_msg)
    if event_type == "turn_context" {
        if let Some(m) = v.get("payload").and_then(|p| p.get("model")).and_then(|m| m.as_str()) {
            *model = m.to_string();
        }
        return None;
    }

    if event_type != "event_msg" {
        return None;
    }

    let payload = v.get("payload")?;
    let payload_type = payload.get("type")?.as_str()?;

    // Also check task_started for model (fallback)
    if payload_type == "task_started" {
        if let Some(m) = payload
            .get("collaboration_mode")
            .and_then(|c| c.get("settings"))
            .and_then(|s| s.get("model"))
            .and_then(|m| m.as_str())
        {
            *model = m.to_string();
        }
        return None;
    }

    if payload_type != "token_count" {
        return None;
    }

    let info = payload.get("info")?;
    if info.is_null() {
        return None;
    }

    let total = info.get("total_token_usage")?;
    let input_tokens = total.get("input_tokens")?.as_u64()?;
    let output_tokens = total.get("output_tokens")?.as_u64().unwrap_or(0);
    let cached = total
        .get("cached_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let last = info.get("last_token_usage");
    let delta_input = last
        .and_then(|l| l.get("input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(input_tokens);
    let delta_output = last
        .and_then(|l| l.get("output_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(output_tokens);
    let delta_cached = last
        .and_then(|l| l.get("cached_input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(cached);

    if delta_input == 0 && delta_output == 0 {
        return None;
    }

    let timestamp_str = v.get("timestamp")?.as_str()?;
    let timestamp: DateTime<Utc> = timestamp_str.parse().ok()?;

    let cost_usd = pricing::calculate_cost(model, delta_input, delta_output, delta_cached, 0);

    Some(UsageRecord {
        timestamp,
        platform: Platform::Codex,
        model: model.clone(),
        project: project.to_string(),
        input_tokens: delta_input,
        output_tokens: delta_output,
        cache_read_tokens: delta_cached,
        cache_creation_tokens: 0,
        cost_usd,
        service_tier: String::new(),
        message_id: String::new(),
        request_id: String::new(),
    })
}

fn extract_codex_project(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.split('-').next_back().unwrap_or(s).to_string())
        .unwrap_or_else(|| "codex".to_string())
}

fn find_rollout_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                find_rollout_recursive(&path, files);
            } else if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
                .unwrap_or(false)
            {
                files.push(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn turn_context(model: &str) -> String {
        format!(
            r#"{{"type":"turn_context","timestamp":"2026-05-29T10:00:00Z","payload":{{"model":"{model}"}}}}"#
        )
    }

    fn token_count(ts: &str, input: u64, output: u64) -> String {
        format!(
            r#"{{"type":"event_msg","timestamp":"{ts}","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{input},"output_tokens":{output},"cached_input_tokens":0}},"last_token_usage":{{"input_tokens":{input},"output_tokens":{output},"cached_input_tokens":0}}}}}}}}"#
        )
    }

    fn write_rollout(dir: &Path, name: &str, lines: &[String]) -> PathBuf {
        let mut content = lines.join("\n");
        content.push('\n'); // committed lines are newline-terminated
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    fn reader_for(sessions: &Path) -> CodexReader {
        // CodexReader::new joins "sessions"; point it at the parent.
        CodexReader::new(sessions.parent().unwrap().to_path_buf())
    }

    #[test]
    fn poll_delta_returns_nothing_when_file_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        write_rollout(
            &sessions,
            "rollout-1.jsonl",
            &[turn_context("gpt-5.4"), token_count("2026-05-29T10:01:00Z", 100, 50)],
        );

        let mut reader = reader_for(&sessions);
        assert_eq!(reader.scan_all().len(), 1);
        assert!(reader.poll_delta().is_empty(), "unchanged file re-emitted records");
    }

    #[test]
    fn appended_token_count_keeps_its_own_files_model() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).unwrap();

        // Two sessions with different models, each only a turn_context so far.
        let file_a = write_rollout(&sessions, "rollout-a.jsonl", &[turn_context("gpt-5.4")]);
        write_rollout(&sessions, "rollout-b.jsonl", &[turn_context("gpt-5.3-codex")]);

        let mut reader = reader_for(&sessions);
        assert!(reader.scan_all().is_empty(), "no token_count yet");

        // A token_count is appended to file A only; its turn_context line was
        // already consumed, so the model must still resolve to file A's model.
        let mut f = fs::OpenOptions::new().append(true).open(&file_a).unwrap();
        f.write_all(format!("{}\n", token_count("2026-05-29T10:05:00Z", 200, 80)).as_bytes())
            .unwrap();

        let delta = reader.poll_delta();
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0].model, "gpt-5.4", "model leaked from another session");
    }
}
