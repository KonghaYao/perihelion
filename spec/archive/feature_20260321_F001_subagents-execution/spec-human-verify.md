# subagents-execution 人工验收清单

**生成时间:** 2026-03-21 14:00
**关联计划:** [spec-plan.md](./spec-plan.md)
**关联设计:** [spec-design.md](./spec-design.md)

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 全量构建确认无错误: `cargo build 2>&1 | grep -E "^error" | head -5`
- [ ] [AUTO] 确认新增源文件已存在: `test -f rust-agent-middlewares/src/subagent/mod.rs && test -f rust-agent-middlewares/src/subagent/tool.rs && echo "files ok"`

### 测试数据准备

- [ ] [AUTO] 创建临时 agent 定义文件用于集成测试（测试已内置 tempdir，无需手动准备）: `cargo test -p rust-agent-middlewares --lib -- subagent --no-fail-fast 2>&1 | tail -3`

---

## 验收项目

### 场景 1：构建与编译

#### - [x] 1.1 全量构建通过，无编译错误

- **来源:** Task 1 检查步骤 / Task 4 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-middlewares 2>&1 | grep -E "^error"` → 期望: 无任何输出（表示无编译错误）
  2. [A] `cargo build 2>&1 | grep -E "^error"` → 期望: 无任何输出（全 workspace 构建无错误）
- **异常排查:**
  - 如果出现 `error[E0...]`：查看错误消息定位具体文件，通常在 `rust-agent-middlewares/src/subagent/` 或 `rust-create-agent/src/agent/react.rs`
  - 如果出现 `cannot find type`：确认 `use` 导入语句正确

#### - [x] 1.2 文档生成无错误

- **来源:** Task 4 检查步骤
- **操作步骤:**
  1. [A] `cargo doc -p rust-agent-middlewares 2>&1 | grep -E "^error"` → 期望: 无任何输出（文档生成无错误）
- **异常排查:**
  - 如果出现 `error[E0...]`：文档注释中有语法错误，检查 `///` doc 注释格式

---

### 场景 2：ArcToolWrapper 包装层

#### - [x] 2.1 ArcToolWrapper 正确实现 BaseTool，可包装 Arc<dyn BaseTool>

- **来源:** Task 1 检查步骤 / spec-design.md 实现要点
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib 2>&1 | grep -E "FAILED|test result"` → 期望: 输出包含 `test result: ok`，不出现 `FAILED`
  2. [A] `grep -n "pub struct ArcToolWrapper" rust-agent-middlewares/src/tools/mod.rs` → 期望: 找到带行号的匹配行（如 `14:pub struct ArcToolWrapper`）
- **异常排查:**
  - 如果 `ArcToolWrapper` 不存在：检查 `rust-agent-middlewares/src/tools/mod.rs` 文件是否包含该结构体定义
  - 如果测试 FAILED：运行 `cargo test -p rust-agent-middlewares --lib 2>&1` 查看完整失败原因

---

### 场景 3：launch_agent 工具行为

#### - [x] 3.1 工具名称为 "launch_agent"，JSON Schema 包含必需字段 agent_id 和 task

- **来源:** Task 2 检查步骤 / spec-design.md 接口设计
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares -- test_tool_name 2>&1 | grep -E "ok|FAILED"` → 期望: `test subagent::tool::tests::test_tool_name ... ok`
  2. [A] `cargo test -p rust-agent-middlewares -- test_tool_parameters_has_required_fields 2>&1 | grep -E "ok|FAILED"` → 期望: `test subagent::tool::tests::test_tool_parameters_has_required_fields ... ok`
- **异常排查:**
  - 如果 `test_tool_name` FAILED：检查 `tool.rs` 中 `fn name()` 是否返回 `"launch_agent"`
  - 如果 `test_tool_parameters_has_required_fields` FAILED：检查 `fn parameters()` 中 JSON Schema 的 `required` 数组是否包含 `"agent_id"` 和 `"task"`

#### - [x] 3.2 agent 定义文件不存在时，工具返回清晰的错误字符串（而非 Err）

- **来源:** Task 2 检查步骤 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares -- test_tool_agent_not_found 2>&1 | grep -E "ok|FAILED"` → 期望: `test subagent::tool::tests::test_tool_agent_not_found ... ok`
- **异常排查:**
  - 如果 FAILED：检查 `invoke()` 中未找到 agent 文件时是否返回 `Ok(...)` 而非 `Err(...)`
  - 确认错误信息包含"找不到"字样，方便 LLM 理解错误原因

#### - [x] 3.3 tools 字段为空时，子 agent 继承所有父工具，但不包含 launch_agent 自身（防递归）

- **来源:** Task 2 检查步骤 / spec-design.md 实现要点（循环防护）
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares -- test_tool_filter_inherit_all 2>&1 | grep -E "ok|FAILED"` → 期望: `test subagent::tool::tests::test_tool_filter_inherit_all ... ok`
  2. [A] `grep -A5 "launch_agent" rust-agent-middlewares/src/subagent/tool.rs | grep -E "return false|continue"` → 期望: 找到排除 `launch_agent` 的判断逻辑（如 `return false`）
- **异常排查:**
  - 如果 FAILED：检查 `filter_tools()` 方法中是否有 `name == "launch_agent"` 的排除逻辑
  - 如果子 agent 能递归调用 `launch_agent`：这是严重 bug，检查工具过滤逻辑

#### - [x] 3.4 tools 字段有值时，子 agent 仅保留允许列表中的工具

- **来源:** Task 2 检查步骤 / spec-design.md 子 Agent 执行流程
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares -- test_tool_filter_allowlist 2>&1 | grep -E "ok|FAILED"` → 期望: `test subagent::tool::tests::test_tool_filter_allowlist ... ok`
  2. [A] `cargo test -p rust-agent-middlewares -- subagent::tool 2>&1 | grep -E "FAILED"` → 期望: 无任何输出（无失败）
- **异常排查:**
  - 如果 FAILED：检查 `filter_tools()` 中 `allowed_list` 非空时的过滤逻辑，确认使用的是 `allowed_list.iter().any(|n| n == name)` 判断

#### - [x] 3.5 disallowedTools 字段正确排除指定工具

- **来源:** Task 2 检查步骤 / spec-design.md 子 Agent 执行流程
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares -- test_tool_filter_disallow 2>&1 | grep -E "ok|FAILED"` → 期望: `test subagent::tool::tests::test_tool_filter_disallow ... ok`
  2. [A] `grep -n "disallowed" rust-agent-middlewares/src/subagent/tool.rs` → 期望: 找到处理 `disallowed_list` 的代码行
- **异常排查:**
  - 如果 FAILED：检查 `filter_tools()` 中 `disallowed_list.iter().any(|n| n == name)` 的判断是否在允许列表过滤之后执行

#### - [x] 3.6 子 agent 能成功执行并返回结果文本

- **来源:** Task 2 检查步骤 / spec-design.md 验收标准（LLM 调用 launch_agent 时子 agent 能正确加载并执行）
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares -- test_tool_executes_with_valid_agent_file 2>&1 | grep -E "ok|FAILED"` → 期望: `test subagent::tool::tests::test_tool_executes_with_valid_agent_file ... ok`
- **异常排查:**
  - 如果 FAILED：该测试使用 `EchoLLM`（Mock LLM）和临时目录，确认 `invoke()` 中 `agent.execute()` 调用正确，且结果通过 `Ok(output.text)` 返回

---

### 场景 4：SubAgentMiddleware 集成

#### - [x] 4.1 SubAgentMiddleware 的 collect_tools() 向父 agent 注入 launch_agent 工具，且可从 crate 公开 API 访问

- **来源:** Task 3 检查步骤 / Task 4 检查步骤 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares -- test_middleware_collect_tools 2>&1 | grep -E "ok|FAILED"` → 期望: `test subagent::mod::tests::test_middleware_collect_tools ... ok`
  2. [A] `cargo test -p rust-agent-middlewares -- test_launch_agent_tool_in_list 2>&1 | grep -E "ok|FAILED"` → 期望: `test subagent::tool::tests::test_launch_agent_tool_in_list ... ok`
  3. [A] `grep -n "SubAgentMiddleware\|SubAgentTool\|ArcToolWrapper" rust-agent-middlewares/src/lib.rs` → 期望: 找到 `pub use` 行，确认三个类型均已从 crate root 导出
  4. [A] `cargo test -p rust-agent-middlewares --lib 2>&1 | tail -5` → 期望: 输出包含 `test result: ok`，显示总测试数量（应 ≥ 34 个）
- **异常排查:**
  - 如果 `test_middleware_collect_tools` FAILED：检查 `SubAgentMiddleware::collect_tools()` 是否返回包含名称为 `"launch_agent"` 的工具
  - 如果导出未找到：检查 `lib.rs` 中 `pub use subagent::{SubAgentMiddleware, SubAgentTool};` 是否存在

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | 全量构建通过 | 2 | 0 | ✅ | |
| 场景 1 | 1.2 | 文档生成无错误 | 1 | 0 | ✅ | |
| 场景 2 | 2.1 | ArcToolWrapper 正确实现 BaseTool | 2 | 0 | ✅ | |
| 场景 3 | 3.1 | 工具名称与参数 Schema 正确 | 2 | 0 | ✅ | |
| 场景 3 | 3.2 | agent 文件不存在时返回清晰错误 | 1 | 0 | ✅ | |
| 场景 3 | 3.3 | tools 为空时继承父工具并排除 launch_agent | 2 | 0 | ✅ | |
| 场景 3 | 3.4 | tools 有值时仅保留允许工具 | 2 | 0 | ✅ | |
| 场景 3 | 3.5 | disallowedTools 正确排除工具 | 2 | 0 | ✅ | |
| 场景 3 | 3.6 | 子 agent 执行成功并返回结果 | 1 | 0 | ✅ | |
| 场景 4 | 4.1 | collect_tools 注入 launch_agent，公开 API 可访问 | 4 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
