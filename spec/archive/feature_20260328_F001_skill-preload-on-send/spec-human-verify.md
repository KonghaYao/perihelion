# skill-preload-on-send 人工验收清单

**生成时间:** 2026-03-28 00:00
**关联计划:** [spec-plan.md](./spec-plan.md)
**关联设计:** [spec-design.md](./spec-design.md)

> 所有验收项均可自动化验证，无需人类参与。

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 编译 rust-agent-tui（确认无编译错误）: `cargo build -p rust-agent-tui 2>&1 | grep "^error"`
- [ ] [AUTO] 运行 SkillPreloadMiddleware 单元测试（确认已有测试全部通过）: `cargo test -p rust-agent-middlewares --lib -- skill_preload 2>&1 | grep "test result"`

### 测试数据准备
- [ ] [AUTO] 确认磁盘上存在至少一个 skill（用于集成验证）: `ls ~/.claude/skills/ 2>/dev/null | head -5`

---

## 验收项目

### 场景 1：代码结构验证

#### - [x] 1.1 编译通过无错误

- **来源:** Task 1/2 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | grep "^error"` → 期望: 无输出（无编译错误）
- **异常排查:**
  - 如果有 `missing field` 错误: 检查 `rust-agent-tui/src/app/agent_ops.rs` 中 `AgentRunConfig` 初始化是否含 `preload_skills` 字段

#### - [x] 1.2 中间件链顺序正确

- **来源:** Task 1 检查步骤 + spec-design.md 约束
- **操作步骤:**
  1. [A] `grep -n "SkillsMiddleware\|SkillPreloadMiddleware\|FilesystemMiddleware" rust-agent-tui/src/app/agent.rs | grep add_middleware` → 期望: 三行按顺序出现，SkillPreloadMiddleware 行号介于 SkillsMiddleware 和 FilesystemMiddleware 之间
  2. [A] `grep -A3 "SkillsMiddleware::new" rust-agent-tui/src/app/agent.rs | grep "SkillPreload"` → 期望: 输出含 `SkillPreloadMiddleware`（紧随其后）
- **异常排查:**
  - 如果顺序不正确: 检查 `rust-agent-tui/src/app/agent.rs` 中 `ReActAgent` 构建链，参照 spec-design.md 中的中间件执行顺序表

#### - [x] 1.3 SkillPreloadMiddleware 通过 prelude 引入无需额外 import

- **来源:** Task 1 实现要点
- **操作步骤:**
  1. [A] `grep "SkillPreloadMiddleware" rust-agent-tui/src/app/agent.rs` → 期望: 仅在 `add_middleware` 行出现，无独立 `use` 语句（已通过 `use rust_agent_middlewares::prelude::*` 引入）
- **异常排查:**
  - 如果编译报找不到 `SkillPreloadMiddleware`: 检查 `rust-agent-middlewares/src/lib.rs` 的 `prelude` 模块是否包含 `SkillPreloadMiddleware`

---

### 场景 2：解析逻辑验证

#### - [x] 2.1 单个 `#skill-name` 正确解析

- **来源:** Task 2 检查步骤 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -A15 "解析消息中的" rust-agent-tui/src/app/agent_ops.rs | grep "starts_with\|split_whitespace\|trim_start_matches"` → 期望: 至少 3 行包含这些方法调用（解析逻辑存在）
  2. [A] `grep "preload_skills" rust-agent-tui/src/app/agent_ops.rs` → 期望: 至少 2 行匹配（定义行 + 填充 AgentRunConfig 的行）
- **异常排查:**
  - 如果找不到解析逻辑: 在 `submit_message` 中 AgentInput 构建之后、`ensure_thread_id()` 之前添加解析代码块

#### - [x] 2.2 多个 `#skill-name` 全部被解析

- **来源:** Task 2 检查步骤 + spec-design.md 多 skill 示例
- **操作步骤:**
  1. [A] `grep -A8 "解析消息中的" rust-agent-tui/src/app/agent_ops.rs | grep "collect"` → 期望: 含 `.collect()` 的行（收集为 Vec）
  2. [A] `cargo test -p rust-agent-middlewares --lib -- test_inject_multiple 2>&1 | grep "ok"` → 期望: `test_inject_multiple_skills ... ok`
- **异常排查:**
  - 如果多 skill 未全部注入: 检查 `split_whitespace()` 迭代和 `collect::<Vec<String>>()` 是否正确

#### - [x] 2.3 普通消息 `preload_skills` 为空

- **来源:** Task 2 检查步骤 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -B2 -A8 "解析消息中的" rust-agent-tui/src/app/agent_ops.rs | grep "filter.*starts_with"` → 期望: 含 `starts_with('#')` 的 filter 调用（无 `#` 时过滤掉所有 token）
  2. [A] `cargo test -p rust-agent-middlewares --lib -- test_no_op_when_empty_names 2>&1 | grep "ok"` → 期望: `test_no_op_when_empty_names ... ok`
- **异常排查:**
  - 如果普通消息触发了预加载: 检查 filter 条件 `starts_with('#') && len() > 1` 是否完整

---

### 场景 3：SkillPreloadMiddleware 注入行为

#### - [x] 3.1 skill 不存在时静默跳过

- **来源:** Task 3 End-to-end + spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- test_skip_missing_skill 2>&1 | grep "ok"` → 期望: `test_skip_missing_skill ... ok`
  2. [A] `cargo test -p rust-agent-middlewares --lib -- test_no_op_when_all_skills_missing 2>&1 | grep "ok"` → 期望: `test_no_op_when_all_skills_missing ... ok`
- **异常排查:**
  - 如果测试失败: `SkillPreloadMiddleware` 本身无需修改，检查测试文件 `rust-agent-middlewares/src/subagent/skill_preload.rs`

#### - [x] 3.2 单个 skill 注入消息结构正确（Human + Ai[ToolUse] + Tool[ToolResult]）

- **来源:** Task 3 End-to-end + spec-design.md 数据流
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- test_inject_single_skill 2>&1 | grep "ok"` → 期望: `test_inject_single_skill ... ok`（注入 3 条消息：Human + Ai + Tool）
- **异常排查:**
  - 如果消息数量不对: 检查 `skill_preload.rs` 中 `prepend_message` 调用链

#### - [x] 3.3 多个 skill 注入消息结构正确（Human + Ai[ToolUse×N] + Tool×N）

- **来源:** Task 3 End-to-end + spec-design.md 多 skill 示例
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- test_inject_multiple_skills 2>&1 | grep "ok"` → 期望: `test_inject_multiple_skills ... ok`（3 个 skill 注入 5 条消息：Human + Ai + Tool×3）
- **异常排查:**
  - 如果消息顺序错误: 检查 `skill_preload.rs` 中逆序 prepend 逻辑（`enumerate().rev()`）

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 代码结构验证 | 1.1 | 编译通过无错误 | 1 | 0 | ✅ | |
| 代码结构验证 | 1.2 | 中间件链顺序正确 | 2 | 0 | ✅ | |
| 代码结构验证 | 1.3 | SkillPreloadMiddleware 通过 prelude 引入 | 1 | 0 | ✅ | |
| 解析逻辑验证 | 2.1 | 单个 #skill-name 正确解析 | 2 | 0 | ✅ | |
| 解析逻辑验证 | 2.2 | 多个 #skill-name 全部被解析 | 2 | 0 | ✅ | |
| 解析逻辑验证 | 2.3 | 普通消息 preload_skills 为空 | 2 | 0 | ✅ | |
| SkillPreloadMiddleware 行为 | 3.1 | skill 不存在时静默跳过 | 2 | 0 | ✅ | |
| SkillPreloadMiddleware 行为 | 3.2 | 单个 skill 注入消息结构正确 | 1 | 0 | ✅ | |
| SkillPreloadMiddleware 行为 | 3.3 | 多个 skill 注入消息结构正确 | 1 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
