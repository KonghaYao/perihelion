# 系统提示词重构 执行计划

**目标:** 将系统提示词从单体文件拆分为语义独立的 `.md` 段落，建立 Feature-gated 条件注入机制，同步 claude-code 工具提示词质量。

**技术栈:** Rust 2021, include_str! (编译时嵌入), serde_json (参数 schema)

**设计文档:** spec/feature_20260430_F001_system-prompt-restructure/spec-design.md

## 改动总览

- 本次改动涉及 3 个模块：`rust-agent-tui/src/prompt.rs`（系统提示词重构）、`rust-agent-tui/prompts/sections/`（12 个新段落文件）、`rust-agent-middlewares/src/`（9 个工具 description 扩展）。按功能分为 6 个 Task（Task 0 环境准备 → Task 1 静态段落迁移 → Task 2 Feature-gated 机制 → Task 3 工具提示词扩展 → Task 4 清理 → Task 5 验收）
- Task 依赖链：Task 1 建立段落目录和加载框架（同时删除旧常量）→ Task 2 在其上添加条件注入和 PromptFeatures → Task 3 独立扩展工具 description → Task 4 删除旧文件并简化 None 分支 → Task 5 功能验收
- 关键决策：Task 1 保持 `build_system_prompt()` 签名不变直到 Task 2 才引入 `PromptFeatures`；Task 1 的 None 分支保持与旧 `default.md` 行为一致，Task 4 简化为空覆盖块（默认语气已内嵌在段落中）；工具 description 使用 `r#"..."#` 原始字符串常量，不引入新依赖

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**

- [ ] 验证 Rust 工具链可用
  - `rustc --version && cargo --version`
  - 预期: 输出 Rust 版本号和 Cargo 版本号

- [ ] 验证项目可编译
  - `cargo build 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error

- [ ] 验证现有测试通过
  - `cargo test 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"，0 failures

**检查步骤:**

- [ ] 构建命令执行成功
  - `cargo build 2>&1 | grep -c 'error'`
  - 预期: 输出 0

- [ ] 测试命令可用
  - `cargo test --no-run 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error

---

### Task 1: 静态提示词段落迁移

**背景:**
当前系统提示词由 `system.md` 模板 + `default.md` 默认覆盖块组成，通过 `{{agent_overrides}}` 占位符拼接。模板中混合了身份定义、行为规范、环境信息等不同语义的内容，维护成本高。本 Task 将其拆分为 8 个独立的 `.md` 段落文件，每个文件职责单一，使用 `include_str!` 编译时嵌入，消除模板占位符机制。本 Task 同时删除旧常量（`SYSTEM_PROMPT_TEMPLATE`、`SYSTEM_PROMPT_DEFAULT_AGENT`），但保留旧文件 `system.md` 和 `default.md` 在磁盘上（Task 4 最终删除）。Task 2 的 Feature-gated 段落将直接追加到同一目录下，依赖本 Task 建立的加载框架。

**涉及文件:**
- 新建: `rust-agent-tui/prompts/sections/01_intro.md`
- 新建: `rust-agent-tui/prompts/sections/02_system.md`
- 新建: `rust-agent-tui/prompts/sections/03_doing_tasks.md`
- 新建: `rust-agent-tui/prompts/sections/04_actions.md`
- 新建: `rust-agent-tui/prompts/sections/05_using_tools.md`
- 新建: `rust-agent-tui/prompts/sections/06_tone_style.md`
- 新建: `rust-agent-tui/prompts/sections/07_communicating.md`
- 新建: `rust-agent-tui/prompts/sections/08_env.md`
- 修改: `rust-agent-tui/src/prompt.rs`

**执行步骤:**

- [ ] 创建 `rust-agent-tui/prompts/sections/` 目录
  - 位置: `rust-agent-tui/prompts/sections/`
  - 命令: `mkdir -p rust-agent-tui/prompts/sections/`

- [ ] 编写 `01_intro.md` — 身份定义 + 安全策略
  - 位置: `rust-agent-tui/prompts/sections/01_intro.md`
  - 内容来源: `default.md` 第 1-4 行（身份声明 + 安全策略 + URL 禁令）
  - 内容:

    ```markdown
    You are an interactive CLI tool that helps users with software engineering tasks. Use the instructions below and the tools available to you to assist the user.

    IMPORTANT: Assist with defensive security tasks only. Refuse to create, modify, or improve code that may be used maliciously. Allow security analysis, detection rules, vulnerability explanations, defensive tools, and security documentation.
    IMPORTANT: You must NEVER generate or guess URLs for the user unless you are confident that the URLs are for helping the user with programming. You may use URLs provided by the user in their messages or local files.
    ```

- [ ] 编写 `02_system.md` — 系统级行为指导
  - 位置: `rust-agent-tui/prompts/sections/02_system.md`
  - 内容来源: `system.md` 的 "Following conventions" 段 + `default.md` 的 "Proactiveness" 段
  - 内容:

    ```markdown
    # Following conventions

    When making changes to files, first understand the file's code conventions. Mimic code style, use existing libraries and utilities, and follow existing patterns.

    - NEVER assume that a given library is available, even if it is well known. Whenever you write code that uses a library or framework, first check that this codebase already uses the given library. For example, you might look at neighboring files, or check the package.json (or cargo.toml, and so on depending on the language).
    - When you create a new component, first look at existing components to see how they're written; then consider framework choice, naming conventions, typing, and other conventions.
    - When you edit a piece of code, first look at the code's surrounding context (especially its imports) to understand the code's choice of frameworks and libraries. Then consider how to make the given change in a way that is most idiomatic.
    - Always follow security best practices. Never introduce code that exposes or logs secrets and keys. Never commit secrets or keys to the repository.

    # Proactiveness

    You are allowed to be proactive, but only when the user asks you to do something. You should strive to strike a balance between:

    - Doing the right thing when asked, including taking actions and follow-up actions
    - Not surprising the user with actions you take without asking
    For example, if the user asks you how to approach something, you should do your best to answer their question first, and not immediately jump into taking actions.
    ```

- [ ] 编写 `03_doing_tasks.md` — 任务执行行为规范
  - 位置: `rust-agent-tui/prompts/sections/03_doing_tasks.md`
  - 内容来源: `system.md` 的 "Doing tasks" 段
  - 内容:

    ```markdown
    # Doing tasks

    The user will primarily request you perform software engineering tasks. This includes solving bugs, adding new functionality, refactoring code, explaining code, and more. For these tasks the following steps are recommended:

    - Use the available search tools to understand the codebase and the user's query. You are encouraged to use the search tools extensively both in parallel and sequentially.
    - Implement the solution using all tools available to you
    - Verify the solution if possible with tests. NEVER assume specific test framework or test script. Check the README or search codebase to determine the testing approach.
    - When you have completed a task, run the lint and build commands if available to ensure your code is correct.
    NEVER commit changes unless the user explicitly asks you to.
    ```

- [ ] 编写 `04_actions.md` — 危险操作谨慎原则
  - 位置: `rust-agent-tui/prompts/sections/04_actions.md`
  - 内容来源: 新增段落（基于 claude-code 的 `getActionsSection()` 风格），当前 `system.md` 中无直接对应内容
  - 内容:

    ```markdown
    # Actions

    When performing operations, consider reversibility and impact scope:

    - Prefer reversible operations over irreversible ones. For example, prefer editing a file over deleting it.
    - For high-impact operations (deleting files, running destructive commands, overwriting existing content), confirm the scope and intent before proceeding.
    - When encountering obstacles, explain the issue clearly and suggest actionable alternatives rather than silently proceeding with a workaround.
    ```

- [ ] 编写 `05_using_tools.md` — 工具使用策略
  - 位置: `rust-agent-tui/prompts/sections/05_using_tools.md`
  - 内容来源: `system.md` 的 "Tool usage policy" 段 + 扩展的工具选择指导
  - 内容:

    ```markdown
    # Tool usage policy

    - You have the capability to call multiple tools in a single response. When multiple independent pieces of information are requested, batch your tool calls together for optimal performance.

    ## Tool selection

    - When doing file search, prefer `search_files_rg` for content search and `glob_files` for file name search over `bash` commands like `grep` or `find`.
    - When reading files, use `read_file` instead of `bash` commands like `cat`. This provides better output formatting and filtering.
    - When writing or editing files, use `write_file` or `edit_file` instead of `bash` commands like `echo` or `sed`.
    - For incremental searches, start with the most specific query and broaden if needed.
    ```

- [ ] 编写 `06_tone_style.md` — 语气风格
  - 位置: `rust-agent-tui/prompts/sections/06_tone_style.md`
  - 内容来源: `system.md` 的 "Code References" 段 + `default.md` 的 "Tone and style" 段（第 6-63 行）
  - 内容:

    ```markdown
    # Code References

    When referencing specific functions or pieces of code include the pattern `file_path:line_number` to allow the user to easily navigate to the source code location.

    <example>
    user: Where are errors from the client handled?
    assistant: Clients are marked as failed in the `connectToServer` function in src/services/process.ts:712.
    </example>

    # Tone and style

    You should be concise, direct, and to the point.
    You MUST answer concisely with fewer than 4 lines (not including tool use or code generation), unless user asks for detail.
    IMPORTANT: You should minimize output tokens as much as possible while maintaining helpfulness, quality, and accuracy. Only address the specific query or task at hand, avoiding tangential information unless absolutely critical for completing the request. If you can answer in 1-3 sentences or a short paragraph, please do.
    IMPORTANT: You should NOT answer with unnecessary preamble or postamble (such as explaining your code or summarizing your action), unless the user asks you to.
    Do not add additional code explanation summary unless requested by the user. After working on a file, just stop, rather than providing an explanation of what you did.
    Answer the user's question directly, without elaboration, explanation, or details. One word answers are best. Avoid introductions, conclusions, and explanations. You MUST avoid text before/after your response, such as "The answer is <answer>.", "Here is the content of the file..." or "Based on the information provided, the answer is..." or "Here is what I will do next...". Here are some examples to demonstrate appropriate verbosity:

    <example>
    user: 2 + 2
    assistant: 4
    </example>

    <example>
    user: what is 2+2?
    assistant: 4
    </example>

    <example>
    user: is 11 a prime number?
    assistant: Yes
    </example>

    <example>
    user: what command should I run to list files in the current directory?
    assistant: ls
    </example>

    <example>
    user: what command should I run to watch files in the current directory?
    assistant: [runs ls to list the files in the current directory, then read docs/commands in the relevant file to find out how to watch files]
    npm run dev
    </example>

    <example>
    user: How many golf balls fit inside a jetta?
    assistant: 150000
    </example>

    <example>
    user: what files are in the directory src/?
    assistant: [runs ls and sees foo.c, bar.c, baz.c]
    user: which file contains the implementation of foo?
    assistant: src/foo.c
    </example>

    When you run a non-trivial bash command, you should explain what the command does and why you are running it, to make sure the user understands what you are doing (this is especially important when you are running a command that will make changes to the user's system).

    Remember that your output will be displayed on a command line interface. Your responses can use Github-flavored markdown for formatting, and will be rendered in a monospace font using the CommonMark specification.

    Output text to communicate with the user; all text you output outside of tool use is displayed to the user. Only use tools to complete tasks. Never use tools like Bash or code comments as means to communicate with the user during the session.

    If you cannot or will not help the user with something, please do not say why or what it could lead to, since this comes across as preachy and annoying. Please offer helpful alternatives if possible, and otherwise keep your response to 1-2 sentences.

    Only use emojis if the user explicitly requests it. Avoid using emojis in all communication unless asked.

    IMPORTANT: Keep your responses short, since they will be displayed on a command line interface.
    ```

- [ ] 编写 `07_communicating.md` — 用户沟通方式
  - 位置: `rust-agent-tui/prompts/sections/07_communicating.md`
  - 内容来源: 新增段落（基于 claude-code 的 `getOutputEfficiencySection()` 风格）
  - 内容:

    ```markdown
    # Communicating with users

    - Write output for humans, not for consoles. Use natural language, not log-style messages.
    - Do not narrate internal mechanisms (e.g., "I will use the read_file tool to..."). Just perform the action.
    - When providing an update after a long operation, include brief context to restore the user's mental state.
    - Avoid over-formatting. Use plain text for simple answers; use markdown only when structure improves readability.
    - After completing a task, report the result directly. Do not add filler summaries.
    ```

- [ ] 编写 `08_env.md` — 环境信息模板
  - 位置: `rust-agent-tui/prompts/sections/08_env.md`
  - 内容来源: `system.md` 的 `<env>` 块
  - 内容:

    ```markdown
    <env>
    Working directory: {{cwd}}
    Is directory a git repo: {{is_git_repo}}
    Platform: {{platform}}
    OS Version: {{os_version}}
    Today's date: {{date}}
    </env>
    ```

- [ ] 删除旧常量并重构 `build_system_prompt()` 使用 `include_str!` 加载段落
  - 位置: `rust-agent-tui/src/prompt.rs`
  - 保留不变: `PromptEnv` 结构体（L7-L29）、`build_agent_overrides_block()` 函数（L57-L75）、`os_version_string()` 函数（L77-L106）
  - 保留不变: 函数签名 `pub fn build_system_prompt(overrides: Option<&AgentOverrides>, cwd: &str) -> String`（Task 2 才引入 `PromptFeatures` 参数）
  - 删除: `SYSTEM_PROMPT_TEMPLATE` 常量（L3）和 `SYSTEM_PROMPT_DEFAULT_AGENT` 常量（L5）——不再使用模板替换模式，旧文件 `system.md` 和 `default.md` 仍保留直到 Task 4 删除
  - 将 `build_system_prompt()` 函数体（~L36-L52）替换为:

    ```rust
    pub fn build_system_prompt(overrides: Option<&AgentOverrides>, cwd: &str) -> String {
        let env = PromptEnv::detect(cwd);

        // 静态段落（编译时嵌入，按编号顺序）
        let static_sections: &[&str] = &[
            include_str!("../prompts/sections/01_intro.md"),
            include_str!("../prompts/sections/02_system.md"),
            include_str!("../prompts/sections/03_doing_tasks.md"),
            include_str!("../prompts/sections/04_actions.md"),
            include_str!("../prompts/sections/05_using_tools.md"),
            include_str!("../prompts/sections/06_tone_style.md"),
            include_str!("../prompts/sections/07_communicating.md"),
            include_str!("../prompts/sections/08_env.md"),
        ];

        // 无 overrides 时，使用 default.md 中的 Tone and style + Proactiveness 作为默认覆盖块
        // 保持与旧实现一致的行为：unwrap_or(SYSTEM_PROMPT_DEFAULT_AGENT.to_string())
        let overrides_block = match overrides {
            Some(ov) => build_agent_overrides_block(ov),
            None => {
                // 从 06_tone_style.md 提取 "# Tone and style" 之后的内容
                let tone_content = include_str!("../prompts/sections/06_tone_style.md");
                let tone_section = tone_content
                    .split_once("\n# Tone and style\n")
                    .map(|(_, after)| after.trim())
                    .unwrap_or("");
                // 从 02_system.md 提取 "# Proactiveness" 之后的内容
                let system_content = include_str!("../prompts/sections/02_system.md");
                let proactiveness_section = system_content
                    .split_once("\n# Proactiveness\n")
                    .map(|(_, after)| after.trim())
                    .unwrap_or("");

                let mut parts = Vec::new();
                if !tone_section.is_empty() {
                    parts.push(format!("# Tone and style\n{}", tone_section));
                }
                if !proactiveness_section.is_empty() {
                    parts.push(format!("# Proactiveness\n{}", proactiveness_section));
                }
                if parts.is_empty() {
                    String::new()
                } else {
                    format!("{}\n\n", parts.join("\n\n"))
                }
            }
        };

        // 合成：覆盖块在最前面，然后是静态段落
        let mut result = String::new();
        if !overrides_block.is_empty() {
            result.push_str(&overrides_block);
        }
        for (i, section) in static_sections.iter().enumerate() {
            if i > 0 {
                result.push_str("\n\n");
            }
            result.push_str(section);
        }

        result
            .replace("{{cwd}}", &env.cwd)
            .replace("{{is_git_repo}}", if env.is_git_repo { "Yes" } else { "No" })
            .replace("{{platform}}", &env.platform)
            .replace("{{os_version}}", &env.os_version)
            .replace("{{date}}", &env.date)
    }
    ```

  - 原因: 从模板替换模式（`SYSTEM_PROMPT_TEMPLATE.replace("{{agent_overrides}}", ...)`）改为段落 join 模式，每个段落独立文件，消除 `system.md` 中的 `{{agent_overrides}}` 占位符。`None` 分支提取段落中的 Tone/Proactiveness 内容作为覆盖块，保持与旧 `default.md` 行为一致——身份声明已拆入 `01_intro.md`，Tone and style 已拆入 `06_tone_style.md`，Proactiveness 已拆入 `02_system.md`。Task 4 将进一步简化此分支（因默认内容已包含在静态段落中，无 overrides 时覆盖块改为空字符串）

- [ ] 为 `build_system_prompt()` 编写单元测试
  - 测试文件: `rust-agent-tui/src/prompt.rs`（在文件末尾添加 `#[cfg(test)] mod tests`）
  - 测试场景:
    - **无 overrides 时返回非空提示词，包含所有段落关键标识**: `build_system_prompt(None, "/tmp")` 的返回值包含 `"Following conventions"`（来自 02）、`"Doing tasks"`（来自 03）、`"<env>"`（来自 08）、`"Working directory"`（来自 08 的替换后结果）
    - **无 overrides 时包含默认覆盖块内容**: 返回值包含 `"# Tone and style"` 和 `"# Proactiveness"`
    - **有 overrides 时使用覆盖块**: `build_system_prompt(Some(&AgentOverrides { persona: Some("test persona".into()), tone: None, proactiveness: None }), "/tmp")` 返回值以 `"test persona"` 开头
    - **占位符替换正确**: 返回值中 `{{cwd}}` 被替换为传入的 cwd 参数值，`{{platform}}` 被替换为实际平台字符串，不包含任何未替换的 `{{` 残留
    - **环境信息包含 cwd**: `build_system_prompt(None, "/custom/path")` 的返回值包含 `"/custom/path"`
  - 运行命令: `cargo test -p rust-agent-tui --lib -- prompt::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 sections 目录下有 8 个 `.md` 文件
  - `ls rust-agent-tui/prompts/sections/ | wc -l`
  - 预期: 输出 8

- [ ] 验证每个段落文件内容非空
  - `for f in rust-agent-tui/prompts/sections/*.md; do test -s "$f" && echo "$f OK"; done`
  - 预期: 8 行输出，全部为 "OK"

- [ ] 验证 08_env.md 包含所有 5 个占位符
  - `grep -c '{{' rust-agent-tui/prompts/sections/08_env.md`
  - 预期: 输出 5（cwd, is_git_repo, platform, os_version, date）

- [ ] 验证 prompt.rs 不再引用旧的 SYSTEM_PROMPT_TEMPLATE / SYSTEM_PROMPT_DEFAULT_AGENT 常量
  - `grep -c 'SYSTEM_PROMPT_TEMPLATE\|SYSTEM_PROMPT_DEFAULT_AGENT' rust-agent-tui/src/prompt.rs`
  - 预期: 输出 0

- [ ] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 或 "Compiling"，无 error

- [ ] 验证单元测试通过
  - `cargo test -p rust-agent-tui --lib -- prompt::tests 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"，0 failures

---

### Task 2: Feature-gated 提示词机制

**背景:**
当前 `build_system_prompt()` 只加载 8 个静态段落，无法根据运行时功能开关（HITL、SubAgent、Cron、Skills）动态注入对应的提示词段落。LLM 在未启用 HITL 时仍收到审批相关指导，或在启用 SubAgent 时缺少委派策略说明，导致提示词冗余或缺失。本 Task 添加 `PromptFeatures` 结构体和 4 个条件段落文件（10-13），使 `build_system_prompt()` 根据功能开关精确合成提示词。本 Task 依赖 Task 1 建立的 `include_str!` 加载框架和 `sections/` 目录，后续 Task 3/4 不直接依赖本 Task 的段落内容（工具提示词在中间件层定义）。

**涉及文件:**
- 新建: `rust-agent-tui/prompts/sections/10_hitl.md`
- 新建: `rust-agent-tui/prompts/sections/11_subagent.md`
- 新建: `rust-agent-tui/prompts/sections/12_cron.md`
- 新建: `rust-agent-tui/prompts/sections/13_skills.md`
- 修改: `rust-agent-tui/src/prompt.rs`（添加 `PromptFeatures` 结构体，修改 `build_system_prompt` 签名）
- 修改: `rust-agent-tui/src/app/agent.rs`（更新两处 `build_system_prompt` 调用）

**执行步骤:**

- [ ] 编写 `10_hitl.md` — HITL 审批模式段落
  - 位置: `rust-agent-tui/prompts/sections/10_hitl.md`
  - 内容来源: claude-code HITL 相关段落 + 现有 `default_requires_approval()` 工具白名单 + `HitlDecision` 枚举语义
  - 内容:

    ```markdown
    # Human-in-the-Loop (HITL) Approval Mode

    When approval mode is enabled, certain tool calls require explicit user approval before execution. The following tools always require approval:

    - `bash` — shell command execution
    - `folder_operations` — folder create/list/exists
    - `launch_agent` — sub-agent delegation
    - `write_*` — any file write operation
    - `edit_*` — any file edit operation
    - `delete_*` / `rm_*` — any file deletion operation

    When a tool call is submitted for approval, the user may respond with one of these decisions:

    - **Approve**: Execute the tool call with original parameters unchanged.
    - **Reject**: Block the tool call entirely. The rejection reason will be returned as a tool error. Adjust your approach based on the rejection reason — do not retry the same action without modification.
    - **Edit**: The user has modified the tool call parameters. Execute with the updated parameters as provided.
    - **Respond**: The user has provided a message instead of approving. Read the user's message and adjust your plan accordingly.

    When a tool call is rejected, do not repeat the same operation. Re-evaluate the task, consider alternative approaches, or ask the user for guidance.
    ```

  - 原因: 指导 LLM 理解审批模式行为，在工具被拒绝时采取合理应对策略

- [ ] 编写 `11_subagent.md` — SubAgent 工具使用段落
  - 位置: `rust-agent-tui/prompts/sections/11_subagent.md`
  - 内容来源: `SubAgentTool::description()` + `filter_tools()` 逻辑 + `AgentOverrides` 字段
  - 内容:

    ```markdown
    # SubAgent Delegation

    You have access to the `launch_agent` tool, which allows you to delegate sub-tasks to specialized agents defined in `.claude/agents/{agent_id}.md` or `.claude/agents/{agent_id}/agent.md`.

    ## When to use sub-agents

    - For tasks that benefit from independent context isolation (e.g., code review while working on a different feature)
    - For tasks requiring specialized persona or behavior defined in agent configuration files
    - For parallelizable sub-tasks that do not depend on each other's results

    ## Delegation guidelines

    - Provide a clear, self-contained `task` description. The sub-agent has no access to the parent conversation history.
    - Specify `agent_id` matching an existing agent definition file. Available agents can be discovered through the agents management panel.
    - The sub-agent inherits the parent's tool set by default, excluding `launch_agent` itself (to prevent recursion).
    - Agent definitions may restrict available tools via the `tools` and `disallowedTools` fields.

    ## Context isolation

    Sub-agents execute in isolated state — they cannot access the parent's message history or intermediate results. Ensure the `task` parameter contains all necessary context for the sub-agent to complete its work independently.
    ```

  - 原因: 指导 LLM 正确使用 `launch_agent` 工具，理解上下文隔离和委派策略

- [ ] 编写 `12_cron.md` — Cron 定时任务段落
  - 位置: `rust-agent-tui/prompts/sections/12_cron.md`
  - 内容来源: `CronRegisterTool::description()` + `CronScheduler` 行为
  - 内容:

    ```markdown
    # Scheduled Tasks (Cron)

    You have access to scheduled task tools (`cron_register`, `cron_list`, `cron_remove`) for registering recurring automated tasks.

    ## Cron expression format

    Use standard 5-field cron expressions:

    ```
    ┌───────────── minute (0-59)
    │ ┌───────────── hour (0-23)
    │ │ ┌───────────── day of month (1-31)
    │ │ │ ┌───────────── month (1-12)
    │ │ │ │ ┌───────────── day of week (0-6, 0=Sunday)
    * * * * *
    ```

    ## Persistence behavior

    - Cron tasks run **in-memory only**. All registered tasks are lost when the application restarts.
    - Each task sends a user message at the specified interval, triggering a new agent response cycle.

    ## Usage guidelines

    - Use `cron_register` to create a new scheduled task with a cron expression and a prompt message.
    - Use `cron_list` to view all currently registered tasks and their next fire times.
    - Use `cron_remove` to delete a task by its ID when it is no longer needed.
    ```

  - 原因: 指导 LLM 理解 cron 表达式格式和定时任务的持久化行为

- [ ] 编写 `13_skills.md` — Skills 使用与发现段落
  - 位置: `rust-agent-tui/prompts/sections/13_skills.md`
  - 内容来源: `SkillsMiddleware::resolve_dirs()` 搜索路径 + `build_summary()` 输出格式
  - 内容:

    ```markdown
    # Skills

    Skills are specialized capabilities that extend your behavior. Each skill is defined in a `SKILL.md` file with YAML frontmatter containing `name` and `description`.

    ## Skill discovery

    Skills are loaded from the following directories in priority order (first match wins):

    1. `~/.claude/skills/` — user-level skills (highest priority)
    2. Global `skillsDir` configured in `~/.zen-code/settings.json`
    3. `{cwd}/.claude/skills/` — project-level skills

    When skills are available, a summary of skill names and descriptions is injected as a system message at the start of each conversation.

    ## Using skills

    - Mention a skill by name when you want to load its full content. Users typically invoke skills using the `/skill-name` format in their messages.
    - Skills may override default behaviors, add domain-specific knowledge, or provide structured workflows.
    - Multiple skills can be active simultaneously.
    ```

  - 原因: 指导 LLM 理解 Skills 的搜索路径和调用方式

- [ ] 在 `prompt.rs` 中添加 `PromptFeatures` 结构体
  - 位置: `rust-agent-tui/src/prompt.rs`，在 `PromptEnv` 结构体定义之后（~L14）插入
  - 关键逻辑:

    ```rust
    /// 控制 Feature-gated 提示词段落的注入
    pub struct PromptFeatures {
        pub hitl_enabled: bool,
        pub subagent_enabled: bool,
        pub cron_enabled: bool,
        pub skills_enabled: bool,
    }

    impl PromptFeatures {
        /// 根据运行时环境推断功能开关
        pub fn detect() -> Self {
            Self {
                hitl_enabled: std::env::var("YOLO_MODE").as_deref() == Ok("false"),
                subagent_enabled: true,  // TODO: 从中间件注册状态推断
                cron_enabled: true,      // TODO: 从中间件注册状态推断
                skills_enabled: true,    // TODO: 从中间件注册状态推断
            }
        }

        /// 全部关闭的配置（用于测试）
        #[cfg(test)]
        pub fn none() -> Self {
            Self {
                hitl_enabled: false,
                subagent_enabled: false,
                cron_enabled: false,
                skills_enabled: false,
            }
        }
    }
    ```

  - 原因: 封装功能开关，`detect()` 从 `YOLO_MODE` 环境变量推断 HITL 状态（与 `-a` CLI 参数的行为一致：`-a` 设置 `YOLO_MODE=false`），其余三个字段暂时硬编码为 `true`，待后续从中间件注册列表推断

- [ ] 修改 `build_system_prompt()` 签名，添加 `features: PromptFeatures` 参数
  - 位置: `rust-agent-tui/src/prompt.rs` 的 `build_system_prompt()` 函数签名（~L36）
  - 从: `pub fn build_system_prompt(overrides: Option<&AgentOverrides>, cwd: &str) -> String`
  - 到: `pub fn build_system_prompt(overrides: Option<&AgentOverrides>, cwd: &str, features: PromptFeatures) -> String`
  - 在函数体中，静态段落数组之后、`overrides_block` 之前，添加条件段落注入逻辑:

    ```rust
    // Feature-gated 段落（条件拼接）
    let mut gated_sections: Vec<&str> = Vec::new();
    if features.hitl_enabled {
        gated_sections.push(include_str!("../prompts/sections/10_hitl.md"));
    }
    if features.subagent_enabled {
        gated_sections.push(include_str!("../prompts/sections/11_subagent.md"));
    }
    if features.cron_enabled {
        gated_sections.push(include_str!("../prompts/sections/12_cron.md"));
    }
    if features.skills_enabled {
        gated_sections.push(include_str!("../prompts/sections/13_skills.md"));
    }
    ```

  - 修改合成部分（在 `parts.extend(static_sections)` 之后）追加 `parts.extend(&gated_sections);`
  - 原因: 根据功能开关动态拼接条件段落，`include_str!` 在编译时嵌入所有段落文件，运行时通过条件判断决定是否包含

- [ ] 更新 `agent.rs` 中主 agent 的 `build_system_prompt` 调用
  - 位置: `rust-agent-tui/src/app/agent.rs` ~L66
  - 从: `let system_prompt = crate::prompt::build_system_prompt(overrides.as_ref(), &cwd);`
  - 到:

    ```rust
    let features = crate::prompt::PromptFeatures::detect();
    let system_prompt = crate::prompt::build_system_prompt(overrides.as_ref(), &cwd, features);
    ```

  - 原因: 主 agent 组装时根据运行时环境检测功能开关

- [ ] 更新 `agent.rs` 中 subagent `system_builder` 闭包
  - 位置: `rust-agent-tui/src/app/agent.rs` ~L167-L168
  - 从: `Arc::new(|overrides, cwd| crate::prompt::build_system_prompt(overrides, cwd));`
  - 到: `Arc::new(|overrides, cwd| crate::prompt::build_system_prompt(overrides, cwd, crate::prompt::PromptFeatures::detect()));`
  - `system_builder` 签名保持不变: `Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>`
  - 原因: 子 agent 的系统提示词也需要根据当前运行时环境检测功能开关；在闭包内部调用 `PromptFeatures::detect()` 使其每次创建子 agent 时重新检测

- [ ] 为 `PromptFeatures` 和 `build_system_prompt` 的 Feature-gated 逻辑编写单元测试
  - 测试文件: `rust-agent-tui/src/prompt.rs`（在 `#[cfg(test)] mod tests` 块中追加）
  - 测试场景:
    - **全关闭时不包含任何 feature 段落标识**: `build_system_prompt(None, "/tmp", PromptFeatures::none())` 返回值不包含 `"Human-in-the-Loop"`（来自 10）、不包含 `"SubAgent Delegation"`（来自 11）、不包含 `"Scheduled Tasks"`（来自 12）、不包含 `"Skills"` 作为标题（来自 13 的 `# Skills` 标题）
    - **hitl_enabled 时包含 HITL 段落**: `build_system_prompt(None, "/tmp", PromptFeatures { hitl_enabled: true, ..PromptFeatures::none() })` 返回值包含 `"Human-in-the-Loop"`
    - **subagent_enabled 时包含 SubAgent 段落**: 同上模式，返回值包含 `"SubAgent Delegation"`
    - **cron_enabled 时包含 Cron 段落**: 同上模式，返回值包含 `"Scheduled Tasks"`
    - **skills_enabled 时包含 Skills 段落**: 同上模式，返回值包含 `"# Skills"`（精确匹配标题行）
    - **全开启时包含所有 feature 段落**: `PromptFeatures { hitl_enabled: true, subagent_enabled: true, cron_enabled: true, skills_enabled: true }` 返回值包含全部 4 个段落标识
    - **PromptFeatures::detect() 返回合理默认值**: `PromptFeatures::detect()` 的 `hitl_enabled` 在默认环境（无 `YOLO_MODE` 设置或 `YOLO_MODE=true`）下为 `false`，其余三个字段为 `true`
  - 运行命令: `cargo test -p rust-agent-tui --lib -- prompt::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证 sections 目录下新增 4 个 `.md` 文件（共 12 个）
  - `ls rust-agent-tui/prompts/sections/ | wc -l`
  - 预期: 输出 12

- [ ] 验证新增 4 个段落文件内容非空
  - `for f in rust-agent-tui/prompts/sections/1{0,1,2,3}_*.md; do test -s "$f" && echo "$f OK"; done`
  - 预期: 4 行输出，全部为 "OK"

- [ ] 验证 `PromptFeatures` 结构体已导出
  - `grep -c 'pub struct PromptFeatures' rust-agent-tui/src/prompt.rs`
  - 预期: 输出 1

- [ ] 验证 `build_system_prompt` 签名包含 `features` 参数
  - `grep 'fn build_system_prompt' rust-agent-tui/src/prompt.rs`
  - 预期: 输出包含 `features: PromptFeatures`

- [ ] 验证 `agent.rs` 两处调用均已更新
  - `grep -c 'PromptFeatures::detect()' rust-agent-tui/src/app/agent.rs`
  - 预期: 输出 2（主 agent 调用 + subagent system_builder 闭包）

- [ ] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 或 "Compiling"，无 error

- [ ] 验证单元测试通过
  - `cargo test -p rust-agent-tui --lib -- prompt::tests 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"，0 failures


---

### Task 3: 工具提示词扩展

**背景:**
当前 9 个工具的 `description()` 和 `parameters()` 返回值均为一行简短文本，缺少 claude-code 风格的详细用法指导（使用场景、决策树、错误处理建议、注意事项），导致 LLM 不能充分利用工具能力。本 Task 将每个工具的 description 扩展为多段落文本（功能概述 + Usage + 决策指导 + 注意事项），同时丰富 parameters 中每个字段的 description 字符串。本 Task 不依赖 Task 1/2 的输出（工具提示词在中间件层定义，不在系统提示词段落中），Task 4（清理与验收）将验证本 Task 的工具提示词输出完整性。

**涉及文件:**
- 修改: `rust-agent-middlewares/src/tools/filesystem/read.rs`
- 修改: `rust-agent-middlewares/src/tools/filesystem/write.rs`
- 修改: `rust-agent-middlewares/src/tools/filesystem/edit.rs`
- 修改: `rust-agent-middlewares/src/tools/filesystem/glob.rs`
- 修改: `rust-agent-middlewares/src/tools/filesystem/grep.rs`
- 修改: `rust-agent-middlewares/src/middleware/terminal.rs`
- 修改: `rust-agent-middlewares/src/tools/filesystem/folder.rs`
- 修改: `rust-agent-middlewares/src/tools/todo.rs`
- 修改: `rust-agent-middlewares/src/subagent/tool.rs`

**执行步骤:**

- [ ] 确认 description 常量存储方案
  - 经代码确认: `indoc` crate 不在 workspace 依赖中（`rust-agent-middlewares/Cargo.toml` 和根 `Cargo.toml` 均无 `indoc` 依赖）
  - 方案: 不引入新依赖，所有 description 常量使用 Rust 原始字符串字面量 `r#"..."#` + 手工换行管理
  - 以下所有步骤均使用 `r#"..."#` 方案

- [ ] 扩展 `read_file` 工具的 description 和 parameters — 对齐 claude-code 的 FileReadTool 风格
  - 位置: `rust-agent-middlewares/src/tools/filesystem/read.rs`
  - 在 `const MAX_FILE_SIZE` 之后（~L19 之后）、`fn is_binary_extension` 之前（~L21 之前）插入 description 常量:

    ```rust
    const READ_FILE_DESCRIPTION: &str = r#"Reads a file from the local filesystem. You can access any file directly by using this tool.
    Assume this tool is able to read all files on the machine. If the User provides a path to a file assume that path is valid. It is okay to read a file that does not exist; an error will be returned.

    Usage:
    - The file_path parameter must be an absolute path, not a relative path
    - By default, it reads up to 2000 lines starting from the beginning of the file
    - You can optionally specify a line offset and limit (especially handy for long files), but it's recommended to read the whole file by not providing these parameters
    - Any lines longer than 65536 characters will be truncated
    - Results are returned using cat -n format, with line numbers starting at 1
    - This tool reads files from the local filesystem; it cannot handle URLs
    - You can call multiple tools in a single response. It is always better to speculatively read multiple files before making edits
    - You should prefer using the read_file tool over the bash tool with commands like cat, head, tail, or sed to read files. This provides better output formatting and filtering
    - For open-ended searches that may require multiple rounds of globbing and grepping, use the Agent tool instead

    Error handling:
    - File not found: returns an error message indicating the path does not exist
    - Binary files: detected by extension and returns a message indicating the file cannot be displayed as text
    - Files exceeding 32 MB: returns an error suggesting use of offset/limit parameters
    - Offset exceeds file length: returns an error indicating the line range is invalid"#;
    ```

  - 替换 `description()` 方法体（~L39-L41）: 将行内字符串替换为常量引用 `READ_FILE_DESCRIPTION`
  - 替换 `parameters()` 返回值（~L43-L53）为:

    ```rust
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to read"
                },
                "offset": {
                    "type": "number",
                    "description": "The line number to start reading from. Only provide if the file is too large to read in a single call. Not providing this parameter reads the whole file (recommended)"
                },
                "limit": {
                    "type": "number",
                    "description": "The number of lines to read. Only provide if the file is too large to read in a single call. Not providing this parameter reads the whole file (recommended)"
                }
            },
            "required": ["file_path"]
        })
    }
    ```

  - 原因: 对齐 claude-code 的 read_file 描述风格，让 LLM 了解行号格式、大文件处理策略、错误场景

- [ ] 扩展 `write_file` 工具的 description 和 parameters — 对齐 claude-code 的 FileWriteTool 风格
  - 位置: `rust-agent-middlewares/src/tools/filesystem/write.rs`
  - 在 `impl WriteFileTool` 块之前（~L6 之后）插入 description 常量:

    ```rust
    const WRITE_FILE_DESCRIPTION: &str = r#"Writes a file to the local filesystem.

    Usage:
    - This tool will overwrite the existing file if there is one at the provided path
    - If this is an existing file, you MUST use the read_file tool first to read the file's contents. This tool will fail if you did not read the file first
    - ALWAYS prefer editing existing files in the codebase. DO NOT create new files unless explicitly required
    - The file_path parameter must be an absolute path, not a relative path
    - Parent directories are created automatically if they do not exist

    Notes:
    - Uses atomic write (write to temp file then rename) to prevent data loss on crash
    - NEVER create documentation files (*.md) or README files unless explicitly requested by the User
    - Only use emojis if the User explicitly requests it. Avoid writing emojis to files unless asked"#;
    ```

  - 替换 `description()` 方法体（~L23-L25）: 将行内字符串替换为常量引用 `WRITE_FILE_DESCRIPTION`
  - 替换 `parameters()` 返回值（~L27-L36）为:

    ```rust
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to write (must be absolute, not relative)"
                },
                "content": {
                    "type": "string",
                    "description": "The full content to write to the file"
                }
            },
            "required": ["file_path", "content"]
        })
    }
    ```

  - 原因: 对齐 claude-code 的 write_file 描述风格，强调先读后写原则、原子写入行为、不创建文档文件的约束

- [ ] 扩展 `edit_file` 工具的 description 和 parameters — 对齐 claude-code 的 FileEditTool 风格
  - 位置: `rust-agent-middlewares/src/tools/filesystem/edit.rs`
  - 在 `impl EditFileTool` 块之前（~L6 之后）插入 description 常量:

    ```rust
    const EDIT_FILE_DESCRIPTION: &str = r#"Performs exact string replacements in files.

    Usage:
    - You must use your read_file tool at least once in the conversation before editing. This tool will fail if you attempt an edit without reading the file
    - When editing text from read_file tool output, ensure you preserve the exact indentation (tabs/spaces) as it appears AFTER the line number prefix
    - ALWAYS prefer editing existing files in the codebase. DO NOT create new files unless explicitly required
    - The file_path parameter must be an absolute path, not a relative path
    - The old_string parameter must match exactly, including all whitespace and indentation
    - The edit will FAIL if old_string is not unique in the file. Either provide a larger string with more surrounding context to make it unique or use replace_all to change every instance of old_string
    - Use replace_all for replacing and renaming strings across the file

    Error handling:
    - old_string not found: returns an error indicating the string does not exist in the file
    - old_string not unique: returns an error with the count of occurrences, suggesting more context or replace_all
    - old_string is empty: returns an error rejecting the operation
    - File not found: returns an error indicating the path does not exist"#;
    ```

  - 替换 `description()` 方法体（~L23-L25）: 将行内字符串替换为常量引用 `EDIT_FILE_DESCRIPTION`
  - 替换 `parameters()` 返回值（~L27-L37）为:

    ```rust
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to modify"
                },
                "old_string": {
                    "type": "string",
                    "description": "The text to replace. Must match EXACTLY including all whitespace, indentation, and newlines. The edit will fail if old_string is not unique in the file unless replace_all is true"
                },
                "new_string": {
                    "type": "string",
                    "description": "The text to replace it with (must be different from old_string)"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "If true, replace all occurrences of old_string. If false (default), replace only the first occurrence. Use this to rename variables or update repeated patterns across the file"
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }
    ```

  - 原因: 对齐 claude-code 的 edit_file 描述风格，强调 old_string 唯一性要求、行号前缀格式说明、replace_all 用法

- [ ] 扩展 `glob_files` 工具的 description 和 parameters — 对齐 claude-code 的 GlobTool 风格
  - 位置: `rust-agent-middlewares/src/tools/filesystem/glob.rs`
  - 在 `const MAX_RESULTS` 之后（~L19 之后）、`fn should_skip_dir` 之前（~L21 之前）插入 description 常量:

    ```rust
    const GLOB_FILES_DESCRIPTION: &str = r#"Fast file pattern matching tool that works with any codebase size. Supports glob patterns like "**/*.js" or "src/**/*.ts". Returns matching file paths sorted by modification time.

    Usage:
    - Use this tool when you need to find files by name patterns
    - Returns file paths sorted by modification time (most recently modified first)
    - Maximum 1000 results returned; results are truncated beyond this limit with a notice
    - Common directories like node_modules, .git, target, dist, build are automatically excluded from results
    - The path parameter is optional; defaults to the current working directory
    - For searching file contents, use search_files_rg instead

    When to use:
    - Use glob_files when searching for files by name pattern (e.g., find all TypeScript files, find a specific config file)
    - Use search_files_rg when searching for content within files (e.g., find where a function is defined)
    - For open-ended searches requiring multiple rounds, consider using a sub-agent via launch_agent"#;
    ```

  - 替换 `description()` 方法体（~L88-L90）: 将行内字符串替换为常量引用 `GLOB_FILES_DESCRIPTION`
  - 替换 `parameters()` 返回值（~L92-L101）为:

    ```rust
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against (e.g. \"**/*.js\", \"src/**/*.rs\", \"*.config.json\"). Use ** for recursive matching"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. Absolute path or relative to cwd. If not specified, the current working directory is used"
                }
            },
            "required": ["pattern"]
        })
    }
    ```

  - 原因: 对齐 claude-code 的 glob 描述风格，说明排序规则、排除目录、与 search_files_rg 的选择决策

- [ ] 扩展 `search_files_rg` 工具的 description 和 parameters — 对齐 claude-code 的 GrepTool 风格
  - 位置: `rust-agent-middlewares/src/tools/filesystem/grep.rs`
  - 在 `impl SearchFilesRgTool` 块之前（~L9 之后，struct 定义之后）插入 description 常量:

    ```rust
    const SEARCH_FILES_RG_DESCRIPTION: &str = r#"A powerful search tool built on ripgrep (rg). Supports full regex syntax (e.g. "log.*Error", "function\\s+\\w+"). Filter files with glob parameter (e.g. "*.js", "*.{ts,tsx}") or type parameter (e.g. "js", "py", "rust", "go"). Use output_mode to control result format.

    Usage:
    - Use the args parameter as a ripgrep arguments array. Format: [OPTIONS..., PATTERN, PATH]
    - If you need to identify a set of files, prefer glob_files over search_files_rg
    - Supports full regex syntax — literal braces need escaping (use \\{\\} to find interface{} in Go code)
    - Output includes line numbers by default when -n flag is used
    - Search times out after 15 seconds; use more specific patterns for large codebases
    - Maximum 500 lines of output; use head_limit parameter to adjust

    Output modes:
    - Default: shows matching lines with line numbers
    - Use -l flag (in args) to list only file paths that contain matches
    - Use -c flag (in args) to show match counts per file

    When to use:
    - Prefer search_files_rg over bash commands like grep or rg for content search
    - Use glob_files for file name search, search_files_rg for content search
    - For open-ended searches, start with the most specific query and broaden if needed"#;
    ```

  - 替换 `description()` 方法体（~L43-L45）: 将行内字符串替换为常量引用 `SEARCH_FILES_RG_DESCRIPTION`
  - 替换 `parameters()` 返回值（~L47-L59）为:

    ```rust
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Ripgrep arguments as a string array. Format: [OPTIONS..., PATTERN, PATH]. Example: [\"-n\", \"fn main\", \"src/\"]. Supports regex patterns, glob filters (-g flag), file type filters (-t flag), context lines (-C flag), and all standard ripgrep options"
                },
                "head_limit": {
                    "type": "number",
                    "description": "Limit output to first N matching lines (default 500). Use sparingly — large result sets waste context"
                }
            },
            "required": ["args"]
        })
    }
    ```

  - 原因: 对齐 claude-code 的 grep 描述风格，说明正则语法、输出模式选择、超时行为、与 glob_files 的分工

- [ ] 扩展 `bash` 工具的 description 和 parameters — 对齐 claude-code 的 BashTool 风格
  - 位置: `rust-agent-middlewares/src/middleware/terminal.rs`
  - 在 `const MAX_OUTPUT_LINES` 之后（~L24 之后）、`fn truncate_bytes` 之前（~L26 之前）插入 description 常量:

    ```rust
    const BASH_DESCRIPTION: &str = r#"Executes a given shell command and returns its output.

    Usage:
    - The working directory persists between commands, but shell state does not. The shell environment is initialized from the user's profile (bash or zsh)
    - IMPORTANT: Avoid using this tool to run find, grep, cat, head, tail, sed, awk, or echo commands, unless explicitly instructed or after you have verified that a dedicated tool cannot accomplish your task
    - Instead, use the appropriate dedicated tool which will provide a much better experience for the user:
      - File search: Use glob_files (NOT find or ls)
      - Content search: Use search_files_rg (NOT grep or rg)
      - Read files: Use read_file (NOT cat/head/tail)
      - Edit files: Use edit_file (NOT sed/awk)
      - Write files: Use write_file (NOT echo/cat with redirect)
    - You can specify an optional timeout in seconds (up to 300 seconds / 5 minutes). Default is 120 seconds (2 minutes)
    - When issuing multiple commands, use && to chain them together rather than using separate tool calls if the commands depend on each other
    - For long running commands, consider using a timeout to avoid waiting indefinitely

    Platform behavior:
    - Windows: uses cmd /C to execute commands
    - Unix/macOS: uses bash -c to execute commands
    - On Unix, child processes run in their own process group; timeout kills the entire process tree

    Output handling:
    - Output exceeding 2000 lines is truncated (head + tail preserved)
    - Output exceeding 100000 bytes is truncated
    - Non-zero exit codes are reported
    - Both stdout and stderr are captured"#;
    ```

  - 替换 `description()` 方法体（~L72-L74）: 将行内字符串替换为常量引用 `BASH_DESCRIPTION`
  - 替换 `parameters()` 返回值（~L76-L84）为:

    ```rust
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command (and optional arguments) to execute. This can be complex commands that use pipes, &&, or other shell features. For multiple dependent commands, chain them with && rather than making separate calls"
                },
                "timeout_secs": {
                    "type": "number",
                    "description": "Optional timeout in seconds (default 120, max 300). If the command takes longer than this, it will be killed and a timeout error returned"
                }
            },
            "required": ["command"]
        })
    }
    ```

  - 原因: 对齐 claude-code 的 bash 描述风格，强调优先使用专用工具、超时行为、跨平台说明、输出截断策略

- [ ] 扩展 `folder_operations` 工具的 description 和 parameters — 对齐 claude-code 的 FolderTool 风格
  - 位置: `rust-agent-middlewares/src/tools/filesystem/folder.rs`
  - 在 `const MAX_LIST_ENTRIES` 之后（~L19 之后）、`fn list_folder` 之前（~L22 之前）插入 description 常量:

    ```rust
    const FOLDER_OPERATIONS_DESCRIPTION: &str = r#"Unified folder operations tool supporting create, list, and existence check.

    Operations:
    - "create": Creates a directory at the specified path. By default creates parent directories recursively (recursive: true). Use recursive: false to only create a single directory level
    - "list": Lists the contents of a directory, showing files and subdirectories with sizes and modification dates. Output is truncated beyond 500 entries
    - "exists": Checks whether a path exists and whether it is a directory or file

    Usage:
    - The folder_path parameter must be an absolute path, not a relative path
    - You can call multiple tools in a single response. It is always better to check directory existence before creating or listing
    - When creating a directory, the recursive parameter defaults to true, creating all necessary parent directories

    Notes:
    - List output shows entries with file size and modification date
    - Directories are shown with a trailing / indicator
    - For large directories (>500 entries), output is truncated with a summary count"#;
    ```

  - 替换 `description()` 方法体（~L104-L106）: 将行内字符串替换为常量引用 `FOLDER_OPERATIONS_DESCRIPTION`
  - 替换 `parameters()` 返回值（~L108-L118）为:

    ```rust
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["create", "list", "exists"],
                    "description": "The folder operation to perform: \"create\" to create a directory, \"list\" to list directory contents, \"exists\" to check if a path exists"
                },
                "folder_path": {
                    "type": "string",
                    "description": "The absolute path to the folder for the operation"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "For \"create\" operation: whether to create parent directories if needed (default true). Ignored for other operations"
                }
            },
            "required": ["operation", "folder_path"]
        })
    }
    ```

  - 原因: 对齐 claude-code 的 folder 操作描述风格，详细说明三种操作的行为差异和输出格式

- [ ] 扩展 `todo_write` 工具的 description 和 parameters — 对齐 claude-code 的 TodoWriteTool 风格
  - 位置: `rust-agent-middlewares/src/tools/todo.rs`
  - 在 `pub struct TodoWriteTool` 定义之前（~L29 之前），在 `TodoItem` struct 定义之后（~L25 之后）插入 description 常量:

    ```rust
    const TODO_WRITE_DESCRIPTION: &str = r#"Maintain a todo list for complex multi-step tasks. Call this to create or update your todo list with the complete current state. Each call fully replaces the previous list.

    Usage:
    - Use this tool when working on complex, multi-step tasks that benefit from tracking progress
    - Each call sends the COMPLETE todo list — this is a full replacement, not a partial update
    - Include ALL items in every call, not just changed ones
    - Mark items as "in_progress" when starting work on them, and "completed" when done
    - Keep descriptions concise but specific enough to understand at a glance

    When to use:
    - Use for tasks with 3+ distinct steps that require tracking
    - Use when the user explicitly asks for a plan or task breakdown
    - Do NOT use for simple, single-step tasks
    - Do NOT use for tasks that can be completed in a single tool call

    Status values:
    - "pending": Not yet started
    - "in_progress": Currently being worked on
    - "completed": Finished successfully"#;
    ```

  - 替换 `description()` 方法体（~L55-L57）: 将行内字符串替换为常量引用 `TODO_WRITE_DESCRIPTION`
  - 替换 `parameters()` 返回值（~L59-L83）为:

    ```rust
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "The complete todo list (replaces all previous items). Include ALL items in every call, not just new or changed ones. Items not included will be removed",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {
                                "type": "string",
                                "description": "Unique identifier for this todo item. Use simple, stable IDs (e.g. '1', '2', '3') that persist across updates"
                            },
                            "content": {
                                "type": "string",
                                "description": "A concise description of the task to be done (1-2 sentences)"
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"],
                                "description": "Current status: 'pending' (not started), 'in_progress' (actively working), 'completed' (done)"
                            }
                        },
                        "required": ["id", "content", "status"]
                    }
                }
            },
            "required": ["todos"]
        })
    }
    ```

  - 原因: 对齐 claude-code 的 todo_write 描述风格，强调全量替换语义、使用场景决策、状态值含义

- [ ] 扩展 `launch_agent` 工具的 description 和 parameters — 对齐 claude-code 的 AgentTool 风格
  - 位置: `rust-agent-middlewares/src/subagent/tool.rs`
  - 在 `pub struct SubAgentTool` 定义之后（~L39 之后）、`impl SubAgentTool` 块之前（~L41 之前）插入 description 常量:

    ```rust
    const LAUNCH_AGENT_DESCRIPTION: &str = r#"Launch a sub-agent with an independent context to handle a specialized sub-task. The sub-agent executes based on the configuration defined in .claude/agents/{agent_id}.md or .claude/agents/{agent_id}/agent.md.

    Usage:
    - Provide a clear, self-contained task description. The sub-agent has no access to the parent conversation history
    - Specify agent_id matching an existing agent definition file
    - The sub-agent inherits the parent's tool set by default, excluding launch_agent itself (to prevent recursion)
    - Agent definitions may restrict available tools via the tools and disallowedTools fields in frontmatter
    - The sub-agent executes in isolated state — it cannot access the parent's message history or intermediate results

    When to use:
    - For tasks that benefit from independent context isolation (e.g., code review while working on a different feature)
    - For tasks requiring specialized persona or behavior defined in agent configuration files
    - For parallelizable sub-tasks that do not depend on each other's results
    - When you need to break a complex task into smaller, independently executable pieces

    Return format:
    - If the sub-agent made tool calls, the result includes a summary of tools used followed by the final response
    - If no tool calls were made, only the final response text is returned"#;
    ```

  - 替换 `description()` 方法体（~L115-L117）: 将行内字符串替换为常量引用 `LAUNCH_AGENT_DESCRIPTION`
  - 替换 `parameters()` 返回值（~L119-L138）为:

    ```rust
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["agent_id", "task"],
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "The identifier of the agent to launch. Corresponds to a file at .claude/agents/{agent_id}.md or .claude/agents/{agent_id}/agent.md"
                },
                "task": {
                    "type": "string",
                    "description": "The task description to delegate to the sub-agent. Must be clear and self-contained, as the sub-agent has no access to the parent conversation history. Include all necessary context"
                },
                "cwd": {
                    "type": "string",
                    "description": "The working directory for the sub-agent. Defaults to inheriting the parent agent's current working directory if not specified"
                }
            }
        })
    }
    ```

  - 原因: 对齐 claude-code 的 launch_agent 描述风格，强调上下文隔离原则、委派策略、工具过滤行为、返回格式

- [ ] 为所有 9 个工具的 description 扩展编写单元测试
  - 测试文件: 各工具源文件中的 `#[cfg(test)] mod tests` 块（每个文件追加一个测试）
  - 在 `rust-agent-middlewares/src/tools/filesystem/read.rs` 的 `mod tests` 中追加:

    ```rust
    #[test]
    fn test_description_extended() {
        let tool = ReadFileTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(desc.contains("Error handling:"), "description 应包含 Error handling 段落");
        assert!(desc.contains("line numbers"), "description 应提及行号格式");
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本，长度 > 200 字符");
    }
    ```

  - 在 `rust-agent-middlewares/src/tools/filesystem/write.rs` 的 `mod tests` 中追加:

    ```rust
    #[test]
    fn test_description_extended() {
        let tool = WriteFileTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(desc.contains("atomic write"), "description 应提及原子写入");
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }
    ```

  - 在 `rust-agent-middlewares/src/tools/filesystem/edit.rs` 的 `mod tests` 中追加:

    ```rust
    #[test]
    fn test_description_extended() {
        let tool = EditFileTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(desc.contains("old_string"), "description 应提及 old_string");
        assert!(desc.contains("replace_all"), "description 应提及 replace_all");
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }
    ```

  - 在 `rust-agent-middlewares/src/tools/filesystem/glob.rs` 的 `mod tests` 中追加:

    ```rust
    #[test]
    fn test_description_extended() {
        let tool = GlobFilesTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(desc.contains("modification time"), "description 应提及排序规则");
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }
    ```

  - 在 `rust-agent-middlewares/src/tools/filesystem/grep.rs` 的 `mod tests` 中追加:

    ```rust
    #[test]
    fn test_description_extended() {
        let tool = SearchFilesRgTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("regex"), "description 应提及正则支持");
        assert!(desc.contains("Output modes:"), "description 应包含 Output modes 段落");
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }
    ```

  - 在 `rust-agent-middlewares/src/middleware/terminal.rs` 的 `mod tests` 中追加:

    ```rust
    #[test]
    fn test_bash_description_extended() {
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(desc.contains("dedicated tool"), "description 应强调优先使用专用工具");
        assert!(desc.contains("timeout"), "description 应提及超时");
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }
    ```

  - 在 `rust-agent-middlewares/src/tools/filesystem/folder.rs` 的 `mod tests` 中追加:

    ```rust
    #[test]
    fn test_description_extended() {
        let tool = FolderOperationsTool::new("/tmp");
        let desc = tool.description();
        assert!(desc.contains("create") && desc.contains("list") && desc.contains("exists"),
            "description 应提及三种操作");
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }
    ```

  - 在 `rust-agent-middlewares/src/tools/todo.rs` 中添加测试块（当前文件无 `#[cfg(test)] mod tests`）:

    ```rust
    #[cfg(test)]
    mod tests {
        use super::*;
        use tokio::sync::mpsc;

        #[test]
        fn test_description_extended() {
            let (tx, _rx) = mpsc::channel(8);
            let tool = TodoWriteTool::new(tx);
            let desc = tool.description();
            assert!(desc.contains("full replacement") || desc.contains("fully replaces"),
                "description 应提及全量替换语义");
            assert!(desc.contains("pending") && desc.contains("in_progress") && desc.contains("completed"),
                "description 应提及三种状态值");
            assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
        }
    }
    ```

  - 在 `rust-agent-middlewares/src/subagent/tool.rs` 的 `mod tests` 中追加:

    ```rust
    #[test]
    fn test_launch_agent_description_extended() {
        let t = make_subagent_tool(vec![]);
        let desc = t.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(desc.contains("sub-agent") || desc.contains("sub agent"),
            "description 应提及 sub-agent");
        assert!(desc.contains("isolated") || desc.contains("isolation"),
            "description 应提及上下文隔离");
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }
    ```

  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- test_description_extended 2>&1 | tail -15`
  - 预期: 输出包含 "test result: ok"，所有 9 个 description 扩展测试通过

**检查步骤:**

- [ ] 验证所有 9 个工具文件包含 description 常量
  - `for f in rust-agent-middlewares/src/tools/filesystem/read.rs rust-agent-middlewares/src/tools/filesystem/write.rs rust-agent-middlewares/src/tools/filesystem/edit.rs rust-agent-middlewares/src/tools/filesystem/glob.rs rust-agent-middlewares/src/tools/filesystem/grep.rs rust-agent-middlewares/src/middleware/terminal.rs rust-agent-middlewares/src/tools/filesystem/folder.rs rust-agent-middlewares/src/tools/todo.rs rust-agent-middlewares/src/subagent/tool.rs; do echo "=== $f ==="; grep -c '_DESCRIPTION' "$f"; done`
  - 预期: 每个文件输出 >= 1（表示各有一个 DESCRIPTION 常量）

- [ ] 验证 description() 方法引用了常量而非行内字符串
  - `grep -A1 'fn description' rust-agent-middlewares/src/tools/filesystem/read.rs`
  - 预期: 输出包含 `READ_FILE_DESCRIPTION` 常量引用

- [ ] 验证工具名已替换为 snake_case（无 claude-code PascalCase 工具名残留）
  - `grep -rn 'use the Read tool\|use the Write tool\|use the Edit tool\|use the Glob tool\|use the Grep tool\|use the Bash tool' rust-agent-middlewares/src/tools/ rust-agent-middlewares/src/middleware/ rust-agent-middlewares/src/subagent/ || echo "clean"`
  - 预期: 输出 "clean"（0 匹配）

- [ ] 验证编译通过
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -3`
  - 预期: 输出包含 "Finished" 或 "Compiling"，无 error

- [ ] 验证 description 扩展测试通过
  - `cargo test -p rust-agent-middlewares --lib -- test_description_extended 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok"，0 failures

- [ ] 验证所有现有测试无回归
  - `cargo test -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"，0 failures


---

### Task 4: 清理与验收

**背景:**
Task 1 将系统提示词从单体文件拆分为 sections/ 目录下的独立段落，并删除了旧常量（`SYSTEM_PROMPT_TEMPLATE`、`SYSTEM_PROMPT_DEFAULT_AGENT`），但保留了旧文件（`system.md`、`default.md`）以确保 `include_str!` 编译兼容（Task 1 旧常量引用已删除，旧文件不再有编译依赖，但文件本身仍留在磁盘上）。Task 2 在此基础上添加了 `PromptFeatures` 参数。Task 3 扩展了工具提示词。本 Task 执行最终清理：删除旧文件，确保无 overrides 时 `build_system_prompt()` 使用空覆盖块（因为默认 Tone/Proactiveness 已包含在段落中），更新 CLAUDE.md 文档，运行全量测试和端到端验收。

**涉及文件:**
- 删除: `rust-agent-tui/prompts/system.md`
- 删除: `rust-agent-tui/prompts/default.md`
- 修改: `rust-agent-tui/src/prompt.rs`（简化 `None` 分支为空覆盖块；旧常量已在 Task 1 中删除，此处不再操作）
- 修改: `CLAUDE.md`（更新系统提示词架构说明、新增 PromptFeatures 说明、更新工具清单描述）

**执行步骤:**

- [ ] 删除旧提示词文件 `system.md`
  - 位置: `rust-agent-tui/prompts/system.md`
  - 命令: `rm rust-agent-tui/prompts/system.md`
  - 原因: 内容已拆分到 `sections/01-08` 文件中，模板替换机制已被 `include_str!` join 模式取代。`prompt.rs` 中引用此文件的 `SYSTEM_PROMPT_TEMPLATE` 常量已在 Task 1 中删除，不再有编译依赖

- [ ] 删除旧提示词文件 `default.md`
  - 位置: `rust-agent-tui/prompts/default.md`
  - 命令: `rm rust-agent-tui/prompts/default.md`
  - 原因: 默认覆盖块内容（Tone/style、Proactiveness）已包含在 `sections/06_tone_style.md` 和 `sections/02_system.md` 中，无 overrides 时覆盖块应为空字符串。`prompt.rs` 中引用此文件的 `SYSTEM_PROMPT_DEFAULT_AGENT` 常量已在 Task 1 中删除，不再有编译依赖

- [ ] 简化 `build_system_prompt()` 的 `None` 分支——无 overrides 时覆盖块为空字符串
  - 位置: `rust-agent-tui/src/prompt.rs` 的 `build_system_prompt()` 函数中 overrides_block 赋值处
  - 经代码分析确认: 旧实现中 `None` 分支使用 `SYSTEM_PROMPT_DEFAULT_AGENT`（`default.md` 全文）作为覆盖块。`default.md` 包含身份声明（→已拆入 01_intro.md）、Tone and style（→已拆入 06_tone_style.md）、Proactiveness（→已拆入 02_system.md）。新实现中这些内容已作为静态段落始终包含，因此无 overrides 时覆盖块应为空字符串，避免 Tone/Proactiveness 重复注入
  - 替换 `overrides_block` 赋值为:

    ```rust
    let overrides_block = overrides
        .map(build_agent_overrides_block)
        .unwrap_or_default();
    ```

  - 原因: `unwrap_or_default()` 在 `Option<String>` 上返回空字符串 `""`，`build_agent_overrides_block()` 不会返回 `Some("")`（空 parts 返回 `String::new()`），因此无 overrides 时不会在提示词开头注入任何覆盖块，默认行为已由静态段落覆盖

- [ ] 更新 `build_system_prompt()` 函数文档注释
  - 位置: `rust-agent-tui/src/prompt.rs` 的 `build_system_prompt()` 函数上方文档注释（~L31-L35）
  - 替换为:

    ```rust
    /// 构建系统提示词。
    ///
    /// 从 `prompts/sections/` 目录加载静态段落（01-08），根据 `PromptFeatures`
    /// 条件注入 feature-gated 段落（10-13），将环境占位符替换为运行时值。
    ///
    /// `overrides` 存在时，将 agent.md 中定义的角色/风格/主动性拼成一个覆盖块，
    /// 注入到提示词最前面；为 `None` 时覆盖块为空（默认行为已由静态段落覆盖）。
    ```

  - 原因: 文档需反映新的段落加载架构，不再提及 `{{agent_overrides}}` 占位符

- [ ] 更新 CLAUDE.md 中的系统提示词相关说明
  - 位置: `CLAUDE.md` 的 `## 关键模式` 章节（~L274-L305）
  - 将代码注释 `// 组装 agent（系统提示词通过 PrependSystemMiddleware 注入）` 更新为 `// 组装 agent（系统提示词通过 with_system_prompt() 注入）`
  - 将 `build_system_prompt(overrides, cwd)` 示例更新为 `build_system_prompt(overrides, cwd, PromptFeatures::detect())`
  - 原因: 旧注释引用已废弃的 `PrependSystemMiddleware`，函数签名已新增 `features` 参数

- [ ] 更新 CLAUDE.md 新增系统提示词架构说明
  - 位置: `CLAUDE.md` 的 `## 数据流` 章节下（在 `### TUI 异步通信` 之前），新增 `### 系统提示词架构` 子章节
  - 内容:

    ```markdown
    ### 系统提示词架构

    系统提示词通过 `build_system_prompt(overrides, cwd, features)` 函数合成，段落文件位于 `rust-agent-tui/prompts/sections/`：

    - **静态段落**（01-08，始终包含）：身份定义、系统行为、任务执行、危险操作、工具策略、语气风格、沟通方式、环境信息
    - **Feature-gated 段落**（10-13，条件包含）：HITL 审批、SubAgent、Cron、Skills
    - **动态覆盖块**：从 `AgentOverrides` 的 persona/tone/proactiveness 字段生成，注入到提示词最前面

    `PromptFeatures` 结构体控制条件段落注入：

    | 字段 | 触发条件 |
    |------|---------|
    | `hitl_enabled` | `YOLO_MODE=false`（`-a` CLI 参数） |
    | `subagent_enabled` | 默认 `true`（TODO: 从中间件注册状态推断） |
    | `cron_enabled` | 默认 `true`（TODO: 从中间件注册状态推断） |
    | `skills_enabled` | 默认 `true`（TODO: 从中间件注册状态推断） |
    ```

  - 原因: 记录新的提示词架构，帮助后续开发者理解段落加载和条件注入机制

- [ ] 为 `build_system_prompt()` 的清理行为编写单元测试
  - 测试文件: `rust-agent-tui/src/prompt.rs`（在 `#[cfg(test)] mod tests` 块中追加，该块由 Task 1 创建）
  - 测试场景:
    - **无 overrides 时覆盖块为空，不重复注入 Tone/Proactiveness**: `build_system_prompt(None, "/tmp", PromptFeatures::none())` 返回值中 `"# Tone and style"` 仅出现 1 次（来自 06_tone_style.md 静态段落，不来自覆盖块），`"# Proactiveness"` 仅出现 1 次（来自 02_system.md 静态段落）
    - **无 overrides 时提示词不以空行开头**: 返回值不以 `"\n\n"` 开头（`overrides_block` 为空字符串时不产生前导空行）
    - **有 overrides 时覆盖块正确注入**: `build_system_prompt(Some(&AgentOverrides { persona: Some("custom persona".into()), tone: None, proactiveness: None }), "/tmp", PromptFeatures::none())` 返回值以 `"custom persona"` 开头
    - **旧常量不存在**: `grep -c 'SYSTEM_PROMPT_TEMPLATE\|SYSTEM_PROMPT_DEFAULT_AGENT' rust-agent-tui/src/prompt.rs` 返回 0（此为检查步骤，测试中通过断言 `build_system_prompt` 不引用旧常量间接验证）
  - 运行命令: `cargo test -p rust-agent-tui --lib -- prompt::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [ ] 验证旧文件已删除
  - `ls rust-agent-tui/prompts/system.md rust-agent-tui/prompts/default.md 2>&1`
  - 预期: 输出 "No such file or directory"（两个文件均已删除）

- [ ] 验证 prompt.rs 不再包含旧常量引用
  - `grep -c 'SYSTEM_PROMPT_TEMPLATE\|SYSTEM_PROMPT_DEFAULT_AGENT' rust-agent-tui/src/prompt.rs`
  - 预期: 输出 0

- [ ] 验证 prompt.rs 不再包含 `include_str!("../prompts/system.md")` 或 `include_str!("../prompts/default.md")`
  - `grep -c 'prompts/system.md\|prompts/default.md' rust-agent-tui/src/prompt.rs`
  - 预期: 输出 0

- [ ] 验证 `unwrap_or_default()` 替换已生效
  - `grep 'unwrap_or_default' rust-agent-tui/src/prompt.rs`
  - 预期: 输出包含 `unwrap_or_default()`

- [ ] 验证编译通过（全 workspace）
  - `cargo build 2>&1 | tail -5`
  - 预期: 输出包含 "Finished"，无 error

- [ ] 验证全量测试通过
  - `cargo test 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok"，0 failures（所有 crate 的所有测试）

- [ ] 验证 CLAUDE.md 已更新
  - `grep -c 'PromptFeatures' CLAUDE.md`
  - 预期: 输出 >= 1（新增了 PromptFeatures 说明）

- [ ] 验证 CLAUDE.md 中不再引用 PrependSystemMiddleware
  - `grep -c 'PrependSystemMiddleware' CLAUDE.md`
  - 预期: 输出 0

---

### Task 5: 功能验收

**前置条件:**
- Task 1-4 全部完成
- 构建环境: `cargo build` 成功
- 测试环境: `cargo test` 全部通过

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test 2>&1 | tail -10`
   - 预期: 全部测试通过，0 failures
   - 失败排查: 检查各 Task 的检查步骤，逐个 crate 运行 `cargo test -p <crate-name>` 定位失败测试

2. 验证 TUI 启动正常
   - `cargo run -p rust-agent-tui -- --help 2>&1 || echo "exit: $?"`
   - 预期: 输出包含 CLI 参数帮助信息或程序正常启动
   - 失败排查: 检查 `prompt.rs` 中 `include_str!` 引用的段落文件是否全部存在于 `prompts/sections/` 目录

3. 验证 HITL 审批模式下 feature 段落注入
   - `YOLO_MODE=false cargo test -p rust-agent-tui --lib -- prompt::tests 2>&1 | tail -5`
   - 预期: 测试通过，`PromptFeatures::detect()` 在 `YOLO_MODE=false` 时 `hitl_enabled` 为 `true`
   - 失败排查: 检查 `PromptFeatures::detect()` 中 `YOLO_MODE` 环境变量读取逻辑

4. 验证 sections 目录结构完整（8 静态 + 4 feature-gated = 12 个文件）
   - `ls rust-agent-tui/prompts/sections/ | wc -l`
   - 预期: 输出 12
   - 失败排查: 检查 Task 1（01-08）和 Task 2（10-13）的段落文件创建步骤

5. 验证旧文件已完全清除
   - `find rust-agent-tui/prompts/ -maxdepth 1 -name '*.md' 2>/dev/null | wc -l`
   - 预期: 输出 0（顶层 prompts/ 目录下无 .md 文件，所有 .md 文件均在 sections/ 子目录中）
   - 失败排查: 检查 Task 4 的删除步骤是否执行

6. 验证工具 description 扩展生效
   - `cargo test -p rust-agent-middlewares --lib -- test_description_extended 2>&1 | tail -10`
   - 预期: 所有 9 个工具的 description 扩展测试通过
   - 失败排查: 检查 Task 3 各工具的 description 常量和测试
