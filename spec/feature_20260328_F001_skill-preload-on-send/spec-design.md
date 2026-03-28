# Feature: 20260328_F001 - skill-preload-on-send

## 需求背景

TUI 中输入 `#skill-name` 目前仅触发浮层补全（Tab 导航 + Enter 补全），`SkillsMiddleware` 只将 skill 摘要注入到 system prompt。用户发送消息时，skill 的**完整内容**并不会进入 LLM 的上下文，导致 LLM 只知道 skill 的名称和简短描述，无法按照完整 skill 指令执行。

子 Agent 场景中，`SkillPreloadMiddleware` 已经实现了将 skill 全文以 fake `read_file` 工具调用序列注入 state 的机制，但主 Agent 场景中没有利用这一能力。

## 目标

- 用户在 TUI 消息中输入 `#skill-name`（可多个）并发送时，自动将对应 skill 全文通过 `SkillPreloadMiddleware` 注入到 agent state
- 消息原文保留 `#skill-name` 文本，不修改用户输入
- 复用现有 `SkillPreloadMiddleware` 机制，不引入新的消息格式或新的中间件

## 方案设计

### 数据流

![用户发送 #skill-name 时的完整预加载数据流](./images/01-flow.png)

```
用户输入: "#sdd-brainstorming 帮我设计一个功能"
  ↓
submit_message: 正则 #([a-zA-Z0-9_-]+) 解析出 ["sdd-brainstorming"]
  ↓
AgentRunConfig { preload_skills: ["sdd-brainstorming"], input: AgentInput::text("..."), ... }
  ↓
run_universal_agent:
  .add_middleware(SkillsMiddleware::new())
  .add_middleware(SkillPreloadMiddleware::new(["sdd-brainstorming"], &cwd))  ← 新增
  .add_middleware(FilesystemMiddleware::new())
  ...
  ↓
before_agent 执行 (SkillPreloadMiddleware):
  注入 [Human("系统：预加载 skill 文件") + Ai[ToolUse(read_file)] + Tool[ToolResult(skill 全文)]]
  ↓
LLM 首轮推理即可看到 sdd-brainstorming 完整内容 → 按完整 skill 指令执行
```

### 变更点（最小改动，3 处）

**1. `AgentRunConfig` 扩展（`rust-agent-tui/src/app/agent.rs`）**

```rust
pub struct AgentRunConfig {
    // ... 现有字段 ...
    pub preload_skills: Vec<String>,  // 新增：用户消息中解析出的 skill 名列表
}
```

**2. `submit_message` 解析（`rust-agent-tui/src/app/agent_ops.rs`）**

```rust
// 解析消息中的 #skill-name（如 #sdd-brainstorming #code-review 等）
let skill_re = regex::Regex::new(r"#([a-zA-Z0-9_-]+)").unwrap();
let preload_skills: Vec<String> = skill_re
    .captures_iter(&input)
    .map(|c| c[1].to_string())
    .collect();

// 传入 AgentRunConfig
agent::run_universal_agent(agent::AgentRunConfig {
    // ... 现有字段 ...
    preload_skills,
}).await;
```

**3. `run_universal_agent` 插入中间件（`rust-agent-tui/src/app/agent.rs`）**

```rust
let executor = ReActAgent::new(model)
    // ...
    .add_middleware(Box::new(SkillsMiddleware::new()))
    // 新增：当有 preload_skills 时插入 SkillPreloadMiddleware
    .add_middleware(Box::new(SkillPreloadMiddleware::new(preload_skills, &cwd)))
    .add_middleware(Box::new(FilesystemMiddleware::new()))
    // ...
```

> 注意：`SkillPreloadMiddleware::new(vec![], &cwd)` 时，`before_agent` 内部 early return（已有逻辑），无空列表性能损耗。

### 中间件插入位置

按照 architecture.md 规定的标准执行顺序：

```
SkillsMiddleware        ← 摘要注入 system（已有）
SkillPreloadMiddleware  ← 全文注入 state（新增，紧随 SkillsMiddleware 之后）
FilesystemMiddleware    ← 文件系统工具
TerminalMiddleware      ← bash 工具
...
```

### 多 skill 示例

输入 `#skill-a #skill-b 请帮我做这件事`：
- 解析得到 `["skill-a", "skill-b"]`
- `SkillPreloadMiddleware` 注入：`Human + Ai[ToolUse×2] + Tool[ToolResult] + Tool[ToolResult]`
- 找不到的 skill 名静默跳过（`SkillPreloadMiddleware` 现有行为）

## 实现要点

- **正则**：`#([a-zA-Z0-9_-]+)` 匹配合法 skill 名（含字母、数字、连字符、下划线），避免误匹配 `#123` 等非 skill token
- **依赖**：`rust-agent-tui` 需添加 `regex` crate（或使用标准库手动解析，可选）
- **无 `#` 的普通消息**：`preload_skills` 为空 → `SkillPreloadMiddleware.before_agent` early return → 行为与现有完全一致，无任何额外开销
- **`regex` crate 复用**：可考虑用简单字符串分割（`split_whitespace + starts_with('#')` + `trim_start_matches('#')`）代替 regex，消除外部依赖

## 约束一致性

- 遵循 **Workspace 分层约束**：修改仅在 `rust-agent-tui`（应用层），`rust-agent-middlewares` 的 `SkillPreloadMiddleware` 无需改动
- 遵循 **Middleware Chain 模式**：通过 `add_middleware` 接口插入，不侵入 ReAct 执行器核心
- 遵循 **消息不可变历史**：`prepend_message` 是 `before_agent` 阶段的合法操作，skill 注入消息在正式 ReAct 循环开始前完成

## 验收标准

- [ ] 输入 `#foo-skill hello` 发送后，foo-skill 全文通过 `ToolResult` 注入 agent state（可通过 tracing 日志验证）
- [ ] 输入 `#skill-a #skill-b hello` 发送后，两个 skill 均被注入（state 中出现 `Human + Ai[ToolUse×2] + Tool×2` 序列）
- [ ] 输入 `# `（无 skill 名）或普通消息，无报错，行为与修改前完全一致
- [ ] skill 名在磁盘不存在时，静默跳过，不影响 Agent 正常运行
- [ ] `SkillPreloadMiddleware` 插入位置在 `SkillsMiddleware` 之后、`FilesystemMiddleware` 之前
