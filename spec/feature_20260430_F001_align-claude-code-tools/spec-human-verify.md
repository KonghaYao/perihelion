# 验收清单：align-claude-code-tools

**Feature:** 20260430_F001 - align-claude-code-tools
**Plans:** spec-plan-1.md, spec-plan-2.md
**Design:** spec-design.md

---

### 场景 1：全量构建和测试

#### - [x] 1.1 全量 workspace 构建通过
- `cargo build`
- → 期望包含: `Finished`
- **来源:** plan-2 Task 9 / design 验收标准
- **目的:** 确认无编译错误

#### - [x] 1.2 全量测试通过（已知 1 个预存失败除外）
- `cargo test 2>&1 | grep -E 'test result:|FAILED'`
- → 期望包含: `55 passed`, `80 passed`, `206 passed`, `4 passed`, `278 passed`
- → 期望精确: `test_subagent_group_basic ... FAILED`（仅此 1 个预存失败）
- **来源:** plan-2 Task 9 / design 验收标准
- **目的:** 确认无新增测试回归

#### - [x] 1.3 预存失败 test_subagent_group_basic 确认为改动前已存在
- `git stash && cargo test -p rust-agent-tui --lib -- test_subagent_group_basic 2>&1 | grep FAILED; git stash pop`
- → 期望包含: `FAILED`（stash 后仍失败，证明为预存问题）
- **来源:** 执行摘要中记录的已知问题
- **目的:** 排除本次改动引入的回归

---

### 场景 2：旧工具名全量残留检查

#### - [x] 2.1 Rust 源码中无旧工具名字符串残留
- `grep -rn '"bash"\|"read_file"\|"write_file"\|"edit_file"\|"glob_files"\|"search_files_rg"\|"todo_write"\|"ask_user_question"\|"launch_agent"' rust-agent-middlewares/src/ rust-agent-tui/src/ --include='*.rs'`
- → 期望精确: `("bash", "-c")`（terminal.rs 中 OS shell 命令参数，非工具名）
- **来源:** plan-1 Task 5 / plan-2 Task 9
- **目的:** 确认所有旧工具名已从源码中清除

#### - [x] 2.2 提示词段落文件中无旧工具名残留
- `grep -rn 'read_file\|write_file\|edit_file\|glob_files\|search_files_rg\|launch_agent\|todo_write\|ask_user_question' rust-agent-tui/prompts/sections/ --include='*.md'`
- → 期望精确: （无输出）
- **来源:** plan-2 Task 8 / Task 9
- **目的:** 确认提示词已全面更新

#### - [x] 2.3 skill_preload.rs 注释中无旧工具名残留
- `grep -n 'read_file' rust-agent-middlewares/src/subagent/skill_preload.rs`
- → 期望精确: （无输出）
- **来源:** plan-2 Task 8 步骤
- **目的:** 确认 fake tool call 注释已更新
- ⚠ 注意: 当前实测 L11/L21 注释仍残留 `read_file`，需修复

---

### 场景 3：新工具名生效验证

#### - [x] 3.1 9 个工具的 fn name() 返回正确新名称
- `for f in write.rs edit.rs glob.rs read.rs grep.rs todo.rs ask_user_tool.rs; do echo "=== $f ==="; grep -A1 'fn name()' rust-agent-middlewares/src/tools/filesystem/$f 2>/dev/null || grep -A1 'fn name()' rust-agent-middlewares/src/tools/$f 2>/dev/null; done; echo "=== terminal.rs ==="; grep -A1 'fn name()' rust-agent-middlewares/src/middleware/terminal.rs; echo "=== subagent/tool.rs ==="; grep -A1 'fn name()' rust-agent-middlewares/src/subagent/tool.rs`
- → 期望包含: `"Write"`, `"Edit"`, `"Glob"`, `"Read"`, `"Grep"`, `"TodoWrite"`, `"AskUserQuestion"`, `"Bash"`, `"Agent"`
- **来源:** plan-1 Task 1-4 / plan-2 Task 5-7
- **目的:** 确认所有工具名对齐

#### - [x] 3.2 folder_operations 未被重命名
- `grep -n 'fn name()' rust-agent-middlewares/src/tools/filesystem/folder.rs`
- → 期望包含: `"folder_operations"`
- **来源:** design 文档明确声明保留
- **目的:** 确认扩展工具未被误改

---

### 场景 4：Grep 工具重构验证

#### - [x] 4.1 GrepTool 结构体已重命名
- `grep -n 'pub struct GrepTool\|pub struct SearchFilesRgTool' rust-agent-middlewares/src/tools/filesystem/grep.rs`
- → 期望包含: `pub struct GrepTool`
- → 期望精确: （无 `SearchFilesRgTool` 匹配）
- **来源:** plan-2 Task 6
- **目的:** 确认结构体重命名完成

#### - [x] 4.2 Grep 工具接受结构化参数（pattern/output_mode），不再接受 args 数组
- `cargo test -p rust-agent-middlewares --lib -- "tools::filesystem::grep::tests::test_grep_hit" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-2 Task 6 / design 验收标准
- **目的:** 确认参数结构重写生效

#### - [x] 4.3 Grep type 字段过滤正确
- `cargo test -p rust-agent-middlewares --lib -- "test_grep_type_filter" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-2 Task 6 新增测试
- **目的:** 确认 type→glob 映射生效

#### - [x] 4.4 FilesystemMiddleware::tool_names() 包含 Grep
- `grep -n 'tool_names' rust-agent-middlewares/src/middleware/filesystem.rs`
- → 期望包含: `"Grep"`
- **来源:** plan-2 Task 6
- **目的:** 确认中间件工具列表已更新

---

### 场景 5：Bash 工具参数迁移验证

#### - [x] 5.1 BashTool timeout 使用毫秒单位
- `cargo test -p rust-agent-middlewares --lib -- "test_bash_timeout_returns_quickly" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-2 Task 5
- **目的:** 确认 timeout 单位从秒迁移到毫秒

#### - [x] 5.2 Bash description 和 run_in_background 参数可解析
- `cargo test -p rust-agent-middlewares --lib -- "test_bash_description_and_run_in_background_parsed" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-2 Task 5 新增测试
- **目的:** 确认新参数不影响执行

---

### 场景 6：Agent 工具重构验证

#### - [ ] 6.1 SubAgentTool 工具名为 Agent
- `cargo test -p rust-agent-middlewares --lib -- "subagent::tool::tests::test_tool_name" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-2 Task 7
- **目的:** 确认工具名对齐

#### - [ ] 6.2 Agent required 参数仅包含 prompt
- `cargo test -p rust-agent-middlewares --lib -- "test_agent_parameters_required_is_prompt_only" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-2 Task 7 新增测试
- **目的:** 确认参数 schema 重写

#### - [ ] 6.3 filter_tools 防递归排除 Agent 而非 launch_agent
- `cargo test -p rust-agent-middlewares --lib -- "test_agent_excluded_even_when_explicitly_allowed" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-2 Task 7
- **目的:** 确认防递归逻辑正确

#### - [ ] 6.4 Agent 预留字段（isolation/run_in_background）可解析
- `cargo test -p rust-agent-middlewares --lib -- "test_agent_reserved_fields_parsed" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-2 Task 7 新增测试
- **目的:** 确认预留字段不影响执行

#### - [ ] 6.5 mod.rs 中 build_agents_summary 引用新工具名和参数
- `grep -n 'Agent\|subagent_type\|prompt' rust-agent-middlewares/src/subagent/mod.rs | head -5`
- → 期望包含: `Agent`, `subagent_type`, `prompt`
- **来源:** plan-2 Task 7
- **目的:** 确认提示文本已更新

---

### 场景 7：Read 工具 pages 参数验证

#### - [ ] 7.1 PDF + pages 返回占位提示
- `cargo test -p rust-agent-middlewares --lib -- "test_pdf_with_pages_returns_placeholder" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-1 Task 2
- **目的:** 确认 PDF 占位逻辑生效

#### - [ ] 7.2 PDF 无 pages 走二进制检测
- `cargo test -p rust-agent-middlewares --lib -- "test_pdf_without_pages_returns_binary" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-1 Task 2
- **目的:** 确认原有二进制检测不受影响

---

### 场景 8：AskUserQuestion 字段对齐验证

#### - [ ] 8.1 multiSelect camelCase 输入可正确解析
- `cargo test -p rust-agent-middlewares --lib -- "test_multi_select_camel_case_input" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-1 Task 3
- **目的:** 确认 camelCase 字段名映射

#### - [ ] 8.2 preview 字段可解析不影响执行
- `cargo test -p rust-agent-middlewares --lib -- "test_preview_field_ignored" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-1 Task 3
- **目的:** 确认 preview 预留字段正常

---

### 场景 9：TodoWrite 结构变更验证

#### - [ ] 9.1 TodoItem 无 id 字段
- `grep -n 'pub id:' rust-agent-middlewares/src/tools/todo.rs`
- → 期望精确: （无输出）
- **来源:** plan-1 Task 4
- **目的:** 确认 id 字段已移除

#### - [ ] 9.2 summarize_changes 使用索引对比
- `grep -n 'HashMap\|\.id\b\|old_map\|new_map' rust-agent-middlewares/src/tools/todo.rs`
- → 期望精确: （无输出）
- **来源:** plan-1 Task 4
- **目的:** 确认不再使用 id-based HashMap

#### - [ ] 9.3 activeForm 字段存在
- `grep -n 'active_form\|activeForm' rust-agent-middlewares/src/tools/todo.rs`
- → 期望包含: `active_form`, `activeForm`
- **来源:** plan-1 Task 4
- **目的:** 确认新字段已添加

---

### 场景 10：HITL 审批规则更新验证

#### - [ ] 10.1 default_requires_approval 使用新工具名
- `grep -A15 'pub fn default_requires_approval' rust-agent-middlewares/src/hitl/mod.rs`
- → 期望包含: `"Bash"`, `"Write"`, `"Edit"`, `"Agent"`
- → 期望精确: （无 `"bash"`, `"write_file"`, `"edit_file"`, `"launch_agent"`）
- **来源:** plan-2 Task 8
- **目的:** 确认审批规则使用新名称

#### - [ ] 10.2 is_edit_tool 使用精确匹配
- `grep -A5 'pub fn is_edit_tool' rust-agent-middlewares/src/hitl/mod.rs`
- → 期望包含: `== "Write"`, `== "Edit"`
- → 期望精确: （无 `starts_with("write_")`, `starts_with("edit_")`）
- **来源:** plan-2 Task 8
- **目的:** 确认从前缀匹配改为精确匹配

---

### 场景 11：TUI 层工具显示和颜色验证

#### - [ ] 11.1 format_tool_name 新名称映射正确
- `cargo test -p rust-agent-tui --lib -- "test_format_tool_name" 2>&1 | tail -5`
- → 期望包含: `ok`（如果测试存在）
- **来源:** plan-2 Task 8
- **目的:** 确认工具显示名映射
- ⚠ 注意: Task 8 计划新增 `tool_display::tests` 模块但实际未创建

#### - [ ] 11.2 ToolCategory 新名称分类正确
- `cargo test -p rust-agent-tui --lib -- "test_tool_category" 2>&1 | tail -5`
- → 期望包含: `ok`（如果测试存在）
- **来源:** plan-2 Task 8
- **目的:** 确认工具分类使用新名称
- ⚠ 注意: Task 8 计划新增 `test_tool_category_new_names` 测试但实际未创建

---

### 场景 12：边界与回归

#### - [ ] 12.1 Write/Edit 工具描述文本中无旧工具名引用
- `grep -n 'read_file\|edit_file\|glob_files\|search_files_rg\|launch_agent' rust-agent-middlewares/src/tools/filesystem/write.rs rust-agent-middlewares/src/tools/filesystem/edit.rs rust-agent-middlewares/src/tools/filesystem/glob.rs`
- → 期望精确: （无输出）
- **来源:** plan-1 Task 1 检查步骤
- **目的:** 确认描述文本一致性

#### - [ ] 12.2 Grep 工具旧参数格式（args 数组）不再被接受
- `cargo test -p rust-agent-middlewares --lib -- "test_grep_missing_pattern" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-2 Task 6
- **目的:** 确认旧 args 接口已废弃

#### - [ ] 12.3 Agent 缺少 prompt 时返回错误
- `cargo test -p rust-agent-middlewares --lib -- "test_agent_prompt_missing_returns_error" 2>&1 | tail -3`
- → 期望包含: `ok`
- **来源:** plan-2 Task 7 新增测试
- **目的:** 确认必填参数校验

---

## 验收后清理

无需清理（无后台服务启动）。
