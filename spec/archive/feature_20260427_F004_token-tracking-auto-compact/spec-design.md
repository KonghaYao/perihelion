# F004: Token 用量追踪与 Auto-Compact 机制

## TL;DR

在 `rust-create-agent` 核心层新增 `TokenTracker` 累积追踪 token 用量 + `ContextBudget` 计算上下文窗口阈值，在 TUI 层于 agent 完成（`Done` 事件）后检查是否触发 auto-compact。分两个阶段交付：P0 先做 token 追踪与状态栏展示，P1 再做 auto-compact 自动触发。

---

## 一、现状分析

### 1.1 已有的基础设施

| 组件 | 位置 | 现状 |
|------|------|------|
| `TokenUsage` | `rust-create-agent/src/llm/types.rs` | 已有结构体（input/output/cache_create/cache_read），仅用于 Langfuse 追踪 |
| `LlmCallEnd` 事件 | `rust-create-agent/src/agent/events.rs` | 每轮 LLM 调用后发出，携带 `usage: Option<TokenUsage>` |
| `/compact` 命令 | `rust-agent-tui/src/command/compact.rs` | 手动触发压缩，调用 LLM 生成摘要 |
| `compact_task()` | `rust-agent-tui/src/app/agent.rs:286` | 独立异步任务，格式化消息 → LLM 摘要 → 替换 thread |
| `AgentState` | `rust-create-agent/src/agent/state.rs` | 仅追踪 `messages: Vec<BaseMessage>` 和 `current_step`，无 token 感知 |

### 1.2 缺失的关键能力

1. **没有累积 token 追踪**：每轮 `TokenUsage` 随事件发出后即丢弃，无会话级累计
2. **没有上下文窗口感知**：不知道当前模型的 context window 大小，无法判断"还剩多少空间"
3. **没有 auto-compact 触发**：仅在手动 `/compact` 时压缩
4. **没有 micro-compact**：无法轻量清除旧工具结果
5. **没有 token 用量展示**：TUI 状态栏不显示 token 信息

---

## 二、架构设计

### 2.1 模块职责划分

```
rust-create-agent（核心框架）
├── llm/types.rs          TokenUsage（已有，扩展）
├── agent/state.rs        AgentState（已有，扩展）
├── agent/token.rs        [新增] TokenTracker + ContextBudget
├── agent/events.rs       AgentEvent（已有，新增事件变体）
│
rust-agent-tui（TUI 应用层）
├── app/agent.rs          compact_task（已有，重构）
├── app/agent_ops.rs      事件处理（已有，扩展 auto-compact 分支）
├── app/token_tracker.rs  [新增] 会话级 token 聚合器
├── app/auto_compact.rs   [新增] auto-compact 触发器
├── ui/status_bar.rs      [新增/扩展] token 用量展示
```

### 2.2 数据流

```
LLM 调用结束
  └─ emit(AgentEvent::LlmCallEnd { usage, model, .. })
      │
      ├─ [核心层] state.token_tracker.accumulate(usage)
      │    └─ 累加 input/output/cache tokens
      │    └─ 记录最新 usage（用于估算当前上下文大小）
      │
      ├─ [应用层] App::handle_llm_call_end(event)
      │    └─ 更新状态栏: "ctx: 72% | 45K/200K tokens"
      │    └─ 若 context_pct >= 85%: 标记 needs_auto_compact = true
      │
      └─ [应用层] AgentEvent::Done 到达后
           └─ 若 needs_auto_compact == true
                └─ micro_compact (70%-85%: 清除旧工具结果)
                └─ full compact  (>= 85%: LLM 摘要 + 新 thread)
```

### 2.3 核心层新增类型

#### `TokenTracker`（rust-create-agent/src/agent/token.rs）

```rust
/// 会话级 token 用量追踪器
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenTracker {
    /// 累计输入 token（含 cache_read + cache_creation）
    pub total_input_tokens: u64,
    /// 累计输出 token
    pub total_output_tokens: u64,
    /// 累计 cache_creation token
    pub total_cache_creation_tokens: u64,
    /// 累计 cache_read token
    pub total_cache_read_tokens: u64,
    /// 最近一次 LLM 响应的 usage（用于估算当前上下文大小）
    pub last_usage: Option<TokenUsage>,
    /// 已完成的 LLM 调用次数
    pub llm_call_count: u32,
}

impl TokenTracker {
    /// 累加一次 LLM 调用的 token 用量
    pub fn accumulate(&mut self, usage: &TokenUsage) { ... }

    /// 估算当前上下文窗口占用（基于最近一次 API 响应）
    /// 计算: last_usage.input_tokens + last_usage.output_tokens
    ///       + last_usage.cache_creation + last_usage.cache_read
    pub fn estimated_context_tokens(&self) -> Option<u64> { ... }

    /// 计算上下文窗口使用百分比
    pub fn context_usage_percent(&self, context_window: u32) -> Option<f64> { ... }
}
```

#### `ContextBudget`（rust-create-agent/src/agent/token.rs）

```rust
/// 上下文窗口预算配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBudget {
    /// 模型的上下文窗口大小（token 数）
    pub context_window: u32,
    /// auto-compact 触发阈值（百分比，0.0-1.0）
    pub auto_compact_threshold: f64,
    /// 警告阈值（百分比，0.0-1.0）
    pub warning_threshold: f64,
}

impl ContextBudget {
    pub const DEFAULT_CONTEXT_WINDOW: u32 = 200_000;
    pub const DEFAULT_AUTO_COMPACT_THRESHOLD: f64 = 0.85;  // 85% 触发
    pub const DEFAULT_WARNING_THRESHOLD: f64 = 0.70;       // 70% 警告

    pub fn new(context_window: u32) -> Self { ... }

    /// 检查是否应触发 auto-compact
    pub fn should_auto_compact(&self, tracker: &TokenTracker) -> bool {
        match tracker.context_usage_percent(self.context_window) {
            Some(pct) => pct >= self.auto_compact_threshold,
            None => false,
        }
    }

    /// 检查是否应显示上下文警告
    pub fn should_warn(&self, tracker: &TokenTracker) -> bool { ... }
}
```

#### `AgentEvent` 扩展

```rust
// 在 AgentEvent 枚举中新增：
pub enum AgentEvent {
    // ... 已有变体 ...

    /// 上下文窗口使用警告（阈值触发时发出）
    ContextWarning {
        used_tokens: u64,
        total_tokens: u64,
        percentage: f64,
    },
}
```

### 2.4 AgentState 集成

```rust
// 在 AgentState 中新增字段：
pub struct AgentState {
    // ... 已有字段 ...
    /// Token 用量追踪器
    pub token_tracker: TokenTracker,
}
```

在 `ReActAgent::execute` 中，每次收到 `LlmCallEnd` 事件后，自动调用 `state.token_tracker.accumulate(usage)`。

### 2.5 应用层：Auto-Compact 触发

#### 触发时机

**关键约束**：auto-compact 必须在 agent 完全停止后才执行，不能在 agent 仍处于推理循环中时替换消息列表。

采用两阶段策略：

1. **标记阶段**（`LlmCallEnd` 事件处理时）：只检查阈值、更新状态栏、设置 `needs_auto_compact` 标记
2. **执行阶段**（`AgentEvent::Done` 事件处理时）：agent 已完全停止，安全执行 compact

```rust
// 伪代码 — agent_ops.rs 事件处理

// 阶段一：标记
AgentEvent::LlmCallEnd { usage, .. } => {
    if let Some(ref usage) = usage {
        // 更新状态栏（每轮都刷新）
        app.update_context_display(usage);
        // 检查阈值，只标记不执行
        let budget = ContextBudget::new(model_context_window);
        if budget.should_auto_compact(&state.token_tracker) {
            app.needs_auto_compact = true;
        }
    }
}

// 阶段二：执行（agent 已停止，安全替换消息）
AgentEvent::Done => {
    app.set_loading(false);
    app.agent.agent_rx = None;
    if app.needs_auto_compact {
        app.needs_auto_compact = false;
        app.start_compact("auto".to_string());
    }
}
```

#### Micro-Compact（轻量级）

在触发完整 LLM compact 之前，先尝试 micro-compact：

```rust
/// 轻量级压缩：清除旧工具结果中的大段内容
fn micro_compact(messages: &mut Vec<BaseMessage>, keep_recent: usize) {
    let total = messages.len();
    let cutoff = total.saturating_sub(keep_recent);

    for msg in messages.iter_mut().take(cutoff) {
        if let BaseMessage::Tool { content, .. } = msg {
            let text = content.text_content();
            if text.len() > 500 {
                // 替换为简短摘要
                *content = MessageContent::text("[旧工具结果已清除]");
            }
        }
    }
}
```

Micro-compact 特点：
- **不需要 LLM 调用**，纯本地操作
- 保留最近 N 条消息的工具结果完整内容
- 仅清除旧的长工具结果（>500 字符）
- 触发条件：上下文达到 70% 时作为第一道防线

#### 完整 Auto-Compact 流程

```
LlmCallEnd 事件
  └─ 更新 TokenTracker
  └─ 计算 context_usage_percent
  └─ 70% ~ 85%: micro_compact（清除旧工具结果）
  └─ >= 85%:    full compact（调用 LLM 生成摘要，创建新 thread）
```

### 2.6 Context Window 大小获取

通过 `ReactLLM` trait 扩展或 `BaseModel` 获取：

```rust
// 方案 A：在 ReactLLM trait 中新增方法
#[async_trait::async_trait]
pub trait ReactLLM: Send + Sync {
    async fn generate_reasoning(...) -> AgentResult<Reasoning>;
    fn model_name(&self) -> String { "unknown".into() }

    /// 新增：返回模型的上下文窗口大小
    fn context_window(&self) -> u32 {
        ContextBudget::DEFAULT_CONTEXT_WINDOW
    }
}
```

模型 → context_window 映射表（在 `BaseModelReactLLM` 中实现）：

| 模型 | Context Window |
|------|---------------|
| claude-sonnet-* | 200K |
| claude-opus-* | 200K |
| claude-haiku-* | 200K |
| deepseek-* | 128K |
| gpt-4o | 128K |
| 默认 | 200K |

### 2.7 状态栏展示

在 TUI 状态栏中新增 token 信息显示：

```
┌─────────────────────────────────────────────────────────┐
│ sonnet-4 | ctx: 72% (144K/200K) | steps: 5 | msgs: 23 │
└─────────────────────────────────────────────────────────┘
```

- 正常：< 70%：绿色
- 警告：70%-85%：黄色
- 危险：> 85%：红色
- auto-compact 后：重置显示

---

## 三、分阶段落地步骤

### P0: Token 追踪与展示（基础能力）

**目标**：能在 TUI 中看到 token 用量，为 auto-compact 打基础。

| 步骤 | 文件 | 变更内容 |
|------|------|---------|
| 1 | `rust-create-agent/src/agent/token.rs` | 新增 `TokenTracker` + `ContextBudget` |
| 2 | `rust-create-agent/src/agent/state.rs` | `AgentState` 新增 `token_tracker` 字段 |
| 3 | `rust-create-agent/src/agent/executor.rs` | `execute()` 中每轮 LLM 调用后自动 accumulate |
| 4 | `rust-create-agent/src/agent/events.rs` | 新增 `ContextWarning` 事件变体 |
| 5 | `rust-create-agent/src/lib.rs` | 导出 `token` 模块 |
| 6 | `rust-agent-tui/src/app/agent_ops.rs` | 处理 `LlmCallEnd` 时更新 token 展示 |
| 7 | TUI 状态栏 | 显示 context 使用百分比 |
| 8 | 单元测试 | `TokenTracker` 和 `ContextBudget` 测试 |

### P1: Auto-Compact（自动化）

**目标**：上下文接近满时自动压缩，无需用户手动干预。

| 步骤 | 文件 | 变更内容 |
|------|------|---------|
| 1 | `rust-create-agent/src/agent/token.rs` | micro-compact 实现（纯函数，操作 `&mut Vec<BaseMessage>`） |
| 2 | `rust-agent-tui/src/app/auto_compact.rs` | auto-compact 触发逻辑（阈值检查 + 流程编排） |
| 3 | `rust-agent-tui/src/app/agent_ops.rs` | 集成 auto-compact 检查到事件循环 |
| 4 | `rust-agent-tui/src/app/agent.rs` | 重构 `compact_task()`，支持 auto 模式 |
| 5 | `BaseModelReactLLM` | 实现 `context_window()` 方法 |
| 6 | 集成测试 | 验证 auto-compact 触发时机和结果 |

---

## 四、关键设计决策

### 4.1 为什么 auto-compact 放在 TUI 层而非核心层？

**理由**：
- `ReActAgent::execute()` 是一个同步循环（虽然有 async），在其中途插入"暂停当前执行、调用另一个 LLM、替换消息列表"会显著复杂化控制流
- TUI 层天然拥有"事件循环"模式——在 `LlmCallEnd` 和下一轮用户输入之间检查阈值是最佳时机
- CC 的做法也是在 query loop（应用层）触发，而非在底层 LLM 调用中
- 核心层只负责**追踪**和**通知**（发出 `ContextWarning` 事件），TUI 层负责**决策**和**执行**

### 4.2 为什么使用 API 返回的 usage 而不是自行估算？

**理由**：
- API 返回的 `input_tokens` 精确反映了发送给模型的完整 prompt 大小（含 system、cache、工具定义等）
- 自行估算需要复制模型的 tokenizer 逻辑，容易产生偏差
- CC 同样优先使用 API usage，仅在不可用时 fallback 到估算

### 4.3 为什么不把 TokenTracker 放在 AgentState 之外？

**考虑**：`TokenTracker` 确实可以独立于 `AgentState` 存在（比如放在 TUI App 中）。

**最终选择放在 AgentState 中**的理由：
- SubAgent 继承 `AgentState`，子 agent 的 token 追踪自然聚合到父级
- 持久化（ThreadStore）时 token 信息随 thread 一起保存，恢复历史对话时能还原 token 状态
- 核心层的 `execute()` 可以直接更新，不需要额外的事件回调

### 4.4 Micro-Compact 的定位

Micro-compact 是 full compact 的轻量级前置防线。它的设计原则：
- **零 API 调用**：纯字符串操作，不消耗 token
- **可逆**：不删除消息，只替换内容（用户如果需要可以通过 thread 历史恢复）
- **保守**：仅清除工具结果中的长文本，不触碰 Human/Ai 消息
- **时机**：在 70%-85% 区间触发，为 full compact 争取时间

---

## 五、风险点与缓解

| 风险 | 缓解措施 |
|------|---------|
| API 返回的 usage 不准确（如第三方模型不支持） | `TokenUsage` 全字段为 `Option`，不支持时 fallback 到消息字符数粗估 |
| Auto-compact 在 agent 仍在运行时触发（竞态） | **两阶段策略**：`LlmCallEnd` 只标记，`Done` 事件后才执行 compact |
| Compact 本身失败（API 错误） | 保留 circuit breaker（连续 3 次失败后停止自动触发） |
| Micro-compact 清除的内容对后续推理有用 | 仅清除 cutoff 之前的长工具结果，保留最近 N 条完整 |
| 不同模型 context window 大小不同 | 通过 `ReactLLM::context_window()` 动态获取，按模型映射表 |
| `TokenTracker` 职责膨胀导致 `AgentState` 变胖 | 严格限制 TokenTracker 只做"累积计数 + 上下文估算"，成本计算/按模型统计留在 TUI 层 |

---

## 六、与 Claude Code 的差异

| 方面 | Claude Code | Perihelion（本方案） |
|------|------------|---------------------|
| Token 追踪位置 | 全局 state（TS 单例） | `AgentState.token_tracker`（Rust struct） |
| Auto-compact 触发 | query loop 中检查 | TUI 事件循环中检查 |
| Compact 实现 | 三层（micro → session memory → full API） | 两层（micro → full API），暂不做 session memory |
| 上下文估算 | `tokenCountWithEstimation()` 复杂估算 | 优先用 API `usage`，无 usage 时 fallback 粗估 |
| 状态栏 | React 组件，高度可定制 | ratatui 状态栏，固定格式 |
| Token 预算（+500k） | 支持 ANT token budget 特性 | P0 不做，作为未来扩展点 |
| 成本计算 | 按 model cost tier 精确计算美元 | P0 不做，未来可扩展 |
