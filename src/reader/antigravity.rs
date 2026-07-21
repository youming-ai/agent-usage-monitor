use crate::reader::pricing;
use crate::reader::{FileScanner, UsageSource, basename, find_recursive, session_label};
use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Per-file state tracking session metadata across lines.
#[derive(Clone, Default)]
struct AgFileState {
    cwd: String,
    conversation_id: String,
    /// The model name, if discovered from transcript metadata.
    model: String,
}

impl AgFileState {
    fn session_label(&self) -> String {
        if self.cwd.is_empty() {
            // No cwd available — use conversation_id as the full label.
            self.conversation_id.clone()
        } else {
            session_label(&basename(&self.cwd), &self.conversation_id)
        }
    }

    fn effective_model(&self) -> &str {
        if self.model.is_empty() {
            "gemini-3"
        } else {
            &self.model
        }
    }
}

pub struct AntigravityReader {
    data_dir: PathBuf,
    scanner: FileScanner<AgFileState>,
}

impl AntigravityReader {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            scanner: FileScanner::new(),
        }
    }

    fn find_files(&self) -> Vec<PathBuf> {
        let brain_dir = self.data_dir.join("brain");
        let mut files = Vec::new();
        if brain_dir.exists() {
            find_recursive(&brain_dir, &mut files, &|p| {
                p.file_name().is_some_and(|n| n == "transcript_full.jsonl")
            });
        }
        files
    }

    /// Derive conversation_id and cwd from the file path structure:
    /// `brain/<conversationId>/.system_generated/logs/transcript_full.jsonl`
    fn derive_session_info(file: &Path, st: &mut AgFileState) {
        // Walk up from the file to find the conversationId directory
        // (direct parent of `.system_generated`).
        let mut current = file.parent();
        while let Some(dir) = current {
            if dir.file_name().is_some_and(|n| n == ".system_generated") {
                // The parent of .system_generated is the conversationId dir.
                if let Some(conv_dir) = dir.parent() {
                    st.conversation_id = conv_dir
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                }
                break;
            }
            current = dir.parent();
        }
        // Do not set cwd from conversation_id — the transcript does not
        // carry a real working directory. Leave cwd empty so session_label()
        // falls back to just the conversation_id.
    }

    fn scan(&mut self) -> Vec<UsageRecord> {
        let files = self.find_files();
        self.scanner.scan(
            files,
            |file| {
                let mut st = AgFileState::default();
                Self::derive_session_info(file, &mut st);
                st
            },
            read_transcript_from_offset,
        )
    }
}

impl UsageSource for AntigravityReader {
    fn platform(&self) -> Platform {
        Platform::Antigravity
    }

    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.scanner.reset();
        self.scan()
    }

    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        self.scan()
    }
}

fn read_transcript_from_offset(
    path: &Path,
    skip_bytes: u64,
    st: &mut AgFileState,
) -> (Vec<UsageRecord>, u64) {
    crate::reader::read_lines_from_offset(path, skip_bytes, |line| parse_transcript_line(line, st))
}

/// Count characters in a `content` field that may be either a plain string or
/// an array of content blocks (each with a `"text"` sub-field).
fn content_char_count(content: Option<&Value>) -> u64 {
    let Some(val) = content else { return 0 };
    if let Some(s) = val.as_str() {
        return s.chars().count() as u64;
    }
    if let Some(arr) = val.as_array() {
        return arr
            .iter()
            .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
            .map(|s| s.chars().count() as u64)
            .sum();
    }
    0
}

fn parse_transcript_line(line: &str, st: &AgFileState) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;

    let source = v.get("source")?.as_str()?;
    let status = v.get("status")?.as_str()?;

    // Only count completed model responses.
    if source != "MODEL" || status != "DONE" {
        return None;
    }

    let char_count = content_char_count(v.get("content"));
    if char_count == 0 {
        return None;
    }

    // Estimate tokens: ~4 chars per token (same heuristic as Cursor reader).
    let estimated_tokens = (char_count / 4).max(1);

    let model = st.effective_model().to_string();
    let timestamp = v
        .get("timestamp")
        .and_then(|t| t.as_str())
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .unwrap_or_else(Utc::now);

    // Model responses are output; we have no input token data from the transcript.
    let cost = pricing::calculate_cost(&model, 0, estimated_tokens, 0, 0);

    // `step` is a monotonic per-conversation counter; pair it with the
    // conversation id for a stable identity, falling back to the raw line.
    let record_id = match v.get("step").and_then(|s| s.as_i64()) {
        Some(step) => format!("{}:{step}", st.conversation_id),
        None => format!("{}:{line}", st.conversation_id),
    };

    Some(UsageRecord {
        timestamp,
        platform: Platform::Antigravity,
        model: crate::state::intern(&model),
        session: crate::state::intern(&st.session_label()),
        id: crate::state::intern(&record_id),
        input_tokens: 0,
        output_tokens: estimated_tokens,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        cost_usd: cost,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn write_transcript(_dir: &Path, conv_id: &Path, lines: &[&str]) -> PathBuf {
        let log_dir = conv_id.join(".system_generated").join("logs");
        fs::create_dir_all(&log_dir).unwrap();
        let path = log_dir.join("transcript_full.jsonl");
        let mut f = fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
        path
    }

    #[test]
    fn parses_model_responses() {
        let dir = tempfile::tempdir().unwrap();
        let conv_dir = dir.path().join("brain").join("conv-abc-123");
        write_transcript(
            dir.path(),
            &conv_dir,
            &[
                r#"{"step":0,"source":"USER","type":"USER_MESSAGE","status":"DONE","content":"Hello","timestamp":"2026-06-01T10:00:00Z"}"#,
                r#"{"step":1,"source":"MODEL","type":"MODEL_RESPONSE","status":"DONE","content":"Hi there! How can I help you today? I am ready to assist with your coding tasks.","timestamp":"2026-06-01T10:00:05Z"}"#,
                r#"{"step":2,"source":"USER","type":"USER_MESSAGE","status":"DONE","content":"Write a function","timestamp":"2026-06-01T10:00:10Z"}"#,
                r#"{"step":3,"source":"MODEL","type":"MODEL_RESPONSE","status":"DONE","content":"Sure, here is a function that does exactly what you need. It takes input and returns the processed result.","timestamp":"2026-06-01T10:00:15Z"}"#,
            ],
        );

        let mut reader = AntigravityReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].platform, Platform::Antigravity);
        assert_eq!(crate::state::resolve(records[0].model), "gemini-3"); // default model
        assert_eq!(crate::state::resolve(records[0].session), "conv-abc-123");
        assert_eq!(records[0].input_tokens, 0); // model responses are output
        assert!(records[0].output_tokens > 0);
        assert!(records[1].output_tokens > 0);
    }

    #[test]
    fn skips_in_progress_and_user_messages() {
        let dir = tempfile::tempdir().unwrap();
        let conv_dir = dir.path().join("brain").join("conv-def");
        write_transcript(
            dir.path(),
            &conv_dir,
            &[
                r#"{"step":0,"source":"USER","type":"USER_MESSAGE","status":"DONE","content":"Hello","timestamp":"2026-06-01T10:00:00Z"}"#,
                r#"{"step":1,"source":"MODEL","type":"MODEL_RESPONSE","status":"STARTED","content":"","timestamp":"2026-06-01T10:00:01Z"}"#,
                r#"{"step":2,"source":"MODEL","type":"MODEL_RESPONSE","status":"DONE","content":"Here is the answer with enough content to estimate tokens from.","timestamp":"2026-06-01T10:00:05Z"}"#,
            ],
        );

        let mut reader = AntigravityReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1); // only the DONE model response
    }

    #[test]
    fn poll_delta_returns_nothing_when_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let conv_dir = dir.path().join("brain").join("conv-ghi");
        write_transcript(
            dir.path(),
            &conv_dir,
            &[
                r#"{"step":0,"source":"MODEL","type":"MODEL_RESPONSE","status":"DONE","content":"This is a response with enough characters for estimation.","timestamp":"2026-06-01T10:00:00Z"}"#,
            ],
        );

        let mut reader = AntigravityReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 1);
        assert!(reader.poll_delta().is_empty());
    }

    #[test]
    fn missing_directory_yields_empty() {
        let mut reader = AntigravityReader::new(PathBuf::from("/nonexistent/ag"));
        assert!(reader.scan_all().is_empty());
        assert!(reader.poll_delta().is_empty());
    }

    #[test]
    fn malformed_json_lines_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let conv_dir = dir.path().join("brain").join("bad-json");
        let log_dir = conv_dir.join(".system_generated").join("logs");
        fs::create_dir_all(&log_dir).unwrap();
        let path = log_dir.join("transcript_full.jsonl");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "not valid json").unwrap();
        writeln!(f, r#"{{"step":1,"source":"MODEL","type":"MODEL_RESPONSE","status":"DONE","content":"This is a valid response with enough content.","timestamp":"2026-06-01T10:00:05Z"}}"#).unwrap();

        let mut reader = AntigravityReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        assert_eq!(crate::state::resolve(records[0].model), "gemini-3");
    }

    #[test]
    fn content_as_array_of_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let conv_dir = dir.path().join("brain").join("conv-array");
        write_transcript(
            dir.path(),
            &conv_dir,
            &[
                r#"{"step":0,"source":"MODEL","type":"MODEL_RESPONSE","status":"DONE","content":[{"type":"text","text":"First block of text content."},{"type":"text","text":"Second block with more text."}],"timestamp":"2026-06-01T10:00:00Z"}"#,
            ],
        );

        let mut reader = AntigravityReader::new(dir.path().to_path_buf());
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        // "First block of text content." (28 chars) + "Second block with more text." (28 chars) = 56 chars
        // 56 / 4 = 14 tokens
        assert_eq!(records[0].output_tokens, 14);
    }

    #[test]
    fn poll_delta_picks_up_appended_lines() {
        let dir = tempfile::tempdir().unwrap();
        let conv_dir = dir.path().join("brain").join("conv-append");
        let log_path = write_transcript(
            dir.path(),
            &conv_dir,
            &[
                r#"{"step":0,"source":"MODEL","type":"MODEL_RESPONSE","status":"DONE","content":"Initial response with enough content to count.","timestamp":"2026-06-01T10:00:00Z"}"#,
            ],
        );

        let mut reader = AntigravityReader::new(dir.path().to_path_buf());
        assert_eq!(reader.scan_all().len(), 1);

        // Append a new line.
        let mut f = fs::OpenOptions::new().append(true).open(&log_path).unwrap();
        writeln!(f, r#"{{"step":1,"source":"MODEL","type":"MODEL_RESPONSE","status":"DONE","content":"Follow-up response with enough content for estimation.","timestamp":"2026-06-01T10:01:00Z"}}"#).unwrap();

        let delta = reader.poll_delta();
        assert_eq!(delta.len(), 1);
        assert!(delta[0].output_tokens > 0);
    }
}
