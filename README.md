# Agent Usage Monitor

Real-time terminal dashboard for **Claude Code** & **Codex** local usage, plus live quota for **Claude Code** and **Codex**. No API keys required for local usage records; vendor credentials are used only for live quota. The command is `aum`.

![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/youming-ai/agent-usage-monitor/main/install.sh | sh
```

The installer verifies release signatures with `minisign`; install it first
(`brew install minisign` or `apt install minisign`).

Also available from [Releases](https://github.com/youming-ai/agent-usage-monitor/releases), or build from source with `cargo build --release`.

## Usage

```bash
aum                # start monitoring (defaults below)
aum -r 2           # use a 2-second fallback poll
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
| `-r, --refresh` | `5` | Fallback poll interval, in seconds |

**Keys:** `q` / `Esc` quit

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
- `refresh` - Fallback polling interval in seconds (minimum: 1)
- `max_records` - Per-platform sliding-window size (default: 20000) used by all
  TUI totals and model rows. Lower it and the headline totals cover
  proportionally less history; `aum stats` is unaffected, it always reads
  the full logs

Example:
```bash
aum config set refresh 2
aum config set max_records 50000
aum config set codex_path ~/.codex
```

## Real-time updates

The TUI uses `notify` (FSEvents on macOS, inotify on Linux) to react to file
changes immediately, instead of polling on a timer.
A 50 ms per-platform debounce coalesces bursts of writes; the configured fallback
poll (5 s by default) ensures the display stays current even if a FS event is
dropped. Watcher events pass the changed path to the reader so normal refreshes
only open affected files; the fallback poll performs full discovery. Paths created
after startup are discovered by that poll and then watched.

Network filesystems (NFS, SMB) are not officially supported by `notify`; the
configured fallback poll still works there.

## MCP server

`aum` can run as an MCP (Model Context Protocol) server, exposing usage
data to AI agent CLIs (Claude Code, Codex, etc.) via JSON-RPC over stdio:

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
| `get_file_operations` | File read/edited/added/deleted / terminal command counts from local logs |
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

All platforms whose data directory exists are stacked top to bottom in order —
no tabs. Each section mirrors Claude Code’s Stats **Overview**: header,
quota, a daily contribution heatmap, and summary stats.
Detailed model/session breakdowns live in `aum stats` JSON / MCP. Sections
share the screen equally.

```
 CLAUDE                                                            ✓ you@mail.com
───────────────────────────────────────────────────────────────────────────────
 ✓ 5h ▓▓▓▓▓▓░░░░  82%  resets 2h30m  |  ✓ 7d ▓▓▓▓░░░░░░  54%  resets 4d6h
Token activity    last 12 months
Lifetime 4.77B · Peak 159M · Streak 9d (best 82d) · Longest task 3h 54m
    Sep   Oct     Nov       Dec     Jan     Feb     Mar       Apr     May       Jun     Jul     Aug
    · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · · ■
Mon · · · · · · · · · · · · · · · · · · · · · · · · · · · · · ■ ■
    · · · · · · · · · · · · · · · · · · · · · · · · · · · · ■ ■ ■
Wed · · · · · · · · · · · · · · · · · · · · · · · · · · · ■ ■ ■
    · · · · · · · · · · · · · · · · · · · · · · · · · · ■ ■ ■ ■
Fri · · · · · · · · · · · · · · · · · · · · · · · · · ■ ■ ■ ■
    · · · · · · · · · · · · · · · · · · · · · · · · · ■ ■ ■ ■
```

- **Live quota** (network) — vendor APIs using your local login: windows (5h/7d
  and model-scoped when present), plan/org, credits/extra-usage. Prefixed
  `live` in the UI. Claude: `api.anthropic.com/api/oauth/usage`; Codex:
  `chatgpt.com/backend-api/wham/usage`.
  No official day-by-day quota history is available.
- **Token activity heatmap + local summary** — from on-disk logs. The header
  shows lifetime tokens, peak day, current/best streak, and longest task. The
  grid has one column per week, all seven weekday rows (Mon…Sun), and month
  labels on top. Each day uses one colored square plus one spacer column,
  stretching across the full padded width so the terminal width decides how
  much history fits; the header reports that visible range. Empty days are
  dim dots, including the rest of the current week, while active days are
  separated colored squares; activity intensity is encoded by color.

Each platform uses an accent color matched to its official CLI theme or brand palette (Claude orange, Codex blue); everything else stays default or dimmed. These are defined in `src/platforms.rs` (`primary_color`).

## How it works

`aum` reads data that the supported agents already write locally — no network calls for usage records, no API keys:

- **Claude Code** — `~/.claude/projects/**/*.jsonl`
- **Codex** — `~/.codex/sessions/**/rollout-*.jsonl`

Quota percentages for Claude Code and Codex also come from official endpoints, authenticated with
their existing local credentials (Claude: macOS Keychain or
`~/.claude/.credentials.json`; Codex: `~/.codex/auth.json`). Cost is computed
from built-in pricing tables for Anthropic & OpenAI models; unknown models show
`$0.00`.

### Adding a new agent

Platform wiring lives in `src/platforms.rs` (`RegistryEntry`). A registry row
contains its path keys, reader factory, and optional quota/account
fetchers, keeping runtime orchestration free of platform-specific match arms.

### Tests

Unit tests live under `src/`; committed log samples under `tests/fixtures/` are scanned by integration tests in `tests/reader_fixtures.rs` to catch format regressions:

```bash
cargo test
```

## License

[MIT](LICENSE)
