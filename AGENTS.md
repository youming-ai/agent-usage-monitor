# Agent Instructions

`aum`: Rust TUI + CLI + MCP server for Claude Code and Codex local usage, cost, and live quota.

## Build
- Build: `cargo build --release` → `target/release/aum`
- Run: `cargo run -- <args>`
- Test: `cargo test` (fixtures: `cargo test --test reader_fixtures`)
- Before commit: `cargo fmt && cargo clippy`

## Commit Attribution
AI commits MUST include:
```
Co-Authored-By: Claude <noreply@anthropic.com>
```

## Layout
| Path | Purpose |
|------|---------|
| `src/platforms.rs` | `REGISTRY`: paths, colors, readers, quota clients per platform |
| `src/reader/` | Incremental JSONL readers, pricing |
| `src/readers.rs` | Persistent reader lifecycle |
| `src/watcher.rs` | FS watcher (50ms debounce) + fallback poll |
| `src/state/` | `AppState` (`Arc<RwLock<_>>`), bounded record window |
| `src/quota/` | Claude / Codex quota API clients |
| `src/ui/` | Ratatui rendering |
| `src/stats.rs`, `src/mcp/` | JSON report, MCP server |
| `src/updater/` | Self-update (atomic rename) |
| `tests/fixtures/` | Real log samples for reader regressions |

## Key Conventions
- Blocking I/O (disk, HTTP) → `tokio::task::spawn_blocking`
- Readers are persistent (`SharedReader`); never rebuild per refresh (loses offsets → double counting)
- UI uses `AppState::try_read`; skip frame on contention, never block
- Default data paths live only in `src/platforms.rs`
- Logs/diagnostics → stderr; stdout stays clean for JSON/MCP
- Releases via release-please; use conventional commits
