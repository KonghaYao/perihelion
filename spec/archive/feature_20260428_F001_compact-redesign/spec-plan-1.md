# Compact 系统重设计 执行计划（一）

**目标:** 全面增强 compact 系统，对齐 Claude Code 的 Micro-compact + Full Compact 两层压缩策略

**技术栈:** Rust 2021, tokio async, serde, thiserror, tracing

**设计文档:** spec/feature_20260428_F001_compact-redesign/spec-design.md

## 改动总览

本计划（spec-plan-1）覆盖核心层 compact 子系统的 4 个 Task：配置结构、工具对保护、Micro-compact 增强、Full Compact 增强。
- 涉及 `rust-create-agent/src/agent/compact/` 目录（config.rs / invariant.rs / micro.rs / full.rs / mod.rs）和 `rust-create-agent/src/agent/` 目录（mod.rs / token.rs）
- Task 依赖链：Task 1（CompactConfig）→ Task 2（invariant）→ Task 3（micro，依赖 config + invariant）→ Task 4（full，依赖 config + invariant）
- 关键决策：compact 核心逻辑全部放在 `rust-create-agent` 核心层，通过 `BaseModel` trait 调用 LLM，保持框架独立性；TUI 层集成在 spec-plan-2 中处理

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证 Cargo 构建工具可用
  - `cargo --version`
- [x] 验证 rust-create-agent crate 可独立构建
  - `cargo build -p rust-create-agent 2>&1 | tail -5`

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build -p rust-create-agent 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 或 "Compiling"，无 error
- [x] 测试命令可用
  - `cargo test -p rust-create-agent --lib 2>&1 | tail -5`
  - 预期: 输出包含 "test result:"，无配置错误

---

### Task 1: CompactConfig 配置结构

**背景:**
Compact 系统当前缺少统一的配置结构——阈值（如 auto_compact_threshold 0.85、warning_threshold 0.70）硬编码在 `token.rs` 的 `ContextBudget` 常量中，工具白名单、重新注入预算等参数无处配置。本 Task 新建 `CompactConfig` 结构体，集中管理 compact 系统的全部可配置参数，支持 serde 反序列化（从 settings.json 读取）和环境变量运行时覆盖。后续 Task 2-6 的 MicroCompact、FullCompact、ReInjector 等组件将直接引用 `CompactConfig` 字段驱动行为。

**涉及文件:**
- 新建: `rust-create-agent/src/agent/compact/config.rs`
- 新建: `rust-create-agent/src/agent/compact/mod.rs`
- 修改: `rust-create-agent/src/agent/mod.rs`

**执行步骤:**

- [x] 新建 `compact/mod.rs` 模块入口文件
  - 位置: `rust-create-agent/src/agent/compact/mod.rs`（新文件）
  - 内容: 声明 `pub mod config;` 和 `pub use config::CompactConfig;`，为后续 Task 预留扩展点
  - 原因: 作为 compact 子系统的模块入口，后续 Task 在此添加 micro / full / invariant / re_inject 子模块声明

- [x] 新建 `CompactConfig` 结构体
  - 位置: `rust-create-agent/src/agent/compact/config.rs`（新文件）
  - 引入依赖: `use serde::{Deserialize, Serialize};`，`use std::env;`
  - 定义默认工具白名单常量:
    ```rust
    const DEFAULT_COMPACTABLE_TOOLS: &[&str] = &[
        "bash", "read_file", "glob_files",
        "search_files_rg", "write_file", "edit_file",
    ];
    ```
  - 为每个字段编写独立的 `fn default_xxx() -> T` 函数，用于 `#[serde(default = "default_xxx")]`:
    ```rust
    fn default_true() -> bool { true }
    fn default_threshold_085() -> f64 { 0.85 }
    fn default_threshold_070() -> f64 { 0.70 }
    fn default_stale_steps() -> usize { 5 }
    fn default_compactable_tools() -> Vec<String> {
        DEFAULT_COMPACTABLE_TOOLS.iter().map(|s| s.to_string()).collect()
    }
    fn default_summary_max_tokens() -> u32 { 16000 }
    fn default_re_inject_max_files() -> usize { 5 }
    fn default_re_inject_max_tokens_per_file() -> u32 { 5000 }
    fn default_re_inject_file_budget() -> u32 { 25000 }
    fn default_re_inject_skills_budget() -> u32 { 25000 }
    fn default_max_consecutive_failures() -> u32 { 3 }
    fn default_ptl_max_retries() -> u32 { 3 }
    ```
  - 定义 `CompactConfig` 结构体:
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct CompactConfig {
        #[serde(default = "default_true")]
        pub auto_compact_enabled: bool,
        #[serde(default = "default_threshold_085")]
        pub auto_compact_threshold: f64,
        #[serde(default = "default_threshold_070")]
        pub micro_compact_threshold: f64,
        #[serde(default = "default_stale_steps")]
        pub micro_compact_stale_steps: usize,
        #[serde(default = "default_compactable_tools")]
        pub micro_compactable_tools: Vec<String>,
        #[serde(default = "default_summary_max_tokens")]
        pub summary_max_tokens: u32,
        #[serde(default = "default_re_inject_max_files")]
        pub re_inject_max_files: usize,
        #[serde(default = "default_re_inject_max_tokens_per_file")]
        pub re_inject_max_tokens_per_file: u32,
        #[serde(default = "default_re_inject_file_budget")]
        pub re_inject_file_budget: u32,
        #[serde(default = "default_re_inject_skills_budget")]
        pub re_inject_skills_budget: u32,
        #[serde(default = "default_max_consecutive_failures")]
        pub max_consecutive_failures: u32,
        #[serde(default = "default_ptl_max_retries")]
        pub ptl_max_retries: u32,
    }
    ```
  - 原因: 每个字段使用独立的 default 函数，确保 serde 反序列化时部分字段缺失也能正确填充默认值，与项目中 `ThinkingConfig` / `AppConfig` 的模式一致

- [x] 实现 `CompactConfig::default()` 方法
  - 位置: `rust-create-agent/src/agent/compact/config.rs`，在 `CompactConfig` 定义之后
  - 使用 `impl Default for CompactConfig`:
    ```rust
    impl Default for CompactConfig {
        fn default() -> Self {
            Self {
                auto_compact_enabled: true,
                auto_compact_threshold: 0.85,
                micro_compact_threshold: 0.70,
                micro_compact_stale_steps: 5,
                micro_compactable_tools: default_compactable_tools(),
                summary_max_tokens: 16000,
                re_inject_max_files: 5,
                re_inject_max_tokens_per_file: 5000,
                re_inject_file_budget: 25000,
                re_inject_skills_budget: 25000,
                max_consecutive_failures: 3,
                ptl_max_retries: 3,
            }
        }
    }
    ```
  - 原因: `Default` trait 允许 `CompactConfig::default()` 直接构造，用于不通过 settings.json 的场景（如测试、headless 模式）

- [x] 实现 `CompactConfig::from_env()` 方法 — 环境变量覆盖
  - 位置: `rust-create-agent/src/agent/compact/config.rs`，在 `impl Default` 之后
  - 方法签名: `pub fn from_env() -> Self`
  - 逻辑:
    ```rust
    impl CompactConfig {
        /// 从环境变量构建配置，未设置的环境变量使用默认值
        pub fn from_env() -> Self {
            let mut config = Self::default();
            if env::var("DISABLE_COMPACT").is_ok() {
                config.auto_compact_enabled = false;
                config.micro_compact_threshold = 1.0; // 永不触发
            }
            if env::var("DISABLE_AUTO_COMPACT").is_ok() {
                config.auto_compact_enabled = false;
            }
            if let Ok(val) = env::var("COMPACT_THRESHOLD") {
                if let Ok(threshold) = val.parse::<f64>() {
                    if (0.0..=1.0).contains(&threshold) {
                        config.auto_compact_threshold = threshold;
                    }
                }
            }
            config
        }

        /// 在已有配置基础上应用环境变量覆盖
        pub fn apply_env_overrides(&mut self) {
            if env::var("DISABLE_COMPACT").is_ok() {
                self.auto_compact_enabled = false;
                self.micro_compact_threshold = 1.0;
            }
            if env::var("DISABLE_AUTO_COMPACT").is_ok() {
                self.auto_compact_enabled = false;
            }
            if let Ok(val) = env::var("COMPACT_THRESHOLD") {
                if let Ok(threshold) = val.parse::<f64>() {
                    if (0.0..=1.0).contains(&threshold) {
                        self.auto_compact_threshold = threshold;
                    }
                }
            }
        }
    }
    ```
  - 原因: 支持三种环境变量覆盖模式——`DISABLE_COMPACT` 完全禁用（micro 阈值设为 1.0 永不触发）、`DISABLE_AUTO_COMPACT` 仅禁用自动触发、`COMPACT_THRESHOLD` 覆盖自动压缩阈值。`from_env()` 用于无 settings.json 的场景，`apply_env_overrides()` 用于在 settings.json 配置基础上叠加环境变量覆盖

- [x] 修改 `agent/mod.rs`，注册 compact 模块
  - 位置: `rust-create-agent/src/agent/mod.rs`（现有文件）
  - 在 `pub mod token;` 行之后添加 `pub mod compact;`
  - 在 `pub use token::{...};` 行之后添加:
    ```rust
    pub use compact::CompactConfig;
    ```
  - 原因: 将 compact 子模块暴露为 agent 模块的公共 API，下游 crate（rust-agent-tui）通过 `rust_create_agent::agent::CompactConfig` 引用

- [x] 为 `CompactConfig` 编写单元测试
  - 测试文件: `rust-create-agent/src/agent/compact/config.rs`（内联 `#[cfg(test)] mod tests`）
  - 测试场景:
    - `test_default_values`: `CompactConfig::default()` → 所有字段等于预期默认值（auto_compact_enabled=true, auto_compact_threshold=0.85, micro_compact_threshold=0.70, micro_compact_stale_steps=5, micro_compactable_tools 包含 6 个默认工具, summary_max_tokens=16000, re_inject_max_files=5, re_inject_max_tokens_per_file=5000, re_inject_file_budget=25000, re_inject_skills_budget=25000, max_consecutive_failures=3, ptl_max_retries=3）
    - `test_serde_roundtrip`: 构造自定义 `CompactConfig`（修改 3 个字段的值）→ serde_json 序列化 → 反序列化 → 断言所有字段与原始值一致
    - `test_serde_partial_deserialize`: JSON 仅包含 `{"auto_compact_threshold": 0.90}` → 反序列化 → 断言 `auto_compact_threshold==0.90` 且其余字段为默认值
    - `test_serde_empty_object`: JSON `{}` → 反序列化 → 断言所有字段均为默认值
    - `test_from_env_disable_compact`: 临时设置 `DISABLE_COMPACT=1` 环境变量 → 调用 `CompactConfig::from_env()` → 断言 `auto_compact_enabled==false` 且 `micro_compact_threshold==1.0`；测试后清理环境变量
    - `test_from_env_disable_auto_compact`: 临时设置 `DISABLE_AUTO_COMPACT=1` → 调用 `from_env()` → 断言 `auto_compact_enabled==false`，`micro_compact_threshold` 仍为默认 0.70
    - `test_from_env_compact_threshold`: 临时设置 `COMPACT_THRESHOLD=0.75` → 调用 `from_env()` → 断言 `auto_compact_threshold==0.75`
    - `test_from_env_compact_threshold_invalid`: 临时设置 `COMPACT_THRESHOLD=abc` → 调用 `from_env()` → 断言 `auto_compact_threshold` 仍为默认 0.85
    - `test_from_env_compact_threshold_out_of_range`: 临时设置 `COMPACT_THRESHOLD=1.5` → 调用 `from_env()` → 断言 `auto_compact_threshold` 仍为默认 0.85（超出 0.0-1.0 范围被忽略）
    - `test_apply_env_overrides_on_custom_config`: 构造 `CompactConfig { auto_compact_threshold: 0.90, ..Default::default() }` → 设置 `COMPACT_THRESHOLD=0.80` → 调用 `apply_env_overrides()` → 断言 `auto_compact_threshold==0.80`（环境变量覆盖 settings.json 的值）
    - `test_compactable_tools_default_content`: 验证 `default().micro_compactable_tools` 包含 `["bash", "read_file", "glob_files", "search_files_rg", "write_file", "edit_file"]` 且长度为 6
  - 环境变量测试使用 `std::env::set_var` / `std::env::remove_var`，每个测试函数在开始时清理、结束时恢复，避免测试间污染
  - 运行命令: `cargo test -p rust-create-agent --lib -- config::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 compact 模块编译通过
  - `cargo build -p rust-create-agent 2>&1 | tail -5`
  - 预期: 输出包含 "Compiling rust-create-agent" 且无 error

- [x] 验证 CompactConfig 在 agent 模块中正确导出
  - `grep -n 'pub mod compact' rust-create-agent/src/agent/mod.rs && grep -n 'pub use compact' rust-create-agent/src/agent/mod.rs`
  - 预期: 输出两行，分别包含 `pub mod compact` 和 `pub use compact::CompactConfig`

- [x] 验证 config.rs 结构体字段与设计规格一致
  - `grep -c 'pub ' rust-create-agent/src/agent/compact/config.rs`
  - 预期: 计数 >= 12（对应 12 个 pub 字段）

- [x] 验证全部单元测试通过
  - `cargo test -p rust-create-agent --lib -- config::tests 2>&1 | tail -20`
  - 预期: 输出包含 "test result: ok" 且无 FAILED

- [x] 验证全量测试无回归
  - `cargo test -p rust-create-agent 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok"

---

### Task 2: 工具对完整性保护

**背景:**
当前 `micro_compact()` 函数按固定 `keep_recent` 位置截断，不感知工具调用对（Ai 消息含 `tool_calls` → 后续 Tool 消息用 `tool_call_id` 引用）的关联关系，可能拆开一对中的 Ai 父消息和 Tool 子消息，或仅清除同一 Ai 消息对应的多个 Tool 消息中的部分，破坏 API 级别的消息完整性约束。本 Task 新建 `invariant.rs` 模块，实现消息按 API round 分组的 `group_messages_by_round()` 函数和清除边界调整函数 `adjust_index_to_preserve_invariants()`，供后续 Task 3（Micro-compact 增强）和 Task 4（Full Compact PTL 降级）直接调用。

**涉及文件:**
- 新建: `rust-create-agent/src/agent/compact/invariant.rs`
- 修改: `rust-create-agent/src/agent/compact/mod.rs`（Task 1 创建，添加 `pub mod invariant;`）

**执行步骤:**

- [x] 新建 `invariant.rs`，实现 `MessageRound` 分组结构体
  - 位置: `rust-create-agent/src/agent/compact/invariant.rs`（新文件）
  - 引入依赖: `use crate::messages::BaseMessage;`
  - 定义 `MessageRound` 结构体:
    ```rust
    /// 一个 API round：从一条有 tool_calls 的 Ai 消息开始，到其所有 Tool 消息结束
    /// 如果 Ai 消息没有 tool_calls，则该 round 仅包含此 Ai 消息本身
    #[derive(Debug, Clone)]
    pub struct MessageRound {
        /// 此 round 在原始消息切片中的起始索引（含）
        pub start: usize,
        /// 此 round 在原始消息切片中的结束索引（不含）
        pub end: usize,
        /// 此 round 中 Ai 消息的 tool_call_id 列表（如果 Ai 有 tool_calls）
        pub tool_call_ids: Vec<String>,
    }
    ```
  - 原因: PTL 降级（Task 4）需要按 round 粒度删除消息组，`MessageRound` 提供每个 round 的索引范围和关联的工具调用 ID

- [x] 实现 `group_messages_by_round()` 函数
  - 位置: `rust-create-agent/src/agent/compact/invariant.rs`，在 `MessageRound` 定义之后
  - 函数签名: `pub fn group_messages_by_round(messages: &[BaseMessage]) -> Vec<MessageRound>`
  - 核心逻辑:
    ```rust
    pub fn group_messages_by_round(messages: &[BaseMessage]) -> Vec<MessageRound> {
        let mut rounds = Vec::new();
        let mut i = 0;
        while i < messages.len() {
            let round_start = i;
            // 跳过当前消息之前所有非 Ai 消息（Human / System）
            // 当遇到 Ai 消息时，检查是否有 tool_calls
            if matches!(&messages[i], BaseMessage::Ai { tool_calls, .. } if !tool_calls.is_empty()) {
                // 提取所有 tool_call_id
                let tool_call_ids: Vec<String> = messages[i].tool_calls()
                    .iter().map(|tc| tc.id.clone()).collect();
                let tc_count = tool_call_ids.len();
                // 向后扫描，收集紧跟的 Tool 消息（tool_call_id 匹配的）
                let mut end = i + 1;
                let mut matched = 0;
                while end < messages.len() && matched < tc_count {
                    if let BaseMessage::Tool { tool_call_id, .. } = &messages[end] {
                        if tool_call_ids.contains(tool_call_id) {
                            matched += 1;
                        } else {
                            // 遇到不匹配的 Tool 消息，停止
                            break;
                        }
                    } else {
                        // 遇到非 Tool 消息，此 round 结束
                        break;
                    }
                    end += 1;
                }
                rounds.push(MessageRound {
                    start: round_start,
                    end,
                    tool_call_ids,
                });
                i = end;
            } else {
                // Ai 消息无 tool_calls，或非 Ai 消息（Human/System）：
                // 单独作为一个 round
                rounds.push(MessageRound {
                    start: round_start,
                    end: i + 1,
                    tool_call_ids: Vec::new(),
                });
                i += 1;
            }
        }
        rounds
    }
    ```
  - **重要边界处理**:
    - 每个 round 的 `start` 精确指向该 round 的第一条消息索引
    - Tool 消息**只能**匹配其前方最近的有 `tool_calls` 的 Ai 消息中的 `tool_call_id`
    - 如果 Tool 消息的 `tool_call_id` 在当前 Ai 消息的 `tool_call_ids` 中找不到，该 Tool 消息不属于当前 round，单独作为一个 round（兼容异常消息序列）
    - 空 `messages` 切片返回空 Vec
  - 原因: `group_messages_by_round()` 是 PTL 降级（Task 4）的基础——按 round 粒度删除消息以缩减 prompt 长度

- [x] 实现 `find_tool_pair_boundary()` 辅助函数
  - 位置: `rust-create-agent/src/agent/compact/invariant.rs`，在 `group_messages_by_round()` 之后
  - 函数签名: `fn find_tool_pair_boundary(messages: &[BaseMessage], index: usize) -> (usize, usize)`
  - 功能: 给定一个索引 `index`，如果 `messages[index]` 是一条 Tool 消息，向前查找包含其 `tool_call_id` 的 Ai 消息，向后查找同一 Ai 消息的所有其他 Tool 消息，返回完整工具对的 `[start, end)` 范围；如果 `messages[index]` 不是 Tool 消息，返回 `(index, index + 1)`
  - 核心逻辑:
    ```rust
    fn find_tool_pair_boundary(messages: &[BaseMessage], index: usize) -> (usize, usize) {
        let tool_call_id = match &messages[index] {
            BaseMessage::Tool { tool_call_id, .. } => tool_call_id.clone(),
            _ => return (index, index + 1),
        };

        // 向前查找包含此 tool_call_id 的 Ai 消息
        let ai_index = messages[..index].iter().enumerate().rev()
            .find_map(|(i, msg)| {
                if msg.has_tool_calls() {
                    let ids: Vec<&str> = msg.tool_calls().iter().map(|tc| tc.id.as_str()).collect();
                    if ids.contains(&tool_call_id.as_str()) {
                        return Some(i);
                    }
                }
                None
            })
            .unwrap_or(index); // 找不到则回退到 index 本身

        // 从 Ai 消息中获取所有 tool_call_ids
        let all_tc_ids: Vec<String> = messages[ai_index].tool_calls()
            .iter().map(|tc| tc.id.clone()).collect();

        // 向后查找所有匹配的 Tool 消息的最远位置
        let mut end = ai_index + 1;
        while end < messages.len() {
            if let BaseMessage::Tool { tool_call_id: tc_id, .. } = &messages[end] {
                if all_tc_ids.iter().any(|id| id == tc_id) {
                    end += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        (ai_index, end)
    }
    ```
  - 原因: `adjust_index_to_preserve_invariants()` 需要在边界处快速定位一个索引的完整工具对范围，此辅助函数封装了前向/后向扫描逻辑

- [x] 实现 `adjust_index_to_preserve_invariants()` 核心函数
  - 位置: `rust-create-agent/src/agent/compact/invariant.rs`，在 `find_tool_pair_boundary()` 之后
  - 函数签名: `pub fn adjust_index_to_preserve_invariants(messages: &[BaseMessage], start: usize, end: usize) -> (usize, usize)`
  - 功能: 给定一个清除范围 `[start, end)`，调整边界以确保不会拆开工具对。返回调整后的 `[adjusted_start, adjusted_end)`
  - 核心逻辑:
    ```rust
    pub fn adjust_index_to_preserve_invariants(
        messages: &[BaseMessage],
        start: usize,
        end: usize,
    ) -> (usize, usize) {
        if messages.is_empty() || start >= end || start >= messages.len() {
            return (start.min(messages.len()), end.min(messages.len()));
        }

        let mut adjusted_start = start;
        let mut adjusted_end = end.min(messages.len());

        // 调整起始边界：如果 start 位置落在某个工具对中间，向前扩展到包含整个工具对
        let (pair_start, _pair_end) = find_tool_pair_boundary(messages, adjusted_start);
        if pair_start < adjusted_start {
            adjusted_start = pair_start;
        }

        // 调整结束边界：如果 end-1 位置的 Tool 消息的工具对延伸到 end 之后，
        // 或者 end 位置落在某个工具对的 Tool 消息区域中，向后扩展
        if adjusted_end < messages.len() {
            // 检查 end 边界是否切断了某个工具对
            let (pair_start, pair_end) = find_tool_pair_boundary(messages, adjusted_end);
            // 如果 end 处的消息是某个工具对的 Tool 消息（pair_start < end < pair_end），
            // 则需要将 end 扩展到 pair_end 以包含整个工具对
            if pair_start < adjusted_end && pair_end > adjusted_end {
                adjusted_end = pair_end;
            }
        }

        // 二次检查：确保 [adjusted_start, adjusted_end) 内不存在被截断的工具对
        // 遍历范围内每条 Tool 消息，验证其对应 Ai 消息也在范围内
        for i in adjusted_start..adjusted_end {
            if matches!(&messages[i], BaseMessage::Tool { .. }) {
                let (ps, pe) = find_tool_pair_boundary(messages, i);
                // 如果工具对起始在范围外，扩展到包含整个对
                if ps < adjusted_start {
                    adjusted_start = ps;
                }
                if pe > adjusted_end {
                    adjusted_end = pe.min(messages.len());
                }
            }
        }

        (adjusted_start, adjusted_end)
    }
    ```
  - **关键约束**:
    - `start` 不能小于 0，`end` 不能超过 `messages.len()`
    - 调整是单调扩展的——只会扩大范围，不会缩小
    - 同一条 Ai 消息的多个 Tool 消息要么全部在范围内，要么全部在范围外
  - 原因: 此函数是 Micro-compact（Task 3）和 Full Compact PTL 降级（Task 4）的核心保护机制，确保压缩操作不会产生 API 不合法的消息序列

- [x] 修改 `compact/mod.rs`，注册 `invariant` 子模块并导出公共 API
  - 位置: `rust-create-agent/src/agent/compact/mod.rs`（Task 1 创建）
  - 在 `pub mod config;` 行之后添加:
    ```rust
    pub mod invariant;
    ```
  - 在 `pub use config::CompactConfig;` 行之后添加:
    ```rust
    pub use invariant::{
        adjust_index_to_preserve_invariants,
        group_messages_by_round,
        MessageRound,
    };
    ```
  - 原因: 将 invariant 模块的三个公共 API 暴露为 compact 模块的公共 API，供 Task 3/4 通过 `crate::agent::compact::adjust_index_to_preserve_invariants` 调用

- [x] 为 `invariant` 模块编写单元测试
  - 测试文件: `rust-create-agent/src/agent/compact/invariant.rs`（内联 `#[cfg(test)] mod tests`）
  - 引入测试依赖:
    ```rust
    use crate::messages::{BaseMessage, MessageContent, ToolCallRequest};
    use serde_json::json;
    ```
  - 辅助函数:
    ```rust
    /// 构造一条含 tool_calls 的 Ai 消息
    fn ai_with_tools(ids: &[&str]) -> BaseMessage {
        let tcs: Vec<ToolCallRequest> = ids.iter()
            .map(|&id| ToolCallRequest::new(id, "bash", json!({"command": "echo"})))
            .collect();
        BaseMessage::ai_with_tool_calls(MessageContent::text("using tools"), tcs)
    }

    /// 构造一条普通 Ai 消息（无 tool_calls）
    fn ai_plain(text: &str) -> BaseMessage {
        BaseMessage::ai(text)
    }

    /// 构造一条 Tool 结果消息
    fn tool_msg(tc_id: &str, text: &str) -> BaseMessage {
        BaseMessage::tool_result(tc_id, text)
    }
    ```
  - 测试场景:
    - **group_messages_by_round**:
      - `test_group_empty`: 空消息序列 → 返回空 Vec
      - `test_group_plain_ai_only`: `[Ai, Ai, Ai]`（无 tool_calls）→ 3 个 round，每个 round 包含单条消息
      - `test_group_human_ai_alternating`: `[Human, Ai, Human, Ai]` → 4 个 round
      - `test_group_single_tool_pair`: `[Ai{tool_calls:["tc1"]}, Tool{tc_id:"tc1"}]` → 1 个 round，`start=0, end=2, tool_call_ids=["tc1"]`
      - `test_group_multiple_tools_one_ai`: `[Ai{tool_calls:["tc1","tc2"]}, Tool{tc_id:"tc1"}, Tool{tc_id:"tc2"}]` → 1 个 round，`end=3, tool_call_ids=["tc1","tc2"]`
      - `test_group_mixed_rounds`: `[Human, Ai{tc:["tc1"]}, Tool{tc1}, Ai_plain, Ai{tc:["tc2"]}, Tool{tc2}]` → 5 个 round（Human 单独 1 个，Ai+Tool 第 2 个，Ai_plain 第 3 个，Ai+Tool 第 4-5 个... 验证实际分组结果）
      - `test_group_orphan_tool_message`: `[Tool{tc_id:"orphan"}]` → 1 个 round，`tool_call_ids=[]`（孤立 Tool 消息单独成组）
      - `test_group_interleaved_human_in_tool_pair`: `[Ai{tc:["tc1","tc2"]}, Tool{tc1}, Human, Tool{tc2}]` → Ai+Tool{tc1} 成一组（round end=2），Human 单独一组，Tool{tc2} 单独一组（因为 Human 打断了连续的 Tool 序列）
    - **find_tool_pair_boundary**:
      - `test_find_boundary_tool_message`: `[Ai{tc:["tc1","tc2"]}, Tool{tc1}, Tool{tc2}]`，对 index=1 调用 → 返回 `(0, 3)`
      - `test_find_boundary_ai_message`: `[Ai{tc:["tc1"]}, Tool{tc1}]`，对 index=0 调用 → 返回 `(0, 1)`（Ai 消息本身不是 Tool，返回自身范围）
      - `test_find_boundary_human_message`: `[Human, Ai, Tool]`，对 index=0 调用 → 返回 `(0, 1)`
    - **adjust_index_to_preserve_invariants**:
      - `test_adjust_no_tool_calls`: `[Human, Ai, Human, Ai]`，`adjust(1, 3)` → `(1, 3)`（无工具对，边界不变）
      - `test_adjust_start_splits_pair`: `[Ai{tc:["tc1"]}, Tool{tc1}, Human, Ai]`，`adjust(1, 4)` → start 从 1 前移到 0 以包含 Ai 父消息，返回 `(0, 4)`
      - `test_adjust_end_splits_pair`: `[Human, Ai{tc:["tc1"]}, Tool{tc1}, Human]`，`adjust(0, 2)` → end 从 2 扩展到 3 以包含 Tool 消息，返回 `(0, 3)`
      - `test_adjust_both_boundaries_split`: `[Ai{tc:["tc1"]}, Tool{tc1}, Ai{tc:["tc2"]}, Tool{tc2}]`，`adjust(1, 3)` → start 前移到 0，end 扩展到 4，返回 `(0, 4)`
      - `test_adjust_multiple_tools_partial`: `[Ai{tc:["tc1","tc2"]}, Tool{tc1}, Tool{tc2}, Human]`，`adjust(0, 2)` → end 从 2 扩展到 3（tc2 也必须包含），返回 `(0, 3)`
      - `test_adjust_already_aligned`: `[Human, Ai{tc:["tc1"]}, Tool{tc1}, Human]`，`adjust(0, 3)` → `(0, 3)`（已对齐，不调整）
      - `test_adjust_empty_messages`: `[]`，`adjust(0, 0)` → `(0, 0)`
      - `test_adjust_full_range`: 任意消息序列，`adjust(0, len)` → `(0, len)`（全范围不调整）
      - `test_adjust_start_at_end`: 任意消息序列，`adjust(len, len)` → `(len, len)`（空范围）
    - 运行命令: `cargo test -p rust-create-agent --lib -- compact::invariant::tests`
    - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 invariant.rs 编译通过
  - `cargo build -p rust-create-agent 2>&1 | tail -5`
  - 预期: 输出包含 "Compiling rust-create-agent" 且无 error

- [x] 验证 invariant 模块在 compact/mod.rs 中正确注册
  - `grep -n 'pub mod invariant' rust-create-agent/src/agent/compact/mod.rs && grep -n 'pub use invariant' rust-create-agent/src/agent/compact/mod.rs`
  - 预期: 输出两行，分别包含 `pub mod invariant` 和 `pub use invariant::{`

- [x] 验证公共 API 导出完整
  - `grep -c 'adjust_index_to_preserve_invariants\|group_messages_by_round\|MessageRound' rust-create-agent/src/agent/compact/mod.rs`
  - 预期: 计数 >= 3（三个导出项各出现一次）

- [x] 验证全部单元测试通过
  - `cargo test -p rust-create-agent --lib -- compact::invariant::tests 2>&1 | tail -20`
  - 预期: 输出包含 "test result: ok" 且无 FAILED

- [x] 验证全量测试无回归
  - `cargo test -p rust-create-agent 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok"

---

### Task 3: Micro-compact 策略增强

**背景:**
现有 `micro_compact()` 函数（`token.rs:96-110`）仅按字符数阈值（500 字符）机械清除旧工具结果，不区分工具类型（可能清除 `ask_user_question` 等重要交互结果）、不感知时间衰减（刚执行的 `bash` 结果与 20 步前的结果同等对待）、不清除图片/大文档等高 token 内容块、不保护工具对完整性（可能拆开 Ai 父消息与 Tool 子消息的关联）。本 Task 新建 `micro.rs` 模块，实现增强版 `micro_compact_enhanced()` 函数：基于 `CompactConfig.micro_compactable_tools` 白名单过滤、基于 `micro_compact_stale_steps` 时间衰减判断、图片/大文档 ContentBlock 清除、调用 `adjust_index_to_preserve_invariants()` 保护工具对完整性。同时将旧 `micro_compact()` 标记为 `#[deprecated]`，引导调用方迁移到新函数。本 Task 的输出是 Task 6（`compact_task()` 统一入口重写）中 Micro-compact 调用链的核心组件。

**涉及文件:**
- 新建: `rust-create-agent/src/agent/compact/micro.rs`
- 修改: `rust-create-agent/src/agent/compact/mod.rs`（Task 1 创建，添加 `pub mod micro;`）
- 修改: `rust-create-agent/src/agent/token.rs`（标记旧函数 `#[deprecated]`）

**执行步骤:**

- [x] 新建 `micro.rs`，实现辅助函数 `estimate_tokens()`
  - 位置: `rust-create-agent/src/agent/compact/micro.rs`（新文件）
  - 引入依赖:
    ```rust
    use crate::agent::compact::config::CompactConfig;
    use crate::agent::compact::invariant::{adjust_index_to_preserve_invariants, group_messages_by_round};
    use crate::messages::{BaseMessage, ContentBlock, MessageContent};
    ```
  - 定义 `estimate_tokens()` 辅助函数:
    ```rust
    /// 简单 token 估算：字符数 / 4
    fn estimate_tokens(text: &str) -> usize {
        text.len() / 4
    }
    ```
  - 原因: 图片/文档清除时需判断 token 大小是否超过阈值，使用字符数/4 估算（与设计规格一致）

- [x] 实现 `find_tool_name_for_tool_result()` 辅助函数
  - 位置: `rust-create-agent/src/agent/compact/micro.rs`，在 `estimate_tokens()` 之后
  - 函数签名: `fn find_tool_name_for_tool_result(messages: &[BaseMessage], tool_call_id: &str) -> Option<String>`
  - 核心逻辑:
    ```rust
    /// 从消息列表中查找 Tool 消息对应的工具名
    /// 向前遍历找到包含匹配 tool_call_id 的 Ai 消息，从其 tool_calls 中提取工具名
    fn find_tool_name_for_tool_result(messages: &[BaseMessage], tool_call_id: &str) -> Option<String> {
        for msg in messages.iter().rev() {
            if let BaseMessage::Ai { tool_calls, .. } = msg {
                for tc in tool_calls {
                    if tc.id == tool_call_id {
                        return Some(tc.name.clone());
                    }
                }
            }
        }
        None
    }
    ```
  - 原因: `micro_compact_enhanced()` 需要根据工具名判断是否在白名单中，而 Tool 消息本身不存储工具名，必须回溯到对应的 Ai 消息的 `tool_calls` 中查找

- [x] 实现 `compact_tool_result_content()` 辅助函数
  - 位置: `rust-create-agent/src/agent/compact/micro.rs`，在 `find_tool_name_for_tool_result()` 之后
  - 函数签名: `fn compact_tool_result_content(content: &mut MessageContent, config: &CompactConfig) -> bool`
  - 功能: 对单条 Tool 消息的 content 执行图片/文档清除。返回 `true` 表示内容被修改
  - 核心逻辑:
    ```rust
    fn compact_tool_result_content(content: &mut MessageContent, config: &CompactConfig) -> bool {
        let original_text = content.text_content();
        if original_text.is_empty() {
            return false;
        }

        let blocks = content.content_blocks();

        // 检查是否有需要清除的 Image / Document blocks
        let has_image_or_doc = blocks.iter().any(|b| {
            matches!(b, ContentBlock::Image { .. } | ContentBlock::Document { .. })
        });

        if !has_image_or_doc {
            return false;
        }

        let mut modified = false;
        let new_blocks: Vec<ContentBlock> = blocks.into_iter().map(|b| match &b {
            ContentBlock::Image { source } => {
                let size_chars = match source {
                    crate::messages::ImageSource::Base64 { data, .. } => data.len(),
                    crate::messages::ImageSource::Url { url } => url.len(),
                };
                let token_est = size_chars / 4;
                modified = true;
                if token_est > config.re_inject_max_tokens_per_file as usize {
                    ContentBlock::text(format!("[compacted: image ~{} tokens]", token_est))
                } else {
                    ContentBlock::text("[image]")
                }
            }
            ContentBlock::Document { source, .. } => {
                let size_chars = match source {
                    crate::messages::DocumentSource::Base64 { data, .. } => data.len(),
                    crate::messages::DocumentSource::Text { text } => text.len(),
                    crate::messages::DocumentSource::Url { url } => url.len(),
                };
                let token_est = size_chars / 4;
                modified = true;
                if token_est > config.re_inject_max_tokens_per_file as usize {
                    ContentBlock::text(format!("[compacted: document ~{} tokens]", token_est))
                } else {
                    ContentBlock::text("[document]")
                }
            }
            _ => b,
        }).collect();

        if modified {
            *content = MessageContent::blocks(new_blocks);
        }
        modified
    }
    ```
  - 原因: 图片 Base64 编码和大型文档是 token 消耗的主要来源，替换为占位符可大幅降低上下文占用

- [x] 实现 `micro_compact_enhanced()` 核心函数
  - 位置: `rust-create-agent/src/agent/compact/micro.rs`，在 `compact_tool_result_content()` 之后
  - 函数签名:
    ```rust
    /// 增强版 Micro-compact
    /// - config: 配置参数（白名单、衰减步数等）
    /// - messages: 可变引用消息列表
    /// 返回清除（内容被替换）的消息数量
    pub fn micro_compact_enhanced(config: &CompactConfig, messages: &mut [BaseMessage]) -> usize
    ```
  - 核心逻辑:
    ```rust
    pub fn micro_compact_enhanced(config: &CompactConfig, messages: &mut [BaseMessage]) -> usize {
        if messages.is_empty() {
            return 0;
        }

        // 1. 按 round 分组，确定每个消息所在的 step
        let rounds = group_messages_by_round(messages);
        let total_rounds = rounds.len();
        let stale_threshold = config.micro_compact_stale_steps;

        // 冷却区边界：round 编号 < stale_round_limit 的属于冷却区
        let stale_round_limit = total_rounds.saturating_sub(stale_threshold);

        // 为每条消息计算其所属 round 编号
        let mut round_index = vec![0usize; messages.len()];
        for (ri, round) in rounds.iter().enumerate() {
            for mi in round.start..round.end {
                if mi < messages.len() {
                    round_index[mi] = ri;
                }
            }
        }

        // 2. 收集可清除的 Tool 消息索引（冷却区 + 白名单 + 非错误）
        let mut compactable_indices: Vec<usize> = Vec::new();
        for (i, msg) in messages.iter().enumerate() {
            if let BaseMessage::Tool { tool_call_id, is_error, .. } = msg {
                if *is_error { continue; }
                if round_index[i] >= stale_round_limit { continue; }
                let tool_name = find_tool_name_for_tool_result(messages, tool_call_id);
                match tool_name {
                    Some(ref name) if config.micro_compactable_tools.contains(name) => {},
                    _ => continue,
                }
                compactable_indices.push(i);
            }
        }

        // 3. 即使没有完全可清除的，也对冷却区 Tool 消息执行图片/文档清除
        if compactable_indices.is_empty() {
            let mut image_cleared = 0;
            for i in 0..messages.len() {
                if round_index[i] >= stale_round_limit { continue; }
                if let BaseMessage::Tool { content, is_error, .. } = &mut messages[i] {
                    if *is_error { continue; }
                    if compact_tool_result_content(content, config) {
                        image_cleared += 1;
                    }
                }
            }
            return image_cleared;
        }

        // 4. 调用 adjust_index_to_preserve_invariants 保护工具对完整性
        let compact_start = *compactable_indices.first().unwrap();
        let compact_end = *compactable_indices.last().unwrap() + 1;
        let (adj_start, adj_end) = adjust_index_to_preserve_invariants(
            messages, compact_start, compact_end,
        );

        // 5. 对调整后范围内的冷却区白名单 Tool 消息执行清除
        let mut cleared = 0;
        for i in adj_start..adj_end {
            if round_index[i] >= stale_round_limit { continue; }
            if let BaseMessage::Tool { tool_call_id, content, is_error } = &mut messages[i] {
                if *is_error { continue; }
                let tool_name = find_tool_name_for_tool_result(messages, tool_call_id);
                let in_whitelist = match tool_name {
                    Some(ref name) => config.micro_compactable_tools.contains(name),
                    None => false,
                };
                if !in_whitelist { continue; }

                // 先执行图片/文档清除
                let original_text = content.text_content();
                let original_len = original_text.len();
                compact_tool_result_content(content, config);

                // 再判断是否需要整体文本替换
                let current_text = content.text_content();
                if !current_text.starts_with("[compacted:")
                    && !current_text.starts_with("[image]")
                    && !current_text.starts_with("[document]") {
                    *content = MessageContent::text(format!(
                        "[compacted: {} chars]", original_len
                    ));
                }
                cleared += 1;
            }
        }

        cleared
    }
    ```
  - **关键设计决策**:
    - 使用 `group_messages_by_round()` 返回的 round 索引计算每条消息的"步数"，round 编号 < `stale_round_limit` 的为冷却区
    - `find_tool_name_for_tool_result()` 向前遍历所有消息查找对应 Ai 消息
    - `adjust_index_to_preserve_invariants()` 以所有可清除索引的范围为输入，自动扩展边界保护工具对
    - 扩大范围后仍需通过白名单和时间衰减二次过滤，避免保护性扩展引入非预期清除
    - 图片/文档清除优先于文本内容替换执行
  - 原因: 替换 `token.rs` 中简单粗暴的 `micro_compact()`，引入白名单、时间衰减、图片清除、工具对保护四重策略

- [x] 修改 `compact/mod.rs`，注册 `micro` 子模块并导出公共 API
  - 位置: `rust-create-agent/src/agent/compact/mod.rs`（Task 1 创建）
  - 在 `pub mod invariant;` 行之后添加:
    ```rust
    pub mod micro;
    ```
  - 在 `pub use invariant::{...};` 块之后添加:
    ```rust
    pub use micro::micro_compact_enhanced;
    ```
  - 原因: 将增强版 micro_compact_enhanced 暴露为 compact 模块的公共 API，供 Task 6 的 `compact_task()` 统一入口调用

- [x] 标记旧 `micro_compact()` 为 `#[deprecated]`
  - 位置: `rust-create-agent/src/agent/token.rs`，在 `pub fn micro_compact` 函数定义之前（~L93）
  - 在 `/// 轻量级压缩：清除旧工具结果中的大段内容` 文档注释之前添加:
    ```rust
    #[deprecated(
        since = "0.2.0",
        note = "使用 `crate::agent::compact::micro_compact_enhanced` 代替，支持白名单过滤、时间衰减、图片清除和工具对保护"
    )]
    ```
  - 原因: 保留旧函数保证编译兼容性（现有测试仍能运行），引导新代码使用增强版。待 Task 6 的 `compact_task()` 重写完成后可在后续版本移除

- [x] 为 `micro` 模块编写单元测试
  - 测试文件: `rust-create-agent/src/agent/compact/micro.rs`（内联 `#[cfg(test)] mod tests`）
  - 引入测试依赖:
    ```rust
    use super::*;
    use crate::messages::{BaseMessage, MessageContent, ToolCallRequest};
    use serde_json::json;
    ```
  - 辅助函数:
    ```rust
    fn test_config() -> CompactConfig {
        CompactConfig::default()
    }

    fn ai_with_tool(id: &str, name: &str) -> BaseMessage {
        BaseMessage::ai_with_tool_calls(
            MessageContent::text("using tool"),
            vec![ToolCallRequest::new(id, name, json!({}))]
        )
    }

    fn tool_result(tc_id: &str, text: &str) -> BaseMessage {
        BaseMessage::tool_result(tc_id, text)
    }

    fn tool_result_with_image(tc_id: &str, text: &str) -> BaseMessage {
        BaseMessage::tool_result(
            tc_id,
            MessageContent::blocks(vec![
                ContentBlock::text(text),
                ContentBlock::image_base64("image/png", "iVBOR...base64data"),
            ])
        )
    }

    fn tool_result_with_large_image(tc_id: &str) -> BaseMessage {
        let large_b64 = "A".repeat(100_000);
        BaseMessage::tool_result(
            tc_id,
            MessageContent::blocks(vec![
                ContentBlock::text("output"),
                ContentBlock::image_base64("image/png", &large_b64),
            ])
        )
    }
    ```
  - 测试场景:

    **白名单过滤:**
    - `test_whitelist_only_compactable_tools`:
      - 输入: `[ai_with_tool("tc1","bash"), tool_result("tc1", 600chars), Human("q"), ai_with_tool("tc2","ask_user_question"), tool_result("tc2", 600chars)]`，配置 `micro_compact_stale_steps=1`
      - 预期: tc1（bash）被替换为 `[compacted: 600 chars]`，tc2（ask_user_question）保持原样
    - `test_whitelist_custom_list`:
      - 输入: 配置 `micro_compactable_tools = vec!["read_file".into()]`，消息包含 bash + read_file 两个工具结果
      - 预期: 仅 read_file 被清除，bash 保持原样
    - `test_whitelist_unknown_tool_preserved`:
      - 输入: `[ai_with_tool("tc1","custom_tool"), tool_result("tc1", "very long...")]`
      - 预期: 不在默认白名单中，内容不被清除

    **时间衰减:**
    - `test_stale_steps_keep_recent`:
      - 输入: 7 步消息序列，每步含一个 bash Tool 结果（600 chars），配置 `micro_compact_stale_steps=5`
      - 预期: 最近 5 步保持完整，前 2 步被清除
    - `test_stale_steps_zero_compact_all`:
      - 输入: 配置 `micro_compact_stale_steps=0`，3 步 bash 结果
      - 预期: 所有 bash 结果都被清除
    - `test_stale_steps_large_keep_all`:
      - 输入: 配置 `micro_compact_stale_steps=100`，3 步 bash 结果
      - 预期: 没有任何消息被清除

    **图片/大文档清除:**
    - `test_image_replaced_with_placeholder`:
      - 输入: `[ai_with_tool("tc1","bash"), tool_result_with_image("tc1", "text")]`，`micro_compact_stale_steps=1`
      - 预期: Image block 被替换为 `[image]` 文本 block，其他文本 block 保留
    - `test_large_image_compacted_with_token_info`:
      - 输入: `[ai_with_tool("tc1","bash"), tool_result_with_large_image("tc1")]`，`micro_compact_stale_steps=1`
      - 预期: 超大 Image block 被替换为 `[compacted: image ~25000 tokens]` 占位符
    - `test_image_in_recent_step_preserved`:
      - 输入: 含 Image 的 Tool 消息在最近步内（`micro_compact_stale_steps=5`，消息仅 2 步）
      - 预期: Image block 保持原样

    **工具对完整性保护:**
    - `test_invariant_preserves_tool_pair`:
      - 输入: `[Human, Ai{tc:["tc1","tc2"],name:"bash"}, Tool{tc1}, Tool{tc2}, Ai]`，`micro_compact_stale_steps=1`
      - 预期: tc1 和 tc2 同时被清除（同一 Ai 消息的多个 Tool 消息不被部分清除）
    - `test_invariant_preserves_ai_parent`:
      - 输入: `[Ai{tc:["tc1"],name:"bash"}, Tool{tc1, long}, Human, Ai]`
      - 预期: Ai 父消息（含 tool_call）不被修改，仅 Tool 消息内容被替换

    **边界情况:**
    - `test_empty_messages`: 空 `messages` 切片 → 返回 0
    - `test_no_tool_messages`: `[Human, Ai, Human, Ai]` → 返回 0
    - `test_error_tool_result_preserved`:
      - 输入: `[ai_with_tool("tc1","bash"), BaseMessage::tool_error("tc1", "error message")]`
      - 预期: 错误结果不被清除
    - `test_already_compacted_skipped`:
      - 输入: `[ai_with_tool("tc1","bash"), tool_result("tc1", "[compacted: 600 chars]")]`
      - 预期: 已压缩内容不被重复处理，返回 0
    - `test_orphan_tool_result_preserved`:
      - 输入: `[tool_result("orphan_id", "long text...")]`（无对应 Ai 消息）
      - 预期: `find_tool_name_for_tool_result` 返回 None，Tool 消息保持原样
    - `test_mixed_compactable_and_protected`:
      - 输入: bash + ask_user_question + bash 混合序列
      - 预期: 仅 bash 结果被清除，ask_user_question 保持原样

  - 运行命令: `cargo test -p rust-create-agent --lib -- compact::micro::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 micro.rs 编译通过
  - `cargo build -p rust-create-agent 2>&1 | tail -5`
  - 预期: 输出包含 "Compiling rust-create-agent" 且无 error

- [x] 验证 micro 模块在 compact/mod.rs 中正确注册
  - `grep -n 'pub mod micro' rust-create-agent/src/agent/compact/mod.rs && grep -n 'pub use micro' rust-create-agent/src/agent/compact/mod.rs`
  - 预期: 输出两行，分别包含 `pub mod micro` 和 `pub use micro::micro_compact_enhanced`

- [x] 验证旧 micro_compact 标记为 deprecated
  - `grep -n 'deprecated' rust-create-agent/src/agent/token.rs`
  - 预期: 输出包含 `#[deprecated` 且 note 中包含 `micro_compact_enhanced`

- [x] 验证 micro_compact_enhanced 函数签名完整
  - `grep -n 'pub fn micro_compact_enhanced' rust-create-agent/src/agent/compact/micro.rs`
  - 预期: 输出一行，包含 `config: &CompactConfig, messages: &mut [BaseMessage]`

- [x] 验证全部单元测试通过
  - `cargo test -p rust-create-agent --lib -- compact::micro::tests 2>&1 | tail -20`
  - 预期: 输出包含 "test result: ok" 且无 FAILED

- [x] 验证旧 micro_compact 测试仍可通过（deprecated 函数不报错）
  - `cargo test -p rust-create-agent --lib -- token::tests::test_micro_compact 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok"

- [x] 验证全量测试无回归
  - `cargo test -p rust-create-agent 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok"

---

### Task 4: Full Compact 策略增强

**背景:**
现有 `compact_task()`（`rust-agent-tui/src/app/agent.rs:302-396`）使用自由格式三段式摘要 prompt（`## 目标 / ## 已完成操作 / ## 关键发现`），无预处理（图片 Base64 浪费大量 token）、无后处理（`<analysis>` 块泄露到摘要中）、无 PTL 降级（压缩请求本身超长时直接报错）。本 Task 新建 `full.rs` 模块，实现增强版 Full Compact 策略：预处理（图片替换为 `[image]` + 每条消息截断）+ 结构化 9 段摘要模板 + 后处理（移除 `<analysis>` 块 + 提取 `<summary>`）+ PTL 降级重试（按 round 分组逐步删除最旧消息）。本 Task 依赖 Task 1 的 `CompactConfig`（`summary_max_tokens`、`ptl_max_retries`）和 Task 2 的 `group_messages_by_round`（PTL 降级按组删除）。本 Task 的输出 `full_compact()` 函数将被 Task 6 的 `compact_task()` 统一入口调用。

**涉及文件:**
- 新建: `rust-create-agent/src/agent/compact/full.rs`
- 修改: `rust-create-agent/src/agent/compact/mod.rs`（Task 1 创建，添加 `pub mod full;`）

**执行步骤:**

- [x] 新建 `full.rs`，定义 `FullCompactResult` 结构体和模块导入
  - 位置: `rust-create-agent/src/agent/compact/full.rs`（新文件）
  - 引入依赖:
    ```rust
    use crate::agent::compact::config::CompactConfig;
    use crate::agent::compact::invariant::{group_messages_by_round, MessageRound};
    use crate::llm::types::LlmRequest;
    use crate::llm::BaseModel;
    use crate::messages::{BaseMessage, ContentBlock, MessageContent};
    use tracing::warn;
    ```
  - 定义结果结构体:
    ```rust
    /// Full Compact 执行结果
    #[derive(Debug, Clone)]
    pub struct FullCompactResult {
        /// 生成的结构化摘要文本
        pub summary: String,
        /// 实际参与摘要的消息数量（PTL 降级后可能少于原始消息数）
        pub messages_used: usize,
    }
    ```
  - 原因: `FullCompactResult` 封装摘要文本和元数据，PTL 降级后 `messages_used` 可能小于原始消息数，供 `compact_task()` 记录日志和决策

- [x] 实现 `preprocess_messages()` 预处理函数
  - 位置: `rust-create-agent/src/agent/compact/full.rs`，在 `FullCompactResult` 定义之后
  - 函数签名: `fn preprocess_messages(messages: &[BaseMessage], truncate_chars: usize) -> Vec<String>`
  - 核心逻辑:
    ```rust
    /// 预处理消息：跳过 System、替换 Image block 为 [image]、截断每条消息
    fn preprocess_messages(messages: &[BaseMessage], truncate_chars: usize) -> Vec<String> {
        let mut lines = Vec::new();
        for msg in messages {
            match msg {
                BaseMessage::System { .. } => {
                    // 跳过系统消息，避免将之前的摘要再次嵌入
                }
                BaseMessage::Human { .. } => {
                    let content = replace_images_and_truncate(&msg.message_content(), truncate_chars);
                    lines.push(format!("[用户] {}", content));
                }
                BaseMessage::Ai { tool_calls, .. } => {
                    let text = replace_images_and_truncate(&msg.message_content(), truncate_chars);
                    let tool_names: Vec<&str> = tool_calls.iter().map(|tc| tc.name.as_str()).collect();
                    let line = if tool_names.is_empty() {
                        format!("[助手] {}", text)
                    } else {
                        format!("[助手] {}（调用了工具: {}）", text, tool_names.join(", "))
                    };
                    lines.push(line);
                }
                BaseMessage::Tool { tool_call_id, .. } => {
                    let content = replace_images_and_truncate(&msg.message_content(), truncate_chars);
                    lines.push(format!("[工具结果:{}] {}", tool_call_id, content));
                }
            }
        }
        lines
    }
    ```
  - 辅助函数 `replace_images_and_truncate()`:
    ```rust
    /// 将 content 中的 Image block 替换为 [image] 文本，然后截断到 max_chars 字符
    fn replace_images_and_truncate(content: &MessageContent, max_chars: usize) -> String {
        let blocks = content.content_blocks();
        let parts: Vec<String> = blocks.iter().map(|b| match b {
            ContentBlock::Image { .. } => "[image]".to_string(),
            _ => {
                match b {
                    ContentBlock::Text { text } => text.clone(),
                    ContentBlock::ToolUse { name, input, .. } => {
                        format!("调用 {}({})", name, input)
                    }
                    ContentBlock::Reasoning { text, .. } => text.clone(),
                    _ => format!("{:?}", b),
                }
            }
        }).collect();
        let full = parts.join("\n");
        truncate_str(&full, max_chars)
    }
    ```
  - 辅助函数 `truncate_str()`:
    ```rust
    /// 按字符数截断，超出时添加 "...(已截断)" 后缀
    fn truncate_str(s: &str, max: usize) -> String {
        if s.chars().count() > max {
            let end: String = s.chars().take(max).collect();
            format!("{}...(已截断)", end)
        } else {
            s.to_string()
        }
    }
    ```
  - 原因: 预处理消除 Base64 图片的 token 浪费（一张典型图片的 Base64 约占 5K-50K 字符），截断过长的工具输出，为 LLM 生成高质量摘要提供精炼输入

- [x] 实现 `postprocess_summary()` 后处理函数
  - 位置: `rust-create-agent/src/agent/compact/full.rs`，在 `preprocess_messages()` 之后
  - 函数签名: `fn postprocess_summary(raw: &str) -> String`
  - 辅助函数 `remove_analysis_blocks()`:
    ```rust
    /// 移除 <analysis>...</analysis> 块（使用字符串操作，不依赖 regex）
    fn remove_analysis_blocks(text: &str) -> String {
        let mut result = text.to_string();
        loop {
            let start_tag = "<analysis>";
            let end_tag = "</analysis>";
            if let Some(start) = result.find(start_tag) {
                if let Some(end) = result[start..].find(end_tag) {
                    let remove_end = start + end + end_tag.len();
                    result = format!("{}{}", &result[..start], &result[remove_end..]);
                } else {
                    // 有开始标签但无结束标签，移除从开始标签到末尾
                    result = result[..start].to_string();
                    break;
                }
            } else {
                break;
            }
        }
        result
    }
    ```
  - 辅助函数 `extract_summary_content()`:
    ```rust
    /// 提取 <summary>...</summary> 标签内的内容
    fn extract_summary_content(text: &str) -> Option<String> {
        let start_tag = "<summary>";
        let end_tag = "</summary>";
        let start = text.find(start_tag)?;
        let content_start = start + start_tag.len();
        if let Some(end) = text[content_start..].find(end_tag) {
            Some(text[content_start..content_start + end].trim().to_string())
        } else {
            Some(text[content_start..].trim().to_string())
        }
    }
    ```
  - 核心逻辑:
    ```rust
    /// 后处理 LLM 输出：
    /// 1. 移除 <analysis>...</analysis> 块
    /// 2. 提取 <summary>...</summary> 内容（如果有）
    /// 3. 添加前缀说明
    /// 4. 清理多余空白行
    fn postprocess_summary(raw: &str) -> String {
        let mut text = raw.to_string();

        // 1. 移除 <analysis>...</analysis> 块
        text = remove_analysis_blocks(&text);

        // 2. 提取 <summary>...</summary> 内容
        if let Some(summary_content) = extract_summary_content(&text) {
            text = summary_content;
        }

        // 3. 添加前缀说明
        let prefix = "此会话从之前的对话延续。以下是之前对话的摘要。";

        // 4. 清理多余空白行（连续 2 个以上换行替换为 2 个）
        text = text.trim().to_string();
        while text.contains("\n\n\n") {
            text = text.replace("\n\n\n", "\n\n");
        }

        format!("{}\n\n{}", prefix, text)
    }
    ```
  - 原因: LLM 输出的 `<analysis>` 块是思考过程，不应包含在最终摘要中。提取 `<summary>` 内容确保只保留结构化摘要。使用纯字符串操作而非 regex 避免引入额外依赖。前缀说明帮助 agent 理解这是压缩后的上下文

- [x] 实现 PTL 降级相关函数
  - 位置: `rust-create-agent/src/agent/compact/full.rs`，在 `postprocess_summary()` 之后
  - 辅助函数 `is_ptl_error()`:
    ```rust
    /// 判断错误是否为 PTL（Prompt Too Long）错误
    fn is_ptl_error(error: &anyhow::Error) -> bool {
        let msg = error.to_string().to_lowercase();
        msg.contains("prompt_too_long")
            || msg.contains("context_length_exceeded")
            || msg.contains("max_context_window")
            || msg.contains("token limit")
            || msg.contains("too many tokens")
    }
    ```
  - 截断函数 `truncate_for_ptl()`:
    ```rust
    /// PTL 降级：从最旧的 round 开始删除指定数量的消息组
    /// 保留至少一个完整的消息组
    fn truncate_for_ptl(
        messages: &[BaseMessage],
        rounds: &[MessageRound],
        drop_count: usize,
    ) -> Vec<BaseMessage> {
        if rounds.len() <= 1 {
            // 至少保留 1 个 round，无法继续删除
            return messages.to_vec();
        }

        let actual_drop = drop_count.min(rounds.len() - 1); // 保留至少 1 个 round
        let drop_end = rounds[actual_drop - 1].end; // 删除到第 actual_drop 个 round 的 end

        messages[drop_end..].to_vec()
    }
    ```
  - 原因: PTL 降级按 round 粒度删除消息，保持 API 级别的消息完整性（不会拆开工具对）。每次降级删除至少 1 个 round，保留至少 1 个 round。`is_ptl_error()` 覆盖主流 LLM provider 的 PTL 错误信息模式

- [x] 实现 `full_compact()` 核心异步函数
  - 位置: `rust-create-agent/src/agent/compact/full.rs`，在 `truncate_for_ptl()` 之后
  - 函数签名:
    ```rust
    /// 执行 Full Compact：预处理 -> LLM 摘要 -> 后处理，支持 PTL 降级重试
    pub async fn full_compact(
        messages: &[BaseMessage],
        model: &dyn BaseModel,
        config: &CompactConfig,
        instructions: &str,
    ) -> anyhow::Result<FullCompactResult>
    ```
  - 常量定义（放在文件顶层，`use` 块之后、`FullCompactResult` 之前）:
    ```rust
    /// 结构化摘要 system prompt
    const SYSTEM_PROMPT: &str =
        "你是一个对话上下文压缩工具，擅长将长对话压缩为结构化摘要。";

    /// 结构化摘要 user prompt 模板
    const USER_PROMPT_TEMPLATE: &str = r#"请分析以下对话历史，按以下 9 个方面进行详细分析：

<analysis>
1. **Primary Request and Intent** — 用户的核心请求和意图
2. **Key Technical Concepts** — 涉及的关键技术概念和框架
3. **Files and Code Sections** — 操作过的文件路径和关键代码片段
4. **Errors and Fixes** — 遇到的错误及其修复方法
5. **Problem Solving** — 问题解决的思路和过程
6. **All User Messages** — 所有用户消息的摘要
7. **Pending Tasks** — 尚未完成的任务
8. **Current Work** — 当前正在进行的工作
9. **Optional Next Step** — 建议的下一步行动
</analysis>

<summary>
基于以上分析，生成精炼的结构化摘要。保留所有文件路径、错误信息和关键决策。使用 Markdown 格式。
</summary>"#;
    ```
  - 核心逻辑:
    ```rust
    pub async fn full_compact(
        messages: &[BaseMessage],
        model: &dyn BaseModel,
        config: &CompactConfig,
        instructions: &str,
    ) -> anyhow::Result<FullCompactResult> {
        // 过滤掉 System 消息用于判断有效消息数
        let non_system_count = messages.iter()
            .filter(|m| !matches!(m, BaseMessage::System { .. }))
            .count();

        if non_system_count == 0 {
            return Ok(FullCompactResult {
                summary: postprocess_summary("## 摘要\n（无有效对话历史）"),
                messages_used: messages.len(),
            });
        }

        // 预处理：每条消息截断到 2000 字符
        let truncated = preprocess_messages(messages, 2000);
        let conversation_text = truncated.join("\n");

        // 构造 user prompt
        let mut user_content = format!(
            "以下是需要压缩的对话历史：\n<conversation>\n{}\n</conversation>\n\n{}",
            conversation_text, USER_PROMPT_TEMPLATE
        );

        if !instructions.trim().is_empty() {
            user_content.push_str(&format!("\n\n压缩时请特别注意：{}", instructions.trim()));
        }

        // 初始 LLM 请求
        let request = LlmRequest::new(vec![BaseMessage::human(user_content)])
            .with_system(SYSTEM_PROMPT.to_string())
            .with_max_tokens(config.summary_max_tokens);

        // 尝试调用 LLM，支持 PTL 降级重试
        let mut current_request = request;
        let mut current_messages: Vec<BaseMessage> = messages.to_vec();
        let max_retries = config.ptl_max_retries as usize;

        for attempt in 0..=max_retries {
            match model.invoke(current_request.clone()).await {
                Ok(response) => {
                    let raw_summary = response.message.content();
                    let summary = postprocess_summary(&raw_summary);
                    return Ok(FullCompactResult {
                        summary,
                        messages_used: current_messages.len(),
                    });
                }
                Err(e) if is_ptl_error(&e) && attempt < max_retries => {
                    warn!(
                        attempt = attempt + 1,
                        max_retries,
                        "Full Compact PTL 降级：prompt 过长，删除最旧消息组后重试"
                    );

                    // 按 round 分组，删除最旧的 1 个 round
                    let rounds = group_messages_by_round(&current_messages);
                    let truncated_messages = truncate_for_ptl(&current_messages, &rounds, 1);
                    current_messages = truncated_messages;

                    // 重新预处理并构造请求
                    let truncated_text = preprocess_messages(&current_messages, 2000).join("\n");
                    let mut new_user_content = format!(
                        "以下是需要压缩的对话历史：\n<conversation>\n{}\n</conversation>\n\n{}",
                        truncated_text, USER_PROMPT_TEMPLATE
                    );
                    if !instructions.trim().is_empty() {
                        new_user_content.push_str(&format!(
                            "\n\n压缩时请特别注意：{}", instructions.trim()
                        ));
                    }
                    current_request = LlmRequest::new(vec![BaseMessage::human(new_user_content)])
                        .with_system(SYSTEM_PROMPT.to_string())
                        .with_max_tokens(config.summary_max_tokens);
                }
                Err(e) => {
                    return Err(e.context(format!(
                        "Full Compact 失败（PTL 降级重试 {} 次后仍失败）",
                        attempt
                    )));
                }
            }
        }

        Err(anyhow::anyhow!("Full Compact 失败：超出最大重试次数"))
    }
    ```
  - **关键设计决策**:
    - 预处理截断阈值 2000 字符/消息，平衡信息保留与 token 预算（经验值：20 条消息 x 2000 字符 约 40K 字符 约 10K tokens）
    - PTL 降级每次删除 1 个 round（而非按 token 估算精确删除），实现简单且可靠
    - 使用 `anyhow::Error::context()` 添加 PTL 降级上下文信息，便于排查
    - `for attempt in 0..=max_retries` 循环包含初始尝试 + max_retries 次重试
  - 原因: 替换 `compact_task()` 中的简单三段式摘要，引入结构化 9 段模板对齐 Claude Code 的摘要质量，PTL 降级确保超长对话也能成功压缩

- [x] 修改 `compact/mod.rs`，注册 `full` 子模块并导出公共 API
  - 位置: `rust-create-agent/src/agent/compact/mod.rs`（Task 1 创建）
  - 在 `pub mod micro;` 行之后添加:
    ```rust
    pub mod full;
    ```
  - 在 `pub use micro::micro_compact_enhanced;` 行之后添加:
    ```rust
    pub use full::{full_compact, FullCompactResult};
    ```
  - 原因: 将 `full_compact` 和 `FullCompactResult` 暴露为 compact 模块的公共 API，供 Task 6 的 `compact_task()` 统一入口调用，以及未来 `compact_task()` 重写时引用

- [x] 为 `full` 模块编写单元测试
  - 测试文件: `rust-create-agent/src/agent/compact/full.rs`（内联 `#[cfg(test)] mod tests`）
  - 引入测试依赖:
    ```rust
    use super::*;
    use crate::llm::types::{LlmRequest, LlmResponse, StopReason};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    ```
  - Mock BaseModel 实现:
    ```rust
    /// Mock BaseModel：返回预设的固定文本，可选前 N 次返回 PTL 错误
    struct MockBaseModel {
        response: String,
        fail_with_ptl: usize,
        call_count: AtomicUsize,
    }

    impl MockBaseModel {
        fn new(response: &str) -> Self {
            Self {
                response: response.to_string(),
                fail_with_ptl: 0,
                call_count: AtomicUsize::new(0),
            }
        }
        fn new_with_ptl_fail(response: &str, ptl_fails: usize) -> Self {
            Self {
                response: response.to_string(),
                fail_with_ptl: ptl_fails,
                call_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl BaseModel for MockBaseModel {
        async fn invoke(&self, _request: LlmRequest) -> anyhow::Result<LlmResponse> {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst);
            if count < self.fail_with_ptl {
                return Err(anyhow::anyhow!(
                    "Error: prompt_too_long: input tokens exceed context window"
                ));
            }
            Ok(LlmResponse {
                message: BaseMessage::ai(&self.response),
                stop_reason: StopReason::EndTurn,
                usage: None,
            })
        }
        fn provider_name(&self) -> &str { "mock" }
        fn model_id(&self) -> &str { "mock-model" }
    }
    ```
  - 测试场景:

    **preprocess_messages:**
    - `test_preprocess_skips_system`:
      - 输入: `[System("old summary"), Human("hello"), Ai("hi")]`
      - 预期: 输出仅 2 行，不包含 System 消息
    - `test_preprocess_truncates_long_text`:
      - 输入: `[Human(3000字符的长文本)]`，truncate_chars=2000
      - 预期: 输出包含 "...(已截断)" 后缀
    - `test_preprocess_replaces_image`:
      - 输入: `[Human(MessageContent::blocks([ContentBlock::text("see"), ContentBlock::image_base64("image/png", "data...")]))]`
      - 预期: 输出中 Image block 被替换为 `[image]`，Text block 保留
    - `test_preprocess_formats_tool_calls`:
      - 输入: `[Ai{tool_calls:[tc1(name:"bash"), tc2(name:"read_file")]}]`
      - 预期: 输出包含 `（调用了工具: bash, read_file）`
    - `test_preprocess_formats_tool_result`:
      - 输入: `[Tool{tool_call_id:"tc1", content:"output text"}]`
      - 预期: 输出格式为 `[工具结果:tc1] output text`
    - `test_preprocess_empty_messages`:
      - 输入: `[]`
      - 预期: 返回空 Vec

    **postprocess_summary:**
    - `test_postprocess_removes_analysis`:
      - 输入: `"<analysis>detailed analysis here</analysis>\n\n## 摘要\ncontent"`
      - 预期: 输出不包含 `<analysis>` 和 `</analysis>`，包含前缀说明
    - `test_postprocess_extracts_summary_tag`:
      - 输入: `"<analysis>思考</analysis>\n<summary>\n## 核心摘要\n实际内容\n</summary>"`
      - 预期: 输出仅包含前缀 + `## 核心摘要\n实际内容`
    - `test_postprocess_no_tags`:
      - 输入: `"## 摘要\n这是直接输出的摘要文本"`
      - 预期: 输出包含前缀 + 原始文本（无标签时保持原样）
    - `test_postprocess_cleans_blank_lines`:
      - 输入: `"## 摘要\n\n\n\n内容\n\n\n\n结尾"`
      - 预期: 连续 3+ 个换行替换为 2 个
    - `test_postprocess_multiple_analysis_blocks`:
      - 输入: `"<analysis>块1</analysis>中间文本<analysis>块2</analysis>剩余"`
      - 预期: 两个 `<analysis>` 块都被移除，保留 "中间文本" 和 "剩余"

    **truncate_for_ptl:**
    - `test_ptl_truncate_single_round`:
      - 输入: 1 个 round（`[Human, Ai]`），drop_count=1
      - 预期: 无法删除（保留至少 1 个 round），返回原始消息
    - `test_ptl_truncate_drops_oldest`:
      - 输入: 4 个 round（R0: Human+Ai, R1: Ai{tc}+Tool, R2: Human, R3: Ai），drop_count=1
      - 预期: 删除 R0，返回 R1+R2+R3 的消息
    - `test_ptl_truncate_drops_multiple`:
      - 输入: 5 个 round，drop_count=3
      - 预期: 删除 R0+R1+R2，返回 R3+R4 的消息
    - `test_ptl_truncate_preserves_at_least_one`:
      - 输入: 3 个 round，drop_count=5
      - 预期: actual_drop 限制为 2（rounds.len()-1），保留最后 1 个 round

    **is_ptl_error:**
    - `test_is_ptl_error_variants`:
      - 输入: 分别包含 "prompt_too_long"、"context_length_exceeded"、"max_context_window"、"token limit"、"too many tokens" 的错误
      - 预期: 全部返回 true
    - `test_is_not_ptl_error`:
      - 输入: 错误信息 "connection timeout"
      - 预期: 返回 false

    **full_compact（集成）:**
    - `test_full_compact_basic`:
      - 输入: `[Human("帮我写个函数"), Ai{tc:["tc1","bash"]}, Tool("tc1","编译成功")]`，MockBaseModel 返回结构化摘要
      - 预期: 返回 `FullCompactResult`，summary 包含前缀说明，messages_used==3
    - `test_full_compact_empty_messages`:
      - 输入: `[]`
      - 预期: 返回 "无有效对话历史" 的摘要，messages_used==0
    - `test_full_compact_system_only`:
      - 输入: `[System("old summary")]`
      - 预期: 返回 "无有效对话历史" 的摘要，messages_used==1
    - `test_full_compact_with_instructions`:
      - 输入: instructions = "请特别关注文件路径信息"，构造 MockBaseModel 在 invoke 中捕获 request 的 messages 内容（通过 call_count 和静态变量或额外字段验证 user_content 包含 instructions）
    - `test_full_compact_ptl_retry_succeeds`:
      - 输入: 10 条消息（5 轮 Human+Ai 交替），MockBaseModel{fail_with_ptl: 2, response: "摘要"}，config.ptl_max_retries=3
      - 预期: 前 2 次返回 PTL 错误，第 3 次成功，返回摘要，messages_used 少于原始消息数（经过 2 次 PTL 降级删除了 2 个 round）
    - `test_full_compact_ptl_retry_exhausted`:
      - 输入: MockBaseModel{fail_with_ptl: 5, response: "摘要"}，config.ptl_max_retries=3
      - 预期: 返回 Err，错误信息包含 "PTL 降级重试" 和 "3 次"
    - `test_full_compact_non_ptl_error`:
      - 输入: 构造 MockBaseModel 在 invoke 中返回 "connection refused" 错误（非 PTL 关键字）
      - 预期: 返回 Err，不触发 PTL 降级重试（call_count==1）

  - 运行命令: `cargo test -p rust-create-agent --lib -- compact::full::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 full.rs 编译通过
  - `cargo build -p rust-create-agent 2>&1 | tail -5`
  - 预期: 输出包含 "Compiling rust-create-agent" 且无 error

- [x] 验证 full 模块在 compact/mod.rs 中正确注册
  - `grep -n 'pub mod full' rust-create-agent/src/agent/compact/mod.rs && grep -n 'pub use full' rust-create-agent/src/agent/compact/mod.rs`
  - 预期: 输出两行，分别包含 `pub mod full` 和 `pub use full::{full_compact, FullCompactResult}`

- [x] 验证 full_compact 函数签名完整
  - `grep -n 'pub async fn full_compact' rust-create-agent/src/agent/compact/full.rs`
  - 预期: 输出一行，包含 `model: &dyn BaseModel, config: &CompactConfig, instructions: &str`

- [x] 验证 9 段摘要模板存在
  - `grep -c 'Primary Request and Intent' rust-create-agent/src/agent/compact/full.rs`
  - 预期: 计数 >= 1

- [x] 验证 PTL 降级逻辑存在
  - `grep -c 'is_ptl_error\|truncate_for_ptl' rust-create-agent/src/agent/compact/full.rs`
  - 预期: 计数 >= 4（函数定义 + 调用各至少出现 2 次）

- [x] 验证后处理函数存在
  - `grep -c 'postprocess_summary\|remove_analysis_blocks\|extract_summary_content' rust-create-agent/src/agent/compact/full.rs`
  - 预期: 计数 >= 6（三个函数各定义 + 调用）

- [x] 验证全部单元测试通过
  - `cargo test -p rust-create-agent --lib -- compact::full::tests 2>&1 | tail -20`
  - 预期: 输出包含 "test result: ok" 且无 FAILED

- [x] 验证全量测试无回归
  - `cargo test -p rust-create-agent 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok"

---


### Task 5: 核心层 Compact 模块本地验收

**前置条件:**
- Task 1-4 全部完成，`rust-create-agent/src/agent/compact/` 目录包含 config.rs / invariant.rs / micro.rs / full.rs / mod.rs
- 构建环境可用

**端到端验证:**

1. 运行 rust-create-agent 全量测试确保无回归
   - `cargo test -p rust-create-agent 2>&1 | tail -10`
   - 预期: 全部测试通过，输出包含 "test result: ok"
   - ✅ 通过

2. 验证 compact 模块导出完整
   - `grep -c 'pub mod\|pub use' rust-create-agent/src/agent/compact/mod.rs`
   - 预期: 至少 5 条导出（config / invariant / micro / full / CompactConfig）
   - ✅ 8 条导出

3. 验证 CompactConfig 默认值完整
   - `cargo test -p rust-create-agent --lib -- compact::config::tests::test_default 2>&1 | tail -5`
   - 预期: 测试通过
   - ✅ 通过

4. 验证 micro_compact_enhanced 编译通过
   - `cargo test -p rust-create-agent --lib -- compact::micro::tests 2>&1 | tail -5`
   - 预期: 所有 micro 测试通过
   - ✅ 17 passed

5. 验证 full_compact 编译通过
   - `cargo test -p rust-create-agent --lib -- compact::full::tests 2>&1 | tail -5`
   - 预期: 所有 full 测试通过
   - ✅ 24 passed

6. 验证全 workspace 编译无错误
   - `cargo build 2>&1 | tail -5`
   - 预期: 输出包含 "Finished"，无编译错误
   - ✅ Finished
