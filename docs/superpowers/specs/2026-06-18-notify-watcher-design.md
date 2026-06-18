# 设计文档：notify watcher (替代 1s 轮询)

**日期：** 2026-06-18
**状态：** 已批准
**作者：** opencode
**关联：** Spec 3/4（splitrail 学习后的改造）

## 概述

为 `agent-usage-monitor` 的 TUI reader task 用 `notify` crate 替代 `tokio::time::interval(1s)` 轮询。文件变更即时触发 `poll_delta()`，30s 长间隔兜底轮询作为安全网。

不动 quota fetch（quota 是 HTTP，不在 FS 范围）。

## 目标

1. 文件变更 < 100ms 内反映到 TUI
2. 空闲状态 CPU/IO 接近零（不再是每秒 13 次 scan）
3. 零功能回归：1s 轮询的现有行为全部保留
4. 跨平台：macOS (FSEvents) + Linux (inotify) + Windows (ReadDirectoryChangesW)
5. 优雅降级：网络 FS / 不支持 FS 上 fallback 到 30s 轮询

## 非目标

- **不动** quota fetch（HTTP, 仍 2 min interval）
- **不动** reader 的 `scan_all()` / `poll_delta()` 逻辑
- **不动** `AppState` / `PlatformState` 的存储结构
- **不做** 实时事件流（push to UI）—— `poll_delta` 已经返回 `Vec<UsageRecord>`，events 走现有 event loop
- **不引入** `notify` 之外的 I/O crate

## 架构设计

### 新增文件

| 文件 | 职责 |
|---|---|
| `src/watcher.rs` | `PlatformWatcher` 结构（包装 `notify` + debouncer）；`start_watchers(paths) -> Vec<PlatformWatcher>` 工厂；`WatcherMessage` enum（`Event(PathBuf)` / `FallbackTick`） |

### 修改文件

| 文件 | 改动 |
|---|---|
| `Cargo.toml` | `notify = "8"` + `notify-debouncer-full = "0.6"`（标准 debouncer 组合） |
| `src/reader/mod.rs` | `UsageSource` trait 加 `fn get_watch_directories(&self) -> Vec<PathBuf>`，默认实现 `vec![]`（向后兼容） |
| `src/reader/claude.rs` 等 10 个 JSONL reader | 实现 `get_watch_directories` 返回 `[<data_path>]` recursive |
| `src/reader/{opencode,hermes,mimo_code}.rs` (3 个 SQLite reader) | 实现 `get_watch_directories` 返回 `[<data_path>]`（单文件） |
| `src/main.rs` | 替换 1s 轮询 task 为 watcher 驱动 |

### 不动

- `src/state/app_state.rs`（`UsageRecord` / `AppState` 不变）
- `src/reader/jsonl_reader.rs`（共用 JSONL 解析逻辑）
- `src/quota/*`（quota 仍 2 min interval）
- `src/event/event_loop.rs`（仍消费 `Vec<UsageRecord>`）

## 实现细节

### `src/watcher.rs`（新模块骨架）

```rust
//! File-system watcher for TUI reader tasks.
//!
//! Replaces the 1s `tokio::time::interval` polling in `main.rs` with
//! `notify` events + 50ms debounce per platform. A 30s fallback poll
//! runs alongside as a safety net for FS edge cases (network FS,
//! dropped events).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_full::{
    new_debouncer, DebounceEventResult, Debouncer, RecommendedCache,
};
use tokio::sync::mpsc;

use crate::platforms::{self, Platform, Tab};

/// Messages emitted by a per-platform watcher.
#[derive(Debug)]
pub enum WatcherMessage {
    /// A file under the platform's data path was created/modified/removed.
    Event { platform: Platform, path: PathBuf },
    /// 30s fallback tick — same effect as Event for any platform.
    FallbackTick,
}

pub struct PlatformWatcher {
    platform: Platform,
    /// Keep the debouncer alive (drop = watcher stops).
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

impl PlatformWatcher {
    pub fn platform(&self) -> Platform { self.platform }
}

/// Build one PlatformWatcher per registered platform, plus a fallback tick source.
/// `paths` is the resolved AgentPaths.
pub fn start_watchers(
    paths: &platforms::AgentPaths,
) -> (Vec<PlatformWatcher>, mpsc::Receiver<WatcherMessage>) {
    let (tx, rx) = mpsc::channel::<WatcherMessage>(64);
    let mut watchers = Vec::with_capacity(13);

    for entry in platforms::entries() {
        let path = paths.path_for(entry.tab);
        if !path.exists() {
            continue; // skip unavailable platforms
        }
        let mut reader = entry.build_reader(path.clone());
        let watch_dirs = reader.get_watch_directories();
        if watch_dirs.is_empty() {
            continue; // reader has no watch directories (defensive)
        }
        let tx = tx.clone();
        let platform = entry.platform;
        let mut debouncer = new_debouncer(
            Duration::from_millis(50),  // 50ms debounce
            None,
            move |res: DebounceEventResult| {
                if let Ok(events) = res {
                    for event in events {
                        for path in &event.paths {
                            let _ = tx.blocking_send(WatcherMessage::Event {
                                platform,
                                path: path.clone(),
                            });
                        }
                    }
                }
            },
        ).expect("create debouncer");
        // Recursive for JSONL (multiple files in subdirs); flat for SQLite (single file).
        let is_sqlite = matches!(entry.tab, Tab::OpenCode | Tab::Hermes | Tab::MimoCode);
        for dir in &watch_dirs {
            let mode = if is_sqlite && dir.is_file() {
                // SQLite: watch the parent dir, filter to file events
                if let Some(parent) = dir.parent() {
                    debouncer.watcher().watch(parent, RecursiveMode::NonRecursive)
                        .expect("watch parent of sqlite file");
                }
            } else {
                debouncer.watcher().watch(dir, RecursiveMode::Recursive)
                    .expect("watch directory");
            };
        }
        watchers.push(PlatformWatcher { platform, _debouncer: debouncer });
    }

    // Fallback tick source: emit FallbackTick every 30s
    // (The actual tokio interval is driven in main.rs to keep this module
    // transport-agnostic; we just provide the message type.)

    (watchers, rx)
}
```

### `src/reader/mod.rs`（trait 扩展）

```rust
pub trait UsageSource: Send {
    fn platform(&self) -> Platform;
    fn scan_all(&mut self) -> Vec<UsageRecord>;
    fn poll_delta(&mut self) -> Vec<UsageRecord>;

    /// Directories the file watcher should monitor for this platform.
    /// Default: empty (no watching). JSONL readers return `[<data_dir>]`;
    /// SQLite readers return `[<db_file_path>]`.
    fn get_watch_directories(&self) -> Vec<std::path::PathBuf> {
        vec![]
    }
}
```

### 13 个 reader 的 `get_watch_directories` 实现

| Reader | 实现 |
|---|---|
| Claude, Codex, OpenClaw, KimiCode, Pi, Factory, Grok, Cursor, Copilot, Antigravity | `vec![self.data_path.clone()]` recursive |
| OpenCode, Hermes, MiMo Code (SQLite) | `vec![self.db_path.clone()]`（watch parent dir, non-recursive） |

每个 reader 文件加 ~3 行：

```rust
// e.g., src/reader/claude.rs
fn get_watch_directories(&self) -> Vec<PathBuf> {
    vec![self.data_path.clone()]
}
```

### `src/main.rs`（替换 1s 轮询）

旧逻辑（已存在，~line 100）：

```rust
let mut interval = tokio::time::interval(Duration::from_secs(refresh as u64));
loop {
    interval.tick().await;
    for entry in platforms::entries() {
        // ... spawn_blocking poll_delta ...
    }
}
```

新逻辑：

```rust
let (watchers, mut rx) = watcher::start_watchers(&agent_paths);
let mut fallback = tokio::time::interval(Duration::from_secs(30));
fallback.tick().await; // discard immediate first tick

loop {
    tokio::select! {
        Some(msg) = rx.recv() => {
            match msg {
                WatcherMessage::Event { platform, .. } => {
                    spawn_reader_poll_delta(platform, &app_state);
                }
                WatcherMessage::FallbackTick => {
                    // Sweep all platforms
                    for w in &watchers {
                        spawn_reader_poll_delta(w.platform(), &app_state);
                    }
                }
            }
        }
        _ = fallback.tick() => {
            for w in &watchers {
                spawn_reader_poll_delta(w.platform(), &app_state);
            }
        }
        // existing event loop ticks / quota ticks unchanged
    }
}
```

保持 `refresh` 参数但**只用于非 watcher 路径**（e.g., quota 或统计仪表）。或者干脆不传给 watcher 路径。

### `Cargo.toml`

```toml
[dependencies]
# 已有...
notify = "8"
notify-debouncer-full = "0.6"
```

`notify 8` 用 platform 原生后端（FSEvents/inotify/ReadDirectoryChangesW）；`notify-debouncer-full 0.6` 是官方 debouncer。

## 关键决策

| 决策 | 选择 | 理由 |
|---|---|---|
| Notify 库 | `notify 8` + `notify-debouncer-full 0.6` | Rust 生态标准，跨平台后端 |
| Scope | 仅 reader 轮询 | 最小化改动，quota 仍 2 min |
| 兜底轮询 | 30s | 平衡 I/O 和安全性；splitrail 类似 |
| Debounce | 50ms 固定 per-platform | 足以合并原子写入；可调但 spec 3 固定 |
| Watcher 拓扑 | Per-platform（13 个） | 简单；event → platform 映射自然 |
| 递归 watch | JSONL: recursive；SQLite: non-recursive on parent | JSONL 跨多级；SQLite 单文件 |
| `UsageSource` 扩展 | 新增 `get_watch_directories()`，默认空 | 向后兼容；其他 reader 显式 override |
| 现有 1s 轮询 | 移除 | 替换掉，避免双源 |
| `refresh` CLI 参数 | 保留 | 但不再用于 reader 轮询；只影响其他定时 |
| 新 deps | `notify 8`, `notify-debouncer-full 0.6` | 都有活跃维护 |
| 进程模型 | TUI 进程内 | 不影响 `aum stats` / `aum mcp` |
| LoC 估计 | ~400 行 watcher.rs + ~50 行 13 reader + ~50 行 main.rs 改 = ~500 行 |

## 测试

### 单元测试 (`src/watcher.rs`)

| 测试 | 验证 |
|---|---|
| `start_watchers_skips_unavailable_platforms` | 不存在的 data_path 不创建 watcher |
| `start_watchers_emits_event_on_file_create` | 临时目录里 create → WatcherMessage::Event 发出 |
| `start_watchers_debounces_burst` | 100ms 内的 5 次写入 → 只 1 个 Event |
| `start_watchers_jsonl_uses_recursive` | JSONL reader 配置 RecursiveMode::Recursive |
| `start_watchers_sqlite_uses_parent_non_recursive` | SQLite reader 配置 parent + NonRecursive |

### 单元测试 (每个 reader)

- `claude_get_watch_directories_returns_data_path`
- ...（10 个 JSONL + 3 个 SQLite = 13 个小测试，或合并为 1 个 trait 行为测试）

### 集成测试

- `tests/watcher.rs` 新建：创建临时 JSONL 目录，启动 watcher，写入文件，验证 event 在 200ms 内到达
- 现有 `tests/reader_fixtures.rs` 不动（fixture-based reader 单元测试）

## 向后兼容

- 现有 TUI 行为：文件变更后最多 1s 内更新 → spec 3 改为 < 100ms（实际改进）
- 1s 兜底消失（被 30s 兜底替代）
- `UsageSource` trait 加方法 + 默认实现：所有现有 reader 自动兼容（不实现也返回空，等于不 watch）
- `refresh` CLI 参数保留：仅影响 quota 间隔等（不直接影响 reader 轮询）
- 配置文件 `~/.config/aum/config.toml` 0 改动
- `aum stats` / `aum mcp` 0 改动（不走 watcher 路径）

## 依赖项

- `notify = "8"`（跨平台 FS 事件）
- `notify-debouncer-full = "0.6"`（debouncer 包装）
- `tokio`, `serde`, `chrono` 已有

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| macOS FSEvents 偶发丢事件 | 30s 兜底轮询 |
| Linux inotify 限制（递归深度、watch 数） | 1 个 platform 1 个 watcher 远低于限制 |
| 网络 FS（nfs/smb）不支持 | 文档说明；30s 兜底仍工作 |
| SQLite 文件 rotate → 新 inode 丢失 | watch parent dir 而非 file；rotate 后新 file 仍触发 |
| 13 个 reader 都 watch → fd 占用 | Linux fd 限制 1024+ per process；13 个 watcher 远低于 |
| 写频繁 → event 风暴 | 50ms debounce；每平台最多 1 poll_delta / 50ms |
| `notify 8` API 与 splitrail 的版本不同 | brief 要求实施时 `cargo doc --open` 验证 |
| 现有 1s 轮询的 race condition（已存在）| spec 3 移除它，单源更简单 |

## 文档

- README.md 加一段 `## Real-time updates`：说明文件变更 < 100ms 反映，30s 兜底，watcher 不支持网络 FS
- CHANGELOG：release-please 自动，conventional commit `feat: replace 1s polling with notify watcher + 30s fallback`

## 实施顺序（writing-plans 阶段细化）

1. 加 `Cargo.toml` 的 `notify` + `notify-debouncer-full` 依赖
2. 加 `src/watcher.rs` 骨架（type + 工厂 + 单元测试）
3. `UsageSource` trait 加 `get_watch_directories` 方法（默认空）
4. 13 个 reader 各加 `get_watch_directories` 实现
5. main.rs 替换 1s 轮询为 watcher + 30s 兜底
6. 集成测试 `tests/watcher.rs`
7. README + smoke test
8. 最终 verification

## 待办

- [ ] 实施时 `cargo doc --open` 验证 `notify 8` + `notify-debouncer-full 0.6` API
- [ ] SQLite reader 的 watch 策略：parent dir non-recursive vs file itself
- [ ] 写 `src/watcher.rs` 骨架 + 单元测试
- [ ] 加 13 reader 的 `get_watch_directories`
- [ ] main.rs 替换 1s 轮询
- [ ] 写 `tests/watcher.rs` 集成测试
- [ ] 更新 README
- [ ] 跑 `cargo fmt` + `cargo clippy --all-targets -- -D warnings` + `cargo test`
- [ ] 端到端 smoke：在临时目录写 JSONL，验证 TUI < 100ms 更新
- [ ] commit `feat: replace 1s polling with notify watcher + 30s fallback`
