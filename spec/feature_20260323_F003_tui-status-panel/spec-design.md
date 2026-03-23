# Feature: 20260323_F003 - tui-status-panel

## 需求背景

TUI 的工具调用显示目前存在以下问题：

1. **工具名称与参数颜色混同**：`ToolBlock` 标题行将工具名和参数合并为一个字符串，统一用工具颜色渲染，视觉区分度低。
2. **路径参数冗长**：文件操作工具（`read_file`、`write_file` 等）显示的路径包含完整绝对路径，占用大量空间，实际上只需显示相对路径。
3. **TODO 状态嵌入消息流**：`TodoStatus` 消息混入消息历史流中，随着消息滚动消失，且 TODO 写操作本身也会产生 `ToolBlock` 噪音，无法在视觉上固定展示当前任务进度。

## 目标

- 工具名称用对应颜色 + BOLD 高亮，参数描述统一用 `DarkGray`（dimmed），二者视觉分层清晰
- 对文件操作工具的路径参数做 pwd 前缀剥离，显示相对路径，保持简短
- 在输入框正上方引入固定的 TODO 状态面板，展示完整 TodoItem 列表；无内容时高度收缩为 0

## 方案设计

### 1. 工具调用颜色分层

**涉及文件：**
- `rust-agent-tui/src/ui/message_render.rs`（渲染层）
- `rust-agent-tui/src/ui/message_view.rs`（ViewModel）
- `rust-agent-tui/src/app/tool_display.rs`（格式化工具）

**ViewModel 变更：**

`ToolBlock` 变体新增 `args_display: Option<String>` 字段，将工具名称和参数描述拆开存储：

```rust
ToolBlock {
    tool_name: String,
    display_name: String,     // PascalCase 工具名（不含参数）
    args_display: Option<String>,  // 参数摘要（可选）
    content: String,
    is_error: bool,
    collapsed: bool,
    color: Color,
}
```

**渲染变更（`message_render.rs`）：**

`ToolBlock` 标题行拆为两个 Span：

```
[icon] [PascalName]      ← 工具颜色 + BOLD
       ([args_display])  ← DarkGray
```

示例效果：
```
⚙ ReadFile  (src/main.rs)
⚙ Bash  (cargo build)
⚙ EditFile  (Cargo.toml)
```

**`format_tool_call_display` 拆分：**

`tool_display.rs` 中将函数一分为二：
- `format_tool_name(tool: &str) -> String`：返回 PascalCase 工具名
- `format_tool_args(tool: &str, input: &serde_json::Value, cwd: Option<&str>) -> Option<String>`：返回参数摘要（含路径缩短逻辑）

### 2. 路径参数缩短

**策略：**

对以下工具的路径参数做 `strip_prefix(cwd)` 处理：
- `read_file`（`file_path` 字段）
- `write_file`（`file_path` 字段）
- `edit_file`（`file_path` 字段）
- `glob_files`（`pattern` 字段，如果以 `/` 开头则尝试剥离）
- `folder_operations`（`folder_path` 字段）

**不缩短的工具（显示原始内容）：**
- `bash`（`command` 字段，命令本身不含 cwd）
- `search_files_rg`（`args` 数组，已经是简短参数）

**路径剥离逻辑：**

```rust
fn strip_cwd(path: &str, cwd: Option<&str>) -> String {
    if let Some(cwd) = cwd {
        let base = if cwd.ends_with('/') { cwd.to_string() } else { format!("{}/", cwd) };
        if let Some(rel) = path.strip_prefix(&base) {
            return rel.to_string();
        }
    }
    // fallback：取最后一段文件名
    path.rsplit('/').next().unwrap_or(path).to_string()
}
```

**cwd 传递链：**

TUI 中 `App.cwd` → `MessageViewModel::tool_block(tool_name, display, is_error, cwd)` → `format_tool_args(tool, input, Some(&cwd))` → 参数摘要。

### 3. TODO 状态面板（固定布局区域）

![TODO 面板与布局示意](./images/01-layout.png)

**布局变更（`main_ui.rs`）：**

在现有 4-slot layout 中插入第 3 个槽：

```rust
Layout::default()
    .direction(Direction::Vertical)
    .constraints([
        Constraint::Length(1),              // [0] 标题栏
        Constraint::Min(3),                 // [1] 消息区
        Constraint::Length(todo_height),    // [2] TODO 面板（动态 0 或 N+2）
        Constraint::Length(input_height),   // [3] 输入框
        Constraint::Length(1),              // [4] 帮助栏
    ])
```

`todo_height` 计算：
```rust
let todo_height = if app.todo_items.is_empty() {
    0
} else {
    (app.todo_items.len() as u16 + 2).min(10)
};
```

**面板渲染函数 `render_todo_panel`：**

- 有边框（`Borders::ALL`），标题 `" 📋 TODO "`
- 边框颜色：loading 中用 `Color::Yellow`，空闲用 `Color::Cyan`
- 每行 TodoItem：
  - `Pending`：`○`，颜色 `Color::White`
  - `InProgress`：`→`，颜色 `Color::Yellow` + BOLD
  - `Completed`：`✓`，颜色 `Color::DarkGray`
- 超出面板高度时截断（最多显示 8 条）

**App 状态变更：**

```rust
// App struct 新增
pub todo_items: Vec<TodoItem>,

// 删除
pub todo_message_index: Option<usize>,
```

`handle_agent_event(TodoUpdate)` 分支变为：
```rust
AgentEvent::TodoUpdate(todos) => {
    self.todo_items = todos;
    (true, false, false)
}
```

不再写入消息流，不再需要 `LoadHistory` 重渲染。

### 4. 渲染流程整合

TODO 面板属于主 UI 线程（`render` 函数）直接渲染，从 `App.todo_items` 读取，无需经过渲染线程（`render_thread.rs`）的消息流管道。现有渲染线程只负责消息气泡区渲染，职责不变。

## 实现要点

1. **ViewModel 改动向后兼容**：`from_base_message` 在历史消息回放时无法获取 cwd，`args_display` 直接从 input JSON 格式化，不做路径剥离（历史消息仅做简略展示）；实时事件流通过 `app.cwd` 补全剥离。
2. **路径剥离安全性**：`strip_prefix` 仅在路径以 `cwd/` 开头时生效，否则 fallback 到显示末段文件名，不会 panic。
3. **`todo_height = 0` 时的渲染**：ratatui 的 `Constraint::Length(0)` 会使该区域不占空间，`render_todo_panel` 在 `area.height == 0` 时提前返回，不渲染任何内容。

## 约束一致性

- 本方案仅修改 `rust-agent-tui` 的 UI 层（`src/ui/`, `src/app/`），不涉及 `rust-create-agent` 和 `rust-agent-middlewares` 核心 crate，符合模块边界约定。
- TUI 渲染双线程架构（主 UI 线程 + 渲染线程）不变，TODO 面板直接由主 UI 线程渲染，消息流仍走渲染线程，职责分离保持清晰。
- 不引入新依赖，不改变现有持久化或事件通信机制。

## 验收标准

- [ ] 工具调用 `ToolBlock` 标题行：工具名用对应颜色 + BOLD 显示，参数用 `DarkGray` 显示，视觉上可区分
- [ ] `read_file`/`write_file`/`edit_file`/`glob_files`/`folder_operations` 的路径参数显示为相对路径（相对于当前 cwd），绝对路径已剥离
- [ ] `bash` 和 `search_files_rg` 的参数不做路径缩短处理
- [ ] 当有 TodoItems 时，输入框正上方出现 TODO 状态面板，展示完整列表
- [ ] 当 TodoItems 为空时，TODO 面板高度为 0，不占用任何布局空间
- [ ] TODO 面板不再出现在消息历史流中（消息流中无 `TodoStatus` 气泡）
- [ ] 面板中 InProgress 条目用黄色 BOLD 高亮，Completed 条目用 DarkGray，Pending 用白色
- [ ] 新建 thread 时 TODO 面板清空消失
