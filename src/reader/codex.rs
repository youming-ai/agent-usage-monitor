use crate::reader::pricing;
use crate::state::{Platform, UsageRecord};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::{Path, PathBuf};

use super::FileScanner;
use super::UsageSource;
use super::find_recursive;

/// Per-file running state. A rollout file declares its model and working
/// directory in events at the top, which are skipped on later polls — so this
/// must persist per file rather than in a single shared field that would leak
/// between sessions.
#[derive(Clone)]
struct FileState {
    model: String,
    dir: String,
    /// Full working-directory path (from `session_meta`/`turn_context` `cwd`),
    /// kept alongside `dir` (its basename) so a resume can launch there.
    cwd: String,
    sid: String,
}

impl FileState {
    fn session(&self) -> String {
        crate::reader::session_label(&self.dir, &self.sid)
    }
}

pub struct CodexReader {
    pub(crate) sessions_dir: PathBuf,
    scanner: FileScanner<FileState>,
}

impl CodexReader {
    pub fn new(codex_dir: PathBuf) -> Self {
        let sessions_dir = codex_dir.join("sessions");
        Self {
            sessions_dir,
            scanner: FileScanner::new(),
        }
    }

    fn find_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if self.sessions_dir.exists() {
            find_recursive(&self.sessions_dir, &mut files, &|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
                    .unwrap_or(false)
            });
        }
        files
    }

    fn scan(&mut self) -> Vec<UsageRecord> {
        let files = self.find_files();
        self.scanner.scan(
            files,
            |file| FileState {
                model: "unknown".to_string(),
                dir: "codex".to_string(),
                cwd: String::new(),
                sid: extract_codex_project(file),
            },
            read_codex_from_offset,
        )
    }
}

impl UsageSource for CodexReader {
    fn platform(&self) -> Platform {
        Platform::Codex
    }

    fn scan_all(&mut self) -> Vec<UsageRecord> {
        self.scanner.reset();
        self.scan()
    }

    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        self.scan()
    }
}

fn read_codex_from_offset(
    path: &Path,
    skip_bytes: u64,
    st: &mut FileState,
) -> (Vec<UsageRecord>, u64) {
    crate::reader::read_lines_from_offset(path, skip_bytes, |line| parse_codex_line(line, st))
}

/// Parse one rollout line. `st` carries the session's running model and working
/// directory across lines and polls; meta events update it in place.
fn parse_codex_line(line: &str, st: &mut FileState) -> Option<UsageRecord> {
    let v: Value = serde_json::from_str(line).ok()?;
    let event_type = v.get("type")?.as_str()?;

    // session_meta and turn_context are top-level events (not in event_msg)
    if event_type == "session_meta" {
        let payload = v.get("payload");
        if let Some(cwd) = payload.and_then(|p| p.get("cwd")).and_then(|c| c.as_str()) {
            st.dir = crate::reader::basename(cwd);
            st.cwd = cwd.to_string();
        }
        if let Some(id) = payload.and_then(|p| p.get("id")).and_then(|i| i.as_str()) {
            st.sid = id.to_string();
        }
        return None;
    }

    if event_type == "turn_context" {
        if let Some(m) = v
            .get("payload")
            .and_then(|p| p.get("model"))
            .and_then(|m| m.as_str())
        {
            st.model = m.to_string();
        }
        if let Some(cwd) = v
            .get("payload")
            .and_then(|p| p.get("cwd"))
            .and_then(|c| c.as_str())
        {
            st.dir = crate::reader::basename(cwd);
            st.cwd = cwd.to_string();
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
            st.model = m.to_string();
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

    // `last_token_usage` carries the per-turn delta; `total_token_usage` is the
    // session's running cumulative total. When `last_token_usage` is missing
    // OR explicitly `null` (both collapse to `None` via `Option::get`+`and_then`),
    // falling back to the cumulative total would record it as a delta and wildly
    // over-count (e.g. cumulative 10k -> 20k -> 30k would sum to 60k instead of
    // 30k). Treat "no usable delta" as "nothing new happened this event" (0, 0,
    // 0) instead, matching the case where the rollout simply omits the field.
    let last = info.get("last_token_usage").filter(|l| !l.is_null());
    let delta_input = last
        .and_then(|l| l.get("input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let delta_output = last
        .and_then(|l| l.get("output_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let delta_cached = last
        .and_then(|l| l.get("cached_input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if delta_input == 0 && delta_output == 0 {
        return None;
    }

    let timestamp_str = v.get("timestamp")?.as_str()?;
    let timestamp: DateTime<Utc> = timestamp_str.parse().ok()?;

    let cost_usd = pricing::calculate_cost(&st.model, delta_input, delta_output, delta_cached, 0);

    // Codex token_count events carry no per-event id. The cumulative totals
    // change monotonically, so pairing them with the timestamp gives a stable
    // fallback identity: a truncation-triggered re-read of an unchanged line
    // reproduces the same key, while a genuinely new event has a different
    // cumulative total.
    let record_id = format!(
        "{}:{timestamp_str}:{input_tokens}:{output_tokens}:{cached}",
        st.session()
    );

    Some(UsageRecord {
        timestamp,
        platform: Platform::Codex,
        model: crate::state::intern(&st.model),
        session: crate::state::intern(&st.session()),
        session_id: crate::state::intern(&st.sid),
        cwd: crate::state::intern(&st.cwd),
        // Codex rollout files don't record a conversation title.
        title: crate::state::intern(""),
        id: crate::state::intern(&record_id),
        input_tokens: delta_input,
        output_tokens: delta_output,
        cache_read_tokens: delta_cached,
        cache_creation_tokens: 0,
        cost_usd,
    })
}

fn extract_codex_project(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.split('-').next_back().unwrap_or(s).to_string())
        .unwrap_or_else(|| "codex".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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

    fn token_count_with_last(ts: &str, input: u64, output: u64, last: &str) -> String {
        format!(
            r#"{{"type":"event_msg","timestamp":"{ts}","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{input},"output_tokens":{output},"cached_input_tokens":0}},"last_token_usage":{last}}}}}}}"#
        )
    }

    #[test]
    fn session_meta_populates_session_id_and_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        let meta = r#"{"type":"session_meta","timestamp":"2026-05-29T09:59:00Z","payload":{"cwd":"/Users/me/proj","id":"sess-42"}}"#.to_string();
        write_rollout(
            &sessions,
            "rollout-meta.jsonl",
            &[
                meta,
                turn_context("gpt-5.4"),
                token_count("2026-05-29T10:00:00Z", 100, 50),
            ],
        );
        let mut reader = reader_for(&sessions);
        let records = reader.scan_all();
        assert_eq!(records.len(), 1);
        // The real id and working dir must be threaded through for resume.
        assert_eq!(crate::state::resolve(records[0].session_id), "sess-42");
        assert_eq!(crate::state::resolve(records[0].cwd), "/Users/me/proj");
    }

    /// A rollout whose `last_token_usage` is `null` (as opposed to absent)
    /// must not fall back to the cumulative total as if it were a delta —
    /// that would record 10k, then 20k, then 30k as deltas summing to 60k
    /// instead of the true cumulative 30k.
    #[test]
    fn null_last_token_usage_does_not_inflate_cumulative_into_a_delta() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        write_rollout(
            &sessions,
            "rollout-1.jsonl",
            &[
                turn_context("gpt-5.4"),
                token_count_with_last("2026-05-29T10:01:00Z", 10_000, 0, "null"),
                token_count_with_last("2026-05-29T10:02:00Z", 20_000, 0, "null"),
                token_count_with_last("2026-05-29T10:03:00Z", 30_000, 0, "null"),
            ],
        );

        let mut reader = reader_for(&sessions);
        let records = reader.scan_all();
        // With a null last_token_usage there is no usable per-event delta, so
        // no records should be emitted (not 60k worth of phantom deltas).
        let total_input: u64 = records.iter().map(|r| r.input_tokens).sum();
        assert_eq!(
            total_input, 0,
            "null last_token_usage must not be treated as a delta, got total {total_input}"
        );
    }

    #[test]
    fn poll_delta_returns_nothing_when_file_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        write_rollout(
            &sessions,
            "rollout-1.jsonl",
            &[
                turn_context("gpt-5.4"),
                token_count("2026-05-29T10:01:00Z", 100, 50),
            ],
        );

        let mut reader = reader_for(&sessions);
        assert_eq!(reader.scan_all().len(), 1);
        assert!(
            reader.poll_delta().is_empty(),
            "unchanged file re-emitted records"
        );
    }

    #[test]
    fn appended_token_count_keeps_its_own_files_model() {
        let dir = tempfile::tempdir().unwrap();
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).unwrap();

        // Two sessions with different models, each only a turn_context so far.
        let file_a = write_rollout(&sessions, "rollout-a.jsonl", &[turn_context("gpt-5.4")]);
        write_rollout(
            &sessions,
            "rollout-b.jsonl",
            &[turn_context("gpt-5.3-codex")],
        );

        let mut reader = reader_for(&sessions);
        assert!(reader.scan_all().is_empty(), "no token_count yet");

        // A token_count is appended to file A only; its turn_context line was
        // already consumed, so the model must still resolve to file A's model.
        let mut f = fs::OpenOptions::new().append(true).open(&file_a).unwrap();
        f.write_all(format!("{}\n", token_count("2026-05-29T10:05:00Z", 200, 80)).as_bytes())
            .unwrap();

        let delta = reader.poll_delta();
        assert_eq!(delta.len(), 1);
        assert_eq!(
            crate::state::resolve(delta[0].model),
            "gpt-5.4",
            "model leaked from another session"
        );
    }
}
