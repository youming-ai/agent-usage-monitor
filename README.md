# LLM Usage Monitor

Real-time terminal monitoring tool for Claude Code & Codex API usage. Shows quota limits and token usage from local JSONL files.

![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Features

- **Claude Code** — parses `~/.claude/projects/**/*.jsonl` and shows quota from Anthropic API
- **Codex** — parses `~/.codex/sessions/**/rollout-*.jsonl` and shows quota from OpenAI API
- **Quota monitoring** — displays remaining usage percentage and reset times
- **Progress bar** — visual gauge showing quota usage with color-coded warnings
- **Dual-tab TUI** — switch between Claude Code / Codex views
- **Token tracking** — input, output, cache read/creation tokens with cost level indicators
- **Cost calculation** — built-in pricing tables for Anthropic & OpenAI models
- **Real-time polling** — configurable refresh interval

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/youming-ai/llm-usage-monitor/main/install.sh | sh
```

Or download manually from [Releases](https://github.com/youming-ai/llm-usage-monitor/releases).

### Build from source

```bash
git clone https://github.com/youming-ai/llm-usage-monitor.git
cd llm-usage-monitor
cargo build --release
```

## Usage

```bash
# Default (reads from ~/.claude/projects and ~/.codex)
usage-monitor

# Custom paths
usage-monitor --claude-path /path/to/.claude/projects --codex-path /path/to/.codex

# Adjust refresh rate (seconds)
usage-monitor --refresh 2

# Check for updates
usage-monitor update

# Check for updates without installing
usage-monitor update --dry-run

# Force update even if already on latest
usage-monitor update --force
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
┌─────────────────────────────────────┐
│ Usage Monitor                       │ ← Tab bar
│ [☁ CLAUDE] │ [⚡ CODEX]            │
├─────────────────────────────────────┤
│ Quota Info                          │ ← Account & quota window info
├─────────────────────────────────────┤
│ Usage Progress ████████░░ 84%       │ ← Visual progress bar
├─────────────────────────────────────┤
│ ☁ CLAUDE Sessions (12)             │ ← Model usage table
│ Model      Input  Output  Cost      │
│ claude-3   1.2M   340k    $2.45     │
│ gpt-4      890k   120k    $1.23     │
├─────────────────────────────────────┤
│ Status  12 calls │ $3.68           │ ← Status bar
└─────────────────────────────────────┘
```

### Color Themes

Each platform has its own color theme:
- **CLAUDE**: Orange (RGB 255, 165, 0) — borders, headers, badges
- **CODEX**: Blue (RGB 59, 130, 246) — borders, headers, badges

### Quota Display

The quota info bar shows:
- **Account info** — authenticated email address
- **Reset time** — when the quota window resets

The progress bar shows:
- **Remaining percentage** — visual gauge with color coding
  - 🟢 Green: ≥50% remaining
  - 🟡 Yellow: ≥20% remaining
  - 🔴 Red: <20% remaining

Example:
```
☁ CLAUDE │ ✓ youmin.tang@elestyle.jp │ 5h window reset 1h19m │ 7d window reset 6d
☁ CLAUDE: 84% remaining  ████████████████░░░░░░░░░░░░░░░░░░░░░░░░
```

## License

[MIT](LICENSE)
