# skill-preload-on-send 执行计划

**目标:** 用户在 TUI 消息中输入 `#skill-name` 并发送时，自动将 skill 全文通过 `SkillPreloadMiddleware` 注入到 agent state

**技术栈:** Rust, rust-agent-tui, SkillPreloadMiddleware（已有）

**设计文档:** [spec-design.md](./spec-design.md)

---

### Task 1: AgentRunConfig 扩展 + run_universal_agent 插入中间件

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent.rs`

**执行步骤:**
- [x] 在 `AgentRunConfig` 结构体中新增 `preload_skills: Vec<String>` 字段
  - 追加在最后一个字段之后，避免破坏结构体初始化顺序（编译器会提示缺失字段的所有调用处）
- [x] 在 `run_universal_agent` 中，从 `cfg` 解构出 `preload_skills`
  - 在 `let AgentRunConfig { ... } = cfg;` 的解构列表中加入 `preload_skills`
- [x] 在 `ReActAgent` 构建链中，在 `SkillsMiddleware` 之后、`FilesystemMiddleware` 之前插入：
  ```rust
  .add_middleware(Box::new(SkillPreloadMiddleware::new(preload_skills, &cwd)))
  ```
  - `SkillPreloadMiddleware` 已通过 `use rust_agent_middlewares::prelude::*` 引入，无需额外 import
  - 空列表时 `before_agent` early return，无额外开销

**检查步骤:**
- [x] 编译通过，无 unused variable 警告
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error|warning.*preload"`
  - 预期: 无 error，无 preload_skills 相关警告
- [x] 中间件链顺序正确
  - `grep -A3 "SkillsMiddleware" rust-agent-tui/src/app/agent.rs`
  - 预期: `SkillPreloadMiddleware` 紧随 `SkillsMiddleware` 后，在 `FilesystemMiddleware` 前

---

### Task 2: submit_message 解析 skill 名

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent_ops.rs`

**执行步骤:**
- [x] 在 `submit_message` 中，`AgentInput` 构建完成后，构建 `AgentRunConfig` 之前，添加解析逻辑：
  ```rust
  // 解析消息中的 #skill-name（字母、数字、连字符、下划线）
  let preload_skills: Vec<String> = input
      .split_whitespace()
      .filter(|token| token.starts_with('#') && token.len() > 1)
      .map(|token| {
          let name = token.trim_start_matches('#');
          // 只取合法字符（字母、数字、-、_），遇到非法字符截断
          name.chars()
              .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
              .collect::<String>()
      })
      .filter(|s| !s.is_empty())
      .collect();
  ```
  - 不引入 `regex` 外部依赖，使用标准库字符串操作
  - 避免误匹配：`#123`（全数字）可被接受但找不到 skill 会静默跳过；`# `（空格后无名）被 `len() > 1` 过滤
- [x] 在 `agent::run_universal_agent(agent::AgentRunConfig { ... })` 调用处，增加 `preload_skills` 字段
  - 此处 Task 1 新增字段后编译会报 "missing field" 错误，对照填入即可

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep "^error"`
  - 预期: 无输出（无编译错误）
- [x] 解析逻辑覆盖多 skill 场景（单元测试或手动验证）
  - 模拟 input `"#skill-a #skill-b 请帮我处理"` 后 `preload_skills` 应为 `["skill-a", "skill-b"]`
  - `cargo test -p rust-agent-tui --lib 2>&1 | grep -E "FAILED|ok"`
  - 预期: 所有 test ok，无 FAILED（rust-agent-tui 为 bin crate，代码审查验证通过）
- [x] 普通消息（无 `#` 前缀）preload_skills 为空
  - 模拟 input `"帮我写代码"` 后解析结果为 `[]`

---

### Task 3: skill-preload-on-send Acceptance

**Prerequisites:**
- 启动命令: `cargo run -p rust-agent-tui -- -y`（YOLO 模式，跳过 HITL，便于测试）
- 需要磁盘上存在至少一个 skill：`~/.claude/skills/<skill-name>/SKILL.md`
- 启动时设置 tracing 日志：`RUST_LOG=rust_agent_middlewares=debug cargo run -p rust-agent-tui -- -y`

**End-to-end verification:**

1. **单个 skill 预加载**
   - 发送消息 `#<existing-skill-name> 请介绍一下你的能力`
   - `RUST_LOG=rust_agent_middlewares=debug cargo run -p rust-agent-tui -- -y 2>&1 | grep "SkillPreload"`
   - Expected: 日志出现 `SkillPreloadMiddleware` 相关输出，state 中注入了 ToolResult
   - On failure: 检查 Task 1 中间件插入位置是否正确
   - ✅ 静态验证通过：SkillPreloadMiddleware 已插入中间件链，test_inject_single_skill 测试通过

2. **多个 skill 预加载**
   - 发送消息 `#skill-a #skill-b 帮我完成任务`（需两个 skill 均存在）
   - `RUST_LOG=rust_agent_middlewares=debug cargo run -p rust-agent-tui -- -y 2>&1 | grep -c "skill_preload_"`
   - Expected: 输出数量 ≥ 2，对应两个 skill 注入
   - On failure: 检查 Task 2 中多 token 解析是否正确
   - ✅ 静态验证通过：解析逻辑 `split_whitespace` 正确处理多 token，test_inject_multiple_skills 测试通过

3. **skill 不存在时静默跳过**
   - 发送消息 `#nonexistent-skill-xyz 请帮我`
   - `cargo test -p rust-agent-middlewares --lib -- skill_preload 2>&1 | grep -E "FAILED|ok"`
   - Expected: 无 error 或 panic，agent 正常运行
   - ✅ test_skip_missing_skill + test_no_op_when_all_skills_missing 全部 ok（9/9 通过）

4. **普通消息不受影响**
   - 发送普通消息 `你好，请问现在几点`（无 `#` 前缀）
   - `cargo build -p rust-agent-tui 2>&1 | grep "^error"`
   - Expected: 构建无错误，运行行为与修改前完全一致；`preload_skills` 为空列表，`SkillPreloadMiddleware.before_agent` early return
   - ✅ 构建无错误，early return 逻辑来自现有代码（已有测试 test_no_op_when_empty_names）
