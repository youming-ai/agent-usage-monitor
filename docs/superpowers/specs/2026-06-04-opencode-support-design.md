# opencode support (usage now, quota slot reserved)

- **Date:** 2026-06-04
- **Status:** Approved — ready for implementation plan
- **Branch:** `feat/opencode-support`

## Context & motivation

`aum` currently monitors two agents — Claude Code and Codex — each as a tab
showing per-model usage, per-session usage, and (for Claude/Codex) a quota
panel. We want to add support for more agents, starting with
[opencode](https://opencode.ai). This is also the first step toward a reusable
"add an agent" path, so the reader layer is generalized as part of this work.

## Goals

- Add an **opencode** tab with the same per-model and per-session usage views
  the existing tabs have, sourced from opencode's local data.
- Generalize the reader layer behind a `UsageSource` trait so Claude, Codex,
  and opencode are wired uniformly (and future agents slot in cheaply).
- Reserve a quota slot for opencode so the Go-plan quota can be added later
  with minimal change.

## Non-goals (scope guardrails)

- **No opencode quota fetching now.** Research (below) shows opencode-go has no
  queryable balance/usage API today. The opencode quota panel shows
  `no quota data`.
- No multi-provider balance scraping, no billed gateway requests.
- No restructuring of `AppState`'s per-platform named fields into a generic
  map — we only add an `add_records(platform, …)` dispatcher.

## Background: opencode's data model (verified on a live install, v1.15.13)

- **Storage is SQLite**, not JSONL: `~/.local/share/opencode/opencode.db`
  (WAL mode). The project already depends on `rusqlite` (bundled), so reading
  it needs no new dependency.
- **Per-call data** lives in the `message` table. Each assistant row's `data`
  column is JSON with keys: `role`, `modelID`, `providerID`, `cost`,
  `tokens {total, input, output, reasoning, cache {read, write}}`,
  `time {created}`. (~10k assistant messages on the test install.)
- The `message` row also has `session_id` and `time_created` (epoch ms,
  integer) columns. `session.directory` gives the working directory; join
  `message.session_id → session.id`.
- The `session` table additionally carries pre-aggregated columns
  (`cost`, `tokens_input/output/reasoning/cache_read/cache_write`, `model`,
  `directory`, `slug`, `title`). We do **not** use these — per-message gives
  the same granularity as Claude/Codex.
- `auth.json` holds provider API keys as `{ key, type }` (e.g. `opencode-go`,
  `deepseek`). The `account`/`account_state`/`control_account` tables are for
  the console OAuth login and were empty on the test install.

### Quota research conclusion (why quota is deferred)

opencode-go (the "Zen"/Go hosted gateway) has **no available balance/usage
API** today:

- Official docs + open feature requests confirm it:
  [#10448](https://github.com/anomalyco/opencode/issues/10448) (proposed
  `GET /zen/v1/balance`, unimplemented) and
  [#16017](https://github.com/anomalyco/opencode/issues/16017) ("no API to
  retrieve Go plan usage; only the web dashboard").
- opencode reads IETF rate-limit response headers
  (`x-ratelimit-limit/remaining/reset-*`) which appear **only on billed model
  responses**, and it does not persist them to a readable local file/table.
- Empirically (with the configured `opencode-go` key): `/zen/go/v1/models`
  returns 200 but carries no rate-limit/balance headers; 9 candidate
  balance/usage endpoints all 404.
- The balance lives behind the console OAuth dashboard
  (`opencode.ai/workspace/{id}`), not the gateway `sk-` key.

When opencode ships the proposed endpoint, quota can be added in
`src/quota/opencode.rs` + one line in the quota task.

## Design

### 1. `UsageSource` trait (reader unification)

New trait (in `src/reader/mod.rs`):

```rust
pub trait UsageSource: Send {
    fn platform(&self) -> Platform;
    fn scan_all(&mut self) -> Vec<UsageRecord>;
    fn poll_delta(&mut self) -> Vec<UsageRecord>;
}
```

- `ClaudeReader` / `CodexReader`: explicit `impl UsageSource` delegating to
  their existing `JsonlReader::scan_all/poll_delta` (no blanket impl — avoids
  trait-coherence issues with the non-JSONL opencode reader).
- `OpencodeReader`: implements `UsageSource` directly (SQLite).
- `main.rs` holds the readers as `Vec<Arc<Mutex<Box<dyn UsageSource>>>>` and
  spawns **one** uniform reader task per source (initial `scan_all`, then a
  `poll_delta` loop on `tokio::time::interval`, each call in `spawn_blocking`,
  recovering a poisoned lock via `unwrap_or_else(|e| e.into_inner())`). Records
  are written back with `state.add_records(source.platform(), recs)`. This
  collapses the three near-duplicate reader blocks into one.

### 2. `OpencodeReader` (`src/reader/opencode.rs`)

- `new(opencode_path)`: open `opencode_path/opencode.db` **read-only**
  (`OpenFlags::SQLITE_OPEN_READ_ONLY`). WAL mode permits concurrent readers;
  each auto-committed query sees the latest committed data. If the DB is
  absent/unopenable, store `None` and make every method return empty (graceful
  when opencode isn't installed).
- Incremental cursor: track the max `time_created` seen. `scan_all` uses
  cursor `0`; `poll_delta` uses `WHERE m.time_created > cursor`. (Millisecond
  granularity + sequential writes make same-ms loss negligible; documented.)
- Query:
  ```sql
  SELECT m.data, s.directory, m.time_created
  FROM message m JOIN session s ON m.session_id = s.id
  WHERE json_extract(m.data,'$.role') = 'assistant'
    AND m.time_created > ?cursor
  ORDER BY m.time_created
  ```
  Parse `m.data` with serde_json into a `UsageRecord`. Rows without a `tokens`
  object (errors, etc.) parse to `None` and are skipped, mirroring the Claude
  parser.

### 3. `message.data` → `UsageRecord` mapping

| `UsageRecord` field    | Source                                  | Notes |
|------------------------|-----------------------------------------|-------|
| `timestamp`            | `time.created` (epoch ms)               | → `DateTime<Utc>` |
| `model`                | `"{providerID}/{modelID}"`              | e.g. `ollama/kimi-k2.6:cloud` — opencode is multi-provider, prefix disambiguates |
| `input_tokens`         | `tokens.input`                          | |
| `output_tokens`        | `tokens.output + tokens.reasoning`      | reasoning folded into output (billed as output) |
| `cache_read_tokens`    | `tokens.cache.read`                     | |
| `cache_creation_tokens`| `tokens.cache.write`                    | |
| `cost_usd`             | `cost`                                  | often 0 for local models |
| `session`              | `session_label(basename(directory), session_id)` | same scheme as Claude/Codex |
| `platform`             | `Platform::OpenCode`                    | |

### 4. State & UI

- **`Platform`** and **`Tab`** each gain `OpenCode`. `Tab::next/prev` becomes a
  3-way cycle. Label `OPENCODE`. Accent color `Rgb(16, 185, 129)` (emerald),
  distinct from Claude orange and Codex blue.
- **`AppState`**: add `opencode_records`, `opencode_sessions`,
  `opencode_total_calls`, `opencode_total_cost`, `opencode_quota`,
  `opencode_max_records`. Refactor the identical bodies of
  `add_claude_records`/`add_codex_records` into a private helper, and add
  `add_records(&mut self, platform: Platform, records: Vec<UsageRecord>)` that
  dispatches to the matching per-platform fields. `main.rs` calls
  `add_records`.
- **`tabs.rs`**: render three tab entries. **`ui/mod.rs`**: extend the
  active-tab data `match` with an opencode arm. The model table, session
  table, and cumulative-total status bar are reused unchanged.
- **Quota panel**: `opencode_quota` stays `None`; the render layer
  special-cases the opencode tab to draw a static `no quota data` line, rather
  than the `loading…` shown for a not-yet-fetched Claude/Codex quota.

### 5. Quota slot reservation

New `src/quota/opencode.rs` with `pub fn fetch_quota() -> Option<QuotaInfo>`
returning `None`, with a comment referencing issues #16017 / #10448. The quota
task does not call it yet. Implementing the endpoint later means filling in
this function and adding one fetch line in the quota task.

### 6. Config

- `Config.opencode_path: PathBuf`, default = `$XDG_DATA_HOME/opencode` if set,
  else `~/.local/share/opencode`. **Do not use `dirs::data_dir()`** — on macOS
  it returns `~/Library/Application Support`, but opencode uses the XDG path on
  all platforms. The reader appends `opencode.db`.
- CLI: add `--opencode-path: Option<PathBuf>`; merge with
  `args.opencode_path.unwrap_or(config.opencode_path)`.
- `config set`: accept the `opencode_path` key (update the "Available keys"
  message). `opencode_max_records = max_records`.

### 7. Testing

- **`OpencodeReader`**: build an in-memory DB (`Connection::open_in_memory()`)
  with minimal `session` + `message` tables; insert one session, two assistant
  messages, and one user message; assert `scan_all` returns the two records
  with correct fields and skips the user row; insert a third assistant message
  and assert `poll_delta` returns only it. Add a `message.data` JSON fixture
  test for the `UsageRecord` mapping (including the `provider/model` join and
  reasoning-into-output folding).
- Reuse the existing aggregation/UI tests; add an opencode-tab render smoke
  test (asserts the `OPENCODE` tab and a model row render, and that the quota
  line reads `no quota data`).

## Files touched

- `src/reader/mod.rs` — `UsageSource` trait + `impl`s for Claude/Codex readers.
- `src/reader/opencode.rs` — **new** SQLite reader.
- `src/state/app_state.rs` — `Platform::OpenCode`, `Tab::OpenCode` (+ 3-way
  next/prev, label, color), opencode `AppState` fields, `add_records` dispatch.
- `src/quota/opencode.rs` — **new** reserved `fetch_quota` returning `None`.
- `src/quota/mod.rs` — register `pub mod opencode;`.
- `src/ui/mod.rs` — opencode arm in the render data match; opencode quota line.
- `src/ui/tabs.rs` — render three tabs.
- `src/config.rs` — `opencode_path` field + default.
- `src/cli.rs` — `--opencode-path` arg.
- `src/main.rs` — `opencode_path` merge; unified reader-task loop over
  `Vec<dyn UsageSource>`.

## Future work (out of scope here)

- opencode-go quota once the upstream usage/balance endpoint ships
  (fill in `src/quota/opencode.rs`).
- The `UsageSource` trait makes adding further agents mostly a new reader +
  enum variants + a render arm.
