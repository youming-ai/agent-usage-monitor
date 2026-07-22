# Agent Usage Monitor

Real-time terminal dashboard for **Claude Code**, **Codex** & **Cursor CLI** usage — quota windows, token usage, and cost, read straight from local log files. No API keys required. The command is `aum`.

![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/youming-ai/agent-usage-monitor/main/install.sh | sh
```

Also available from [Releases](https://github.com/youming-ai/agent-usage-monitor/releases), or build from source with `cargo build --release`.

## Usage

```bash
aum                # start monitoring (defaults below)
aum -r 2           # refresh every 2 seconds
aum update         # self-update to the latest release (--dry-run / --force)
aum config         # show current configuration
aum config set <key> <value>  # set a configuration value
aum config reset   # reset configuration to defaults
aum mcp           # run as MCP server over stdio (no TUI)
aum stats         # print JSON usage report (no TUI)
```

| Flag | Default | Description |
|------|---------|-------------|
| `--claude-path` | `~/.claude/projects` | Claude Code data directory |
| `--codex-path` | `~/.codex` | Codex data directory |
| `--cursor-path` | `~/.cursor` | Cursor CLI data directory |
| `-r, --refresh` | `5` | Poll interval, in seconds |

**Keys:** `Tab` / `←` / `→` switch tab · `r` clear current tab · `q` quit

Only tabs for agents whose data directory exists are shown in the TUI.

## JSON stats

For scripts, CI, and external monitoring, `aum` can emit a structured JSON
report without launching the TUI:

```bash
# Pretty-printed to stdout (TTY auto-detected)
aum stats

# Compact JSON, suitable for `jq`
aum stats --compact | jq '.platforms.claude_code.totals.cost_usd'

# Filter to specific agents
aum stats --platform claude_code,codex

# Time-bounded report
aum stats --since 2026-06-01 --until 2026-06-30

# Include live quota (Claude / Codex; requires local credentials)
aum stats --platform claude_code --include-quota
```

### Schema

| Field | Type | Description |
|---|---|---|
| `generated_at` | RFC 3339 timestamp | When the report was generated |
| `platforms` | object | Per-platform breakdown keyed by `config_key` (e.g. `claude_code`) |
| `platforms.<k>.available` | bool | Whether the data directory exists |
| `platforms.<k>.data_path` | path | Resolved path (CLI override > config > default) |
| `platforms.<k>.totals` | object | Aggregate: `calls`, `cost_usd`, `input_tokens`, `output_tokens`, `cache_read_tokens`, `cache_creation_tokens` |
| `platforms.<k>.models` | object | Per-model breakdown, same fields plus `sessions` |
| `platforms.<k>.sessions[]` | array | Per-session summary with `models` list |
| `platforms.<k>.dates` | object | Per-day bucket keyed by `YYYY-MM-DD` |
| `platforms.<k>.quota` | object | Only present with `--include-quota`, only for Claude / Codex |
| `totals` | object | Cross-platform: `total_calls`, `total_cost_usd`, `platforms_with_data` |

Top-level keys are alphabetically ordered (BTreeMap). Use `--compact` for
scripting, default (pretty) for human reading.

### Configuration

Configuration is stored in `~/.config/aum/config.toml` (or platform equivalent). Available keys:

- `claude_path` - Path to Claude Code data directory
- `codex_path` - Path to Codex data directory
- `cursor_path` - Path to Cursor CLI data directory
- `refresh` - Polling interval in seconds
- `max_records` - Maximum number of records to keep in memory

Example:
```bash
aum config set refresh 2
aum config set max_records 200
aum config set cursor_path ~/.cursor
```

## Real-time updates

The TUI uses `notify` (FSEvents on macOS, inotify on Linux, ReadDirectoryChangesW
on Windows) to react to file changes immediately, instead of polling on a timer.
A 50 ms per-platform debounce coalesces bursts of writes; a 30 s fallback poll
ensures the display stays current even if a FS event is dropped.

Network filesystems (NFS, SMB) are not officially supported by `notify`; on those
paths the 30 s fallback still works.

## MCP server

`aum` can run as an MCP (Model Context Protocol) server, exposing usage
data to AI agent CLIs (Claude Code, Cursor, Copilot) via JSON-RPC over stdio:

```bash
aum mcp
```

### Client configuration

Add to your client's MCP config (e.g., `~/.config/claude-code/.mcp.json`):

```json
{
  "mcpServers": {
    "aum": {
      "command": "aum",
      "args": ["mcp"]
    }
  }
}
```

### Tools

| Tool | Description |
|---|---|
| `get_daily_stats` | Daily usage breakdown (calls, cost, tokens, models) |
| `get_model_usage` | AI model usage counts (sorted desc) |
| `get_cost_breakdown` | Cost over a date range |
| `get_file_operations` | File read/edited/added/deleted counts (returns 0 in current spec; reader-side data not yet collected) |
| `get_session_stats` | Per-session summary |
| `get_quota` | Live Claude Code / Codex quota |

### Resources

| URI | Description |
|---|---|
| `aum://summary` | Cross-platform totals (calls, cost, platforms with data) |
| `aum://platforms` | Platform index with availability and resolved paths |

### Manual smoke test

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}' | aum mcp
```

Expect a JSON response with `serverInfo.name = "aum"`.

## Layout

Each agent tab uses the same layout; only the accent color and data source change. Claude and Codex also show quota bars when credentials are available.

### Claude Code (quota + usage)

```
 CLAUDE   codex   CURSOR                                              ✓ you@mail.com
───────────────────────────────────────────────────────────────────────────────
 ✓ 5h ▓▓▓▓▓▓▓▓▓▓░░  82%  resets 2h30m
 ✓ 7d ▓▓▓▓▓▓░░░░░░  54%  resets 4d6h
┌ CLAUDE models (42) ───────────────────────────────────────────────────────────┐
│ MODEL          INPUT   OUTPUT   CACHE     COST     #                          │
│ claude-opus-4  1.2M    340.0k   8.1M      $12.34   42                         │
└───────────────────────────────────────────────────────────────────────────────┘
┌ sessions ─────────────────────────────────────────────────────────────────────┐
│ SESSION                       TOKENS    REQUESTS                              │
│ agent-usage-monitor a3f2c1d8  10.5k     2                                     │
│ my-web-app 9b4e7f02           2.3k      1                                     │
└───────────────────────────────────────────────────────────────────────────────┘
 42 calls · $12.34                                                    tab·r·q
```

### Cursor CLI (usage only)

```
 CURSOR   CLAUDE   codex
───────────────────────────────────────────────────────────────────────────────
 (no quota API — usage from transcripts and store.db)
┌ CURSOR models (3) ────────────────────────────────────────────────────────────┐
│ MODEL               INPUT   OUTPUT   CACHE   COST     #                      │
│ claude-sonnet-4-5   8.4k    2.1k     0      $0.00    3                       │
└───────────────────────────────────────────────────────────────────────────────┘
┌ sessions ─────────────────────────────────────────────────────────────────────┐
│ SESSION                       TOKENS    REQUESTS                              │
│ myproject a3f2c1d8            3.2k      1                                     │
└───────────────────────────────────────────────────────────────────────────────┘
 3 calls · $0.00                                                      tab·r·q
```

- **Quota bars** — one per window (Claude & Codex only); the fill shows remaining usage, with a status glyph (`✓` ≥50%, `⚠` ≥20%, `✗` <20%) and reset time.
- **models** — per-model totals: tokens, cost, and request count.
- **sessions** — per-conversation usage (tokens, requests), labelled `<dir> <id>` so multiple sessions in one project stay distinct.

Each platform uses an accent color matched to its official CLI theme or brand palette (Claude orange, Codex magenta, Cursor cyan); everything else stays default or dimmed. These are defined in `src/state/app_state.rs` (`Tab::primary_color`).

## How it works

`aum` reads data that the supported agents already write locally — no network calls for usage records, no API keys:

- **Claude Code** — `~/.claude/projects/**/*.jsonl`
- **Codex** — `~/.codex/sessions/**/rollout-*.jsonl`
- **Cursor CLI** — `~/.cursor/projects/**/agent-transcripts/**/*.jsonl`, `~/.cursor/chats/**/store.db`, and `~/.config/cursor/chats/**/store.db`

Quota percentages come from the official endpoints, authenticated with your existing local credentials (Claude: macOS Keychain or `~/.claude/.credentials.json`; Codex: `~/.codex/auth.json`). Cost is computed from built-in pricing tables for Anthropic & OpenAI models; unknown models show `$0.00`.

### Adding a new agent

Platform wiring lives in `src/platforms.rs` (`RegistryEntry`). A new agent needs one registry row (path keys, reader factory) instead of scattered changes across `main.rs` and config handlers.

### Tests

Unit tests live under `src/`; committed log samples under `tests/fixtures/` are scanned by integration tests in `tests/reader_fixtures.rs` to catch format regressions:

```bash
cargo test
```

## License

[MIT](LICENSE)