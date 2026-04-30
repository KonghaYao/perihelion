# Feature: 20260430_F001 - align-claude-code-tools

## 需求背景

Perihelion 当前实现了 10 个内置工具，其接口设计与 Claude Code 存在显著差异——工具名称不同（`read_file` vs `Read`）、参数结构不同（`search_files_rg` 传 ripgrep 原始参数 vs `Grep` 结构化字段）、语义不完全对齐（`TodoWrite` 有 id 但 Claude Code 没有、`ask_user_question` 批量模式等）。

这导致三个问题：
1. 直接复用 Claude Code 的系统提示词（含工具使用指导）时，LLM 无法正确调用工具
2. 社区积累的 agent 定义（`.claude/agents/*.md`）和 skills 中引用的工具名/参数无法直接使用
3. 新用户从 Claude Code 迁移到 Perihelion 需要重新学习工具接口

## 目标

- 10 个现有工具的**名称完全对齐** Claude Code（`read_file` → `Read`、`search_files_rg` → `Grep` 等）
- 参数结构对齐 Claude Code（字段名、类型、语义一致）
- `search_files_rg` → `Grep` 重构为结构化接口（不再传 ripgrep 原始参数数组）
- `launch_agent` → `Agent` 对齐 Claude Code 的 `prompt`/`description` 模式
- `folder_operations` 保留为 Perihelion 扩展工具，不做重命名

## 方案设计

### 工具对齐映射总表

| # | 现有名称 | 目标名称 | 参数变更摘要 | 变更级别 |
|---|---------|---------|-------------|---------|
| 1 | `read_file` | `Read` | 参数一致，新增 `pages`（PDF 页范围） | 小 |
| 2 | `write_file` | `Write` | 完全一致，无变化 | 无 |
| 3 | `edit_file` | `Edit` | 完全一致，无变化 | 无 |
| 4 | `glob_files` | `Glob` | 完全一致，无变化 | 无 |
| 5 | `search_files_rg` | `Grep` | `args` 数组 → 结构化字段（`pattern`/`path`/`glob`/`type`/`output_mode`/`-i`/`-C`/`-n`/`multiline`/`head_limit`/`offset`） | **重大重构** |
| 6 | `bash` | `Bash` | `timeout_secs`(秒)→`timeout`(毫秒)，新增 `description`、`run_in_background` | 中 |
| 7 | `todo_write` | `TodoWrite` | todo item 移除 `id`，新增 `activeForm`，`status` 枚举对齐 | 中 |
| 8 | `ask_user_question` | `AskUserQuestion` | 保留批量能力（1-4 题），字段命名对齐（`multi_select`→`multiSelect`），新增 `options.preview` | 小 |
| 9 | `launch_agent` | `Agent` | `agent_id`+`task` → `prompt`+`description`+`subagent_type`+`name`+`isolation`+`run_in_background`+`cwd` | **重大重构** |
| 10 | `folder_operations` | `folder_operations` | 不做变更，作为 Perihelion 扩展工具 | 无 |

### 逐工具详细设计

#### 1. `Read`（原 `read_file`）

参数结构（已有字段不变）：

```json
{
  "file_path": "string (必填) — 绝对路径",
  "offset": "number (可选) — 起始行号",
  "limit": "number (可选) — 读取行数",
  "pages": "string (可选) — PDF 页范围，如 '1-5', '3', '10-20'"
}
```

变更点：新增 `pages` 参数，支持 PDF 文件的指定页范围读取。PDF 检测逻辑复用已有的二进制文件扩展名判断。PDF 解析需引入新依赖（如 `pdf` crate 或 `lopdf`），初期可返回提示信息。

#### 2. `Write`（原 `write_file`）

无参数变更，仅改名。

#### 3. `Edit`（原 `edit_file`）

无参数变更，仅改名。

#### 4. `Glob`（原 `glob_files`）

无参数变更，仅改名。

#### 5. `Grep`（原 `search_files_rg`）— 重大重构

**目标参数结构：**

```json
{
  "pattern": "string (必填) — 正则表达式模式",
  "path": "string (可选) — 搜索路径，默认 cwd",
  "glob": "string (可选) — 文件过滤 glob 模式，如 '*.rs', '*.{ts,tsx}'",
  "type": "string (可选) — 文件类型过滤，如 'rust', 'js', 'py'",
  "output_mode": "string (必填) — 'content' | 'files_with_matches' | 'count'",
  "-i": "boolean (可选) — 大小写不敏感搜索",
  "-C": "number (可选) — 上下文行数",
  "-n": "boolean (可选) — 显示行号（默认 true）",
  "multiline": "boolean (可选) — 启用跨行匹配（-U --multiline-dotall）",
  "head_limit": "number (可选) — 限制输出行数（默认 250）",
  "offset": "number (可选) — 跳过前 N 行"
}
```

**参数 → ripgrep 标志映射表：**

| 结构化字段 | ripgrep 命令行等效 | 默认值 |
|-----------|-------------------|--------|
| `pattern` | 位置参数（正则） | — |
| `path` | 位置参数（路径） | cwd |
| `glob` | `--glob <value>` | 无 |
| `type` | `--type <value>` | 无 |
| `output_mode="files_with_matches"` | `-l` | — |
| `output_mode="count"` | `-c` | — |
| `output_mode="content"` | 无额外标志 | — |
| `-i=true` | `-i` | false |
| `-C=N` | `-C N` | 无 |
| `-n=true` | `-n` | true |
| `multiline=true` | `-U --multiline-dotall` | false |
| `head_limit=N` | 输出截断逻辑 | 250 |

**实现要点：**
- 重构 `SearchFilesRgTool` 为 `GrepTool`（文件名保持 `grep.rs`）
- `invoke()` 内部将结构化参数转译为 ripgrep 调用参数
- `type` 字段需维护一个类型→后缀映射表（`rust` → `*.rs` 等），或直接传递给 ripgrep 的 `--type` 参数
- 行号默认启用（`-n` 默认 true），`output_mode="files_with_matches"` 时不传 `-n`
- `head_limit` 实现为输出截断（取前 N 行），`offset` 为跳过前 N 行再截断
- 保留现有的超时机制（15 秒）和最大输出限制

#### 6. `Bash`（原 `bash`）

**目标参数结构：**

```json
{
  "command": "string (必填) — 要执行的命令",
  "description": "string (可选) — 命令用途简述",
  "timeout": "number (可选) — 超时毫秒数（默认 120000，最大 600000）",
  "run_in_background": "boolean (可选) — 后台运行"
}
```

变更点：
- `timeout_secs`（秒，1-300）→ `timeout`（毫秒，最大 600000/10 分钟），对齐 Claude Code
- 新增 `description`：LLM 提供命令用途说明，用于 UI 展示和日志
- 新增 `run_in_background`：支持后台运行模式（初期可预留，不实现完整后台逻辑）

**timeout 单位转换注意：** 现有代码内部用 `Duration::from_secs(timeout_secs)`，需改为 `Duration::from_millis(timeout)`，并更新验证逻辑（最大值从 300 秒改为 600000 毫秒）。

#### 7. `TodoWrite`（原 `todo_write`）

**目标参数结构：**

```json
{
  "todos": [
    {
      "content": "string (必填) — 任务描述",
      "activeForm": "string (可选) — 进行时形式（如 'Running tests'）",
      "status": "string (必填) — 'pending' | 'in_progress' | 'completed'"
    }
  ]
}
```

变更点：
- 移除 todo item 的 `id` 字段（全量替换语义下用数组索引标识）
- 新增 `activeForm` 字段（进行时形式，用于 UI spinner 展示）
- `status` 枚举值不变（`pending`/`in_progress`/`completed`）

**兼容性处理：** 内部 `TodoItem` 结构体移除 `id` 字段，变更摘要（additions/deletions/status changes）逻辑改为基于数组索引对比。

#### 8. `AskUserQuestion`（原 `ask_user_question`）

**目标参数结构（保留批量能力）：**

```json
{
  "questions": [
    {
      "question": "string (必填) — 问题内容",
      "header": "string (必填) — 短标签 ≤12 字",
      "multiSelect": "boolean (可选，默认 false) — 是否多选",
      "options": [
        {
          "label": "string (必填) — 选项文本",
          "description": "string (可选) — 选项说明",
          "preview": "string (可选) — 预览内容"
        }
      ]
    }
  ]
}
```

变更点：
- 字段命名对齐：`multi_select` → `multiSelect`
- 新增 `options.preview` 字段（预览内容）
- 保留 1-4 个问题的批量能力（与 Claude Code 单问题接口不同，但作为 Perihelion 增强保留）

#### 9. `Agent`（原 `launch_agent`）— 重大重构

**目标参数结构：**

```json
{
  "prompt": "string (必填) — 委派给子 agent 的任务描述",
  "description": "string (可选) — 3-5 词简短描述",
  "subagent_type": "string (可选) — agent 类型/ID",
  "name": "string (可选) — agent 别名",
  "isolation": "string (可选) — 隔离模式，'worktree' 或 null",
  "run_in_background": "boolean (可选) — 是否后台运行",
  "cwd": "string (可选) — 工作目录"
}
```

**参数语义映射：**

| 目标参数 | 现有参数 | 说明 |
|---------|---------|------|
| `prompt` | `task` | 重命名，语义不变 |
| `description` | (新增) | 简短描述，用于 UI 展示 |
| `subagent_type` | `agent_id` | 重命名，映射到 `.claude/agents/` 查找 |
| `name` | (新增) | agent 别名，用于 UI 标识 |
| `isolation` | (新增) | 预留字段，值 `"worktree"` 或 null，初期不实现 |
| `run_in_background` | (新增) | 预留字段，初期不实现 |
| `cwd` | `cwd` | 不变 |

**预留字段策略：** `isolation` 和 `run_in_background` 参数解析但不执行，返回成功响应即可。未来实现时只需在 `invoke()` 中添加分支逻辑。

**无 `subagent_type` 的行为：** 当 `subagent_type` 为空时，等价于 Claude Code 的 "fork yourself" 模式——创建一个继承所有父工具（排除 `launch_agent` 自身）的子 agent，使用相同系统提示词。这与现有的 `SubAgentTool` 空工具列表行为一致。

#### 10. `folder_operations`（保留）

不做任何变更，作为 Perihelion 扩展工具保留。

### 受影响的文件清单

**工具定义层（rust-agent-middlewares/src/tools/）：**

| 文件 | 变更内容 |
|------|---------|
| `filesystem/read.rs` | 工具名 `read_file` → `Read`，新增 `pages` 参数 |
| `filesystem/write.rs` | 工具名 `write_file` → `Write` |
| `filesystem/edit.rs` | 工具名 `edit_file` → `Edit` |
| `filesystem/glob.rs` | 工具名 `glob_files` → `Glob` |
| `filesystem/grep.rs` | **重构**：工具名 → `Grep`，参数结构完全重写 |
| `filesystem/folder.rs` | 不变 |
| `ask_user_tool.rs` | 工具名 → `AskUserQuestion`，字段命名对齐 |
| `todo.rs` | 工具名 → `TodoWrite`，移除 `id`，新增 `activeForm` |
| `mod.rs` | 更新 re-export 名称 |

**中间件层（rust-agent-middlewares/src/middleware/）：**

| 文件 | 变更内容 |
|------|---------|
| `terminal.rs` | 工具名 → `Bash`，参数结构更新 |

**子 Agent 层（rust-agent-middlewares/src/subagent/）：**

| 文件 | 变更内容 |
|------|---------|
| `tool.rs` | 工具名 → `Agent`，参数结构重写 |
| `mod.rs` | 适配新参数名 |

**TUI 层（rust-agent-tui/src/）：**

| 文件 | 变更内容 |
|------|---------|
| `prompt.rs` | 系统提示词中工具引用名更新 |
| `app/agent.rs` | 工具注册名更新 |
| `app/tool_display.rs` | 工具颜色映射表更新 |
| `app/hitl.rs` | HITL 审批工具名匹配更新 |
| `langfuse/tracer.rs` | 工具名日志更新 |

**提示词段落（rust-agent-tui/prompts/sections/）：**

| 文件 | 变更内容 |
|------|---------|
| 所有段落文件 | 工具引用名更新（如 `read_file` → `Read`） |

## 实现要点

### 关键技术决策

1. **Grep 工具参数转译层：** 在 `GrepTool::invoke()` 中新增 `fn build_grep_args(input: &GrepInput) -> Vec<String>` 方法，将结构化参数转译为 ripgrep 兼容参数。底层搜索引擎保持不变（`ignore` crate + `grep` crate）。

2. **type 字段映射：** 维护 `FILE_TYPE_MAP: HashMap<&str, &[&str]>` 映射表（`"rust"` → `&["rs"]`、`"js"` → `&["js", "mjs"]` 等），或利用 ripgrep 的内置类型支持（`--type` 参数）。

3. **timeout 单位迁移：** Bash 工具内部从 `Duration::from_secs()` 改为 `Duration::from_millis()`，验证边界从 `1..=300`（秒）改为 `1..=600000`（毫秒）。

4. **TodoWrite id 移除：** 内部 `TodoItem` 移除 `id` 字段，`TodoMiddleware::after_tool` 的变更摘要逻辑改为基于数组索引的 diff。

5. **Agent 预留字段：** `isolation` 和 `run_in_background` 在参数结构体中定义，`invoke()` 中正常解析但不影响执行流程。

### 实施顺序建议

1. **第一批（仅改名，无逻辑变更）：** `Write`、`Edit`、`Glob`、`folder_operations` — 修改 `tool_def().name` 和相关引用
2. **第二批（小改）：** `Read`（新增 pages）、`AskUserQuestion`（字段重命名）、`TodoWrite`（移除 id + 新增 activeForm）
3. **第三批（中改）：** `Bash`（timeout 单位 + 新增参数）
4. **第四批（重大重构）：** `Grep`（参数结构重写）、`Agent`（参数结构重写）

### 向后兼容

- 工具名变更会影响已保存的 Thread 中旧工具名。`SqliteThreadStore` 读取历史消息时无需特殊处理（消息中存储的是原始文本），但系统提示词中的工具引用需全部更新。
- 所有提示词段落文件中引用的工具名需同步更新。

## 约束一致性

- **符合 Middleware Chain 模式：** 工具重命名不影响中间件链执行顺序，`FilesystemMiddleware` 和 `TerminalMiddleware` 仍通过 `collect_tools` 提供工具
- **符合 BaseTool trait 接口：** 所有变更仅涉及 `tool_def()` 返回的名称和 schema，以及 `invoke()` 的输入参数结构
- **符合异步优先原则：** 所有新增参数处理逻辑在现有 async `invoke()` 方法中完成
- **无架构偏离：** 不引入新的 crate 依赖（除 Read 的 PDF 功能可能需要 `lopdf`）

## 验收标准

- [ ] 10 个工具名称全部对齐 Claude Code（`Read`/`Write`/`Edit`/`Glob`/`Grep`/`Bash`/`TodoWrite`/`AskUserQuestion`/`Agent`/`folder_operations`）
- [ ] `Grep` 工具接受结构化参数（pattern/path/glob/type/output_mode 等），不再接受 args 数组
- [ ] `Agent` 工具接受 prompt/description/subagent_type/name 参数
- [ ] `Bash` 工具 timeout 单位为毫秒，支持 description 和 run_in_background 参数
- [ ] `TodoWrite` 的 todo item 无 id 字段，有 activeForm 字段
- [ ] `AskUserQuestion` 字段命名对齐（multiSelect），支持 preview 字段
- [ ] `Read` 支持 pages 参数
- [ ] 所有系统提示词段落文件中工具引用已更新
- [ ] TUI 工具颜色映射表已更新
- [ ] HITL 审批工具名匹配已更新
- [ ] 现有单元测试全部通过（测试中的工具名/参数需同步更新）
- [ ] `cargo build` 和 `cargo test` 无错误
