# Agent Usage Monitor

Real-time terminal dashboard for **Claude Code** & **Codex** usage — quota windows, token usage, and cost, read straight from local log files. No API keys required. The command is `aum`.

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
| `-r, --refresh` | `5` | Poll interval, in seconds |

**Keys:** `Tab` / `←` / `→` switch tab · `r` clear current tab · `q` quit

### Configuration

Configuration is stored in `~/.config/aum/config.toml` (or platform equivalent). Available keys:

- `claude_path` - Path to Claude Code data directory
- `codex_path` - Path to Codex data directory
- `refresh` - Polling interval in seconds
- `max_records` - Maximum number of records to keep in memory

Example:
```bash
aum config set refresh 2
aum config set max_records 200
```

## Layout

```
 CLAUDE   codex                                   ✓ you@mail.com
────────────────────────────────────────────────────────────────
 ✓ 5h ▓▓▓▓▓▓▓▓▓▓░░  82%  resets 2h30m
 ✓ 7d ▓▓▓▓▓▓░░░░░░  54%  resets 4d6h
┌ CLAUDE models (42) ──────────────────────────────────────────┐
│ MODEL          INPUT   OUTPUT   CACHE     COST     #          │
│ claude-opus-4  1.2M    340.0k   8.1M      $12.34   42         │
└───────────────────────────────────────────────────────────────┘
┌ sessions ────────────────────────────────────────────────────┐
│ SESSION                       TOKENS    REQUESTS             │
│ agent-usage-monitor a3f2c1d8  10.5k     2                    │
│ my-web-app 9b4e7f02           2.3k      1                    │
└───────────────────────────────────────────────────────────────┘
 42 calls · $12.34                                  tab·r·q
```

- **Quota bars** — one per window; the fill shows remaining usage, with a status glyph (`✓` ≥50%, `⚠` ≥20%, `✗` <20%) and reset time.
- **models** — per-model totals: tokens, cost, and request count.
- **sessions** — per-conversation usage (tokens, requests), labelled `<dir> <id>` so multiple sessions in one project stay distinct.

Each platform uses one accent color (Claude orange, Codex blue); everything else stays default or dimmed.

## How it works

`aum` reads the JSONL that Claude Code and Codex already write locally — no network calls for usage records, no API keys:

- **Claude Code** — `~/.claude/projects/**/*.jsonl`
- **Codex** — `~/.codex/sessions/**/rollout-*.jsonl`

Quota percentages come from the official endpoints, authenticated with your existing local credentials (Claude: macOS Keychain or `~/.claude/.credentials.json`; Codex: `~/.codex/auth.json`). Cost is computed from built-in pricing tables for Anthropic & OpenAI models; unknown models show `$0.00`.

## License

[MIT](LICENSE)
