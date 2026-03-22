# subagents-execution 执行计划

**目标:** 在 rust-agent-middlewares 中实现 `launch_agent` 工具，允许 LLM 将子任务委派给专门配置的子 agent 执行

**技术栈:** Rust / Tokio / async-trait / serde_json / rust-create-agent

**设计文档:** [spec-design.md](./spec-design.md)

---

### Task 1: ArcToolWrapper 包装层

**涉及文件:**
- 修改: `rust-agent-middlewares/src/tools/mod.rs`

**执行步骤:**
- [x] 在 `tools/mod.rs` 中新增 `ArcToolWrapper` 结构体，将 `Arc<dyn BaseTool>` 包装为 `Box<dyn BaseTool>` 可用的形式
  - 实现 `BaseTool` trait，所有方法委托给内部 `Arc<dyn BaseTool>`
  - `name()` / `description()` / `parameters()` / `invoke()` 均透传
  - 这样父工具集可存储为 `Arc<Vec<Arc<dyn BaseTool>>>`，子 agent 注册时用 `ArcToolWrapper` 包一层

```rust
pub struct ArcToolWrapper(pub Arc<dyn BaseTool>);

#[async_trait]
impl BaseTool for ArcToolWrapper {
    fn name(&self) -> &str { self.0.name() }
    fn description(&self) -> &str { self.0.description() }
    fn parameters(&self) -> serde_json::Value { self.0.parameters() }
    async fn invoke(&self, input: serde_json::Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        self.0.invoke(input).await
    }
}
```
- [x] 在 `tools/mod.rs` 中 pub 导出 `ArcToolWrapper`
- [x] 在 `rust-agent-middlewares/src/lib.rs` 的 `prelude` 中导出 `ArcToolWrapper`

**检查步骤:**
- [x] 编译通过，无错误
  - `cargo build -p rust-agent-middlewares 2>&1 | grep -E "^error"`
  - 预期: 无输出（无编译错误）
- [x] ArcToolWrapper 可被 Box::new 包裹注册为 BaseTool
  - `cargo test -p rust-agent-middlewares --lib 2>&1 | grep -E "FAILED|ok"`
  - 预期: 所有已有测试仍为 ok

---

### Task 2: SubAgentTool 实现

**涉及文件:**
- 新建: `rust-agent-middlewares/src/subagent/tool.rs`

**执行步骤:**
- [x] 创建 `rust-agent-middlewares/src/subagent/` 目录和 `tool.rs` 文件
- [x] 定义 `SubAgentTool` 结构体，持有三个字段：
  - `parent_tools: Arc<Vec<Arc<dyn BaseTool>>>` — 父 agent 工具集
  - `event_handler: Option<Arc<dyn AgentEventHandler>>` — 父 agent 事件处理器
  - `llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>` — LLM 工厂

- [x] 实现 `BaseTool` for `SubAgentTool`：
  - `name()` → `"launch_agent"`
  - `description()` → 描述委派子任务给专门 agent 执行，说明 agent_id 来源于 `.claude/agents/` 目录
  - `parameters()` → JSON Schema：
    ```json
    {
      "type": "object",
      "required": ["agent_id", "task"],
      "properties": {
        "agent_id": { "type": "string", "description": "Agent 定义文件名（不含 .md 扩展名）" },
        "task": { "type": "string", "description": "委派给子 agent 的任务描述" },
        "cwd": { "type": "string", "description": "子 agent 工作目录，默认继承父 agent cwd" }
      }
    }
    ```

- [x] 实现 `invoke()` 主逻辑：
  1. 从 JSON input 解析 `agent_id`、`task`、可选 `cwd`
  2. 调用 `AgentDefineMiddleware::candidate_paths(cwd, agent_id)` 查找 agent 定义文件，取第一个存在的文件
  3. 若未找到，返回 `Ok(format!("错误：找不到 agent 定义文件 '{}'，请检查 .claude/agents/ 目录", agent_id))`
  4. 读取文件内容，调用 `parse_agent_file()` 解析，失败返回 error 字符串
  5. 工具过滤：遍历 `parent_tools`，根据 `frontmatter.tools` 和 `frontmatter.disallowed_tools` 过滤：
     - `tools` 为 `Empty` → 继承所有父工具（但排除 `launch_agent` 自身）
     - `tools` 有值 → 仅保留名称在列表中的工具（同时排除 `launch_agent`）
     - 再从结果中移除 `disallowed_tools` 列出的工具
  6. 调用 `llm_factory()` 创建 LLM，用 `agent_def.system_prompt` 作为 system
  7. 组装子 `ReActAgent`（`AgentState`）：
     - 工具：用 `ArcToolWrapper` 包裹过滤后的工具，逐一 `register_tool`
     - `max_iterations`：`agent_def.frontmatter.max_turns.unwrap_or(20)`
     - `event_handler`：透传父 agent 的 `event_handler`
  8. 执行 `agent.execute(AgentInput::text(task), &mut AgentState::new(cwd), None).await`
  9. 成功返回 `Ok(output.text)`，`AgentError` 转为 `Ok(format!("子 agent 执行失败：{}", e))`

- [x] 实现 `SubAgentTool::new()` 构造函数

**检查步骤:**
- [x] 工具名称正确
  - `cargo test -p rust-agent-middlewares -- subagent::tool 2>&1 | grep -E "FAILED|ok"`
  - 预期: 所有单测 ok
- [x] 工具 JSON Schema 参数格式正确（required 包含 agent_id 和 task）
  - `cargo test -p rust-agent-middlewares -- subagent 2>&1 | grep -E "FAILED|ok"`
  - 预期: 无 FAILED
- [x] agent 文件不存在时返回错误字符串（非 Err）
  - 单测 `test_tool_agent_not_found` 验证 `invoke` 返回包含"找不到"的 Ok 字符串
  - 预期: 测试 ok
- [x] 工具过滤正确：tools 为空时继承父工具、tools 有值时仅保留指定工具、launch_agent 不递归
  - 单测 `test_tool_filter_*` 系列
  - 预期: 全部 ok

---

### Task 3: SubAgentMiddleware 实现

**涉及文件:**
- 新建: `rust-agent-middlewares/src/subagent/mod.rs`

**执行步骤:**
- [x] 在 `mod.rs` 中定义 `SubAgentMiddleware` 结构体（同 `SubAgentTool` 持有相同三字段），再加 `cwd: Option<String>` 用于构建时传入
- [x] 实现 `SubAgentMiddleware::new()` 构造函数
- [x] 实现 `build_tool(&self) -> SubAgentTool` 方法：克隆三个 Arc 字段，构造 `SubAgentTool`
- [x] 实现 `Middleware<S: State>` for `SubAgentMiddleware`：
  - `name()` → `"SubAgentMiddleware"`
  - `collect_tools()` → 返回 `vec![Box::new(self.build_tool())]`
  - 其余钩子默认 no-op
- [x] 在 `mod.rs` 顶部声明 `mod tool; pub use tool::SubAgentTool;`

```rust
pub struct SubAgentMiddleware {
    parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
    event_handler: Option<Arc<dyn AgentEventHandler>>,
    llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
}
```

**检查步骤:**
- [x] 中间件可正常挂载到 ReActAgent
  - `cargo build -p rust-agent-middlewares 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] `collect_tools()` 返回包含 `launch_agent` 的工具列表
  - 单测 `test_middleware_collect_tools` 验证工具名称
  - 预期: `tools[0].name() == "launch_agent"`
- [x] 所有已有测试不受影响
  - `cargo test -p rust-agent-middlewares --lib 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"

---

### Task 4: lib.rs 导出与集成

**涉及文件:**
- 修改: `rust-agent-middlewares/src/lib.rs`
- 修改: `rust-agent-middlewares/src/tools/mod.rs`（pub use ArcToolWrapper）

**执行步骤:**
- [x] 在 `rust-agent-middlewares/src/lib.rs` 中添加 `pub mod subagent;`
- [x] 在 `lib.rs` 中添加顶层导出：`pub use subagent::{SubAgentMiddleware, SubAgentTool};`
- [x] 在 `lib.rs` 的 `prelude` 模块中添加导出：
  ```rust
  pub use crate::subagent::{SubAgentMiddleware, SubAgentTool};
  pub use crate::tools::ArcToolWrapper;
  ```
- [x] 确认 `tools/mod.rs` 已 pub 导出 `ArcToolWrapper`（Task 1 完成）
- [x] 全量构建确认无循环依赖、无 unused import 警告

**检查步骤:**
- [x] 全量构建通过
  - `cargo build 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] prelude 中可一次性导入所有新类型
  - `cargo test --lib 2>&1 | grep -E "FAILED|error\["`
  - 预期: 无 FAILED 也无编译错误
- [x] SubAgentMiddleware 和 SubAgentTool 可从 crate root 访问
  - `cargo doc -p rust-agent-middlewares 2>&1 | grep -E "^error"`
  - 预期: 无文档生成错误

---

### Task 5: Subagents Acceptance

**Prerequisites:**
- 构建环境: `cargo build`（所有 crate 编译通过）
- 测试 agent 定义文件: 在临时目录创建 `.claude/agents/test-agent.md`
- 环境变量: `ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY`（集成测试需要）

**端到端验证:**

1. [x] launch_agent 工具出现在 LLM 工具列表
   - `cargo test -p rust-agent-middlewares -- test_launch_agent_tool_in_list 2>&1 | grep -E "ok|FAILED"`
   - Expected: ok
   - On failure: 检查 Task 3 SubAgentMiddleware::collect_tools

2. [x] agent 定义文件不存在时返回清晰错误
   - `cargo test -p rust-agent-middlewares -- test_tool_agent_not_found 2>&1 | grep -E "ok|FAILED"`
   - Expected: ok，invoke 返回包含"找不到"的 Ok 字符串
   - On failure: 检查 Task 2 invoke 错误路径处理

3. [x] tools 字段为空时子 agent 继承所有父工具（但不含 launch_agent）
   - `cargo test -p rust-agent-middlewares -- test_tool_filter_inherit_all 2>&1 | grep -E "ok|FAILED"`
   - Expected: ok，子 agent 工具集 == 父工具集 - {launch_agent}
   - On failure: 检查 Task 2 工具过滤逻辑

4. [x] tools 字段有值时只保留指定工具
   - `cargo test -p rust-agent-middlewares -- test_tool_filter_allowlist 2>&1 | grep -E "ok|FAILED"`
   - Expected: ok，子 agent 仅有 tools 字段指定的工具
   - On failure: 检查 Task 2 工具过滤 allow list 逻辑

5. [x] disallowedTools 正确排除工具
   - `cargo test -p rust-agent-middlewares -- test_tool_filter_disallow 2>&1 | grep -E "ok|FAILED"`
   - Expected: ok，被拒绝的工具不在子 agent 工具集中
   - On failure: 检查 Task 2 disallowed_tools 过滤逻辑

6. [x] 全量测试无回归
   - `cargo test 2>&1 | tail -10`
   - Expected: 输出包含 "test result: ok" 且无 FAILED
   - On failure: 检查各 Task 的具体失败原因

---

### Task 6: TUI 接入

**涉及文件:**
- 修改: `rust-agent-middlewares/src/tools/mod.rs`
- 修改: `rust-agent-middlewares/src/lib.rs`
- 修改: `rust-agent-tui/src/app/agent.rs`
- 修改: `rust-agent-tui/src/ui.rs`

**执行步骤:**
- [x] 在 `tools/mod.rs` 新增 `BoxToolWrapper`：将 `Box<dyn BaseTool>` 包装为 `Arc<dyn BaseTool>` 可用形式，用于从中间件工具收集结果构建父工具集
- [x] 在 `lib.rs` 和 `prelude` 导出 `BoxToolWrapper`
- [x] 在 `agent.rs` 中将 `handler` 类型标注为 `Arc<dyn AgentEventHandler>` 以便与 SubAgentMiddleware 共享
- [x] 收集父工具集：调用 `FilesystemMiddleware::new().tools(&cwd)` 和 `TerminalMiddleware::new().tools(&cwd)`，用 `BoxToolWrapper` 包装后存入 `Arc<Vec<Arc<dyn BaseTool>>>`
- [x] 构建 `llm_factory`：在 `provider.into_model()` 消耗 provider 前提前 clone，闭包中每次创建独立 LLM 实例
- [x] 将 `SubAgentMiddleware::new(parent_tools, Some(handler.clone()), llm_factory)` 加入中间件链
- [x] 更新标题栏描述文字

**检查步骤:**
- [x] 全量编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
  - 预期: 无输出

---

### Task 7: 子 Agent 执行结果摘要

**涉及文件:**
- 修改: `rust-agent-middlewares/src/subagent/tool.rs`

**执行步骤:**
- [x] 新增 `format_subagent_result(output: &AgentOutput) -> String` 函数
  - 无工具调用 → 直接返回 `output.text`
  - 有工具调用 → 拼接 `[子 agent 执行了 N 个工具调用: tool1, tool2]\n\n最终回答`（中间结果舍弃）
- [x] `invoke()` 成功路径改用 `format_subagent_result(&output)` 替代 `output.text`

**检查步骤:**
- [x] 全量测试无回归
  - `cargo test -p rust-agent-middlewares --lib 2>&1 | tail -3`
  - 预期: 输出包含 "test result: ok"
