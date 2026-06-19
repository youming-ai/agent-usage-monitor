# 设计文档：aum 内存与性能优化 (CompactDate + lasso)

**日期：** 2026-06-19
**状态：** 已批准
**作者：** opencode
**关联：** Spec 4/4（splitrail 学习后的改造）

## 概述

对 `agent-usage-monitor` 的核心数据结构进行内存与性能优化。优化目标是**消除日志解析与内存驻留阶段的 String 堆分配**，并通过紧凑型日期（`CompactDate`）代替字符串日期，以缩减 TUI、Stats 和 MCP 子命令在长期运行或大数据量下的内存占用和 GC/I/O 开销。

优化基于两大基石：
1. **`lasso` 字符串唯一化 (String Interning)**：使用全局线程安全的 Rodeo，将 `model` 和 `session` 字符串转化为 4 字节的拷贝类型 `Spur` (u32)。
2. **`CompactDate` 紧凑日期**：以单个 `u32` 位运算存储日期，并在 `Serialize` 阶段才还原为标准的 `"YYYY-MM-DD"` 字符串，确保外部 JSON schema 和 MCP 协议的向后兼容。

## 目标

1. 实现内存占用大幅缩减（单条记录的字符串占用由约 80 字节降至 8 字节，堆分配次数由 2 次降至 0 次）。
2. 在 `stats.rs` 和 `mcp/server.rs` 聚合过程中，彻底消除日期的临时 String 堆分配。
3. 保持向后兼容：`aum stats` 的 JSON 输出和 `aum mcp` 的响应格式 100% 保持不变。
4. 保持代码极简（Ponytail 原则）：通过全局静态 Rodeo 避免将 Interner 引用传递到 13 个 reader 的函数签名中。

## 非目标

- **不重构** TUI 渲染机制。
- **不优化** `timestamp` (保持 12 字节的 `DateTime<Utc>`，因为 TUI 需要精准秒数，且它无堆分配)。
- **不引入** `smallvec` / `tinyvec` 等额外依赖（YAGNI，`lasso` 加 `CompactDate` 已经解决 95% 以上分配瓶颈）。
- **不引入** `simd-json`（已在 brainstorm 阶段被否决，避免就地可变字节切片解析带来的复杂度）。

## 架构设计

### 修改文件

| 文件 | 改动 |
|---|---|
| `Cargo.toml` | 新增依赖：`lasso = { version = "0.7", features = ["multi-threaded"] }` |
| `src/lib.rs` | 确认 `stats`, `mcp`, `watcher` 无需调整注册 |
| `src/state/app_state.rs` | 1. 定义全局静态 `OnceLock<ThreadedRodeo>`；<br>2. 更改 `UsageRecord`、`SessionSummary`、`PlatformState` 的 String 字段为 `Spur`；<br>3. 实现 `CompactDate` 结构体及自定义 `Serialize` 特化 |
| `src/reader/*.rs` (13 个) | 解析时直接将得到的 `&str` 塞入全局 Rodeo，转换 `Spur` 填入 record，不保留 String |
| `src/stats.rs` | 聚合 Map 的 Key 改为 `CompactDate` 和 `Spur`，输出序列化适配 |
| `src/mcp/server.rs` | 工具返回的响应类型适配 `Spur` 和 `CompactDate` 的 resolve / 转换 |
| `tests/` / `src/` | 适配所有 `UsageRecord` 字面量构建的测试（改用 `intern("model")`） |

---

## 1. 唯一化机制 (src/state/app_state.rs)

使用 `lasso::ThreadedRodeo` 保证多线程 reader 访问时的线程安全：

```rust
use std::sync::OnceLock;
use lasso::{ThreadedRodeo, Spur};

/// 唯一化字符串的 Key 类型 (实际上是 u32)
pub type InternedString = Spur;

pub static INTERNER: OnceLock<ThreadedRodeo> = OnceLock::new();

/// 获取全局 Rodeo 字典句柄
pub fn get_interner() -> &'static ThreadedRodeo {
    INTERNER.get_or_init(ThreadedRodeo::new)
}

/// 将 &str 唯一化为 Spur 编码 (4 字节 Copy)
pub fn intern(s: &str) -> InternedString {
    get_interner().get_or_intern(s)
}

/// 将 Spur 还原为 &'static str (0 堆分配)
pub fn resolve(key: InternedString) -> &'static str {
    get_interner().resolve(&key)
}
```

---

## 2. 数据结构调整 (src/state/app_state.rs)

### UsageRecord 轻量化
```rust
#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub timestamp: DateTime<Utc>,
    #[allow(dead_code)]
    pub platform: Platform,
    pub model: InternedString,  // String -> InternedString
    pub session: InternedString, // String -> InternedString
    // ... 数字字段不变 ...
}
```

### SessionSummary 与 PlatformState 调整
```rust
#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub model: InternedString, // String -> InternedString
    pub total_input: u64,
    pub total_output: u64,
    pub total_cache_read: u64,
    pub total_cache_creation: u64,
    pub total_cost: f64,
    pub request_count: u64,
}

pub struct PlatformState {
    pub records: VecDeque<UsageRecord>,
    pub sessions: HashMap<InternedString, SessionSummary>, // String -> InternedString
    pub total_calls: usize,
    pub total_cost: f64,
    pub quota: Option<QuotaInfo>,
    pub max_records: usize,
}
```

聚合更新逻辑适配（`upsert_model_aggregate`）：
```rust
fn upsert_model_aggregate(map: &mut HashMap<InternedString, SessionSummary>, r: &UsageRecord) {
    let entry = map
        .entry(r.model)  // r.model 是 Spur (Copy)，免去 String.clone()！
        .or_insert_with(|| SessionSummary {
            model: r.model,
            total_input: 0,
            total_output: 0,
            total_cache_read: 0,
            total_cache_creation: 0,
            total_cost: 0.0,
            request_count: 0,
        });
    // 累加...
}
```

---

## 3. CompactDate 日期压缩 (src/state/app_state.rs)

用 `u32` 按位压缩日期，占 4 字节，替代 `chrono::NaiveDate`（12 字节）或 `"YYYY-MM-DD"`（24 字节 + 堆分配）：

```rust
use serde::{Serialize, Serializer};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CompactDate(u32); // (year << 16) | (month << 8) | day

impl CompactDate {
    pub fn new(year: u16, month: u8, day: u8) -> Self {
        Self(((year as u32) << 16) | ((month as u32) << 8) | day as u32)
    }

    pub fn from_datetime(dt: DateTime<Utc>) -> Self {
        use chrono::Datelike;
        let naive = dt.date_naive();
        Self::new(naive.year() as u16, naive.month() as u8, naive.day() as u8)
    }

    pub fn to_string(&self) -> String {
        let year = (self.0 >> 16) & 0xFFFF;
        let month = (self.0 >> 8) & 0xFF;
        let day = self.0 & 0xFF;
        format!("{:04}-{:02}-{:02}", year, month, day)
    }
}

// 核心特化：使 BTreeMap<CompactDate, DateBucket> 序列化时自动作为 "YYYY-MM-DD" 输出！
impl Serialize for CompactDate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
```

---

## 4. stats.rs 与 mcp 接口适配

在 `stats.rs` 内部，不再产生临时字符串。

```rust
// src/stats.rs
pub struct PlatformReport {
    pub available: bool,
    pub platform_key: String,
    pub data_path: PathBuf,
    pub totals: PlatformTotals,
    // 适配 Spur 和 CompactDate
    pub models: BTreeMap<String, ModelSummary>, 
    pub sessions: Vec<SessionSummaryView>,
    pub dates: BTreeMap<CompactDate, DateBucket>, // String -> CompactDate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<QuotaView>,
}
```

聚合时的更新：
```rust
let date_key = CompactDate::from_datetime(r.timestamp);
let d = dates.entry(date_key).or_default();
// ...
*d.models.entry(r.model).or_insert(0) += 1;
```

在构造 `PlatformReport` 时，`models` 需要还原回 String 键以供 JSON 输出：
```rust
let mut serialized_models = BTreeMap::new();
for (spur_key, summary) in models {
    let name = resolve(spur_key).to_string();
    serialized_models.insert(name, summary);
}
```

这只在序列化时单次发生，且只发生在外部 stats/mcp 命令路径中，TUI 主循环 0 额外负担。

---

## 5. 测试适配

测试里构建 `UsageRecord` 的字面量（如 `tests/watcher.rs`, `src/state/app_state.rs` 等）全部改为用 `intern("model_name")` 构造：

```rust
UsageRecord {
    model: intern("claude-sonnet-4"),
    session: intern("s1"),
    // ...
}
```

## 关键决策

| 决策 | 选择 | 理由 |
|---|---|---|
| Interner 库 | `lasso 0.7` with `multi-threaded` | Rust 标准字符串唯一化库，支持 `ThreadedRodeo` 并发 |
| 传输层解析 | `serde_json` 保持不变（零拷贝） | 避免 `simd-json` 带来的可变字节就地解析复杂度（YAGNI） |
| Date 优化 | `CompactDate` 包装 `u32` (4 字节) | 消除 `BTreeMap` 日期 Key 产生的临时 String 分配 |
| Key 序列化 | 为 `CompactDate` 适配 `Serialize` | 保证输出 JSON schema 与先前 100% 相同 |
| Quota | 保持不变 | Quota 数据量极小（仅 2 个），无优化必要 |
| 内存降幅 | String 堆分配减少 > 90% | TUI 常驻运行几乎只有 reader log 读取的单次 interning 堆分配 |

## 测试策略

### 单元测试 (src/state/app_state.rs 内部)

- `compact_date_ymd_conversion_correct`
- `compact_date_datetime_conversion_correct`
- `compact_date_serializes_as_yyyy_mm_dd`
- `interner_roundtrip_resolves_identical_strings`
- `interner_different_strings_yield_distinct_spurs`

### 全程验证
- 跑通全部 156 个集成与单元测试。
- 用 `cargo clippy` 确保新代码 0 warning。

## 向后兼容

- 配置文件 `~/.config/aum/config.toml` 0 改动。
- `aum stats` JSON 结构无任何字段变动。
- `aum mcp` tools/resources schema 无任何字段变动。

## TODOs

- [ ] `Cargo.toml` 加 `lasso` 依赖
- [ ] `src/state/app_state.rs` 实现 `INTERNER`, `intern`, `resolve`, `CompactDate`
- [ ] 更改 `UsageRecord`、`SessionSummary` 成员为 `Spur` 并编译通过
- [ ] 修复所有 reader 文件里构造 `UsageRecord` 的字面量编译错误
- [ ] 适配 `stats.rs` 内部的 `BTreeMap` 类型和 `build_platform_report`
- [ ] 适配 `mcp/server.rs` 内部的 `Spur` 到 String 的解析
- [ ] 跑通 `cargo test` 全量测试
- [ ] commit `feat(mcp): optimize memory using CompactDate and lasso interning`
