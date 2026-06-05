# 设计文档：支持 pi、openclaw、hermes-agent、Factory AI

**日期：** 2026-06-05
**状态：** 已批准
**作者：** opencode

## 概述

为 agent-usage-monitor 添加对四个新 AI coding agent 的支持：
- **pi** (earendil-works/pi)
- **openclaw** (openclaw/openclaw)
- **hermes-agent** (NousResearch/hermes-agent)
- **Factory AI** (Factory-AI/factory)

## 目标

1. 为每个 agent 创建独立的 reader 模块
2. 扩展 Platform 和 Tab 枚举以支持新 agent
3. 在 AppState 中添加对应的状态字段
4. 添加 CLI 参数和配置键
5. 在 UI 中显示新 agent 的使用情况
6. **只在 TUI 中显示已安装的 agent（配置目录存在的）**

## 数据格式分析

### pi

- **格式：** JSONL
- **默认路径：** `~/.pi/agent/sessions/`
- **文件命名：** `<timestamp>_<uuid>.jsonl`
- **数据结构：**
  ```json
  {
    "type": "message",
    "message": {
      "role": "assistant",
      "model": "claude-sonnet-4-5",
      "provider": "anthropic",
      "usage": {
        "input": 1200,
        "output": 450,
        "cacheRead": 800,
        "cacheWrite": 150,
        "totalTokens": 1650,
        "cost": {
          "input": 0.018,
          "output": 0.00675,
          "cacheRead": 0.006,
          "cacheWrite": 0.003375,
          "total": 0.034125
        }
      }
    }
  }
  ```

### openclaw

- **格式：** JSONL
- **默认路径：** `~/.openclaw/agents/<agentId>/sessions/`
- **文件命名：** `<sessionId>.jsonl`
- **数据结构：**
  ```json
  {
    "type": "message",
    "message": {
      "role": "assistant",
      "model": "claude-opus-4-6",
      "provider": "anthropic",
      "usage": {
        "input": 1200,
        "output": 450,
        "cacheRead": 800,
        "cacheWrite": 150,
        "totalTokens": 1650,
        "cost": {
          "input": 0.018,
          "output": 0.00675,
          "cacheRead": 0.006,
          "cacheWrite": 0.003375,
          "total": 0.034125
        }
      }
    }
  }
  ```

### hermes-agent

- **格式：** SQLite
- **默认路径：** `~/.hermes/state.db`
- **表结构：** `sessions` 表
- **关键字段：**
  - `input_tokens` - 输入 token 数
  - `output_tokens` - 输出 token 数
  - `cache_read_tokens` - 缓存读取 token 数
  - `cache_write_tokens` - 缓存写入 token 数
  - `estimated_cost_usd` - 估算费用
  - `model` - 模型名称
  - `started_at` - 开始时间

### Factory AI (droid)

- **格式：** JSONL
- **默认路径：** `~/.factory/projects/`
- **文件命名：** `<session-uuid>.jsonl`
- **数据结构：**
  ```json
  {
    "type": "message",
    "message": {
      "role": "assistant",
      "model": "claude-sonnet-4-5",
      "usage": {
        "input_tokens": 1200,
        "output_tokens": 450,
        "cache_read_tokens": 800,
        "cache_write_tokens": 150
      }
    }
  }
  ```

## 架构设计

### 新增文件

1. `src/reader/pi.rs` - pi reader 实现
2. `src/reader/openclaw.rs` - openclaw reader 实现
3. `src/reader/hermes.rs` - hermes-agent reader 实现（SQLite）
4. `src/reader/factory.rs` - Factory AI reader 实现

### 修改文件

1. `src/state/app_state.rs`
   - 添加 `Platform` 枚举变体：`Pi`, `OpenClaw`, `Hermes`, `Factory`
   - 添加 `Tab` 枚举变体：`Pi`, `OpenClaw`, `Hermes`, `Factory`
   - 添加对应的状态字段（records, sessions, total_calls, total_cost, quota, max_records）
   - 添加 `available_tabs: Vec<Tab>` 字段
   - 添加 `detect_available_tabs()` 函数
   - 修改 `Tab::next()` 和 `Tab::prev()` 接受 `&[Tab]` 参数

2. `src/reader/mod.rs`
   - 注册新 reader 模块
   - 为每个 reader 实现 `UsageSource` trait

3. `src/cli.rs`
   - 添加 CLI 参数：`--pi-path`, `--openclaw-path`, `--hermes-path`, `--factory-path`

4. `src/config.rs`
   - 添加配置键：`pi_path`, `openclaw_path`, `hermes_path`, `factory_path`

5. `src/ui/tabs.rs`
   - 修改 `tab_line()` 接受 `&[Tab]` 参数，只渲染可用 tab

6. `src/main.rs`
   - 启动时检测可用 tab
   - 修改 TUI 事件循环使用可用 tab 列表

## 实现细节

### Reader 实现模式

每个 JSONL-based reader 将遵循现有模式：

```rust
pub struct PiReader {
    data_dir: PathBuf,
    file_positions: HashMap<PathBuf, u64>,
}

impl JsonlReader for PiReader {
    fn file_positions(&mut self) -> &mut HashMap<PathBuf, u64> {
        &mut self.file_positions
    }

    fn find_files(&self) -> Vec<PathBuf> {
        // 查找 JSONL 文件
    }

    fn parse_line(&self, line: &str) -> Option<UsageRecord> {
        // 解析 JSONL 行
    }
}
```

### SQLite reader 实现

hermes-agent 使用 SQLite，需要实现不同的 reader：

```rust
pub struct HermesReader {
    db_path: PathBuf,
    last_session_id: Option<String>,
}

impl UsageSource for HermesReader {
    fn platform(&self) -> Platform {
        Platform::Hermes
    }

    fn scan_all(&mut self) -> Vec<UsageRecord> {
        // 查询 SQLite 数据库
    }

    fn poll_delta(&mut self) -> Vec<UsageRecord> {
        // 增量查询新记录
    }
}
```

## 智能 Tab 显示

### 需求

只在 TUI 中显示已安装的 agent（配置目录存在的），未安装的不显示。

### 检测逻辑

| Agent | 检测路径 |
|-------|----------|
| Claude Code | `~/.claude/projects` |
| Codex | `~/.codex` |
| opencode | `$XDG_DATA_HOME/opencode` |
| Kimi Code | `~/.kimi-code` |
| pi | `~/.pi/agent/sessions` |
| openclaw | `~/.openclaw/agents` |
| hermes-agent | `~/.hermes/state.db` |
| Factory AI | `~/.factory/projects` |

### 实现方案

1. 在 `AppState` 中添加 `available_tabs: Vec<Tab>` 字段
2. 启动时检测各 agent 目录是否存在，填充 `available_tabs`
3. 修改 `Tab::next()` 和 `Tab::prev()` 接受 `&[Tab]` 参数，只在可用 tab 间切换
4. 修改 `tab_line()` 只渲染可用 tab
5. 如果当前 active_tab 不在可用列表中，自动切换到第一个可用 tab

### 目录不存在时的行为

- Reader 仍然创建（路径不存在时返回空记录）
- Tab 不显示在 TUI 中
- 用户可通过 CLI 参数或配置强制启用某个 agent（即使目录不存在）

## 测试策略

1. **单元测试：** 为每个 reader 添加解析函数的单元测试
2. **集成测试：** 测试 reader 与 AppState 的集成
3. **手动测试：** 使用真实数据验证解析正确性

## 向后兼容性

- 所有新功能都是增量添加
- 现有 CLI 参数和配置保持不变
- 新 agent 默认禁用（需要通过 CLI 参数或配置启用）

## 依赖项

- `rusqlite` - 已存在，用于 hermes-agent 的 SQLite 读取
- 无新增外部依赖

## 风险与缓解

| 风险 | 缓解措施 |
|------|----------|
| 数据格式变化 | 添加版本检查和向后兼容处理 |
| 性能问题 | 使用增量读取和缓存 |
| 路径不存在 | 优雅处理缺失目录 |

## 待办事项

- [ ] 实现 pi reader
- [ ] 实现 openclaw reader
- [ ] 实现 hermes-agent reader
- [ ] 实现 Factory AI reader
- [ ] 扩展 Platform/Tab 枚举
- [ ] 添加 CLI 参数
- [ ] 添加配置支持
- [ ] 更新 UI
- [ ] 添加测试
- [ ] 更新文档
