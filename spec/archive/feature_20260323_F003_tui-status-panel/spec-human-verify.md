# TUI 状态面板与工具显示优化 人工验收清单

**生成时间:** 2026-03-23 00:00
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 编译整个项目（无错误）: `cargo build -p rust-agent-tui 2>&1 | grep -E "^error" | wc -l`
- [ ] [AUTO] 全量测试通过: `cargo test -p rust-agent-tui 2>&1 | tail -3`
- [ ] [MANUAL] 确保已配置 API Key（`ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY`），以便运行 TUI 进行视觉验收

### 测试数据准备

- [ ] [MANUAL] 准备一个包含文件操作的测试 prompt，例如：「请读取 Cargo.toml 文件」（用于验证路径缩短）
- [ ] [MANUAL] 准备含 `todo_write` 工具调用的测试 prompt，例如：「用 todo_write 工具记录3个任务：任务A、任务B（进行中）、任务C（完成）」

---

## 验收项目

### 场景 1：代码结构与静态验证

#### - [x] 1.1 工具名/参数颜色分层代码实现

- **来源:** Task 2 检查步骤 + spec-design.md
- **操作步骤:**
  1. [A] `grep -n "args_display" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/message_render.rs` → 期望: 输出包含 `args_display` 字段的解构和条件渲染行
  2. [A] `grep -n "Color::DarkGray" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/message_render.rs` → 期望: 存在 `DarkGray` 着色行，对应 args_display 的参数文本
  3. [A] `grep -n "args_display: Option<String>" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/message_view.rs` → 期望: 输出含 `args_display: Option<String>` 的字段声明行
- **异常排查:**
  - 若步骤 1/2 无输出: 检查 `message_render.rs` 的 `ToolBlock` 渲染分支是否正确合并
  - 若步骤 3 无输出: 检查 `message_view.rs` 的 `MessageViewModel::ToolBlock` 变体定义

#### - [x] 1.2 路径缩短函数存在验证

- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -n "^fn strip_cwd\|^pub fn format_tool_args\|^pub fn format_tool_name" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/tool_display.rs` → 期望: 输出 3 行，分别包含 `strip_cwd`、`format_tool_args`、`format_tool_name` 定义
  2. [A] `grep -n "bash.*command\|search_files_rg" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/tool_display.rs` → 期望: `bash` 和 `search_files_rg` 分支不含 `strip_cwd` 调用，路径不剥离
- **异常排查:**
  - 若函数不存在: Task 1 执行步骤中函数定义未写入，重新检查 `tool_display.rs`

#### - [x] 1.3 TODO 面板布局代码

- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `grep -c "Constraint::Length" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/main_ui.rs` → 期望: 输出 `4`（标题栏 + TODO面板 + 输入框 + 帮助栏）
  2. [A] `grep -n "todo_height\|render_todo_panel\|todo_items.is_empty" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/main_ui.rs` → 期望: 3 个关键词均出现，说明 TODO 面板逻辑已插入
- **异常排查:**
  - 若 `Constraint::Length` 计数不为 4: Layout 未改为 5-slot，检查 `main_ui.rs` 的 constraints 数组
  - 若 `render_todo_panel` 不存在: 函数未添加，检查文件末尾

#### - [x] 1.4 旧状态清理完整性

- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `grep -rn "todo_message_index" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ 2>/dev/null | wc -l` → 期望: 输出 `0`（无任何残留引用）
  2. [A] `grep -rn "TodoStatus\|todo_status\|render_todos" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ | grep -v "main_ui" | wc -l` → 期望: 输出 `0`（main_ui.rs 中的枚举使用除外）
- **异常排查:**
  - 若有残留: 使用具体文件路径定位残留引用，手动清理

---

### 场景 2：视觉效果验收（需运行 TUI）

> **前置操作：** 在项目根目录运行 `cargo run -p rust-agent-tui -- -y`（YOLO 模式跳过审批），保持终端打开

#### - [x] 2.1 工具标题颜色分层视觉效果

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -A15 "fn render_view_model" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/message_render.rs | grep -c "Modifier::BOLD"` → 期望: 输出 `1` 或以上（工具名有 BOLD 修饰符）
  2. [H] 在 TUI 中输入「请读取 Cargo.toml 文件」，等待 Agent 执行 `read_file` 工具调用后，观察聊天记录区域的工具调用条目标题行：工具名称（如 `ReadFile`）是否比参数文字更亮/颜色更饱和、参数文字（如 `Cargo.toml`）是否显示为暗灰色（DimGray）？ → 是/否
  3. [H] 同一工具条目中，工具名称颜色（如 `ReadFile` 显示青色）与参数文字颜色（DarkGray）是否明显不同，能一眼区分？ → 是/否
- **异常排查:**
  - 若颜色无区分: 检查 `message_render.rs` 的 `ToolBlock` 分支，确认 `header_spans` 拆分是否生效
  - 若 args 不显示: 检查 `agent.rs` 中 `AgentEvent::ToolCall` 的 `args` 字段是否被正确赋值

#### - [x] 2.2 文件路径显示为相对路径

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [A] `pwd` → 期望: 输出当前工作目录绝对路径（记下，如 `/Users/konghayao/code/ai/perihelion`），后续验证路径缩短是否生效
  2. [H] 在 TUI 中输入「请读取 Cargo.toml」，等待工具调用完成后，查看工具调用标题行的参数部分：显示的是相对路径（如 `Cargo.toml`）还是绝对路径（如 `/Users/konghayao/code/ai/perihelion/Cargo.toml`）？ → 相对路径/绝对路径
  3. [H] 再输入「请读取 rust-agent-tui/src/main.rs」，工具调用标题是否显示为 `rust-agent-tui/src/main.rs` 而非完整绝对路径？ → 是/否
- **异常排查:**
  - 若显示绝对路径: 检查 `agent.rs` 中 `cwd_for_handler` 是否正确传入 `format_tool_args`，以及 `strip_cwd` 的 prefix 逻辑

#### - [x] 2.3 bash 和 search_files_rg 参数不缩短

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在 TUI 中输入「执行 bash 命令：echo hello world」，等待工具调用后，查看工具标题行：`bash` 工具的参数显示是否为完整命令（如 `echo hello world`），而非路径格式？ → 是/否
- **异常排查:**
  - 若 bash 参数被路径化处理: 检查 `tool_display.rs` 的 `format_tool_args` 中 `bash` 分支，确认未调用 `strip_cwd`

---

### 场景 3：TODO 状态面板行为

> **前置操作：** 保持 TUI 运行（`cargo run -p rust-agent-tui -- -y`）

#### - [x] 3.1 TODO 面板显示与消失

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] TUI 刚启动或空闲状态下，输入框上方（消息区与输入框之间）是否**没有**出现任何 TODO 面板区域（布局紧凑，输入框贴近消息区）？ → 是（无面板）/否
  2. [H] 输入「用 todo_write 工具记录3个任务：任务A待办、任务B进行中、任务C完成」并发送，等待 Agent 调用 `todo_write` 后，输入框上方是否**出现**了带有青色或黄色边框、标题为「📋 TODO」的面板，并展示3条任务？ → 是/否
- **异常排查:**
  - 若面板不出现: 检查 `app/mod.rs` 中 `TodoUpdate` 分支是否正确设置 `self.todo_items`，以及 `main_ui.rs` 的 `todo_height` 计算是否生效

#### - [x] 3.2 TODO 条目颜色分类

- **来源:** spec-design.md 验收标准（面板颜色规范）
- **操作步骤:**
  1. [H] 在 TODO 面板中，「任务B进行中」条目（图标 `→`）是否显示为黄色加粗字体？ → 是/否
  2. [H] 「任务C完成」条目（图标 `✓`）是否显示为暗灰色（DarkGray，视觉上较暗）？而「任务A待办」条目（图标 `○`）是否显示为白色？ → 是/否
- **异常排查:**
  - 若颜色不符: 检查 `main_ui.rs` 的 `render_todo_panel` 函数中 `match item.status` 分支颜色配置

#### - [x] 3.3 新建 thread 时 TODO 面板清空

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在 TODO 面板可见的状态下（上一步已有任务），输入 `/clear` 或 `/history` 新建对话（或使用 Esc 新建），重置对话后 TODO 面板是否**消失**，布局恢复为输入框贴近消息区？ → 是/否
- **异常排查:**
  - 若面板未消失: 检查 `app/mod.rs` 的 `new_thread()` 函数是否包含 `self.todo_items.clear()`

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | 工具名/参数颜色分层代码 | 3 | 0 | ⬜ | |
| 场景 1 | 1.2 | 路径缩短函数存在 | 2 | 0 | ⬜ | |
| 场景 1 | 1.3 | TODO 面板布局代码 | 2 | 0 | ⬜ | |
| 场景 1 | 1.4 | 旧状态清理完整性 | 2 | 0 | ⬜ | |
| 场景 2 | 2.1 | 工具标题颜色分层视觉效果 | 1 | 2 | ⬜ | |
| 场景 2 | 2.2 | 文件路径显示为相对路径 | 1 | 2 | ⬜ | |
| 场景 2 | 2.3 | bash 参数不缩短 | 0 | 1 | ⬜ | |
| 场景 3 | 3.1 | TODO 面板显示与消失 | 0 | 2 | ⬜ | |
| 场景 3 | 3.2 | TODO 条目颜色分类 | 0 | 2 | ⬜ | |
| 场景 3 | 3.3 | 新建 thread 时面板清空 | 0 | 1 | ⬜ | |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
