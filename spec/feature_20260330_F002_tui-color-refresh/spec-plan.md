# tui-color-refresh 执行计划

**目标:** TUI 配色系统降噪，橙色只留给最高优先级交互，其余靠 MUTED/WARNING 自然分层

**技术栈:** ratatui 0.30，rust-agent-tui

**设计文档:** ./spec-design.md

---

### Task 1: Markdown 标题降噪

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/markdown/render_state.rs`

**执行步骤:**
- [x] 修改 `handle_event` 中 H1/H2 的颜色：`theme::ACCENT` → `theme::WARNING`
- [x] 移除 H1 的 `"── "` 前缀（`prefix` 字段改为 `None`），不再额外添加下划线效果
  - 关键：H1/H2 仅保留 `WARNING + BOLD` 样式，通过亮度差异区分层级

**检查步骤:**
- [x] 验证 render_state.rs 中 H1/H2 不再引用 theme::ACCENT
  - `grep -n "ACCENT" rust-agent-tui/src/ui/markdown/render_state.rs`
  - 预期: H1/H2 分支消失，仅 H3 及其以上保留原样

---

### Task 2: 工具名颜色分级

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/message_view.rs`

**执行步骤:**
- [x] 重写 `tool_color(name: &str) -> Color` 函数：
  ```rust
  pub fn tool_color(name: &str) -> Color {
      match name {
          "bash" => theme::ACCENT,
          "write_file" | "edit_file" | "folder_operations"
          | "delete_file" | "delete_folder" | "rm" | "rm_rf" => theme::WARNING,
          "read_file" | "glob_files" | "search_files_rg"
          | "launch_agent" | "ask_user_question" | "todo_write" => theme::MUTED,
          _ if name.contains("error") => theme::ERROR,
          _ => theme::MUTED,
      }
  }
  ```
  - `TOOL_NAME` 引用从该文件中移除（已无别名定义，直接查表）
- [x] 确保 `is_error` 路径仍优先返回 `ERROR`（两处调用点已有 `if is_error` 保护，`tool_color` 内部无需处理）

**检查步骤:**
- [x] 验证 write_file/edit_file/folder_operations 返回 WARNING
  - `grep -A5 "fn tool_color" rust-agent-tui/src/ui/message_view.rs | grep "WARNING"`
  - 预期: write_file/edit_file/folder_operations 在 WARNING 分支
- [x] 验证 bash 返回 ACCENT
  - `grep "bash" rust-agent-tui/src/ui/message_view.rs`
  - 预期: bash => theme::ACCENT

---

### Task 3: SubAgent 组边框语义化

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/message_render.rs`

**执行步骤:**
- [x] 确认 SubAgentGroup 渲染路径（render_view_model 返回 Vec<Line>，无 Block 边框）
- [x] 确认 agent_color 已使用 theme::SUB_AGENT (=SAGE)，无需改动

**检查步骤:**
- [x] 确认 agent_panel.rs 或 main_ui 中 SubAgent 渲染边框颜色为 SAGE
  - `grep -n "SAGE\|ACCENT" rust-agent-tui/src/ui/main_ui/panels/agent.rs | head -10`
  - 预期: SubAgent 相关边框为 SAGE 而非 ACCENT

---

### Task 4: 状态栏信息降噪

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui/status_bar.rs`

**执行步骤:**
- [x] 非 loading 状态下的 timer_color：`theme::ACCENT` → `theme::MUTED`
  - 位置: 第 33 行 `timer_color` 赋值，条件 `if app.core.loading { theme::LOADING } else { theme::ACCENT }`
  - 改为: `if app.core.loading { theme::LOADING } else { theme::MUTED }`
- [x] Agent 名称颜色：`theme::ACCENT` → `theme::MUTED`
  - 位置: 第 76 行和第 86 行（两处 agent name 渲染）

**检查步骤:**
- [x] 验证状态栏不再有 ACCENT
  - `grep -n "ACCENT" rust-agent-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 无输出（仅 loading 分支保留 LOADING 色）

---

### Task 5: 配置面板边框分层

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui/panels/model.rs`
- 修改: `rust-agent-tui/src/ui/main_ui/panels/relay.rs`
- 修改: `rust-agent-tui/src/ui/main_ui/panels/agent.rs`
- 修改: `rust-agent-tui/src/ui/main_ui/panels/thread_browser.rs`

**执行步骤:**
- [x] `model.rs` 第 22-23 行：`AliasConfig` 和 `Browse` 模式的 border_color `theme::ACCENT` → `theme::MUTED`
  - `Edit` 模式保持 `theme::WARNING`；`New` 模式保持 `theme::SAGE`；`ConfirmDelete` 保持 `theme::ERROR`
- [x] `relay.rs` 第 20 行：`View` 模式的 border_color `theme::ACCENT` → `theme::MUTED`；`Edit` 保持 `theme::WARNING`
- [x] `agent.rs` 第 28-30 行：边框 `theme::ACCENT` → `theme::MUTED`
- [x] `thread_browser.rs` 第 22-25 行：边框 `theme::ACCENT` → `theme::MUTED`

**检查步骤:**
- [x] 验证所有配置面板边框不再是纯 ACCENT
  - `grep -n "border_color.*ACCENT\|border_style.*ACCENT" rust-agent-tui/src/ui/main_ui/panels/model.rs rust-agent-tui/src/ui/main_ui/panels/relay.rs rust-agent-tui/src/ui/main_ui/panels/agent.rs rust-agent-tui/src/ui/main_ui/panels/thread_browser.rs`
  - 预期: 仅 model.rs 的 Edit 模式/ConfirmDelete 模式、relay.rs 的 Edit 模式可保留 ACCENT，其他为 MUTED

---

### Task 6: AskUser 弹窗边框 WARNING 化

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui/popups/ask_user.rs`

**执行步骤:**
- [x] `ask_user.rs` 第 23 行和第 26 行：边框 `theme::ACCENT` → `theme::WARNING`
  - 同时标题色从 `theme::ACCENT` 改为 `theme::WARNING`
- [x] 检查 popup 内其他 ACCENT 引用（如 active tab 背景色）是否保留

**检查步骤:**
- [x] 验证 ask_user 边框和标题色为 WARNING
  - `grep -n "ACCENT\|WARNING" rust-agent-tui/src/ui/main_ui/popups/ask_user.rs | grep -E "border|title|Style.*fg""
  - 预期: 边框和标题为 WARNING；active tab 背景色仍可保留 ACCENT（用于操作区分）

---

### Task 7: TUI-STYLE.md v1.1 同步更新

**涉及文件:**
- 修改: `TUI-STYLE.md`

**执行步骤:**
- [x] 版本号从 v1.0 更新为 v1.1，日期更新为今天
- [x] 配色对照表（"语义色"节）更新：
  - THINKING 色值: `#8C6EB4` → `#A78BFA`（对应 theme.rs 中的实际值）
  - 工具颜色映射表更新为三级分级（bash=ACCENT / 写操作=WARNING / 只读=MUTED）
  - 边框表更新：配置面板激活边框改为 MUTED，HITL 弹窗改为 WARNING
  - 状态栏颜色编码：时间/Agent 名改为 MUTED
- [x] 在"刻意回避"节后新增"配色变更说明"小节，简要记录 v1.0→v1.1 的关键变化

**检查步骤:**
- [x] 验证文档版本
  - `head -5 TUI-STYLE.md`
  - 预期: 包含 "v1.1 · 2026-03-30"

---

### Task 8: tui-color-refresh Acceptance

**Prerequisites:**
- 启动命令: `cargo run -p rust-agent-tui`
- 测试数据: 无需特殊数据，使用任意对话即可触发各 UI 元素

**End-to-end verification:**

1. [A] **H1/H2 标题颜色**
   - 向 TUI 发送一条包含 `# H1标题` 和 `## H2标题` 的 markdown 内容
   - Expected: H1/H2 显示为琥珀黄色（WARNING #C8942A），非橙色，无下划线前缀
   - On failure: 检查 Task 1 render_state.rs

2. [A] **工具名颜色 — bash**
   - 发送一条触发 `bash` 工具的消息（如 `ls -la`）
   - Expected: 工具名 "Shell" 显示为橙色（ACCENT #FF6B2B）
   - On failure: 检查 Task 2 message_view.rs tool_color() bash 分支

3. [A] **工具名颜色 — 写操作**
   - 触发 `write_file` 或 `edit_file` 工具
   - Expected: 工具名显示为琥珀黄色（WARNING #C8942A），非绿色
   - On failure: 检查 Task 2 message_view.rs write_file 分支

4. [A] **工具名颜色 — 只读操作**
   - 触发 `read_file` 或 `glob_files` 工具
   - Expected: 工具名显示为灰色（MUTED #8C7D78），无颜色强调
   - On failure: 检查 Task 2 message_view.rs 只读工具分支

5. [A] **工具执行成功 — SAGE 绿色**
   - 触发任意工具并成功执行（预期成功）
   - Expected: 工具结果区域（非工具名）显示绿色（SAGE #6EB56A）
   - On failure: 检查 message_render.rs ToolBlock content 渲染路径

6. [A] **配置面板边框 — MUTED**
   - 输入 `/model` 打开 ModelPanel
   - Expected: 面板边框为灰色（MUTED），非橙色；Tab 栏内 active 背景色保留 ACCENT
   - On failure: 检查 Task 5 model.rs border_color

7. [A] **状态栏 — 无橙色时间/Agent 名**
   - 运行 agent 并观察状态栏
   - Expected: 任务时长和 Agent 名称为灰色（MUTED），非橙色；loading spinner 仍为电光青色
   - On failure: 检查 Task 4 status_bar.rs

8. [A] **AskUser 弹窗边框**
   - 触发 `ask_user_question` 工具（如 agent 调用需用户选择的场景）
   - Expected: 弹窗边框为琥珀黄色（WARNING）
   - On failure: 检查 Task 6 ask_user.rs
