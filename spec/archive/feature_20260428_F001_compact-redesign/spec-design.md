# Feature: 20260428_F001 - compact-redesign

## 需求背景

当前 compact 系统存在以下不足：

1. **Micro-compact 过于粗暴** — 仅按字符数阈值（500 字符）清除旧工具结果，不区分工具类型，可能清除重要信息而保留无用内容
2. **Full Compact 摘要质量低** — 使用自由格式摘要 prompt，缺乏结构化指导，关键信息（文件路径、错误修复、待处理任务）容易丢失
3. **压缩后信息断裂** — 没有重新注入机制，agent 压缩后可能丢失当前工作上下文（最近读取的文件、激活的 skills 等）
4. **工具对完整性无保护** — 压缩可能拆开 tool_use 和 tool_result 消息对，破坏 API 级别约束
5. **缺乏容错** — 没有处理"压缩请求本身超出上下文窗口"的 PTL（Prompt Too Long）降级场景
6. **配置不灵活** — 阈值硬编码在代码中，无法根据模型/场景调整

本设计参照 Claude Code 项目的 compact 实现，全面对齐其核心能力（不含 Session Memory 层），采用分层渐进式重构策略。

## 目标

- Micro-compact 引入可压缩工具白名单 + 时间衰减清除策略 + 图片清除 + 工具对完整性保护
- Full Compact 采用 9 段结构化摘要模板，对齐 Claude Code 的摘要质量
- 实现压缩后重新注入（最近文件 + 激活 Skills），保证 agent 压缩后能无缝继续工作
- 实现 PTL 降级重试，处理压缩请求本身超长的情况
- 配置可调（`settings.json` + 环境变量），支持按模型/场景灵活调整

## 方案设计

### 整体架构

保持现有的两层压缩架构（Micro + Full），逐层增强。整体模块划分如下：

```
rust-create-agent（核心层）
├── agent/
│   ├── compact/
│   │   ├── mod.rs           — CompactManager 统一入口
│   │   ├── micro.rs         — MicroCompact 策略
│   │   ├── full.rs          — FullCompact 策略（摘要生成 + PTL 降级）
│   │   ├── re_inject.rs     — PostCompactReInjector 重新注入
│   │   ├── invariant.rs     — 工具对完整性保护
│   │   └── config.rs        — CompactConfig 配置结构
│   ├── token.rs             — ContextBudget / TokenTracker（现有，扩展）
│   └── state.rs             — AgentState（现有，扩展 compact 方法）

rust-agent-middlewares（中间件层）
└── 无新增模块（compact 不需要中间件参与）

rust-agent-tui（应用层）
├── app/
│   ├── agent.rs             — compact_task()（现有，重写）
│   └── agent_ops.rs         — auto-compact 触发逻辑（现有，扩展）
├── command/
│   └── compact.rs           — /compact 命令（现有，扩展参数）
└── config/
    └── types.rs             — CompactConfig 序列化（现有，扩展字段）
```

**分层决策**：

- `MicroCompact`、工具对保护、`CompactConfig` 放在 `rust-create-agent` — 纯消息操作，不依赖 TUI 或具体中间件
- `FullCompact`（LLM 摘要调用）也放在 `rust-create-agent` — 通过 `BaseModel` trait 调用，保持框架独立性
- `PostCompactReInjector` 放在 `rust-create-agent` — 基于消息历史和目录扫描，不需要 TUI 依赖
- TUI 层仅负责触发时机控制和 UI 展示

### Micro-compact 层增强

**现有实现**：`micro_compact(messages, keep_recent)` — 遍历旧消息，清除 >500 字符的工具结果内容。

**增强策略**：

#### 1. 可压缩工具白名单

仅清除指定工具的结果，避免意外清除重要工具输出：

```rust
const DEFAULT_COMPACTABLE_TOOLS: &[&str] = &[
    "bash",
    "read_file",
    "glob_files",
    "search_files_rg",
    "write_file",
    "edit_file",
];
```

不在白名单中的工具（如 `ask_user_question`、`launch_agent`）结果保持原样。

#### 2. 时间衰减清除

基于"距最后一次 assistant 消息的步数"而非简单的 `keep_recent` 计数：

- 超过 `micro_compact_stale_steps`（默认 5 步）的工具结果视为"冷却"，可以清除
- 最近 N 步内的工具结果保持完整
- "步数"定义为：一条 Ai 消息（含或不含 tool_calls）到下一条 Ai 消息之间的消息序列

#### 3. 图片/大文档清除

- 工具结果中的图片 ContentBlock（Base64 编码）替换为 `[image]` 占位符
- 超过 `image_max_token_size`（默认 2000 token 估算）的图片直接清除
- 同样适用于 Document ContentBlock

#### 4. 清除方式

不删除消息本身，而是将 `ContentBlock::ToolResult` 的内容替换为 `[compacted: {原长度} chars]` 占位文本，保持消息骨架和 ID 完整。

#### 5. 工具对完整性保护

清除时通过 `adjust_index_to_preserve_invariants()` 确保不会拆开：
- `Tool` 消息与其对应的 `Ai` 消息（含 `ToolCallRequest`）不被分离
- 同一条 `Ai` 消息中的多个 `ToolCallRequest` 对应的 `Tool` 消息不被部分清除

### Full Compact 层增强

**现有实现**：`compact_task()` 格式化消息 → 构造简单 system prompt → 调用 LLM → 返回自由格式摘要。

**增强策略**：

#### 1. 预处理

- 移除图片 ContentBlock，替换为 `[image]` 标记（减少 token 开销）
- 按消息步数（API round）分组，用于 PTL 降级时按组删除
- 每条消息截断到合理长度（保留关键信息，避免超长工具输出浪费 token）

#### 2. 结构化摘要 Prompt

对齐 Claude Code 的摘要模板，分为 Analysis 和 Summary 两个块：

```
<analysis>
请分析以下对话历史，按以下 9 个方面进行详细分析：

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
</summary>
```

#### 3. LLM 调用配置

- 禁用 thinking 模式（节省 token）
- 设置 `max_output_tokens` 为 `summary_max_tokens`（默认 16000）
- 使用当前激活模型（不强制切换到特定模型）
- System prompt 极简："你是一个对话上下文压缩工具，擅长将长对话压缩为结构化摘要。"

#### 4. 后处理

- 移除 `<analysis>` 块，仅保留 `<summary>` 内容
- 添加前缀说明：`此会话从之前的对话延续。以下是之前对话的摘要。`
- 清理多余空白行

#### 5. PTL 降级重试

当 LLM 返回 `prompt_too_long` 错误时触发降级：

1. 按消息步数组（API round groups）从最旧开始删除
2. 计算需要删除的 token 差距
3. 保留至少一个完整的消息组
4. 最多重试 `ptl_max_retries`（默认 3）次
5. 每次重试前记录 `warn!` 日志
6. 全部失败则通过 `AgentEvent::CompactError` 报错

### 重新注入（Post-compact Re-injection）

Full Compact 生成摘要后，在新的消息历史中重新注入关键上下文。

#### 注入策略

**1. 最近读取的文件**（最高优先级）

- 从压缩前的消息中提取最近通过 `read_file` 工具读取的文件路径（从 `ToolStart` 事件的 `input` 中解析 `path` 字段，或从 `Tool` 消息的上下文中提取）
- 取最近 `re_inject_max_files`（默认 5）个文件
- 每个文件截断到 `re_inject_max_tokens_per_file`（默认 5000）tokens
- 总预算 `re_inject_file_budget`（默认 25000）tokens
- 以 `System` 消息形式注入：`[最近读取的文件: {path}]\n{content}`

**2. 激活的 Skills 指令**

- 从压缩前的消息中识别被 SkillPreloadMiddleware 注入的 skills（通过 fake `read_file` 工具调用序列中的路径判断）
- 每个 skill 截断到 5000 tokens
- 总预算 `re_inject_skills_budget`（默认 25000）tokens
- 以 `System` 消息形式注入

**3. 注入位置**

在摘要 System 消息之后、新对话消息之前：

```
System: [摘要]
System: [重新注入的文件内容]
System: [重新注入的 Skills]
```

#### 与 Thread 迁移的交互

- 保持现有的 Thread 迁移方案（创建新 Thread）
- 新 Thread 的初始消息 = 摘要 + 重新注入内容
- 旧 Thread 完整保留在数据库中

### 配置体系

#### settings.json 新增字段

```json
{
  "compact": {
    "autoCompactEnabled": true,
    "autoCompactThreshold": 0.85,
    "microCompactThreshold": 0.70,
    "microCompactStaleSteps": 5,
    "microCompactableTools": [
      "bash", "read_file", "glob_files",
      "search_files_rg", "write_file", "edit_file"
    ],
    "summaryMaxTokens": 16000,
    "reInjection": {
      "maxFiles": 5,
      "maxTokensPerFile": 5000,
      "fileBudget": 25000,
      "skillsBudget": 25000
    },
    "maxConsecutiveFailures": 3,
    "ptlMaxRetries": 3
  }
}
```

#### 环境变量覆盖

| 环境变量 | 对应字段 | 说明 |
|---------|---------|------|
| `DISABLE_COMPACT` | — | 完全禁用 compact |
| `DISABLE_AUTO_COMPACT` | `autoCompactEnabled` | 仅禁用自动 compact |
| `COMPACT_THRESHOLD` | `autoCompactThreshold` | 覆盖自动压缩阈值（0-1 浮点数） |
| `COMPACT_CONTEXT_WINDOW` | — | 覆盖上下文窗口大小（token 数） |

#### CompactConfig 结构体

```rust
/// Compact 配置，所有字段均有默认值
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactConfig {
    pub auto_compact_enabled: bool,          // 默认 true
    pub auto_compact_threshold: f64,         // 默认 0.85
    pub micro_compact_threshold: f64,        // 默认 0.70
    pub micro_compact_stale_steps: usize,    // 默认 5
    pub micro_compactable_tools: Vec<String>,// 默认白名单
    pub summary_max_tokens: u32,            // 默认 16000
    pub re_inject_max_files: usize,         // 默认 5
    pub re_inject_max_tokens_per_file: u32, // 默认 5000
    pub re_inject_file_budget: u32,         // 默认 25000
    pub re_inject_skills_budget: u32,       // 默认 25000
    pub max_consecutive_failures: u32,      // 默认 3
    pub ptl_max_retries: u32,               // 默认 3
}
```

**配置传递链**：TUI 层读取 `settings.json` → Agent 组装时以 `CompactConfig` 传入核心层 → `CompactManager` 使用。核心层不直接读取 `settings.json`，保持分层独立性。

### 触发机制

保持现有的两阶段触发，增加配置驱动：

1. **标记阶段**：`LlmCallEnd` 事件中，`ContextBudget::should_compact()` 根据配置的阈值判断
   - Token 使用率 ≥ `micro_compact_threshold`（默认 0.70）→ 标记 micro-compact
   - Token 使用率 ≥ `auto_compact_threshold`（默认 0.85）→ 标记 full-compact
2. **执行阶段**：`Done` 事件中 agent 完全停止后执行，避免打断正在进行的 ReAct 循环
3. **失败保护**：连续失败 `max_consecutive_failures`（默认 3）次后停止自动触发

## 实现要点

1. **消息步数分组**：PTL 降级和 Micro-compact 都需要按"API round"（一轮 LLM 调用 + 工具调用）分组消息。通过 `Ai` 消息中的 `tool_calls` 来推断分组边界。

2. **工具对完整性保护的实现**：在 Micro-compact 确定清除范围时，向前/向后扫描确保 `Tool` 消息对应的 `Ai` 消息（含 `ToolCallRequest`）在同一个清除边界内。如果清除会拆开一对，则调整清除边界包含整对。

3. **重新注入的文件内容获取**：从压缩前的 `ToolStart` 事件记录或 `Tool` 消息上下文中提取最近 `read_file` 的文件路径，在 compact task 中异步重新读取文件内容。

4. **向后兼容**：新 compact 系统与现有 Thread 持久化兼容。旧 Thread 的消息格式不变，新 compact 的输出（摘要 + 重新注入）也使用现有的 `BaseMessage` 格式。

5. **PTL 降级中的 token 估算**：降级需要估算消息组的 token 数。可复用 `TokenTracker` 中已有的估算逻辑，或使用简单的字符数/4 估算。

6. **图片清除的 token 估算**：Base64 编码的图片大小约为原始大小的 4/3，可用 ContentBlock 的字符串长度估算 token 数。

## 约束一致性

- **Workspace 分层**：compact 核心逻辑放在 `rust-create-agent`，TUI 层仅做触发和 UI，符合"禁止下层依赖上层"约束
- **异步优先**：compact task 中的 LLM 调用和文件读取都是异步操作，通过 `async-trait` 标注
- **消息不可变历史**：compact 生成新的消息序列，不修改旧 Thread 的历史（创建新 Thread）
- **事件驱动通信**：compact 完成后通过 `AgentEvent::CompactDone` / `CompactError` 通知 TUI
- **编码规范**：使用 `thiserror` 定义 compact 专用错误类型，`tracing` 宏记录日志

## 验收标准

- [ ] Micro-compact 按工具白名单清除，不在白名单中的工具结果不受影响
- [ ] Micro-compact 按时间衰减策略清除，最近 N 步的工具结果保持完整
- [ ] Micro-compact 清除图片 ContentBlock，替换为 `[image]` 占位符
- [ ] Micro-compact 保持工具对完整性（tool_use + tool_result 不被拆开）
- [ ] Full Compact 使用 9 段结构化摘要模板生成摘要
- [ ] Full Compact 后处理正确移除 `<analysis>` 块
- [ ] PTL 降级重试在压缩请求超长时自动触发，最多重试 3 次
- [ ] Full Compact 后重新注入最近读取的文件内容
- [ ] Full Compact 后重新注入激活的 Skills 指令
- [ ] 新 Thread 正确创建，旧 Thread 完整保留
- [ ] CompactConfig 所有字段可通过 `settings.json` 配置
- [ ] 环境变量 `DISABLE_COMPACT` / `DISABLE_AUTO_COMPACT` / `COMPACT_THRESHOLD` 正确覆盖配置
- [ ] 自动触发在连续失败 3 次后停止
- [ ] 单元测试覆盖 Micro-compact 策略（白名单过滤、时间衰减、工具对保护）
- [ ] 单元测试覆盖 PTL 降级重试逻辑
- [ ] Headless 集成测试验证完整 compact 流程
