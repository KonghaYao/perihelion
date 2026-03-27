# Subagent Skill 预加载 人工验收清单

**生成时间:** 2026-03-26 今日
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

> 所有验收项均可自动化验证，无需人类参与。

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链: `rustc --version`
- [ ] [AUTO] 编译 rust-agent-middlewares: `cargo build -p rust-agent-middlewares`

### 说明

本功能为纯 Rust 库逻辑，不涉及 UI 或外部服务，所有验收通过单元测试和集成测试完成。

---

## 验收项目

### 场景 1：数据模型 — AgentFrontmatter skills 字段

#### - [x] 1.1 ClaudeAgentFrontmatter 正确反序列化 skills 字段
- **来源:** Task 3 端到端验证场景 1，spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares claude_agent_parser -- --nocapture 2>&1 | tail -5` → 期望: `test result: ok. N passed; 0 failed`
  2. [A] `grep -n "skills" rust-agent-middlewares/src/claude_agent_parser.rs` → 期望: 输出含 `pub skills: Vec<String>` 和 `#[serde(default)]`
- **异常排查:**
  - 如果反序列化失败: 确认 `claude_agent_parser.rs` 中 `ClaudeAgentFrontmatter` 含 `#[serde(default)] pub skills: Vec<String>`

---

### 场景 2：SkillPreloadMiddleware 核心逻辑

#### - [x] 2.1 空 skill_names 时 before_agent 为 no-op（不修改 state）
- **来源:** Task 1 单元测试，spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares test_no_op_when_empty_names -- --nocapture 2>&1 | grep -E "ok|FAILED"` → 期望: 输出 `test_no_op_when_empty_names ... ok`
- **异常排查:**
  - 如果测试失败: 检查 `skill_preload.rs` 中 `before_agent` 开头的 `if self.skill_names.is_empty() { return Ok(()); }`

#### - [x] 2.2 单个 skill 注入 3 条消息（Human + Ai + Tool）
- **来源:** Task 1 单元测试
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares test_inject_single_skill -- --nocapture 2>&1 | grep -E "ok|FAILED"` → 期望: `test_inject_single_skill ... ok`
- **异常排查:**
  - 如果消息数不为 3: 检查 `before_agent` 注入逻辑中的 prepend 调用次数

#### - [x] 2.3 多个 skill 注入 N+2 条消息（Human + Ai + Tool×N）
- **来源:** Task 1 单元测试
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares test_inject_multiple_skills -- --nocapture 2>&1 | grep -E "ok|FAILED"` → 期望: `test_inject_multiple_skills ... ok`
- **异常排查:**
  - 如果消息数错误: 检查 `skill_preload.rs` 中 Tool 消息的逆序 prepend 循环

#### - [x] 2.4 找不到的 skill 名称静默跳过，不影响其余 skill 注入
- **来源:** Task 1 单元测试，spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares test_skip_missing_skill -- --nocapture 2>&1 | grep -E "ok|FAILED"` → 期望: `test_skip_missing_skill ... ok`
  2. [A] `cargo test -p rust-agent-middlewares test_no_op_when_all_skills_missing -- --nocapture 2>&1 | grep -E "ok|FAILED"` → 期望: `test_no_op_when_all_skills_missing ... ok`
- **异常排查:**
  - 如果跳过逻辑有误: 检查 `skill_preload.rs` 中 `filter_map` 过滤逻辑

#### - [x] 2.5 注入消息顺序正确：Human[0] → Ai[1] → Tool[2..N]
- **来源:** Task 1 单元测试，Task 3 端到端验证场景 3，spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares test_message_order -- --nocapture 2>&1 | grep -E "ok|FAILED"` → 期望: `test_message_order ... ok`
- **异常排查:**
  - 如果顺序错误: 检查 `skill_preload.rs` 中 prepend 顺序（逆序 prepend Tool，再 prepend Ai，最后 prepend Human）

#### - [x] 2.6 Ai 消息的 tool_calls 与 ContentBlock::ToolUse 数量和 ID 一致
- **来源:** Task 1 单元测试，spec-design.md 实现要点
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares test_ai_message_has_tool_calls -- --nocapture 2>&1 | grep -E "ok|FAILED"` → 期望: `test_ai_message_has_tool_calls ... ok`
  2. [A] `cargo test -p rust-agent-middlewares test_tool_call_ids_match -- --nocapture 2>&1 | grep -E "ok|FAILED"` → 期望: `test_tool_call_ids_match ... ok`
- **异常排查:**
  - 如果 tool_calls 为空: 确认使用了 `BaseMessage::ai_from_blocks(...)` 而非 `BaseMessage::ai(...)`
  - 如果 ID 不匹配: 检查 fake ID 格式 `format!("skill_preload_{}", index)` 在 Ai 和 Tool 消息中一致

---

### 场景 3：SubAgentTool 集成与导出

#### - [x] 3.1 SkillPreloadMiddleware 可从 rust_agent_middlewares 根路径导入
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep "SkillPreloadMiddleware" rust-agent-middlewares/src/lib.rs` → 期望: 输出含 `pub use subagent::{...SkillPreloadMiddleware...}`
  2. [A] `grep "SkillPreloadMiddleware" rust-agent-middlewares/src/lib.rs | grep "prelude"` → 期望: 输出含 prelude 中的导出行
- **异常排查:**
  - 如果未导出: 检查 `lib.rs` 的 `pub use subagent::` 行是否包含 `SkillPreloadMiddleware`

#### - [x] 3.2 SubAgentTool::invoke 当 frontmatter.skills 非空时正确注册 SkillPreloadMiddleware
- **来源:** Task 2 集成测试，Task 3 端到端验证场景 4，spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares test_skill_preload_registered -- --nocapture 2>&1 | grep -E "ok|FAILED"` → 期望: `test_skill_preload_registered ... ok`
  2. [A] `grep -A5 "frontmatter.skills.is_empty" rust-agent-middlewares/src/subagent/tool.rs` → 期望: 输出含 `SkillPreloadMiddleware::new` 的注册代码
- **异常排查:**
  - 如果 LLM 未收到预加载消息: 检查 `tool.rs` 中 `if !agent_def.frontmatter.skills.is_empty()` 条件及 SkillPreloadMiddleware 注册位置

#### - [x] 3.3 全量测试无回归（所有 65 个测试通过）
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares 2>&1 | grep "test result"` → 期望: 所有 `test result` 行均含 `0 failed`
  2. [A] `cargo test -p rust-agent-middlewares 2>&1 | grep "FAILED"` → 期望: 无输出（即无 FAILED）
- **异常排查:**
  - 如果有测试失败: 运行 `cargo test -p rust-agent-middlewares -- --nocapture 2>&1 | grep -A5 "FAILED"` 查看详情

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 数据模型 | 1.1 | AgentFrontmatter skills 字段反序列化 | 2 | 0 | ✅ | |
| 中间件核心逻辑 | 2.1 | 空 skill_names no-op | 1 | 0 | ✅ | |
| 中间件核心逻辑 | 2.2 | 单 skill 注入 3 条消息 | 1 | 0 | ✅ | |
| 中间件核心逻辑 | 2.3 | 多 skill 注入 N+2 条消息 | 1 | 0 | ✅ | |
| 中间件核心逻辑 | 2.4 | 找不到 skill 静默跳过 | 2 | 0 | ✅ | |
| 中间件核心逻辑 | 2.5 | 消息顺序 Human→Ai→Tool | 1 | 0 | ✅ | |
| 中间件核心逻辑 | 2.6 | Ai tool_calls 与 ToolUse 一致 | 2 | 0 | ✅ | |
| SubAgentTool 集成 | 3.1 | SkillPreloadMiddleware 导出正确 | 2 | 0 | ✅ | |
| SubAgentTool 集成 | 3.2 | skills 非空时注册中间件 | 2 | 0 | ✅ | |
| SubAgentTool 集成 | 3.3 | 全量测试无回归 | 2 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
