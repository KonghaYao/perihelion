# subagent-middleware-injection 执行计划

**目标:** 在 `SubAgentTool::invoke` 中补全三个缺失中间件（AgentsMdMiddleware、SkillsMiddleware、TodoMiddleware），使子 agent 上下文与父 agent 一致

**技术栈:** Rust 2021、tokio mpsc、rust-agent-middlewares 内部中间件

**设计文档:** ./spec-design.md

---

### Task 1: 补全子 agent 中间件注入

**涉及文件:**
- 修改: `rust-agent-middlewares/src/subagent/tool.rs`

**执行步骤:**
- [x] 在文件顶部新增 4 个 import
  - 新增：`use crate::agents_md::AgentsMdMiddleware;`
  - 新增：`use crate::middleware::todo::TodoMiddleware;`
  - 新增：`use crate::skills::SkillsMiddleware;`
  - 新增：`use tokio::sync::mpsc;`
- [x] 在 `invoke` 方法中，`let mut agent_builder = ReActAgent::new(llm).max_iterations(max_iterations);` 之后，`PrependSystemMiddleware` 注册代码之前，插入三个中间件注册
  - 注册顺序严格按照：`AgentsMdMiddleware` → `SkillsMiddleware` → `TodoMiddleware`，确保 `before_agent` prepend 顺序正确
  - `TodoMiddleware` 创建独立 channel：`let (tx, _rx) = mpsc::channel(8);`，`_rx` 立即丢弃
  - `SkillsMiddleware` 需调用 `.with_global_config()` 加载 `~/.zen-code/settings.json` 中的 skills 目录
  ```rust
  agent_builder = agent_builder
      .add_middleware(Box::new(AgentsMdMiddleware::new()))
      .add_middleware(Box::new(SkillsMiddleware::new().with_global_config()))
      .add_middleware(Box::new(TodoMiddleware::new({
          let (tx, _rx) = mpsc::channel(8);
          tx
      })));
  ```

**检查步骤:**
- [x] 编译通过，无错误无警告
  - `cargo build -p rust-agent-middlewares 2>&1 | grep -E "^error"`
  - 预期: 无输出（无编译错误）
- [x] 现有子 agent 测试全部通过
  - `cargo test -p rust-agent-middlewares -- subagent 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok`，`0 failed`
- [x] 全量测试无回归
  - `cargo test -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 输出包含 `test result: ok`，`0 failed`

---

### Task 2: subagent-middleware-injection Acceptance

**Prerequisites:**
- 编译环境: `cargo build -p rust-agent-middlewares`
- 无需额外环境或服务

**End-to-end verification:**

1. [x] AGENTS.md 内容被注入子 agent 上下文
   - `cargo test -p rust-agent-middlewares -- test_tool_executes_with_valid_agent_file --nocapture 2>&1 | tail -5`
   - Expected: 测试通过（此测试验证子 agent 能正常执行，不崩溃）
   - On failure: check Task 1 AgentsMdMiddleware import 是否正确

2. [x] 新增的三个中间件不破坏现有工具过滤逻辑
   - `cargo test -p rust-agent-middlewares -- test_tool_filter 2>&1 | grep -E "ok|FAILED"`
   - Expected: 所有 `test_tool_filter_*` 测试均显示 `ok`
   - On failure: check Task 1 中间件注册位置是否在 filter_tools 调用之后

3. [x] 全量测试无回归（含 subagent、filesystem、terminal、hitl、skills 等模块）
   - `cargo test -p rust-agent-middlewares 2>&1 | grep -E "test result"`
   - Expected: `test result: ok. N passed; 0 failed`
   - On failure: check Task 1 是否引入了意外的 import 冲突或类型错误
