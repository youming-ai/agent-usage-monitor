# LLM Usage Monitor

Real-time terminal monitoring tool for Claude Code & Codex API usage. Reads local JSONL files — no proxy, no API key, no interception.

![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Features

- **Claude Code** — parses `~/.claude/projects/**/*.jsonl`
- **Codex** — parses `~/.codex/sessions/**/rollout-*.jsonl`
- **Dual-tab TUI** — switch between Claude Code / Codex views
- **Token tracking** — input, output, cache read/creation tokens
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
```

### CLI Options

| Option | Default | Description |
|--------|---------|-------------|
| `--claude-path` | `~/.claude/projects` | Claude Code data directory |
| `--codex-path` | `~/.codex` | Codex data directory |
| `-r, --refresh` | `5` | Polling interval in seconds |
| `-h, --help` | | Print help |

### Keybindings

| Key | Action |
|-----|--------|
| `Tab` / `←` / `→` | Switch between Claude Code / Codex |
| `r` | Clear current tab history |
| `q` / `Esc` | Quit |

## How it works

Claude Code and Codex automatically write usage data to local files:

- **Claude Code**: Each API call is logged as a JSONL entry in `~/.claude/projects/<project>/<session>.jsonl`, containing token counts, model name, and timestamps.
- **Codex**: Session rollouts are logged in `~/.codex/sessions/YYYY/MM/DD/rollout-<id>.jsonl`, with `token_count` events tracking cumulative usage.

This tool reads those files directly — no network calls, no API keys, no proxy needed.

## Supported Platforms

| Platform | Data Source |
|----------|-------------|
| Claude Code | `~/.claude/projects/**/*.jsonl` |
| Codex | `~/.codex/sessions/**/rollout-*.jsonl` |

Cost is calculated from built-in pricing tables for Anthropic and OpenAI models. Unknown models show $0.00.

## License

[MIT](LICENSE)
