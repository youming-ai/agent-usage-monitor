# AGENTS.md — agent-usage-monitor

## Project snapshot

Single Rust binary (`aum`) — a TUI dashboard that reads local agent logs (JSONL / SQLite) and displays per-model usage, cost, and quota. Supports 13 agents: Claude Code, Codex, opencode, Kimi Code, pi, openclaw, hermes-agent, Factory AI, Grok Build, Cursor CLI, Copilot CLI, Antigravity CLI, MiMo Code.

## Build & test

```bash
cargo build --release          # binary: target/release/aum
cargo test                     # unit + integration tests
cargo test --test reader_fixtures   # fixture tests only
```

## Architecture (high-signal)

### Adding a new agent
The only file you must edit is `src/platforms.rs`. Add one `RegistryEntry` to `REGISTRY` — it wires the path keys, reader factory, and config setter. No changes needed in `main.rs` or CLI handlers.

### Core modules
- `src/platforms.rs` — `RegistryEntry` table; single source of truth for platform wiring.
- `src/reader/` — One reader per agent. Most JSONL readers implement `JsonlReader` (see `jsonl_reader.rs`); SQLite readers (opencode, hermes) implement `UsageSource` directly.
- `src/reader/pricing.rs` — Hard-coded USD-per-1M-token tables. Update when upstream pricing changes. Unknown models show `$0.00`.
- `src/state/app_state.rs` — Per-platform state fields (records, sessions, totals, quota). Each platform has its own `add_*_records` / `clear_*` method. Eviction from the bounded ring reverses the per-model aggregate but **not** lifetime totals.
- `src/quota/` — Quota fetchers (Claude, Codex). Use local credentials (Keychain / `~/.claude/.credentials.json` / `~/.codex/auth.json`).
- `src/ui/` — ratatui widgets. Accent colors are defined in `Tab::primary_color()` in `app_state.rs`.

### Key quirks
- **Reader tasks run in `task::spawn_blocking`** — `UsageSource` uses `std::sync::Mutex`, not `tokio::sync::Mutex`, because the reader does blocking I/O.
- **Refresh is clamped to ≥1s** — `tokio::time::interval` panics on zero; `main.rs` enforces `refresh.max(1)`.
- **opencode and MiMo Code paths are XDG, not macOS App Support** — `config.rs` has `xdg_data_dir()`; on macOS this resolves to `~/.local/share/opencode` and `~/.local/share/mimocode`, not `~/Library/Application Support/`.
- **Tab availability detection is platform-specific** — Some agents check for subdirectories/files (e.g., Hermes checks `state.db`, Cursor checks `projects` or `chats`, Grok checks `sessions`). See `Tab::is_available_at()`.

### Session labels
Format: `<basename> <short-id>`. The short-id is the first 8 **characters** (not bytes) to avoid panics on multibyte UTF-8 in free-form log data.

## Configuration

Stored in `~/.config/aum/config.toml` (platform equivalent). CLI `--*-path` args override config values.

```bash
aum config set refresh 2
aum config set max_records 200
aum config set grok_path ~/.grok
```

## Tests

- **Unit tests** — Inline in `src/` modules (e.g., `reader::claude`, `state::app_state`, `platforms`).
- **Integration tests** — `tests/reader_fixtures.rs` scans committed samples under `tests/fixtures/` to catch format regressions. Add a new fixture directory when adding a new agent reader.
- **Pricing tests** — `reader/pricing.rs` has regression tests for specific-vs-base model matching (e.g., `gpt-4.1-mini` must not match `gpt-4.1`).

## Release workflow

- Uses **release-please** (`release-please-config.json`).
- Release assets are `aum-darwin-arm64.tar.gz` and `aum-linux-amd64.tar.gz` (see `install.sh` and `src/updater/platform.rs`).
- `CHANGELOG.md` currently has an **unresolved merge conflict** (lines 3–7). Resolve before the next release.

## What NOT to put here

- Generic Rust advice (cargo, clippy, rustfmt) — follows standard conventions.
- Exhaustive file tree — structure is obvious from `src/`.
- TUI tutorial — ratatui docs cover it.
