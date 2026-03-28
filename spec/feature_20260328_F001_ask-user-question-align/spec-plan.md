# ask-user-question-align 执行计划

**目标:** 将 `ask_user` 工具全面对齐 Claude `AskUserQuestion`（名称、Schema、TUI 展示、前端展示）

**技术栈:** Rust 2021、tokio、serde_json、ratatui、Preact + htm

**设计文档:** [spec-design.md](./spec-design.md)

---

### Task 1: 核心数据结构变更

**涉及文件:**
- 修改: `rust-create-agent/src/interaction/mod.rs`
- 修改: `rust-create-agent/src/ask_user/mod.rs`

**执行步骤:**
- [x] 更新 `QuestionOption` struct：新增 `description: Option<String>` 字段
- [x] 更新 `QuestionItem` struct：新增 `header: String` 字段；移除 `allow_custom_input: bool` 和 `placeholder: Option<String>` 字段
- [x] 更新 `AskUserOption` struct（`ask_user/mod.rs`）：新增 `description: Option<String>` 字段
- [x] 更新 `AskUserQuestionData` struct：
  - `description` 字段改名为 `question`
  - 新增 `header: String` 字段
  - 移除 `allow_custom_input: bool` 和 `placeholder: Option<String>` 字段

**检查步骤:**
- [x] 数据结构编译通过（仅核心库）
  - `cargo build -p rust-create-agent 2>&1 | tail -5`
  - 预期: 输出包含 `Finished` 且无 `error`

---

### Task 2: 工具 Schema 与实现变更

**涉及文件:**
- 修改: `rust-agent-middlewares/src/ask_user/mod.rs`
- 修改: `rust-agent-middlewares/src/tools/ask_user_tool.rs`

**执行步骤:**
- [x] 重写 `ask_user_tool_definition()` 中的 JSON Schema：
  - 工具名改为 `ask_user_question`
  - 顶层参数改为 `questions` 数组（1–4 项）
  - 每项含 `question`（string）、`header`（string, ≤12字）、`multi_select`（bool, default false）、`options`（array of `{label, description?}`）
  - 移除 `type`、`allow_custom_input`、`placeholder`
- [x] 更新 `parse_ask_user()`：
  - 工具名检查从 `"ask_user"` 改为 `"ask_user_question"`
  - 返回类型从 `Option<AskUserQuestionData>` 改为 `Vec<AskUserQuestionData>`
  - 解析 `questions` 数组，每个 `QuestionItem` 映射到 `AskUserQuestionData`（含 `header`、`option.description`）
  - 更新 `InputOption` 结构体加入 `description: Option<String>`
  - 用 `multi_select: bool` 替换 `SelectType` 枚举
- [x] 更新 `AskUserTool`（`ask_user_tool.rs`）：
  - `name()` 返回 `"ask_user_question"`
  - `parse_question()` 解析 `questions` 数组（产出 `Vec<QuestionItem>`）
  - `invoke()` 对多问题拼接可读响应：
    ```
    [问: {header}]
    回答: {answer}
    ```
    单问题时直接返回答案字符串（不加前缀）
- [x] 更新 `rust-agent-middlewares/src/lib.rs` 中 `parse_ask_user` 的导出签名注释（返回类型变更）

**检查步骤:**
- [x] 中间件库编译通过
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: `Finished` 无 `error`
- [x] 工具名称正确
  - `cargo test -p rust-agent-middlewares --lib 2>&1 | grep "FAILED\|ok"`
  - 预期: 所有测试 `ok`，无 `FAILED`

---

### Task 3: TUI 桥接与展示更新

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent_ops.rs`
- 修改: `rust-agent-tui/src/app/ask_user_prompt.rs`
- 修改: `rust-agent-tui/src/ui/main_ui/popups/ask_user.rs`

**执行步骤:**
- [x] 更新 `agent_ops.rs` 中的桥接逻辑（`InteractionContext::Questions` 分支）：
  - `AskUserQuestionData` 构造：`description` → `question`，新增 `header: q.header.clone()`，移除 `allow_custom_input` 和 `placeholder`
  - `AskUserOption` 构造：新增 `description: o.description.clone()`
  - Relay 转发的 JSON：字段名同步更新（`description` → `question`，加 `header`，选项加 `description`，移除 `allow_custom_input`/`placeholder`）
- [x] 更新 `ask_user_prompt.rs` 中的 `QuestionState`：
  - 移除 `allow_custom_input` 字段引用
  - `total_rows()` 改为 `self.data.options.len() as isize + 1`（始终 +1，不再受 `allow_custom_input` 控制）
  - `move_option_cursor()` 中 `in_custom_input` 条件改为 `self.option_cursor == self.data.options.len() as isize`（不再检查 `allow_custom_input`）
- [x] 更新 `ask_user.rs` 弹窗渲染：
  - Tab 标签：将 `q.data.description.chars().take(8)` 改为 `q.data.header.chars().take(12)`
  - 选项渲染：在每个 `opt.label` 行之后，若 `opt.description` 为 `Some(desc)`，追加一行缩进的 `DarkGray` 文字
  - 弹窗高度计算：加入 description 行数（每个有 description 的选项多占1行）
  - 移除 `cur.data.allow_custom_input` 条件，始终渲染自定义输入行
  - 自定义输入占位符改为固定 `"输入自定义内容…"`（不再读 `data.placeholder`）

**检查步骤:**
- [x] TUI 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: `Finished` 无 `error`
- [x] TUI 单元测试通过
  - `cargo test -p rust-agent-tui --lib 2>&1 | grep "FAILED\|test result"`
  - 预期: `test result: ok`，无 `FAILED`

---

### Task 4: 前端 AskUserDialog 更新

**涉及文件:**
- 修改: `rust-relay-server/web/components/AskUserDialog.js`
- 修改: `rust-relay-server/web/components/AskUserDialog.css`（若存在，否则在现有 css 文件中添加样式）
- 修改: `rust-relay-server/src/static_files.rs`（touch 触发重编译）

**执行步骤:**
- [x] 更新 `AskUserDialogInner` 中的 key 逻辑：改为直接用 `q.tool_call_id`（移除多级 fallback）
- [x] 每个问题条目加 `header` 芯片：
  ```js
  q.header && html`<span class="ask-user-header-chip">${q.header}</span>`
  ```
  放在 `ask-user-question` div 之前
- [x] 选项展示加 `description`：将 `<span>${optLabel}</span>` 改为嵌套 div，description 用 `<small class="ask-user-opt-desc">` 展示
- [x] 自定义输入框条件由 `(!hasOptions || q.allow_custom_input)` 改为 `true`（始终显示）
- [x] 在对应 CSS 文件中添加：
  - `.ask-user-header-chip`：小标签样式（背景色、圆角、字体大小）
  - `.ask-user-opt-desc`：选项说明文字样式（灰色、较小字体）
- [x] `touch rust-relay-server/src/static_files.rs` 触发 `include_bytes!` 重新打包

**检查步骤:**
- [x] relay-server 编译通过
  - `cargo build -p rust-relay-server --features server 2>&1 | tail -5`
  - 预期: `Finished` 无 `error`
- [x] 前端文件包含 header-chip 关键词
  - `grep -c "ask-user-header-chip" rust-relay-server/web/components/AskUserDialog.js`
  - 预期: 输出 `1` 或以上（说明已加入 header chip）
- [x] 前端文件包含 opt-desc 关键词
  - `grep -c "ask-user-opt-desc" rust-relay-server/web/components/AskUserDialog.js`
  - 预期: 输出 `1` 或以上

---

### Task 5: ask-user-question-align Acceptance

**Prerequisites:**
- 确保已完成 Task 1–4
- 构建命令: `cargo build 2>&1 | tail -5`
- 无需外部服务，纯本地编译验证

**End-to-end verification:**

1. ✅ 全 workspace 编译无错误
   - `cargo build 2>&1 | grep "^error"`
   - Expected: 无输出（无编译错误）
   - On failure: check Task 1–3（数据结构变更可能引发级联编译错误）

2. ✅ 工具名称已更新为 `ask_user_question`
   - `grep -r '"ask_user"' rust-agent-middlewares/src/ask_user/ rust-agent-middlewares/src/tools/ask_user_tool.rs`
   - Expected: 无输出（旧工具名已全部替换）
   - On failure: check Task 2（工具 Schema 实现）

3. ✅ 旧字段已从源码中移除
   - `grep -r "allow_custom_input\|\.placeholder" rust-agent-middlewares/src/ rust-agent-tui/src/ rust-create-agent/src/interaction/ rust-create-agent/src/ask_user/`
   - Expected: 无输出（旧字段已完全清除）
   - On failure: check Task 1（数据结构）、Task 2（工具实现）、Task 3（TUI）

4. ✅ 前端 header chip 样式已注入
   - `grep "ask-user-header-chip" rust-relay-server/web/components/AskUserDialog.js rust-relay-server/web/components/AskUserDialog.css 2>/dev/null | wc -l`
   - Expected: 输出 `2` 或以上（JS + CSS 各至少 1 处）
   - On failure: check Task 4（前端更新）
