# Agent Usage Monitor

Real-time terminal monitoring tool for Claude Code & Codex API usage. Shows quota limits and token usage from local JSONL files. The command is `aum`.

![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Features

- **Claude Code** — parses `~/.claude/projects/**/*.jsonl` and shows quota from Anthropic API
- **Codex** — parses `~/.codex/sessions/**/rollout-*.jsonl` and shows quota from OpenAI API
- **Quota monitoring** — one bar per quota window showing remaining usage and reset time
- **Dual-tab TUI** — switch between Claude Code / Codex views
- **Token tracking** — input, output, and cache tokens per model
- **Cost calculation** — built-in pricing tables for Anthropic & OpenAI models
- **Real-time polling** — configurable refresh interval

## Installation

### Homebrew

```bash
brew install youming-ai/tap/aum
```

### Install script

```bash
curl -fsSL https://raw.githubusercontent.com/youming-ai/agent-usage-monitor/main/install.sh | sh
```

Or download manually from [Releases](https://github.com/youming-ai/agent-usage-monitor/releases).

### Build from source

```bash
git clone https://github.com/youming-ai/agent-usage-monitor.git
cd agent-usage-monitor
cargo build --release
```

## Usage

```bash
# Default (reads from ~/.claude/projects and ~/.codex)
aum

# Custom paths
aum --claude-path /path/to/.claude/projects --codex-path /path/to/.codex

# Adjust refresh rate (seconds)
aum --refresh 2

# Check for updates
aum update

# Check for updates without installing
aum update --dry-run

# Force update even if already on latest
aum update --force
```

### CLI Options

| Option | Default | Description |
|--------|---------|-------------|
| `--claude-path` | `~/.claude/projects` | Claude Code data directory |
| `--codex-path` | `~/.codex` | Codex data directory |
| `-r, --refresh` | `5` | Polling interval in seconds |
| `-h, --help` | | Print help |

### Subcommands

| Command | Description |
|---------|-------------|
| `update` | Check for updates and install the latest version |
| `update --dry-run` | Show what would be updated without installing |
| `update --force` | Force update even if already on latest version |

### Keybindings

| Key | Action |
|-----|--------|
| `Tab` / `←` / `→` | Switch between Claude Code / Codex |
| `r` | Clear current tab history |
| `q` / `Esc` | Quit |

## How it works

### Quota Information

The tool reads quota information from the official APIs:

- **Claude Code**: Reads OAuth credentials from macOS Keychain (`Claude Code-credentials`) and calls `https://api.anthropic.com/api/oauth/usage`
- **Codex**: Reads access token from `~/.codex/auth.json` and calls `https://chatgpt.com/backend-api/wham/usage`

### Usage Records

Claude Code and Codex automatically write usage data to local files:

- **Claude Code**: Each API call is logged as a JSONL entry in `~/.claude/projects/<project>/<session>.jsonl`, containing token counts, model name, and timestamps.
- **Codex**: Session rollouts are logged in `~/.codex/sessions/YYYY/MM/DD/rollout-<id>.jsonl`, with `token_count` events tracking cumulative usage.

This tool reads those files directly — no network calls for usage records, no API keys needed.

## Supported Platforms

| Platform | Data Source | Quota API |
|----------|-------------|-----------|
| Claude Code | `~/.claude/projects/**/*.jsonl` | `api.anthropic.com/api/oauth/usage` |
| Codex | `~/.codex/sessions/**/rollout-*.jsonl` | `chatgpt.com/backend-api/wham/usage` |

Cost is calculated from built-in pricing tables for Anthropic and OpenAI models. Unknown models show $0.00.

## TUI Layout

```
 ☁ CLAUDE   ⚡ codex                        ✓ you@mail.com   ← tabs + account
────────────────────────────────────────────────────────   ← accent rule
 ✓ 5h ▓▓▓▓▓▓▓▓▓▓░░  82%  resets 2h30m                        ← quota window
 ✓ 7d ▓▓▓▓▓▓░░░░░░  54%  resets 4d6h                         ← quota window
┌ ☁ CLAUDE models (42) ─────────────────────────────────────┐
│ MODEL          INPUT   OUTPUT   CACHE     COST     #       │ ← per-model totals
│ claude-opus-4  1.2M    340.0k   8.1M      $12.34   42      │
└────────────────────────────────────────────────────────────┘
┌ ☁ sessions ───────────────────────────────────────────────┐
│ SESSION              TOKENS        REQUESTS                 │ ← per-session usage
│ ollama-monitor       10.5k         42                       │   (by working dir)
│ my-web-app           2.3k          11                       │
└────────────────────────────────────────────────────────────┘
 42 calls · $12.34                              tab·r·q       ← status line
```

### Colors

The palette is intentionally minimal: a single platform accent color plus the
terminal's default foreground, with secondary text dimmed.

- **CLAUDE**: Orange (RGB 255, 165, 0)
- **CODEX**: Blue (RGB 59, 130, 246)

The accent is used only for the active tab, the header rule, and the filled
portion of the quota bars. Everything else uses default/dim colors.

### Quota Display

Each quota window is shown as its own bar with the remaining fraction filled in
the accent color, plus a status glyph (color is not relied on for status):

- ✓ ≥50% remaining
- ⚠ ≥20% remaining
- ✗ <20% remaining

The header shows the authenticated account; each bar shows its reset time.

## License

[MIT](LICENSE)
