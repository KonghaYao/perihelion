# Feature: 20260321_F001 - subagents-execution

## 需求背景

当前 Rust Agent 框架支持通过工具调用执行文件操作、终端命令等原子操作，但缺乏将复杂子任务委派给专门 AI 子代理的能力。参考 Claude Code 的 Subagents 机制（见 `SUBAGENT.md`），我们需要在框架中实现类似功能：LLM 可通过调用 `launch_agent` 工具，将特定子任务委派给一个按配置文件定义的专门子 agent 执行，子 agent 独立完成任务后将结果返回给父 agent。

## 目标

- 新增 `launch_agent` 工具，允许 LLM 通过工具调用启动子 agent
- 子 agent 从 `.claude/agents/` 目录读取配置文件（Claude Code 兼容格式）
- 子 agent 继承父 agent 的工具集（支持 tools/disallowedTools 过滤）
- 子 agent 事件（工具调用、输出）嵌套透传到父 agent 的事件流，TUI 无需修改
- 子 agent 不包含 HITL 中间件，轻量执行

## 方案设计

### 架构设计

采用 **`SubAgentTool` + `SubAgentMiddleware`** 组合方案：

- **`SubAgentTool`**：实现 `BaseTool` trait，工具名为 `launch_agent`，LLM 通过调用此工具并传入 `agent_id` 来启动子 agent
- **`SubAgentMiddleware`**：实现 `Middleware` trait（以及可选的 `ToolProvider`），负责构建并持有 `SubAgentTool`，向父 agent 注入该工具

新增文件位于 `rust-agent-middlewares/src/subagent/`：

```
rust-agent-middlewares/src/
  subagent/
    mod.rs          # SubAgentMiddleware + pub 导出
    tool.rs         # SubAgentTool (BaseTool 实现)
```

![架构数据流图](./images/01-flow.png)

### 组件关系

```
ReActAgent（父）
  ├── SubAgentMiddleware
  │     ├── parent_tools: Arc<Vec<Arc<dyn BaseTool>>>（与父共享，只读）
  │     ├── event_handler: Option<Arc<dyn AgentEventHandler>>（与父共享）
  │     └── llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM> + Send + Sync>
  │
  └── SubAgentTool（由 SubAgentMiddleware 注册到父 agent）
        └── invoke({agent_id, task, cwd?})
              ├── 查找 .claude/agents/{agent_id}.md
              ├── 解析 ClaudeAgent 定义
              ├── 过滤父工具集（tools/disallowedTools）
              ├── 组装轻量子 ReActAgent
              └── 执行 → 返回摘要字符串（工具列表 + 最终回答）
```

### 接口设计

#### `SubAgentMiddleware`

```rust
pub struct SubAgentMiddleware {
    /// 父 agent 的工具集（Arc<dyn BaseTool> 共享，只读）
    parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
    /// 父 agent 的事件处理器（用于子 agent 事件透传）
    event_handler: Option<Arc<dyn AgentEventHandler>>,
    /// LLM 工厂函数，每次为子 agent 创建新的 LLM 实例
    llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
}

impl SubAgentMiddleware {
    pub fn new(
        parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
        event_handler: Option<Arc<dyn AgentEventHandler>>,
        llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    ) -> Self

    /// 构建 SubAgentTool 以供外部注册
    pub fn build_tool(&self) -> SubAgentTool
}
```

#### `SubAgentTool`

```rust
pub struct SubAgentTool {
    parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
    event_handler: Option<Arc<dyn AgentEventHandler>>,
    llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
}

// BaseTool 实现
// name()        → "launch_agent"
// description() → "委派任务给专门的子 agent 执行..."
// parameters()  → { agent_id: string, task: string, cwd?: string }
// invoke({agent_id, task, cwd?}) → 执行子 agent，返回结果文本
```

#### LLM 调用格式

```json
{
  "name": "launch_agent",
  "input": {
    "agent_id": "code-reviewer",
    "task": "Review the authentication module for security issues",
    "cwd": "/optional/path"
  }
}
```

### 子 Agent 执行流程

```
SubAgentTool::invoke({agent_id, task, cwd?})

1. 查找 agent 定义文件（按优先级）：
   {cwd}/.claude/agents/{agent_id}.md
   {cwd}/.claude/agents/{agent_id}/agent.md
   {cwd}/agents/{agent_id}.md
   {cwd}/agents/{agent_id}/agent.md
   → 未找到 → 返回错误信息（工具级 error）

2. 解析 ClaudeAgentFrontmatter：
   → system_prompt, model, max_turns, tools, disallowedTools

3. 工具过滤（从 parent_tools 过滤）：
   - tools 字段为空 → 继承所有父工具
   - tools 有值      → 仅保留名称在 tools 列表中的工具
   - disallowedTools → 从结果集中排除

4. 组装轻量子 ReActAgent：
   - LLM：llm_factory() 创建新实例，system_prompt = agent_def.system_prompt
   - 工具：步骤 3 过滤后的工具集（register_tool 逐一注册）
   - max_iterations：agent_def.max_turns.unwrap_or(DEFAULT_MAX_TURNS)
   - event_handler：父 agent 共享的 handler（事件透传）
   - 无 HITL 中间件、无 ask_user

5. 执行：
   agent.execute(AgentInput::text(task), &mut AgentState::new(cwd), None).await

6. 返回摘要字符串（成功）或错误信息（失败）：
   - 无工具调用 → 直接返回 AgentOutput.text
   - 有工具调用 → "[子 agent 执行了 N 个工具调用: tool1, tool2]\n\n最终回答"
     （中间工具结果舍弃，避免 token 膨胀）
```

![子 Agent 执行序列图](./images/02-flow.png)

### TUI 集成

子 agent 与父 agent 共享 `event_handler`（`Arc<dyn AgentEventHandler>`），因此子 agent 的 `ToolStart`/`ToolEnd`/`TextChunk` 事件会直接触发父 event_handler，TUI 的 `poll_agent()` 逻辑无需修改即可展示子 agent 的执行过程。

**TUI 实际接入**（`rust-agent-tui/src/app/agent.rs`）：

```rust
// 1. 从 FilesystemMiddleware + TerminalMiddleware 收集父工具集
//    用 BoxToolWrapper 将 Box<dyn BaseTool> 转为 Arc<dyn BaseTool>
let parent_tools: Arc<Vec<Arc<dyn BaseTool>>> = {
    let fs_tools = FilesystemMiddleware::new().tools(&cwd);
    let term_tools = TerminalMiddleware::new().tools(&cwd);
    Arc::new(fs_tools.into_iter().chain(term_tools)
        .map(|t| Arc::new(BoxToolWrapper(t)) as Arc<dyn BaseTool>)
        .collect())
};

// 2. handler 类型标注为 Arc<dyn AgentEventHandler> 以便共享
let handler: Arc<dyn AgentEventHandler> = Arc::new(FnEventHandler(...));

// 3. LLM 工厂：复用相同 provider，每次创建独立实例
let llm_factory = Arc::new(move || {
    Box::new(BaseModelReactLLM::new(provider_clone.clone().into_model())
        .with_system(system)) as Box<dyn ReactLLM + Send + Sync>
});

// 4. 挂载 SubAgentMiddleware
ReActAgent::new(model)
    .add_middleware(Box::new(SubAgentMiddleware::new(parent_tools, Some(Arc::clone(&handler)), llm_factory)))
    .with_event_handler(Arc::clone(&handler))
```

**辅助类型 `BoxToolWrapper`**（`rust-agent-middlewares/src/tools/mod.rs`）：将 `Box<dyn BaseTool>` 包装为可共享的 `Arc<dyn BaseTool>`，是连接中间件工具收集与父工具集构建的桥梁。

## 实现要点

1. **工具过滤**：`parent_tools` 存为 `Arc<Vec<Arc<dyn BaseTool>>>`，过滤时按名称匹配，生成新向量；子 agent 注册时用 `ArcToolWrapper` 将 `Arc<dyn BaseTool>` 包装为 `Box<dyn BaseTool>`，避免所有权转移。TUI 侧用 `BoxToolWrapper` 将中间件的 `Box<dyn BaseTool>` 转为 `Arc<dyn BaseTool>` 以便共享。

2. **LLM 工厂**：`llm_factory` 签名为 `Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>`，在 TUI 层创建时捕获 model 配置（API key、base URL 等），子 agent 执行时调用工厂获得独立 LLM 实例。

3. **事件嵌套**：子 agent 的事件通过共享的 `Arc<dyn AgentEventHandler>` 透传，不新增事件类型。若将来需要区分父子 agent 事件，可扩展 `AgentEvent` 枚举（如 `SubagentEvent { agent_id, inner_event }`），当前不做此扩展。

4. **错误处理**：子 agent 执行失败时，`invoke()` 返回 `Ok(error_message_string)` 而非 `Err`，让 LLM 感知错误并决策下一步（与现有工具错误处理方式一致）。

5. **循环防护**：子 agent 本身不注册 `launch_agent` 工具（工具继承时不包含 `SubAgentTool`），天然防止子 agent 递归启动子 agent。

## 约束一致性

- 无 `spec/global/constraints.md`，跳过约束检查
- 方案完全遵循现有 `BaseTool`/`Middleware`/`ReActAgent` 模式，无破坏性变更
- 新增代码仅在 `rust-agent-middlewares` crate，不修改 `rust-create-agent` 核心 trait

## 验收标准

- [x] `launch_agent` 工具可正常出现在 LLM 的工具列表中
- [x] LLM 调用 `launch_agent` 时，子 agent 能正确加载对应的 agent 定义文件
- [x] 子 agent 执行时继承父 agent 的工具集（tools/disallowedTools 过滤正确）
- [x] 子 agent 的 ToolStart/ToolEnd/TextChunk 事件正确触发父 event_handler（TUI 可见）
- [x] 子 agent 无法再次调用 `launch_agent`（无递归风险）
- [x] agent 定义文件不存在时，工具返回清晰的错误信息
- [x] 单元测试：工具过滤逻辑、agent 文件查找、错误路径覆盖
- [x] TUI 接入：SubAgentMiddleware 已挂载到 rust-agent-tui
- [x] 执行结果摘要：工具调用列表 + 最终回答，中间结果舍弃
