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

## Supported Models (Pricing)

| Model | Input ($/1M) | Output ($/1M) | Cache Read ($/1M) |
|-------|-------------|---------------|-------------------|
| claude-opus-4 | 15.00 | 75.00 | 1.50 |
| claude-sonnet-4 | 3.00 | 15.00 | 0.30 |
| claude-haiku-4 | 1.00 | 5.00 | 0.10 |
| gpt-5.5 | 1.25 | 5.00 | 0.125 |
| gpt-5.3-codex | 2.50 | 10.00 | 0.25 |
| gpt-5.4-mini | 0.15 | 0.60 | 0.015 |

## License

[MIT](LICENSE)
