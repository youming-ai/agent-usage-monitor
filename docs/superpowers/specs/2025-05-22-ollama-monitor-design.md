# Ollama Monitor CLI 设计文档

**日期**: 2025-05-22
**项目**: ollama-monitor
**技术栈**: Rust + ratatui

## 1. 概述

`ollama-monitor` 是一个基于终端的实时监控工具，用于展示本地 Ollama 服务的运行状态与 API 调用统计。它通过启动本地 HTTP 代理拦截 API 请求，提取 usage 指标，同时轮询 `/api/ps` 获取模型运行状态，并在终端中以仪表盘形式实时呈现。

## 2. 功能需求

### 2.1 代理模式（Proxy Mode）
- 启动本地 HTTP 代理，默认监听 `127.0.0.1:11435`
- 将所有请求透明转发到真实 Ollama 服务（默认 `127.0.0.1:11434`）
- 拦截并解析 SSE stream 响应，从 `done=true` 的 chunk 中提取 usage 字段
- 实时计算并维护以下指标：
  - `total_duration`（总耗时，ns → ms）
  - `prompt_eval_count`（输入 token 数）
  - `eval_count`（输出 token 数）
  - `prompt_eval_duration`（输入评估耗时）
  - `eval_duration`（输出生成耗时）
  - 派生指标：`tokens/sec = eval_count / (eval_duration / 1e9)`

### 2.2 仪表盘模式（Dashboard Mode）
- 定期轮询 Ollama `/api/ps` endpoint（默认每 2 秒）
- 显示当前加载模型的信息：模型名、运行时长、内存占用、GPU 利用率

### 2.3 终端 UI
- 全屏 TUI，使用 `ratatui` + `crossterm`
- **上半屏**：Running Models 表格
- **下半屏**：Recent API Calls 列表（保留最近 50 条）
- **底部状态栏**：当前模式、刷新频率、总请求数、操作快捷键提示
- 支持按键交互：`q` 退出、`p` 暂停/恢复代理、`r` 清空历史、`↑/↓` 滚动列表

### 2.4 数据持久化（可选/未来）
- 当前版本不实现持久化，所有数据保存在内存中
- 程序退出后数据清零

## 3. 架构设计

```
ollama-monitor
├── proxy/          # 本地 HTTP 代理（axum）
│   ├── server.rs   # 代理服务器启动与管理
│   └── handler.rs  # 请求转发与响应拦截逻辑
├── ollama_client/  # Ollama HTTP 客户端（reqwest）
│   └── client.rs   # 轮询 /api/ps，解析模型状态
├── ui/             # ratatui 组件
│   ├── mod.rs      # UI 渲染入口
│   ├── model_table.rs    # Running Models 表格组件
│   ├── usage_table.rs    # Recent API Calls 列表组件
│   └── status_bar.rs     # 底部状态栏组件
├── state/          # 共享应用状态
│   └── app_state.rs      # AppState: 模型列表、usage 记录、统计聚合
├── event/          # 事件处理
│   └── event_loop.rs     # 键盘事件 + 定时刷新事件
└── main.rs         # CLI 参数解析（clap）、tokio runtime 启动
```

## 4. 数据流

```
User Request → Proxy (11435) → Forward → Ollama (11434)
                     │
                     └─ Intercept SSE Response ─┐
                                              ▼
                                    Extract usage fields from done=true chunk
                                              │
                                              ▼
                                    Update AppState (Arc<Mutex<AppState>>)
                                              │
                                              ▼
                                    Trigger UI Redraw

Timer (every 2s) → Poll /api/ps → Update AppState.running_models → Trigger UI Redraw
```

### 共享状态
```rust
pub struct AppState {
    pub running_models: Vec<RunningModel>,
    pub recent_calls: Vec<ApiCall>,
    pub total_calls: usize,
    pub proxy_paused: bool,
    pub last_error: Option<String>,
}

pub struct RunningModel {
    pub name: String,
    pub running_for: String,  // human readable duration
    pub size: u64,            // bytes
    pub gpu_utilization: Option<f64>,
}

pub struct ApiCall {
    pub timestamp: DateTime<Local>,
    pub model: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_duration_ms: u64,
    pub tokens_per_sec: f64,
}
```

## 5. UI 布局设计

```
┌─────────────────────────────────────────────┐
│  Ollama Monitor - 127.0.0.1:11435 → 11434  │
├─────────────────────────────────────────────┤
│ Running Models                               │
│ ┌──────────────┬───────────┬────────┬──────┐ │
│ │ Model        │ Running   │ Memory │ GPU  │ │
│ ├──────────────┼───────────┼────────┼──────┤ │
│ │ llama3.1     │ 5m 32s    │ 4.2GB  │ 45%  │ │
│ │ gemma3       │ 1h 12m    │ 3.1GB  │ 78%  │ │
│ └──────────────┴───────────┴────────┴──────┘ │
├─────────────────────────────────────────────┤
│ Recent API Calls (50)                        │
│ ┌──────┬──────────┬─────┬─────┬────────┬───┐│
│ │ Time │ Model    │ In  │ Out │ Total  │ T/s││
│ ├──────┼──────────┼─────┼─────┼────────┼───┤│
│ │11:05 │ llama3.1 │ 128 │ 512 │ 2.4s   │210││
│ │11:04 │ gemma3   │  64 │ 256 │ 1.8s   │142││
│ └──────┴──────────┴─────┴─────┴────────┴───┘│
├─────────────────────────────────────────────┤
│ [Proxy: ON] Calls: 42 | q:quit p:pause r:clr │
└─────────────────────────────────────────────┘
```

## 6. 错误处理

| 场景 | 处理策略 |
|------|---------|
| Ollama 未运行 | 显示错误状态栏，继续尝试轮询 `/api/ps`，代理返回 502 |
| 代理转发失败 | 记录到 `last_error`，UI 显示红色提示 |
| SSE 解析失败 | 忽略该 chunk，不记录 usage，不影响其他功能 |
| TUI 初始化失败 | 打印错误到 stderr，非 0 退出 |
| 键盘事件解析失败 | 忽略非法输入 |

## 7. 技术选型

| 用途 | 库 | 版本约束 |
|------|-----|---------|
| TUI 框架 | `ratatui` | ^0.29 |
| 终端控制 | `crossterm` | ^0.28 |
| HTTP 代理 | `axum` | ^0.8 |
| HTTP 客户端 | `reqwest` | ^0.12 |
| 异步运行时 | `tokio` | ^1 |
| JSON 序列化 | `serde` + `serde_json` | ^1 |
| CLI 参数 | `clap` | ^4 |
| 时间处理 | `chrono` | ^0.4 |
| 字节格式化 | `humansize` | ^2 |

## 8. CLI 参数

```
ollama-monitor [OPTIONS]

Options:
  -p, --proxy-port <PORT>    代理监听端口 [default: 11435]
  -o, --ollama-host <HOST>   Ollama 服务地址 [default: 127.0.0.1:11434]
  -r, --refresh <SECONDS>    轮询 /api/ps 的间隔 [default: 2]
  -h, --help                 打印帮助信息
```

## 9. 快捷键

| 按键 | 功能 |
|------|------|
| `q` / `Ctrl+C` | 退出程序 |
| `p` | 暂停/恢复代理拦截 |
| `r` | 清空 API Calls 历史 |
| `↑` / `↓` | 滚动 API Calls 列表 |
| `Home` / `End` | 跳到列表顶部/底部 |

## 10. 构建与运行

```bash
cargo build --release
./target/release/ollama-monitor
```

用户需要设置客户端指向代理端口：
```bash
export OLLAMA_HOST=http://127.0.0.1:11435
# 或使用 CLI 参数: ollama --host http://127.0.0.1:11435 ...
```

## 11. 范围边界

**当前版本内**：
- 代理拦截 `/api/generate` 和 `/api/chat` 的 SSE streaming 响应
- 轮询 `/api/ps`
- 内存中的实时仪表盘
- 基础快捷键交互

**不在当前版本内**（可未来扩展）：
- 数据持久化到文件/数据库
- 历史查询与过滤
- 多 Ollama 实例监控
- Web 界面
- 告警通知

---

*Spec reviewed and approved by user on 2025-05-22.*
