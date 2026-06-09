# Agent Usage Monitor

Real-time terminal dashboard for **Claude Code**, **Codex**, **opencode**, **Kimi Code**, **pi**, **openclaw**, **hermes-agent**, **Factory AI**, **Grok Build**, **Cursor CLI**, **Copilot CLI** & **Antigravity CLI** usage — quota windows, token usage, and cost, read straight from local log files. No API keys required. The command is `aum`.

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
```

| Flag | Default | Description |
|------|---------|-------------|
| `--claude-path` | `~/.claude/projects` | Claude Code data directory |
| `--codex-path` | `~/.codex` | Codex data directory |
| `--opencode-path` | `$XDG_DATA_HOME/opencode` | opencode data directory |
| `--kimi-code-path` | `~/.kimi-code` | Kimi Code data directory |
| `--pi-path` | `~/.pi/agent/sessions` | pi data directory |
| `--openclaw-path` | `~/.openclaw/agents` | openclaw data directory |
| `--hermes-path` | `~/.hermes` | hermes-agent data directory |
| `--factory-path` | `~/.factory/projects` | Factory AI data directory |
| `--grok-path` | `~/.grok` | Grok Build data directory |
| `--cursor-path` | `~/.cursor` | Cursor CLI data directory |
| `--copilot-path` | `~/.copilot` | Copilot CLI data directory |
| `--antigravity-path` | `~/.gemini/antigravity-cli` | Antigravity CLI data directory |
| `-r, --refresh` | `5` | Poll interval, in seconds |

**Keys:** `Tab` / `←` / `→` switch tab · `r` clear current tab · `q` quit

Only tabs for agents whose data directory exists are shown in the TUI.

### Configuration

Configuration is stored in `~/.config/aum/config.toml` (or platform equivalent). Available keys:

- `claude_path` - Path to Claude Code data directory
- `codex_path` - Path to Codex data directory
- `opencode_path` - Path to opencode data directory
- `kimi_code_path` - Path to Kimi Code data directory
- `pi_path` - Path to pi data directory
- `openclaw_path` - Path to openclaw data directory
- `hermes_path` - Path to hermes-agent data directory
- `factory_path` - Path to Factory AI data directory
- `grok_path` - Path to Grok Build data directory
- `cursor_path` - Path to Cursor CLI data directory
- `copilot_path` - Path to Copilot CLI data directory
- `antigravity_path` - Path to Antigravity CLI data directory
- `refresh` - Polling interval in seconds
- `max_records` - Maximum number of records to keep in memory

Example:
```bash
aum config set refresh 2
aum config set max_records 200
aum config set grok_path ~/.grok
aum config set cursor_path ~/.cursor
aum config set copilot_path ~/.copilot
aum config set antigravity_path ~/.gemini/antigravity-cli
```

## Layout

Each agent tab uses the same layout; only the accent color and data source change. Claude and Codex also show quota bars when credentials are available.

### Claude Code (quota + usage)

```
 CLAUDE   codex   opencode   kimi-code   GROK   CURSOR                 ✓ you@mail.com
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

### Grok Build (usage only)

```
 GROK   CURSOR   CLAUDE   codex
───────────────────────────────────────────────────────────────────────────────
 (no quota API — usage from local session logs)
┌ GROK models (8) ──────────────────────────────────────────────────────────────┐
│ MODEL                 INPUT    OUTPUT   CACHE   COST     #                    │
│ grok-composer-2.5-fast  45.2k    0       0      $0.12    8                    │
└───────────────────────────────────────────────────────────────────────────────┘
┌ sessions ─────────────────────────────────────────────────────────────────────┐
│ SESSION                       TOKENS    REQUESTS                              │
│ my-project 019ea524           12.1k     4                                     │
└───────────────────────────────────────────────────────────────────────────────┘
 8 calls · $0.12                                                      tab·r·q
```

### Cursor CLI (usage only)

```
 CURSOR   GROK   CLAUDE   codex
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

Each platform uses an accent color matched to its official CLI theme or brand palette (Claude orange, Codex magenta, OpenCode peach, Kimi Code cyan, Pi sage, OpenClaw lobster orange, Hermes gold, Factory orange, Grok purple, Cursor cyan, Copilot green, Antigravity blue); everything else stays default or dimmed. These are defined in `src/state/app_state.rs` (`Tab::primary_color`).

## How it works

`aum` reads data that the supported agents already write locally — no network calls for usage records, no API keys:

- **Claude Code** — `~/.claude/projects/**/*.jsonl`
- **Codex** — `~/.codex/sessions/**/rollout-*.jsonl`
- **opencode** — `$XDG_DATA_HOME/opencode/opencode.db` (SQLite, read-only)
- **Kimi Code** — `~/.kimi-code/sessions/**/agents/*/wire.jsonl`
- **pi** — `~/.pi/agent/sessions/**/*.jsonl`
- **openclaw** — `~/.openclaw/agents/**/sessions/*.jsonl`
- **hermes-agent** — `~/.hermes/state.db` (SQLite, read-only)
- **Factory AI** — `~/.factory/projects/**/<session-uuid>.jsonl`
- **Grok Build** — `~/.grok/sessions/**/<session-id>/updates.jsonl` (+ `summary.json` for session metadata)
- **Cursor CLI** — `~/.cursor/projects/**/agent-transcripts/**/*.jsonl`, `~/.cursor/chats/**/store.db`, and `~/.config/cursor/chats/**/store.db`
- **Copilot CLI** — `~/.copilot/session-state/*/events.jsonl`
- **Antigravity CLI** — `~/.gemini/antigravity-cli/brain/*/.system_generated/logs/transcript_full.jsonl`

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