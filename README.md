# Agent Usage Monitor

Terminal dashboard for **Claude Code** and **Codex** usage, cost, and live quota. Command: `aum`.

![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/youming-ai/agent-usage-monitor/main/install.sh | sh
```

Requires `minisign` (`brew install minisign` / `apt install minisign`). Or grab a binary from [Releases](https://github.com/youming-ai/agent-usage-monitor/releases), or `cargo build --release`.

## Usage

```bash
aum                          # dashboard (q / Esc to quit)
aum -r 2                     # 2s fallback poll (default 5)
aum stats                    # JSON usage report
aum mcp                      # MCP server over stdio
aum config                   # show config
aum config set <key> <value> # claude_path, codex_path, refresh, max_records
aum update                   # self-update
```

Override data paths with `--claude-path` (default `~/.claude/projects`) and `--codex-path` (default `~/.codex`).

## JSON stats

```bash
aum stats --compact | jq '.platforms.claude_code.totals.cost_usd'
aum stats --platform codex --since 2026-06-01 --until 2026-06-30
aum stats --include-quota
```

Dates are inclusive UTC days. Invalid arguments exit with status 2.

## MCP

```json
{ "mcpServers": { "aum": { "command": "aum", "args": ["mcp"] } } }
```

Tools: `get_daily_stats`, `get_model_usage`, `get_cost_breakdown`, `get_file_operations`, `get_session_stats`, `get_quota`.
Resources: `aum://summary`, `aum://platforms`.

## How it works

- **Usage** is read from local logs, no API keys needed:
  - Claude Code: `~/.claude/projects/**/*.jsonl`
  - Codex: `~/.codex/sessions/**/rollout-*.jsonl`
- **Quota** comes from the vendors' usage endpoints using your existing local login.
- **Cost** uses built-in pricing tables; unknown models show `$0.00`.
- Updates are near-instant through FS events, with a fallback poll as a safety net.

## License

[MIT](LICENSE)
