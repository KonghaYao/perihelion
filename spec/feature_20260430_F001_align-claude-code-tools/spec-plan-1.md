# Claude Code 工具接口对齐 执行计划（上）

**目标:** 将 Perihelion 现有 10 个工具的名称和参数对齐 Claude Code，实现系统提示词和 agent 定义的零迁移复用

**技术栈:** Rust 2021 edition, async-trait, serde_json, grep crate, ignore crate, tokio

**设计文档:** spec-design.md

## 改动总览

本次计划覆盖 Task 1-4，涉及 `rust-agent-middlewares/src/tools/filesystem/`（Write/Edit/Glob/Read 四个工具定义）、`rust-agent-middlewares/src/tools/`（AskUserTool、TodoWrite）、`rust-agent-tui/src/app/agent.rs`（AskUserQuestion 事件路由）共 7 个源文件的工具名和参数结构对齐。按工具粒度拆分 Task 以保证原子性和可回滚性。

- Task 1~4 各自独立修改一个工具或一组关联工具，互不依赖，可并行执行
- Task 5（本文件验收）依赖 Task 1~4 全部完成，执行全量测试和旧名称残留检查
- 关键设计决策：Read 工具新增 `pages` 参数仅做占位提示（PDF 读取尚未支持），保持接口兼容；TodoWrite 移除 `id` 字段后改用数组索引对比变更摘要，经代码确认 TUI 层不访问 `id` 字段

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。本文件涵盖 `rust-agent-middlewares` 和 `rust-agent-tui` 两个 crate，需验证全量构建和中间件测试。

**执行步骤:**
- [ ] 验证全量构建可用
  - `cargo build`
  - 预期: 构建成功，无编译错误
- [ ] 验证中间件 crate 测试框架可用（dry run）
  - `cargo test -p rust-agent-middlewares --no-run`
  - 预期: 测试编译成功，无配置错误

**检查步骤:**
- [ ] 全量构建成功
  - `cargo build`
  - 预期: 构建成功，无错误
- [ ] 中间件测试编译成功
  - `cargo test -p rust-agent-middlewares --no-run`
  - 预期: 测试二进制编译成功，无配置错误

---

### Task 1: Write/Edit/Glob 仅改名

**背景:**
将 `write_file`、`edit_file`、`glob_files` 三个工具的 `fn name()` 返回值分别改为 `Write`、`Edit`、`Glob`，对齐 Claude Code 工具命名规范。这三个工具的参数结构完全不变，仅需修改工具名和描述文本中引用旧名的部分。Task 2~8 的提示词更新依赖本 Task 的工具名生效；本 Task 不依赖其他 Task。

**涉及文件:**
- 修改: `rust-agent-middlewares/src/tools/filesystem/write.rs`
- 修改: `rust-agent-middlewares/src/tools/filesystem/edit.rs`
- 修改: `rust-agent-middlewares/src/tools/filesystem/glob.rs`

**执行步骤:**
- [ ] 修改 WriteFileTool 的工具名 — 将 `fn name()` 返回值从 `"write_file"` 改为 `"Write"`
  - 位置: `rust-agent-middlewares/src/tools/filesystem/write.rs` → `impl BaseTool for WriteFileTool` 的 `fn name()` (~L33)
  - 将 `"write_file"` 改为 `"Write"`
  - 原因: 对齐 Claude Code 工具命名

- [ ] 更新 WriteFileTool 描述中引用 `read_file` 的文本 — 描述中提到 "use the read_file tool"，`read_file` 将在 Task 2 中改名为 `Read`，此处同步更新以保持一致
  - 位置: `rust-agent-middlewares/src/tools/filesystem/write.rs` → `WRITE_FILE_DESCRIPTION` 常量 (~L10)
  - 将 `read_file tool` 改为 `Read tool`
  - 原因: 描述文本需引用新工具名

- [ ] 修改 EditFileTool 的工具名 — 将 `fn name()` 返回值从 `"edit_file"` 改为 `"Edit"`
  - 位置: `rust-agent-middlewares/src/tools/filesystem/edit.rs` → `impl BaseTool for EditFileTool` 的 `fn name()` (~L37)
  - 将 `"edit_file"` 改为 `"Edit"`
  - 原因: 对齐 Claude Code 工具命名

- [ ] 更新 EditFileTool 描述中引用 `read_file` 的文本
  - 位置: `rust-agent-middlewares/src/tools/filesystem/edit.rs` → `EDIT_FILE_DESCRIPTION` 常量 (~L9, L10)
  - 将 `read_file tool` 改为 `Read tool`（共 2 处）
  - 原因: 描述文本需引用新工具名

- [ ] 修改 GlobFilesTool 的工具名 — 将 `fn name()` 返回值从 `"glob_files"` 改为 `"Glob"`
  - 位置: `rust-agent-middlewares/src/tools/filesystem/glob.rs` → `impl BaseTool for GlobFilesTool` 的 `fn name()` (~L99)
  - 将 `"glob_files"` 改为 `"Glob"`
  - 原因: 对齐 Claude Code 工具命名

- [ ] 更新 GlobFilesTool 描述中引用旧工具名的文本
  - 位置: `rust-agent-middlewares/src/tools/filesystem/glob.rs` → `GLOB_FILES_DESCRIPTION` 常量 (~L29, L33, L34)
  - 将 `glob_files` 改为 `Glob`（共 3 处）
  - 将 `search_files_rg` 改为 `Grep`（共 2 处）
  - 将 `launch_agent` 改为 `Agent`（共 1 处）
  - 原因: 描述文本中引用的工具名需全部对齐新名称

- [ ] 更新 `FilesystemMiddleware::tool_names()` 中 Write/Edit/Glob 的工具名
  - 位置: `rust-agent-middlewares/src/middleware/filesystem.rs` → `fn tool_names()` (~L30-L38)
  - 将 `"write_file"` 改为 `"Write"`
  - 将 `"edit_file"` 改为 `"Edit"`
  - 将 `"glob_files"` 改为 `"Glob"`
  - 原因: `tool_names()` 列表需与各工具的 `fn name()` 返回值一致

- [ ] 为三个工具的新名称编写单元测试
  - 测试文件: `rust-agent-middlewares/src/tools/filesystem/write.rs`、`edit.rs`、`glob.rs` 各自的 `#[cfg(test)] mod tests` 块
  - 在每个文件末尾已有测试中追加一个测试函数，验证 `fn name()` 返回新名称：
    - write.rs: `test_tool_name_is_Write()` → `assert_eq!(tool.name(), "Write")`
    - edit.rs: `test_tool_name_is_Edit()` → `assert_eq!(tool.name(), "Edit")`
    - glob.rs: `test_tool_name_is_Glob()` → `assert_eq!(tool.name(), "Glob")`
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::write::tests::test_tool_name_is_Write tools::filesystem::edit::tests::test_tool_name_is_Edit tools::filesystem::glob::tests::test_tool_name_is_Glob`
  - 预期: 三个测试全部通过

**检查步骤:**
- [ ] 验证三个工具的 `fn name()` 返回新名称
  - `cargo test -p rust-agent-middlewares --lib -- test_tool_name_is`
  - 预期: 三个测试通过（Write / Edit / Glob）
- [ ] 验证描述文本中无旧工具名残留
  - `grep -n 'read_file\|edit_file\|glob_files\|search_files_rg\|launch_agent' rust-agent-middlewares/src/tools/filesystem/write.rs rust-agent-middlewares/src/tools/filesystem/edit.rs rust-agent-middlewares/src/tools/filesystem/glob.rs`
  - 预期: 无匹配输出（所有旧工具名已从描述中清除）
- [ ] 验证模块编译和全量测试通过
  - `cargo test -p rust-agent-middlewares`
  - 预期: 所有测试通过，无编译错误

---

### Task 2: Read 改名 + 新增 pages 参数

**背景:**
将 `read_file` 工具重命名为 `Read`，对齐 Claude Code 工具命名。同时新增 `pages` 参数（string, optional），为 PDF 文件读取预留接口——当文件为 PDF 且提供了 `pages` 参数时，返回提示信息表示 PDF 读取尚未支持；PDF 但未提供 `pages` 时，保持现有二进制文件检测逻辑不变。描述文本中引用的旧工具名同步更新。本 Task 不依赖其他 Task；Task 5（验收）依赖本 Task 完成。

**涉及文件:**
- 修改: `rust-agent-middlewares/src/tools/filesystem/read.rs`
- 修改: `rust-agent-middlewares/src/middleware/filesystem.rs`（更新 `tool_names()` 中 `"read_file"` → `"Read"`）

**执行步骤:**
- [ ] 修改 `fn name()` 返回值从 `"read_file"` 改为 `"Read"` — 对齐 Claude Code 工具命名
  - 位置: `rust-agent-middlewares/src/tools/filesystem/read.rs` → `impl BaseTool for ReadFileTool` 的 `fn name()` (~L83)
  - 将 `"read_file"` 改为 `"Read"`

- [ ] 更新 `READ_FILE_DESCRIPTION` 常量中的旧工具名引用 — 描述文本中引用的工具名需全部对齐新名称
  - 位置: `rust-agent-middlewares/src/tools/filesystem/read.rs` → `READ_FILE_DESCRIPTION` 常量 (~L21-39)
  - 将 `read_file tool` 改为 `Read tool`（~L32）
  - 将 `the bash tool with commands like cat, head, tail, or sed to read files` 改为 `the Bash tool with commands like cat, head, tail, or sed to read files`
  - 将 `the Agent tool` 保留（已是新名称）
  - 原因: 描述文本中的工具引用需与新名称一致

- [ ] 在 `fn parameters()` 的 JSON schema 中新增 `pages` 参数 — 支持 PDF 页范围读取
  - 位置: `rust-agent-middlewares/src/tools/filesystem/read.rs` → `fn parameters()` (~L91-110)
  - 在 `properties` 对象中追加:
    ```json
    "pages": {
        "type": "string",
        "description": "For PDF files, the page range to read, e.g. '1-5', '3', '10-20'. Only applies to PDF files"
    }
    ```
  - 不加入 `required` 数组（可选参数）
  - 原因: 对齐 Claude Code 的 Read 工具 pages 参数

- [ ] 更新 `FilesystemMiddleware::tool_names()` 中 Read 的工具名
  - 位置: `rust-agent-middlewares/src/middleware/filesystem.rs` → `fn tool_names()` (~L30-L38)
  - 将 `"read_file"` 改为 `"Read"`
  - 原因: `tool_names()` 列表需与各工具的 `fn name()` 返回值一致

- [ ] 在 `fn invoke()` 中增加 PDF + pages 判断逻辑 — 在二进制检测之前拦截 PDF + pages 场景
  - 位置: `rust-agent-middlewares/src/tools/filesystem/read.rs` → `fn invoke()` (~L112-174)
  - 在 `let offset = ...` 之后（~L121 之后）、`let resolved = ...` 之后（~L123 之后），即二进制扩展名检测 `if let Some(ext) = resolved.extension()...` 之前，插入 PDF 检测逻辑：
    ```rust
    let pages = input["pages"].as_str().map(|s| s.to_string());

    // PDF + pages: 返回占位提示
    if let Some(ext) = resolved.extension().and_then(|e| e.to_str()) {
        if ext.eq_ignore_ascii_case("pdf") {
            if pages.is_some() {
                return Ok(format!(
                    "[PDF READING NOT YET SUPPORTED]\n\nFile path: {}\nPDF reading with page selection is not yet implemented. Use the Bash tool with a PDF reader command as a workaround.",
                    resolved.display()
                ));
            }
            // PDF 但未提供 pages → 继续走到下面的二进制检测，返回 BINARY FILE DETECTED
        }
    }
    ```
  - 原因: PDF 文件当前按二进制处理；提供 `pages` 参数时返回友好提示，未提供时保持原有二进制检测行为

- [ ] 为 Read 工具新名称和 PDF 逻辑编写单元测试
  - 测试文件: `rust-agent-middlewares/src/tools/filesystem/read.rs` → `#[cfg(test)] mod tests` 块
  - 在现有测试末尾追加以下测试函数：
    - `test_tool_name_is_Read()`: 创建 `ReadFileTool` 实例，断言 `tool.name() == "Read"`
    - `test_pdf_with_pages_returns_placeholder()`: 以 `"test.pdf"` 为路径传入 `pages: "1-5"`，断言返回包含 `"PDF READING NOT YET SUPPORTED"`
    - `test_pdf_without_pages_returns_binary()`: 以 `"test.pdf"` 为路径不传 `pages`，断言返回包含 `"BINARY FILE DETECTED"`
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::read::tests::test_tool_name_is_Read tools::filesystem::read::tests::test_pdf_with_pages_returns_placeholder tools::filesystem::read::tests::test_pdf_without_pages_returns_binary`
  - 预期: 三个测试全部通过

**检查步骤:**
- [ ] 验证 `fn name()` 返回 `"Read"`
  - `cargo test -p rust-agent-middlewares --lib -- tools::filesystem::read::tests::test_tool_name_is_Read`
  - 预期: 测试通过
- [ ] 验证描述文本中无旧工具名残留
  - `grep -n 'read_file\|search_files_rg\|launch_agent' rust-agent-middlewares/src/tools/filesystem/read.rs`
  - 预期: 无匹配输出（所有旧工具名已从描述中清除）
- [ ] 验证 PDF 逻辑测试通过
  - `cargo test -p rust-agent-middlewares --lib -- test_pdf_with_pages_returns_placeholder test_pdf_without_pages_returns_binary`
  - 预期: 两个测试通过
- [ ] 验证模块编译和全量测试通过
  - `cargo test -p rust-agent-middlewares`
  - 预期: 所有测试通过，无编译错误

---

### Task 3: AskUserQuestion 字段对齐

**背景:**
将 `ask_user_question` 工具重命名为 `AskUserQuestion`，并对齐字段命名：`multi_select` → `multiSelect`（camelCase），在选项中新增 `preview` 字段。工具定义来自 `ask_user_tool_definition()` 函数（位于 `rust-agent-middlewares/src/ask_user/mod.rs`），工具实现位于 `rust-agent-middlewares/src/tools/ask_user_tool.rs`。两个文件中有重复的 `InputOption`/`InputQuestion` 反序列化结构体，需同步修改 serde rename。`rust-agent-tui/src/app/agent.rs` 中 `map_executor_event` 函数通过工具名字符串匹配 `ask_user_question`，需同步更新为 `"AskUserQuestion"`。本 Task 不依赖其他 Task；Task 5（验收）依赖本 Task 完成。

**涉及文件:**
- 修改: `rust-agent-middlewares/src/ask_user/mod.rs`
- 修改: `rust-agent-middlewares/src/tools/ask_user_tool.rs`
- 修改: `rust-agent-tui/src/app/agent.rs`

**执行步骤:**
- [ ] 修改 `ask_user_tool_definition()` 中的工具名 — 从 `"ask_user_question"` 改为 `"AskUserQuestion"`
  - 位置: `rust-agent-middlewares/src/ask_user/mod.rs` → `ask_user_tool_definition()` 函数 (~L63-123)
  - 将 `.name` 字段从 `"ask_user_question".to_string()` 改为 `"AskUserQuestion".to_string()` (~L65)

- [ ] 修改 `ask_user_tool_definition()` 参数 schema 中的字段名 — `multi_select` → `multiSelect`
  - 位置: `rust-agent-middlewares/src/ask_user/mod.rs` → `ask_user_tool_definition()` 的 `parameters` JSON (~L71-121)
  - 将 JSON 中 `"multi_select"` key 改为 `"multiSelect"` (~L90)
  - 在选项 `properties` 中新增 `preview` 字段:
    ```json
    "preview": {
        "type": "string",
        "description": "预览内容，展示选项的效果或示例（可选）"
    }
    ```

- [ ] 修改 `parse_ask_user()` 中的工具名匹配 — 从 `"ask_user_question"` 改为 `"AskUserQuestion"`
  - 位置: `rust-agent-middlewares/src/ask_user/mod.rs` → `parse_ask_user()` 函数 (~L31)
  - 将 `if tool_call.name != "ask_user_question"` 改为 `if tool_call.name != "AskUserQuestion"`
  - 将错误信息中的 `"ask_user_question"` 改为 `"AskUserQuestion"` (~L36-37)

- [ ] 修改 `InputQuestion` 反序列化结构体的 serde rename — 使 LLM 传入 `multiSelect` 时能正确反序列化为 `multi_select` 字段
  - 位置: `rust-agent-middlewares/src/ask_user/mod.rs` → `InputQuestion` 结构体 (~L15-22)
  - 在 `multi_select` 字段上添加 `#[serde(default, rename = "multiSelect")]`，使 JSON 中 `multiSelect` 字段映射到 Rust 的 `multi_select`

- [ ] 修改 `InputOption` 反序列化结构体 — 新增 `preview` 字段
  - 位置: `rust-agent-middlewares/src/ask_user/mod.rs` → `InputOption` 结构体 (~L9-12)
  - 在 `description` 字段后追加 `preview: Option<String>` 字段

- [ ] 修改 `parse_ask_user()` 中的选项映射 — 传递 `preview` 字段
  - 位置: `rust-agent-middlewares/src/ask_user/mod.rs` → `parse_ask_user()` 函数内 `.map(|o| AskUserOption { ... })` (~L51-54)
  - `AskUserOption` 结构体当前只有 `label` 和 `description` 两个字段，`preview` 暂不传递（`AskUserOption` 是核心框架 `rust-create-agent` 的类型，不在本 Task 范围内修改），此处忽略 `o.preview` 即可

- [ ] 修改 `ask_user_tool.rs` 中的 `fn name()` — 从 `"ask_user_question"` 改为 `"AskUserQuestion"`
  - 位置: `rust-agent-middlewares/src/tools/ask_user_tool.rs` → `impl BaseTool for AskUserTool` 的 `fn name()` (~L78)
  - 将 `"ask_user_question"` 改为 `"AskUserQuestion"`

- [ ] 修改 `ask_user_tool.rs` 中 `InputQuestion` 反序列化结构体的 serde rename — 与 `mod.rs` 保持一致
  - 位置: `rust-agent-middlewares/src/tools/ask_user_tool.rs` → `InputQuestion` 结构体 (~L36-43)
  - 在 `multi_select` 字段上添加 `#[serde(default, rename = "multiSelect")]`

- [ ] 修改 `ask_user_tool.rs` 中 `InputOption` 反序列化结构体 — 新增 `preview` 字段
  - 位置: `rust-agent-middlewares/src/tools/ask_user_tool.rs` → `InputOption` 结构体 (~L30-34)
  - 在 `description` 字段后追加 `preview: Option<String>` 字段

- [ ] 修改 `agent.rs` 中的工具名匹配 — 从 `"ask_user_question"` 改为 `"AskUserQuestion"`
  - 位置: `rust-agent-tui/src/app/agent.rs` → `map_executor_event()` 函数
  - 将 ~L327 的 `name == "ask_user_question"` 改为 `name == "AskUserQuestion"`
  - 将 ~L351 注释中的 `ask_user_question` 更新为 `AskUserQuestion`

- [ ] 为 AskUserQuestion 工具新名称和字段对齐编写单元测试
  - 测试文件: `rust-agent-middlewares/src/tools/ask_user_tool.rs` → `#[cfg(test)] mod tests` 块
  - 在现有测试末尾追加以下测试函数：
    - `test_tool_name_is_AskUserQuestion()`: 创建 `AskUserTool` 实例（使用 MockBroker），断言 `tool.name() == "AskUserQuestion"`
    - `test_multi_select_camel_case_input()`: 构造 JSON 使用 `"multiSelect": true`（camelCase），断言正常解析并返回正确结果
    - `test_preview_field_ignored()`: 构造 JSON 在 options 中包含 `"preview": "some preview"` 字段，断言解析不报错且正常返回
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- tools::ask_user_tool::tests::test_tool_name_is_AskUserQuestion tools::ask_user_tool::tests::test_multi_select_camel_case_input tools::ask_user_tool::tests::test_preview_field_ignored`
  - 预期: 三个测试全部通过

**检查步骤:**
- [ ] 验证工具名返回 `"AskUserQuestion"`
  - `cargo test -p rust-agent-middlewares --lib -- test_tool_name_is_AskUserQuestion`
  - 预期: 测试通过
- [ ] 验证 camelCase 字段名和 preview 字段正常工作
  - `cargo test -p rust-agent-middlewares --lib -- test_multi_select_camel_case_input test_preview_field_ignored`
  - 预期: 两个测试通过
- [ ] 验证源码中无残留的旧工具名（排除注释和错误信息）
  - `grep -n '"ask_user_question"' rust-agent-middlewares/src/ask_user/mod.rs rust-agent-middlewares/src/tools/ask_user_tool.rs`
  - 预期: 无匹配输出
- [ ] 验证 agent.rs 中工具名匹配已更新
  - `grep -n 'ask_user_question' rust-agent-tui/src/app/agent.rs`
  - 预期: 仅在注释中出现（如有），无字符串字面量匹配
- [ ] 验证模块编译和全量测试通过
  - `cargo test -p rust-agent-middlewares -p rust-agent-tui`
  - 预期: 所有测试通过，无编译错误

---

### Task 4: TodoWrite 移除 id + 新增 activeForm

**背景:**
将 `todo_write` 工具重命名为 `TodoWrite`，移除 `TodoItem` 的 `id` 字段（全量替换语义下用数组索引标识），新增 `activeForm: Option<String>` 字段（进行时形式，用于 UI spinner 展示）。`summarize_changes()` 函数当前基于 `id` 的 HashMap 对比新旧列表，需改为基于数组索引对比。`TodoItem` 被 TUI 层使用（`rust-agent-tui/src/app/mod.rs` 的 `todo_items` 字段、`rust-agent-tui/src/ui/main_ui.rs` 的渲染逻辑），经代码确认 TUI 层仅访问 `content` 和 `status` 字段，不访问 `id`，因此移除 `id` 不影响 TUI 渲染。本 Task 不依赖其他 Task；Task 5（验收）依赖本 Task 完成。

**涉及文件:**
- 修改: `rust-agent-middlewares/src/tools/todo.rs`

**执行步骤:**
- [ ] 修改 `fn name()` 返回值从 `"todo_write"` 改为 `"TodoWrite"` — 对齐 Claude Code 工具命名
  - 位置: `rust-agent-middlewares/src/tools/todo.rs` → `impl BaseTool for TodoWriteTool` 的 `fn name()` (~L125)
  - 将 `"todo_write"` 改为 `"TodoWrite"`

- [ ] 修改 `TodoItem` 结构体 — 移除 `id` 字段，新增 `activeForm` 字段
  - 位置: `rust-agent-middlewares/src/tools/todo.rs` → `TodoItem` 结构体 (~L20-25)
  - 移除 `pub id: String` 字段
  - 在 `content` 字段后新增 `pub active_form: Option<String>` 字段（使用 `#[serde(skip_serializing_if = "Option::is_none")]` 避免 JSON 输出中包含 null）
  - 修改后的结构体:
    ```rust
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct TodoItem {
        pub content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub active_form: Option<String>,
        pub status: TodoStatus,
    }
    ```

- [ ] 修改 `fn parameters()` 的 JSON schema — 移除 `id`，新增 `activeForm`
  - 位置: `rust-agent-middlewares/src/tools/todo.rs` → `fn parameters()` (~L133-163)
  - 从 `properties` 中移除 `id` 属性
  - 新增 `activeForm` 属性:
    ```json
    "activeForm": {
        "type": "string",
        "description": "Present-tense form of the task description (e.g. 'Running tests'), used for UI spinner display"
    }
    ```
  - 从 `required` 数组中移除 `"id"`，保留 `["content", "status"]`

- [ ] 重写 `summarize_changes()` 函数 — 改为基于数组索引对比而非 id HashMap
  - 位置: `rust-agent-middlewares/src/tools/todo.rs` → `fn summarize_changes()` (~L70-121)
  - 将整个函数体替换为基于索引的对比逻辑：
    ```rust
    fn summarize_changes(old: &[TodoItem], new: &[TodoItem]) -> String {
        let mut parts: Vec<String> = Vec::new();
        let max_len = old.len().max(new.len());

        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut status_changes = Vec::new();

        for i in 0..max_len {
            match (old.get(i), new.get(i)) {
                (None, Some(_)) => added.push(format!("[{i}]")),
                (Some(_), None) => removed.push(format!("[{i}]")),
                (Some(old_item), Some(new_item)) => {
                    if old_item.status != new_item.status {
                        let status_str = match &new_item.status {
                            TodoStatus::Pending => "pending",
                            TodoStatus::InProgress => "in_progress",
                            TodoStatus::Completed => "completed",
                        };
                        status_changes.push(format!("[{i}]→{status_str}"));
                    }
                }
            }
        }

        if !added.is_empty() {
            parts.push(format!("+{}", added.join(",")));
        }
        if !removed.is_empty() {
            parts.push(format!("-{}", removed.join(",")));
        }
        if !status_changes.is_empty() {
            parts.push(status_changes.join(","));
        }

        if parts.is_empty() {
            "saved".to_string()
        } else {
            parts.join(" ")
        }
    }
    ```
  - 原因: 移除 `id` 后无法用 HashMap 对比，改为按数组索引位置一一对比，语义为"同位置的 item 视为同一任务"

- [ ] 更新 `fn invoke()` 中的错误信息 — 将 `"todo_write:"` 前缀改为 `"TodoWrite:"`
  - 位置: `rust-agent-middlewares/src/tools/todo.rs` → `fn invoke()` (~L170)
  - 将 `format!("todo_write: invalid input: {e}")` 改为 `format!("TodoWrite: invalid input: {e}")`

- [ ] 更新 `TODO_WRITE_DESCRIPTION` 常量 — 同步描述文本
  - 位置: `rust-agent-middlewares/src/tools/todo.rs` → `TODO_WRITE_DESCRIPTION` 常量 (~L29-47)
  - 将 `todo_write` 相关描述中对 `id` 的引用移除
  - 无需大幅修改，该描述文本当前不提及 `id` 字段

- [ ] 为 TodoWrite 工具新名称、无 id 结构和索引对比逻辑编写单元测试
  - 测试文件: `rust-agent-middlewares/src/tools/todo.rs` → `#[cfg(test)] mod tests` 块
  - 在现有测试末尾追加以下测试函数：
    - `test_tool_name_is_TodoWrite()`: 创建 `TodoWriteTool` 实例，断言 `tool.name() == "TodoWrite"`
    - `test_todo_item_no_id()`: 使用 `serde_json::from_value` 反序列化不含 `id` 的 JSON（仅含 `content` + `status`），断言成功且 `content == "test"`
    - `test_todo_item_active_form()`: 反序列化含 `activeForm` 的 JSON，断言 `active_form == Some("Running tests".to_string())`
    - `test_summarize_changes_by_index()`: 构造 old=[{content:"A",status:Pending},{content:"B",status:Pending}] 和 new=[{content:"A",status:InProgress},{content:"B",status:Pending},{content:"C",status:Pending}]，断言摘要包含 `"[0]→in_progress"` 和 `"+[2]"`
    - `test_summarize_changes_empty()`: old 和 new 相同，断言返回 `"saved"`
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- tools::todo::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [ ] 验证工具名返回 `"TodoWrite"`
  - `cargo test -p rust-agent-middlewares --lib -- tools::todo::tests::test_tool_name_is_TodoWrite`
  - 预期: 测试通过
- [ ] 验证 TodoItem 不再包含 id 字段
  - `grep -n 'pub id:' rust-agent-middlewares/src/tools/todo.rs`
  - 预期: 无匹配输出
- [ ] 验证 summarize_changes 使用索引对比
  - `grep -n 'HashMap\|\.id\b\|id\.as_str\|old_map\|new_map' rust-agent-middlewares/src/tools/todo.rs`
  - 预期: 无匹配输出（所有基于 id 的 HashMap 逻辑已移除）
- [ ] 验证 activeForm 字段存在
  - `grep -n 'active_form\|activeForm' rust-agent-middlewares/src/tools/todo.rs`
  - 预期: 结构体定义、parameters schema、测试中均有匹配
- [ ] 验证模块编译和全量测试通过
  - `cargo test -p rust-agent-middlewares -p rust-agent-tui`
  - 预期: 所有测试通过，无编译错误

---

### Task 5: 工具对齐（上）验收

**前置条件:**
- Task 1（Write/Edit/Glob 改名）已完成
- Task 2（Read 改名 + pages 参数）已完成
- Task 3（AskUserQuestion 字段对齐）已完成
- Task 4（TodoWrite 移除 id + activeForm）已完成

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test -p rust-agent-middlewares`
   - 预期: 全部测试通过
   - 失败排查: 检查各 Task 的测试步骤，优先查看失败测试所属的模块

2. 验证旧工具名在中间件源码描述文本中无残留
   - `grep -rn '"write_file"\|"edit_file"\|"glob_files"\|"read_file"\|"ask_user_question"\|"todo_write"' rust-agent-middlewares/src/tools/ --include='*.rs'`
   - 预期: 无匹配输出（所有旧工具名已从 `fn name()` 和描述文本中清除）
   - 失败排查: 检查 Task 1（write_file/edit_file/glob_files）、Task 2（read_file）、Task 3（ask_user_question）、Task 4（todo_write）的执行步骤

3. 验证新工具名在工具定义中正确生效
   - `grep -rn '"Write"\|"Edit"\|"Glob"\|"Read"\|"AskUserQuestion"\|"TodoWrite"' rust-agent-middlewares/src/tools/ --include='*.rs'`
   - 预期: 每个工具的 `fn name()` 和描述文本中出现对应新名称
   - 失败排查: 检查对应 Task 的 `fn name()` 修改步骤

4. 验证 TUI 层 AskUserQuestion 工具名匹配已更新
   - `grep -n 'ask_user_question' rust-agent-tui/src/app/agent.rs`
   - 预期: 仅在注释中出现（如有），无字符串字面量匹配
   - 失败排查: 检查 Task 3 的 `agent.rs` 更新步骤

5. 验证 TodoWrite 不再包含 id 字段且 summarize_changes 使用索引对比
   - `grep -n 'pub id:\|HashMap\|old_map\|new_map' rust-agent-middlewares/src/tools/todo.rs`
   - 预期: 无匹配输出
   - 失败排查: 检查 Task 4 的结构体修改和 `summarize_changes()` 重写步骤
