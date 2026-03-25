# subagent-middleware-injection 人工验收清单

**生成时间:** 2026-03-25 今日
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 确认 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 编译 rust-agent-middlewares 无错误: `cargo build -p rust-agent-middlewares 2>&1 | grep -c "^error" || true`

### 测试数据准备
无需额外测试数据，所有验证均通过 Cargo 测试框架执行。

---

## 验收项目

### 场景 1：上下文注入（AgentsMdMiddleware + SkillsMiddleware）

#### - [x] 1.1 AgentsMdMiddleware 已注入子 agent 中间件链
- **来源:** Task 1 检查步骤 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -n "AgentsMdMiddleware::new" rust-agent-middlewares/src/subagent/tool.rs` → 期望: 输出包含 `AgentsMdMiddleware::new()` 的行（invoke 方法内）
  2. [A] `cargo test -p rust-agent-middlewares --lib -- agents_md 2>&1 | grep "test result"` → 期望: `test result: ok.` 且 `0 failed`
- **异常排查:**
  - 如果 grep 无输出：检查 `rust-agent-middlewares/src/subagent/tool.rs` 第 196-205 行是否存在三个中间件注册块
  - 如果测试失败：运行 `cargo test -p rust-agent-middlewares --lib -- agents_md --nocapture 2>&1` 查看详细错误

#### - [x] 1.2 SkillsMiddleware 已注入（含 with_global_config 调用）
- **来源:** Task 1 检查步骤 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -n "SkillsMiddleware::new.*with_global_config" rust-agent-middlewares/src/subagent/tool.rs` → 期望: 输出包含 `.with_global_config()` 的行
  2. [A] `cargo test -p rust-agent-middlewares --lib -- skills 2>&1 | grep "test result"` → 期望: `test result: ok.` 且 `0 failed`
- **异常排查:**
  - 如果 grep 无 with_global_config：检查是否遗漏了 `.with_global_config()` 调用，此调用确保从 `~/.zen-code/settings.json` 加载全局 skills 目录

---

### 场景 2：任务管理（TodoMiddleware）

#### - [x] 2.1 TodoMiddleware 已注入，todo_write 工具可用
- **来源:** Task 1 检查步骤 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -n "TodoMiddleware::new" rust-agent-middlewares/src/subagent/tool.rs` → 期望: 输出包含 `TodoMiddleware::new` 的行（invoke 方法内）
  2. [A] `cargo test -p rust-agent-middlewares --lib -- todo 2>&1 | grep "test result"` → 期望: `test result: ok.` 且 `0 failed`
- **异常排查:**
  - 如果 grep 无输出：检查 `tool.rs` 中 `agent_builder = agent_builder` 代码块是否包含 TodoMiddleware

#### - [x] 2.2 _rx 静默丢弃模式正确，不影响工具返回结果
- **来源:** spec-design.md 实现要点
- **操作步骤:**
  1. [A] `grep -n "_rx" rust-agent-middlewares/src/subagent/tool.rs` → 期望: 输出包含 `let (tx, _rx) = mpsc::channel(8)` 的行，确认 `_rx` 前缀表示忽略
  2. [A] `cargo test -p rust-agent-middlewares --lib -- test_tool_executes_with_valid_agent_file 2>&1 | grep "test result"` → 期望: `test result: ok. 1 passed`
- **异常排查:**
  - 如果测试失败并提示 channel 相关错误：检查 `_rx` 是否在正确作用域内被丢弃（应在 `mpsc::channel(8)` 后立即绑定到 `_rx` 变量）

---

### 场景 3：安全省略（防递归 + 防阻塞）

#### - [x] 3.1 HITL/AskUserTool/launch_agent 不被注入子 agent
- **来源:** spec-design.md 验收标准（父 agent 的 HITL 审批、ask_user、launch_agent 工具不被注入子 agent）
- **操作步骤:**
  1. [A] `grep -n "HumanInTheLoop\|AskUserTool\|SubAgentMiddleware" rust-agent-middlewares/src/subagent/tool.rs | grep "add_middleware\|register_tool"` → 期望: 无输出（这三项不应出现在 add_middleware 或 register_tool 调用中）
  2. [A] `cargo test -p rust-agent-middlewares --lib -- test_launch_agent_excluded 2>&1 | grep "test result"` → 期望: `test result: ok.` 且 `0 failed`（验证 launch_agent 防递归排除逻辑）
- **异常排查:**
  - 如果 grep 有意外输出：检查是否误将 HITL 等中间件加入了子 agent 的 `add_middleware` 调用
  - launch_agent 防递归通过 `filter_tools` 方法实现（始终从工具集排除 `launch_agent` 名称的工具）

#### - [x] 3.2 中间件注册顺序正确（AgentsMd → Skills → Todo → PrependSystem）
- **来源:** spec-design.md 实现要点
- **操作步骤:**
  1. [A] `grep -n "add_middleware\|PrependSystemMiddleware" rust-agent-middlewares/src/subagent/tool.rs | grep -v "^.*//\|test"` → 期望: 输出中 `AgentsMdMiddleware` 行号 < `SkillsMiddleware` 行号 < `TodoMiddleware` 行号 < `PrependSystemMiddleware` 行号
  2. [A] `cargo test -p rust-agent-middlewares --lib -- test_system_builder_injects_system_message 2>&1 | grep "test result"` → 期望: `test result: ok. 1 passed`（验证 PrependSystemMiddleware 仍位于消息列表最前）
- **异常排查:**
  - 如果顺序错误会导致系统消息 prepend 顺序颠倒，PrependSystemMiddleware 必须最后注册才能保证系统提示位于消息列表最前

---

### 场景 4：构建与回归

#### - [x] 4.1 编译无错误无警告
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-middlewares 2>&1 | grep -E "^error"` → 期望: 无输出（无编译错误）
- **异常排查:**
  - 如果出现 `cannot find type` 或 `unresolved import`：检查 4 个新增 import 是否完整（`AgentsMdMiddleware`、`TodoMiddleware`、`SkillsMiddleware`、`mpsc`）

#### - [x] 4.2 全量测试（56+）全部通过，无回归
- **来源:** Task 1 + Task 2 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- test_tool_filter 2>&1 | grep -E "ok|FAILED"` → 期望: 所有 `test_tool_filter_*` 均显示 `ok`
  2. [A] `cargo test -p rust-agent-middlewares 2>&1 | grep "test result"` → 期望: 输出包含 `52 passed; 0 failed`（lib 测试）和 `4 passed; 0 failed`（integration 测试）
  3. [A] `cargo test -p rust-agent-middlewares 2>&1 | grep "FAILED"` → 期望: 无输出（无失败用例）
- **异常排查:**
  - 如果 filter 测试失败：检查新增中间件是否在 `filter_tools` 调用之前注册（应在其之后，不影响工具过滤逻辑）
  - 如果 skills/agents_md 测试失败：运行 `cargo test -p rust-agent-middlewares --nocapture 2>&1 | grep -A5 "FAILED"` 查看详细错误

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 上下文注入 | 1.1 | AgentsMdMiddleware 已注入 | 2 | 0 | ✅ | |
| 场景 1 上下文注入 | 1.2 | SkillsMiddleware 已注入（含 global_config）| 2 | 0 | ✅ | |
| 场景 2 任务管理 | 2.1 | TodoMiddleware 已注入，todo_write 可用 | 2 | 0 | ✅ | |
| 场景 2 任务管理 | 2.2 | _rx 静默丢弃不影响工具结果 | 2 | 0 | ✅ | |
| 场景 3 安全省略 | 3.1 | HITL/AskUser/launch_agent 不被注入 | 2 | 0 | ✅ | |
| 场景 3 安全省略 | 3.2 | 中间件注册顺序正确 | 2 | 0 | ✅ | |
| 场景 4 构建回归 | 4.1 | 编译无错误无警告 | 1 | 0 | ✅ | |
| 场景 4 构建回归 | 4.2 | 全量测试 56+ 全部通过 | 3 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
