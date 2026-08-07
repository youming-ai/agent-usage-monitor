# Repository Guidelines

## Project Overview
`agent-usage-monitor` (`aum`) is a single Rust binary that provides a terminal user interface (TUI) dashboard and a command-line interface (CLI) to track local AI agent usage, costs, and API quotas. It currently monitors two developer agent tools: Claude Code and Codex.

## Architecture & Data Flow
- **Data Ingestion**: A dual-track system uses a file system watcher (`src/watcher.rs` based on the `notify` crate) with 50ms debouncing, alongside the configurable fallback timer interval (5 seconds by default). Watcher paths drive targeted reader refreshes; fallback ticks perform full discovery. Logs from various platforms are abstracted via the fallible `UsageSource` trait (JSONL log files).
- **State Management**: The global state `AppState` is wrapped in `Arc<RwLock<AppState>>` for safe multi-threaded sharing. It maintains a fixed-size `PlatformState` array indexed by the platform enum's value. Repetitive model and session strings use `ThreadedRodeo`; unique record identities use fixed-size hashes instead (interning them would retain every source string for the process lifetime). Records are bounded by the configured sliding window; the dedup identity set deliberately is not, so re-reading one file can never displace another file's records.
- **Platform Registry**: Runtime platform metadata (labels, colors, availability rules, default paths, reader factories, and quota/account capabilities) is managed centrally in `src/platforms.rs` (`REGISTRY`). Adding a platform requires a registry entry plus the explicit `Platform`, CLI, and config schema variants, but does not require orchestration changes in `main.rs`.
- **TUI & UI Rendering**: The TUI runs on its own thread, utilizing `std::sync::RwLock`'s `try_read()` method to fetch state from `AppState`. If a lock conflict occurs, the frame is skipped to prevent any interface lag. Available platforms are stacked vertically (no tabs) in `src/ui/mod.rs`, each rendering a header line, optional quota row, and per-model table.
- **Data Flow**:
  1. Log file modified -> `PlatformWatcher` detects change and notifies main loop via `WatcherMessage::Event` over a `tokio::sync::mpsc` channel.
  2. Main loop schedules reader tasks on `tokio::task::spawn_blocking`.
  3. The reader locks a `Mutex` on its persistent instance, uses the watcher path when available, polls only new log lines since the last byte offset or row ID cursor, and parses `UsageRecord`s.
  4. Global write lock (`state.write()`) is acquired; new records are merged and old records outside the sliding window are evicted.
  5. UI redraws every 250ms (Tick).

## Key Directories
- `src/`: Core source files of the application.
  - `src/reader/`: Concrete implementations of log readers for various platforms.
  - `src/quota/`: API clients for fetching platform quotas (such as Claude or Codex).
  - `src/ui/`: Ratatui-based TUI components and layout rendering.
  - `src/state/`: State management, model pricing configurations, and string interning.
- `tests/`: Integration tests.
  - `tests/fixtures/`: Raw log files used for testing the readers against real inputs.
- `.github/workflows/`: Automated CI/CD release configurations.

## Development Commands
- **Build**: `cargo build --release` (Generates binary at `target/release/aum`)
- **Test**:
  - Run all tests: `cargo test`
  - Run specific integration tests: `cargo test --test stats` or `cargo test --test readers`
  - Run fixture tests only: `cargo test --test reader_fixtures`
  - Run pricing unit tests: `cargo test reader::pricing`
- **Lint**: `cargo clippy`
- **Format**: `cargo fmt`
- **Run**: `cargo run -- <args>`

## Code Conventions & Common Patterns
- **Blocking I/O Isolation**: Because disk/SQLite reads and API calls are blocking, they must always be run using `tokio::task::spawn_blocking` to avoid stalling the main Tokio reactor.
- **Persistent Readers**: All readers are stored in a `SharedReader` (`Arc<Mutex<Box<dyn UsageSource>>>`) within `PlatformReaders`. Rebuilding them on every refresh discards cursors/offsets, leading to double-counting and performance degradation.
- **No-Blocking UI**: UI rendering must never block. Use `AppState::try_read` instead of `read` or `unwrap`.
- **Formatting**: Always run `cargo fmt` and `cargo clippy` before committing code.
- **Default Paths**: Each agent's default data directory is defined once in `Platform::default_path` (`src/state/app_state.rs`); config defaults are thin shims around it.

## Important Files
- `src/main.rs`: Entry point containing the asynchronous runner, initializing watchers, TUI, and state update loops.
- `src/platforms.rs`: The central registry for all platforms (`REGISTRY`).
- `src/readers.rs`: Handles lifecycle and concurrency control of persistent platform readers.
- `src/watcher.rs`: Listens for FS changes, filters events, and communicates with the main task.
- `src/state/app_state.rs`: Holds the global dashboard state and handles string interning.
- `src/updater/mod.rs`: Handles safe in-place self-upgrades without shell scripting (uses `ureq`, `flate2`, and `tar`).
- `Cargo.toml`: Declares project dependencies, features, and binary information.

## Runtime/Tooling Preferences
- **Runtime**: Native binary built via Rust compiler (Cargo). Uses the `tokio` runtime for asynchronous orchestration.
- **Release Automation**: Release-please manages versioning and changelogs automatically. Release workflows compile targets for four platforms:
  - macOS ARM64 (`aum-darwin-arm64.tar.gz`)
  - macOS AMD64 (`aum-darwin-amd64.tar.gz`)
  - Linux AMD64 (`aum-linux-amd64.tar.gz`)
  - Linux ARM64 (`aum-linux-arm64.tar.gz`)
- **Self-Update**: Uses atomic rename (`std::fs::rename`) on a temporary file `.{BINARY_NAME}.update.tmp` in the same directory to prevent `ETXTBSY` (text file busy) and `EXDEV` (cross-device link) errors.

## Testing & QA
- **Unit Tests**: Inline tests in modules like `src/reader/pricing.rs` and `src/state/app_state.rs` testing details like price matching and state eviction.
- **Integration Tests**: Grouped under the `tests/` directory. Uses `tests/reader_fixtures.rs` to run regressions against actual committed logs in `tests/fixtures/`.
- **E2E/CLI Tests**: `tests/stats.rs` runs process-level tests against the compiled binary.
- **MCP Tests**: `tests/mcp.rs` tests JSON-RPC interfaces over stdin/stdout.
