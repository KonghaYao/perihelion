# Subagent Skill 预加载 执行计划

**目标:** 子 agent 启动时，根据 agent 定义文件的 `skills` 字段，将指定 skill 全文以 fake `read_file` 工具调用 + 结果消息对注入到 state

**技术栈:** Rust 2021, async-trait, tokio::task::spawn_blocking, rust-agent-middlewares

**设计文档:** ./spec-design.md

---

### Task 1: SkillPreloadMiddleware 实现

**涉及文件:**
- 新建: `rust-agent-middlewares/src/subagent/skill_preload.rs`

**执行步骤:**
- [x] 新建 `SkillPreloadMiddleware` 结构体，持有 `skill_names: Vec<String>` 和 `cwd: String`
  - 复用 `skills::loader::list_skills` 扫描 skills 目录（`~/.claude/skills/` → globalConfig → `{cwd}/.claude/skills/`）
  - 路径解析复用 `SkillsMiddleware::resolve_dirs` 的逻辑（或提取公共函数）
- [x] 实现 `before_agent` 注入逻辑
  - 调用 `spawn_blocking` 包装同步 IO：`list_skills(&dirs)` 扫描元数据，按 `skill_names` 过滤（不区分大小写，找不到静默跳过）
  - 对每个找到的 skill 读取 SKILL.md 全文
  - 构造注入消息序列（逆序 prepend 保证最终顺序为 Human → Ai → Tool...）：
    ```
    prepend Tool × N （逆序：最后一个 skill 先 prepend）
    prepend Ai [ToolUse × N]  — 使用 BaseMessage::ai_from_blocks，自动双写 tool_calls
    prepend Human "（系统：预加载 skill 文件）"
    ```
  - fake ID 格式：`format!("skill_preload_{}", index)`（index 从 0 起）
  - Ai 消息 ContentBlock：`ContentBlock::ToolUse { id, name: "read_file", input: json!({"path": skill.path.to_string_lossy()}) }`
  - Tool 消息：`BaseMessage::tool_result(id, skill_content)`
- [x] 若 `skill_names` 为空或无匹配 skill，`before_agent` 直接返回 `Ok(())`（no-op）
- [x] 添加单元测试（`#[cfg(test)] mod tests`）：
  - `test_no_op_when_empty_names`：skill_names 为空，state 消息数不变
  - `test_inject_single_skill`：匹配 1 个 skill，state 注入 3 条消息（Human + Ai + Tool）
  - `test_inject_multiple_skills`：匹配 N 个，注入 N+2 条（Human + Ai + Tool×N）
  - `test_skip_missing_skill`：部分 skill 不存在，只注入找到的；找不到的静默跳过
  - `test_message_order`：验证注入后消息顺序为 Human[0] → Ai[1] → Tool[2..N]
  - `test_ai_message_has_tool_calls`：Ai 消息的 `tool_calls` 字段与 ContentBlock::ToolUse 数量一致

**检查步骤:**
- [x] 单元测试全部通过
  - `cargo test -p rust-agent-middlewares skill_preload -- --nocapture`
  - 预期: 所有 `skill_preload` 相关测试 PASSED，无 FAILED
- [x] 无编译警告
  - `cargo build -p rust-agent-middlewares 2>&1 | grep -E "^warning|^error"`
  - 预期: 无 `error`，warning 不超过现有基线

---

### Task 2: SubAgentTool 集成与导出

**涉及文件:**
- 修改: `rust-agent-middlewares/src/subagent/mod.rs`
- 修改: `rust-agent-middlewares/src/subagent/tool.rs`
- 修改: `rust-agent-middlewares/src/lib.rs`

**执行步骤:**
- [x] 在 `subagent/mod.rs` 中添加模块声明并重导出
  ```rust
  mod skill_preload;
  pub use skill_preload::SkillPreloadMiddleware;
  ```
- [x] 在 `lib.rs` 的 `pub use subagent::...` 行中加入 `SkillPreloadMiddleware`，并在 `prelude` 模块中同步导出
- [x] 在 `SubAgentTool::invoke` 中，解析 `agent_def` 后注册 `SkillPreloadMiddleware`（仅 `skills` 非空时）
  ```rust
  // 紧接在 SkillsMiddleware 注册之后、TodoMiddleware 之前
  if !agent_def.frontmatter.skills.is_empty() {
      agent_builder = agent_builder.add_middleware(Box::new(
          SkillPreloadMiddleware::new(
              agent_def.frontmatter.skills.clone(),
              &cwd,
          )
      ));
  }
  ```
- [x] 在 `subagent/tool.rs` 的现有测试中补充 `test_skill_preload_registered`：
  - agent.md 含 `skills: ["test-skill"]`，临时目录放 SKILL.md，执行 `invoke`
  - 验证 LLM 收到的第一批消息中包含 "（系统：预加载 skill 文件）"（通过 SystemEchoLLM 回显检查）

**检查步骤:**
- [x] SubAgentTool 相关测试通过
  - `cargo test -p rust-agent-middlewares subagent -- --nocapture`
  - 预期: 所有 `subagent` 测试 PASSED
- [x] `SkillPreloadMiddleware` 可从 `rust_agent_middlewares` 根路径导入
  - `grep -r "SkillPreloadMiddleware" rust-agent-middlewares/src/lib.rs`
  - 预期: 输出含 `SkillPreloadMiddleware`
- [x] 全量测试通过
  - `cargo test -p rust-agent-middlewares`
  - 预期: 无 FAILED

---

### Task 3: Subagent Skill Preload Acceptance

**前置条件:**
- 构建命令: `cargo build -p rust-agent-middlewares`
- 测试工具: `cargo test -p rust-agent-middlewares`

**端到端验证:**

1. **ClaudeAgentFrontmatter 反序列化 skills 字段**
   - `cargo test -p rust-agent-middlewares claude_agent_parser -- --nocapture 2>&1 | grep -E "PASSED|FAILED|skills"`
   - 预期: `skills` 字段测试 PASSED（或相关解析测试无 FAILED）
   - 失败时: 检查 Task 2（`claude_agent_parser.rs` 的 `skills` 字段已存在，需确认 `#[serde(default)]` 生效）
   - [x] ✅ 4 passed; 0 failed

2. **skills 为空时 SkillPreloadMiddleware no-op**
   - `cargo test -p rust-agent-middlewares test_no_op_when_empty_names -- --nocapture`
   - 预期: PASSED，state 消息数为 0
   - 失败时: 检查 Task 1 的空列表 guard 逻辑
   - [x] ✅ 1 passed; 0 failed

3. **单 skill 注入消息顺序正确**
   - `cargo test -p rust-agent-middlewares test_message_order -- --nocapture`
   - 预期: PASSED，messages[0] 为 Human "（系统：预加载 skill 文件）"，messages[1] 为 Ai（has_tool_calls），messages[2] 为 Tool（tool_call_id == "skill_preload_0"）
   - 失败时: 检查 Task 1 中 prepend 逆序逻辑
   - [x] ✅ 1 passed; 0 failed

4. **SubAgentTool 集成：skills 字段触发中间件注册**
   - `cargo test -p rust-agent-middlewares test_skill_preload_registered -- --nocapture`
   - 预期: PASSED，LLM 消息历史中含 "（系统：预加载 skill 文件）"
   - 失败时: 检查 Task 2 中 `SubAgentTool::invoke` 的条件判断和中间件注册顺序
   - [x] ✅ 1 passed; 0 failed
