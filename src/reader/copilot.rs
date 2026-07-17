use crate::reader::pricing;
use crate::reader::{FileScanner, UsageSource, basename, find_recursive, session_label};
use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Per-file state tracking the current model and cwd across events in a single
/// `events.jsonl` session file.
#[derive(Clone, Default)]
struct CopilotFileState {
    model: String,
    cwd: String,
    session_id: String,
}

impl CopilotFileState {
    fn session_label(&self) -> String {
        session_label(&basename(&self.cwd), &self.session_id)
    }
}

pub struct CopilotReader {
    data_dir: PathBuf,
    scanner: FileScanner<CopilotFileState>,
}

impl CopilotReader {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            scanner: FileScanner::new(),
        }
    }

    fn find_files(&self) -> Vec<PathBuf> {
        let session_state = self.data_dir.join("session-state");
        let mut files = Vec::new();
        if session_state.exists() {
            find_recursive(&session_state, &mut files, &|p| {
                p.file_name().is_some_and(|n| n == "events.jsonl")
            });
        }
        files
    }

    fn scan(&mut self) -> Vec<UsageRecord> {
        let files = self.find_files();
        self.scanner.scan(
            files,
            |file| {
                // Derive session_id from the parent directory name (UUID).
                let session_id = file
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                CopilotFileState {
                    session_id,
                    ..Default::default()
                }
            },
            read_events_from_offset,
        )
    }
}

impl UsageSource for CopilotReader {
    fn platform(&self) -> Platform {
        Platform::Copilot
    }

    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.scanner.reset();
        self.scan()
    }

    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        self.scan()
    }
}

fn read_events_from_offset(
    path: &Path,
    skip_bytes: u64,
    st: &mut CopilotFileState,
) -> (Vec<UsageRecord>, u64) {
    crate::reader::read_lines_from_offset(path, skip_bytes, |line| parse_event_line(line, st))
}

fn parse_event_line(line: &str, st: &mut CopilotFileState) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;
    let event_type = v.get("type")?.as_str()?;

    match event_type {
        "session.start" => {
            let data = v.get("data")?;
            if let Some(cwd) = data.get("cwd").and_then(|c| c.as_str()) {
                st.cwd = cwd.to_string();
            }
            if let Some(model) = data.get("model").and_then(|m| m.as_str()) {
                st.model = model.to_string();
            }
            None
        }
        "session.model_change" => {
            let data = v.get("data")?;
            if let Some(model) = data.get("model").and_then(|m| m.as_str()) {
                st.model = model.to_string();
            }
            None
        }
        "tool.execution_complete" => {
            let data = v.get("data")?;
            let model = data
                .get("model")
                .and_then(|m| m.as_str())
                .unwrap_or(&st.model)
                .to_string();
            if model.is_empty() {
                return None;
            }
            let timestamp = parse_timestamp(&v)?;
            let cost = pricing::calculate_cost(&model, 0, 0, 0, 0);
            let record_id = event_record_id(&v, st);
            Some(UsageRecord {
                timestamp,
                platform: Platform::Copilot,
                model: crate::state::intern(&model),
                session: crate::state::intern(&st.session_label()),
                id: crate::state::intern(&record_id),
                // Tool executions don't expose token counts directly;
                // the record serves as a request counter.
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                cost_usd: cost,
            })
        }
        "session.compaction_complete" => {
            let data = v.get("data")?;
            let model = data
                .get("model")
                .and_then(|m| m.as_str())
                .unwrap_or(&st.model)
                .to_string();
            if model.is_empty() {
                return None;
            }
            let timestamp = parse_timestamp(&v)?;
            let input_tokens = data
                .get("preCompactionTokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0);
            let compaction_input = data
                .get("compactionTokensUsed")
                .and_then(|c| c.get("inputTokens"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0);
            let total_input = input_tokens + compaction_input;
            let cost = pricing::calculate_cost(&model, total_input, 0, 0, 0);
            let record_id = event_record_id(&v, st);
            Some(UsageRecord {
                timestamp,
                platform: Platform::Copilot,
                model: crate::state::intern(&model),
                session: crate::state::intern(&st.session_label()),
                id: crate::state::intern(&record_id),
                input_tokens: total_input,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                cost_usd: cost,
            })
        }
        _ => None,
    }
}

/// Copilot events carry a top-level `id` unique within the session; fall back
/// to the session id plus a content hash if it's ever absent.
fn event_record_id(v: &Value, st: &CopilotFileState) -> String {
    match v.get("id").and_then(|i| i.as_str()) {
        Some(id) => format!("{}:{id}", st.session_id),
        None => format!("{}:{v}", st.session_id),
    }
}

fn parse_timestamp(v: &Value) -> Option<DateTime<Utc>> {
    let ts = v.get("timestamp")?;
    if let Some(s) = ts.as_str() {
        s.parse::<DateTime<Utc>>().ok()
    } else if let Some(n) = ts.as_i64() {
        // Heuristic: values < 10^12 are Unix seconds, >= 10^12 are milliseconds.
        if n < 1_000_000_000_000 {
            DateTime::from_timestamp(n, 0)
        } else {
            DateTime::from_timestamp_millis(n)
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn write_events(dir: &Path, session_id: &str, lines: &[&str]) -> PathBuf {
        let session_dir = dir.join("session-state").join(session_id);
        fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join("events.jsonl");
        let mut f = fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
        path
    }

    #[test]
    fn parses_tool_execution_and_compaction() {
        let dir = tempfile::tempdir().unwrap();
        write_events(
            dir.path(),
            "abc-123",
            &[
                r#"{"type":"session.start","data":{"cwd":"/Users/me/project","model":"gpt-4.1"},"id":"1","timestamp":"2026-06-01T10:00:00Z"}"#,
                r#"{"type":"tool.execution_complete","data":{"toolName":"read_file","model":"gpt-4.1","success":true},"id":"2","timestamp":"2026-06-01T10:00:05Z"}"#,
                r#"{"type":"session.compaction_complete","data":{"preCompactionTokens":50000,"compactionTokensUsed":{"inputTokens":2000},"model":"gpt-4.1"},"id":"3","timestamp":"2026-06-01T10:05:00Z"}"#,
            ],
        );

        let mut reader = CopilotReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 2);
        assert_eq!(crate::state::resolve(records[0].model), "gpt-4.1");
        assert_eq!(records[0].platform, Platform::Copilot);
        assert_eq!(crate::state::resolve(records[0].session), "project abc-123");
        assert_eq!(records[1].input_tokens, 52000); // 50000 + 2000
    }

    #[test]
    fn model_change_updates_tracked_model() {
        let dir = tempfile::tempdir().unwrap();
        write_events(
            dir.path(),
            "def-456",
            &[
                r#"{"type":"session.start","data":{"cwd":"/tmp/repo","model":"gpt-4.1"},"id":"1","timestamp":"2026-06-01T10:00:00Z"}"#,
                r#"{"type":"session.model_change","data":{"model":"claude-sonnet-4"},"id":"2","timestamp":"2026-06-01T10:01:00Z"}"#,
                r#"{"type":"tool.execution_complete","data":{"toolName":"bash","success":true},"id":"3","timestamp":"2026-06-01T10:01:05Z"}"#,
            ],
        );

        let mut reader = CopilotReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(crate::state::resolve(records[0].model), "claude-sonnet-4");
    }

    #[test]
    fn poll_delta_returns_nothing_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        write_events(
            dir.path(),
            "ghi-789",
            &[
                r#"{"type":"session.start","data":{"cwd":"/tmp/x","model":"gpt-4.1"},"id":"1","timestamp":"2026-06-01T10:00:00Z"}"#,
                r#"{"type":"tool.execution_complete","data":{"toolName":"read","success":true},"id":"2","timestamp":"2026-06-01T10:00:05Z"}"#,
            ],
        );

        let mut reader = CopilotReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 1);
        assert!(reader.poll_delta().is_empty());
    }

    #[test]
    fn missing_directory_yields_empty() {
        let mut reader = CopilotReader::new(PathBuf::from("/nonexistent/copilot"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }

    #[test]
    fn malformed_json_lines_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let session_dir = dir.path().join("session-state").join("bad-json");
        fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join("events.jsonl");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "not valid json").unwrap();
        writeln!(f, r#"{{"type":"session.start","data":{{"cwd":"/tmp/x","model":"gpt-4.1"}},"id":"1","timestamp":"2026-06-01T10:00:00Z"}}"#).unwrap();
        writeln!(f, r#"{{"type":"tool.execution_complete","data":{{"toolName":"read","success":true}},"id":"2","timestamp":"2026-06-01T10:00:05Z"}}"#).unwrap();

        let mut reader = CopilotReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(crate::state::resolve(records[0].model), "gpt-4.1");
    }

    #[test]
    fn empty_model_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        write_events(
            dir.path(),
            "no-model",
            &[
                // No session.start, so model is empty.
                r#"{"type":"tool.execution_complete","data":{"toolName":"read","success":true},"id":"1","timestamp":"2026-06-01T10:00:05Z"}"#,
            ],
        );

        let mut reader = CopilotReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 0, "empty model should be skipped");
    }

    #[test]
    fn epoch_timestamp_is_parsed() {
        let dir = tempfile::tempdir().unwrap();
        write_events(
            dir.path(),
            "epoch-test",
            &[
                r#"{"type":"session.start","data":{"cwd":"/tmp/x","model":"gpt-4.1"},"id":"1","timestamp":"2026-06-01T10:00:00Z"}"#,
                // Millisecond epoch
                r#"{"type":"tool.execution_complete","data":{"toolName":"read","success":true},"id":"2","timestamp":1717236005000}"#,
            ],
        );

        let mut reader = CopilotReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].timestamp.to_rfc3339(),
            "2024-06-01T10:00:05+00:00"
        );
    }

    #[test]
    fn poll_delta_picks_up_appended_events() {
        let dir = tempfile::tempdir().unwrap();
        write_events(
            dir.path(),
            "append-test",
            &[
                r#"{"type":"session.start","data":{"cwd":"/tmp/x","model":"gpt-4.1"},"id":"1","timestamp":"2026-06-01T10:00:00Z"}"#,
                r#"{"type":"tool.execution_complete","data":{"toolName":"read","success":true},"id":"2","timestamp":"2026-06-01T10:00:05Z"}"#,
            ],
        );

        let mut reader = CopilotReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 1);

        // Append a new event.
        let events_path = dir
            .path()
            .join("session-state")
            .join("append-test")
            .join("events.jsonl");
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(&events_path)
            .unwrap();
        writeln!(
            f,
            r#"{{"type":"tool.execution_complete","data":{{"toolName":"write","success":true}},"id":"3","timestamp":"2026-06-01T10:01:00Z"}}"#
        )
        .unwrap();

        let delta = reader.poll_delta();
        assert_eq!(delta.len(), 1);
        assert_eq!(crate::state::resolve(delta[0].model), "gpt-4.1");
    }
}
