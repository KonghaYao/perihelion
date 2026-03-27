# M3: 消除 PrependSystemMiddleware 排序约束 执行计划

**目标:** 在 `ReActAgent` 上提供 `with_system_prompt()` 专用方法，executor 内部固定在 `run_before_agent` 之后 prepend system 消息，彻底消除中间件注册顺序约束

**技术栈:** Rust / async_trait / rust-create-agent / rust-agent-middlewares / rust-agent-tui

**设计文档:** Plan-M3-system-prompt.md

---

### Task 1: ReActAgent 添加 with_system_prompt 支持

**涉及文件:**
- 修改: `rust-create-agent/src/agent/executor.rs`

**执行步骤:**
- [x] 在 `ReActAgent<L, S>` 结构体中新增字段 `system_prompt: Option<String>`，初始值为 `None`
- [x] 在 `ReActAgent<L, S>` 的 `new()` 中初始化该字段为 `None`
- [x] 新增 builder 方法 `with_system_prompt(mut self, prompt: impl Into<String>) -> Self`，设置 `self.system_prompt = Some(prompt.into())`
- [x] 在 `execute()` 中，`self.chain.run_before_agent(state).await?` 之后（第 131 行附近）插入 prepend 逻辑：
  ```rust
  if let Some(ref prompt) = self.system_prompt {
      state.prepend_message(BaseMessage::system(prompt.clone()));
  }
  ```
  此位置确保：①所有中间件 before_agent 已执行完毕，②system 消息处于列表最前

**检查步骤:**
- [x] 编译核心库无报错
  - `cargo build -p rust-create-agent 2>&1 | grep -E "^error"`
  - 预期: 无输出（无编译错误）
- [x] 新增 `with_system_prompt` 方法存在
  - `grep -n "with_system_prompt" rust-create-agent/src/agent/executor.rs`
  - 预期: 找到至少 2 处（字段定义 + builder 方法）
- [x] 核心库原有测试全部通过
  - `cargo test -p rust-create-agent --lib 2>&1 | tail -5`
  - 预期: `test result: ok. N passed; 0 failed`

---

### Task 2: TUI agent.rs 迁移 PrependSystemMiddleware → with_system_prompt

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent.rs`

**执行步骤:**
- [x] 删除 `.add_middleware(Box::new(rust_agent_middlewares::PrependSystemMiddleware::new(system_prompt)))` 这一行（当前最后一个 add_middleware 调用）
- [x] 在 `ReActAgent::new(model)` 的 builder 链中任意位置添加 `.with_system_prompt(system_prompt)`（推荐紧接 `.max_iterations(500)` 之后，语义清晰）
- [x] 检查是否有其他模块也 use PrependSystemMiddleware 而仅为 TUI 主 agent 服务，若有则清理对应 import（`use rust_agent_middlewares::PrependSystemMiddleware` 等）

**检查步骤:**
- [x] TUI 编译无报错
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] 确认旧调用已删除
  - `grep -n "PrependSystemMiddleware" rust-agent-tui/src/app/agent.rs`
  - 预期: 无输出（已完全移除）
- [x] 确认新调用已存在
  - `grep -n "with_system_prompt" rust-agent-tui/src/app/agent.rs`
  - 预期: 找到 1 处

---

### Task 3: SubAgentTool 迁移 PrependSystemMiddleware → with_system_prompt

**涉及文件:**
- 修改: `rust-agent-middlewares/src/subagent/tool.rs`

**执行步骤:**
- [x] 定位当前使用 `PrependSystemMiddleware` 的代码块（invoke 方法第 216-223 行附近）：
  ```rust
  // 旧代码
  if let Some(ref builder) = self.system_builder {
      let overrides = AgentDefineMiddleware::load_overrides(&cwd, &agent_id);
      let system_content = builder(overrides.as_ref(), &cwd);
      agent_builder = agent_builder
          .add_middleware(Box::new(PrependSystemMiddleware::new(system_content)));
  }
  ```
- [x] 替换为 `with_system_prompt` 调用：
  ```rust
  // 新代码
  if let Some(ref builder) = self.system_builder {
      let overrides = AgentDefineMiddleware::load_overrides(&cwd, &agent_id);
      let system_content = builder(overrides.as_ref(), &cwd);
      agent_builder = agent_builder.with_system_prompt(system_content);
  }
  ```
- [x] 删除文件顶部 `use crate::middleware::PrependSystemMiddleware;` import（如该文件中已无其他 PrependSystemMiddleware 用处）
- [x] 更新 `SubAgentTool` 的 `system_builder` 字段注释，将 "通过 `PrependSystemMiddleware` 注入" 改为 "通过 `with_system_prompt` 注入"

**检查步骤:**
- [x] 中间件库编译无报错
  - `cargo build -p rust-agent-middlewares 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] 确认旧调用已删除
  - `grep -n "PrependSystemMiddleware" rust-agent-middlewares/src/subagent/tool.rs`
  - 预期: 无输出
- [x] 中间件库原有测试全部通过（含 test_system_builder_injects_system_message）
  - `cargo test -p rust-agent-middlewares --lib subagent 2>&1 | tail -10`
  - 预期: `test result: ok. N passed; 0 failed`

---

### Task 4: PrependSystemMiddleware 标记废弃

**涉及文件:**
- 修改: `rust-agent-middlewares/src/middleware/prepend_system.rs`

**执行步骤:**
- [x] 在 `pub struct PrependSystemMiddleware` 定义上方添加 `#[deprecated]` 属性和说明注释：
  ```rust
  /// PrependSystemMiddleware - 在 before_agent 阶段将固定 system 内容注入 state 消息列表
  ///
  /// # 废弃说明
  ///
  /// 请改用 `ReActAgent::with_system_prompt()`，它在 executor 内部固定于
  /// 所有中间件 `before_agent` 执行完毕之后 prepend，无顺序约束。
  ///
  /// 本类型保留用于需要动态 system prompt 或其他高级场景。
  #[deprecated(since = "0.2.0", note = "改用 ReActAgent::with_system_prompt()")]
  pub struct PrependSystemMiddleware {
  ```
- [x] 同步更新 `new()` 方法，加上 `#[allow(deprecated)]` 以避免内部自引用警告（若有的话）
- [x] 检查 `rust-agent-middlewares/src/lib.rs` 的 prelude 导出，若仍导出 `PrependSystemMiddleware`，在导出处加 `#[allow(deprecated)]` 属性

**检查步骤:**
- [x] 编译全量 workspace，确认废弃警告出现在预期位置
  - `cargo build 2>&1 | grep -i deprecated`
  - 预期: 出现 `deprecated` 相关警告（表示标注生效）
- [x] 全量测试无新增失败
  - `cargo test -p rust-create-agent -p rust-agent-middlewares 2>&1 | grep -E "FAILED|test result"`
  - 预期: 所有 `test result: ok`，无 `FAILED`

---

### Task 5: M3 Acceptance

**前置条件:**
- 构建命令: `cargo build 2>&1 | grep -E "^error"`（应无输出）
- 无需额外数据准备

**端到端验证:**

1. **system prompt 在消息列表最前（核心验证）**
   - `cargo test -p rust-create-agent --lib -- test_system_prompt_is_first 2>&1 | tail -5`
   - 预期: `test result: ok. 1 passed`
   - 失败排查: 检查 Task 1，确认 prepend 位置在 run_before_agent 之后

2. **中间件任意顺序注册不影响 system prompt 位置**
   - `cargo test -p rust-create-agent --lib -- test_system_prompt_order_independent 2>&1 | tail -5`
   - 预期: `test result: ok. 1 passed`
   - 失败排查: 检查 Task 1 中 prepend 是否真的在所有 before_agent 之后

3. **SubAgent 系统提示词仍可正确注入（回归验证）**
   - `cargo test -p rust-agent-middlewares --lib -- test_system_builder_injects_system_message 2>&1 | tail -5`
   - 预期: `test result: ok. 1 passed`
   - 失败排查: 检查 Task 3，确认 with_system_prompt 调用正确

4. **全量测试无回归**
   - `cargo test -p rust-create-agent -p rust-agent-middlewares -p rust-agent-tui 2>&1 | grep -E "FAILED|test result"`
   - 预期: 所有 `test result: ok`，无 `FAILED`
   - 失败排查: 根据失败 crate 对应检查 Task 1-4

> **注意:** Task 5 验收中引用了两个新测试（`test_system_prompt_is_first` 和 `test_system_prompt_order_independent`），
> 这两个测试需在 Task 1 执行时一并写入 `executor.rs` 的 `#[cfg(test)]` 块中。
