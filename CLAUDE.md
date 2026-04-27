# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概述

Rust Agent 框架，包含 **3 个 Workspace Crate**：

- **`rust-create-agent`**：核心框架——ReAct 循环执行器、Middleware trait、LLM 适配器、工具系统
- **`rust-agent-middlewares`**：具体中间件实现（文件系统、终端、Skills、HITL、SubAgent、ask_user_question）
- **`rust-agent-tui`**：交互式 TUI playground，基于 ratatui

## 开发命令

```bash
cargo build                          # 构建所有 crate
cargo build -p rust-create-agent     # 构建指定 crate
cargo run -p rust-agent-tui          # 运行 TUI
cargo run -p rust-agent-tui -- -y    # YOLO 模式（已废弃，YOLO 已是默认行为）
cargo run -p rust-agent-tui -- -a    # 启用 HITL 审批模式
cargo test                           # 全量测试
cargo test -p rust-create-agent --lib -- test_name  # 运行单个测试
```

## Workspace 依赖关系

```
rust-create-agent (核心框架，无内部依赖)
    ↑
rust-agent-middlewares (中间件实现)
    ↑
rust-agent-tui (TUI 应用，依赖 middlewares)
```

## 数据流

### ReAct 循环（rust-create-agent）

```
AgentInput
  └─ state.add_message(Human)
  └─ chain.collect_tools(cwd)        # ToolProvider + 中间件工具合并，手动注册的优先级最高
  └─ chain.run_before_agent(state)   # 按注册顺序执行
  └─ loop(max_iterations=50):
      └─ llm.generate_reasoning(state.messages, tools)
      │    └─ BaseModel.invoke(LlmRequest{messages, tools, system})
      │    └─ stop_reason==ToolUse  → Reasoning{tool_calls}
      │       stop_reason==EndTurn  → Reasoning{final_answer}
      │
      ├─ [有工具调用]:
      │   └─ state.add_message(Ai{tool_calls})
      │   └─ chain.run_before_tool()   # HITL 在此拦截
      │   └─ futures::future::join_all(tools)  # 并发执行所有工具
      │   └─ chain.run_after_tool()    # TodoMiddleware 在此解析 todo_write
      │   └─ emit(ToolStart/ToolEnd)
      │   └─ state.add_message(Tool{result})
      │
      └─ [最终回答]:
          └─ emit(TextChunk(answer))
          └─ chain.run_after_agent(state, output) → AgentOutput
```

### TUI 异步通信（rust-agent-tui）

```
submit_message()
  ├─ mpsc(32): AgentEvent channel ──→ agent task
  │                                       └─ run_universal_agent() 产生事件
  │                                       └─ emit → tx.try_send(AgentEvent)
  │  ← poll_agent() 每帧 try_recv ←──────
  │       ToolCall/AssistantChunk → 追加 view_messages[]
  │       ApprovalNeeded          → app.hitl_prompt = Some(...)  [break]
  │       AskUserBatch            → app.ask_user_prompt = Some(...) [break]
  │       Done/Error              → set_loading(false), agent_rx=None
  │
  └─ mpsc(4): ApprovalEvent channel ──→ 转发 task
       ApprovalEvent::Batch        → YOLO（默认）: 直接 response_tx.send(Approve×N)
                                     审批模式（-a）: tx.send(AgentEvent::ApprovalNeeded)
       ApprovalEvent::AskUserBatch → tx.send(AgentEvent::AskUserBatch)  [始终转发]

用户操作弹窗后:
  hitl_confirm()     → response_tx.send(decisions)   → HITL before_tool 的 oneshot 解除
  ask_user_confirm() → response_tx.send(answers)     → AskUserTool::invoke 的 oneshot 解除
```

### 消息类型

`BaseMessage` 四种变体（`Human/Ai/System/Tool`），内容统一用 `MessageContent`。

`ContentBlock` 完整变体：

| 变体 | 说明 |
|------|------|
| `Text` | 纯文本 |
| `Image` | 多模态图片（Base64 或 URL） |
| `Document` | 文档（Anthropic Documents beta） |
| `ToolUse` | AI 发起的工具调用（id/name/input） |
| `ToolResult` | 工具执行结果（tool_use_id/content/is_error） |
| `Reasoning` | 推理/CoT（支持 extended thinking 的 signature 缓存校验） |
| `Unknown` | 原生 block 透传，保证向前兼容 |

`Ai` 变体同时保存 `tool_calls: Vec<ToolCallRequest>`，与 `ContentBlock::ToolUse` 双写保持一致。

### LLM 适配层

`BaseModel` trait（OpenAI/Anthropic 实现）→ `BaseModelReactLLM`（适配为 `ReactLLM`）。

| | OpenAI | Anthropic |
|---|---|---|
| system | 转为 `System` 角色消息 prepend | 提取到顶层 `system` 字段 |
| 工具格式 | `type:"function"` + `function.arguments` | `type:"tool_use"` + `input_schema` |
| 推理内容 | `message.reasoning_content`（deepseek-r1/o系列） | `Reasoning` ContentBlock |
| Prompt Cache | — | 默认开启，`cache_control:ephemeral` |
| 扩展思考 | — | `.with_extended_thinking(budget_tokens)`（3.7+） |

测试用 `MockLLM::tool_then_answer()` 按脚本回放推理，无需真实 API。

### HITL 决策

`HitlDecision` 四种结果：`Approve` / `Edit(new_input)` / `Reject` → 错误 / `Respond(msg)` → 原因。

默认需审批工具：`bash`、`folder_operations`、`launch_agent`、`write_*`、`edit_*`、`delete_*`、`rm_*`。

### Skills 搜索顺序

`~/.claude/skills/` → `skillsDir`（`~/.zen-code/settings.json`） → `./.claude/skills/`

同名 skill 以先出现的为准。每个 skill 是一个子目录，内含 `SKILL.md`（YAML frontmatter: `name`, `description`）。

## 工具清单（rust-agent-middlewares）

| 工具 | 来源 | 需 HITL |
|------|------|---------|
| `read_file` | FilesystemMiddleware | — |
| `write_file` | FilesystemMiddleware | ✓ |
| `edit_file` | FilesystemMiddleware | ✓ |
| `glob_files` | FilesystemMiddleware | — |
| `search_files_rg` | FilesystemMiddleware | — |
| `folder_operations` | FilesystemMiddleware | ✓ |
| `bash` | TerminalMiddleware | ✓ |
| `todo_write` | TodoMiddleware | — |
| `ask_user_question` | 手动注册（AskUserTool） | — |
| `launch_agent` | SubAgentMiddleware | ✓ |

`bash` 默认超时 120 秒。跨平台：Windows 用 `cmd /C`，其他用 `bash -c`。

### ask_user_question 工具参数

批量向用户提问，1-4 个问题一次性发出，支持单选/多选。

```json
{
  "questions": [
    {
      "question": "向用户提出的问题（包含必要上下文）",
      "header": "短标签 <=12字（UI Tab 显示）",
      "multi_select": false,
      "options": [
        { "label": "选项文本（1-50字）", "description": "选项说明（可选）" }
      ]
    }
  ]
}
```

**字段说明：**

- `questions`：1-4 个问题
- `header`：最多 12 字，显示在 UI Tab 上
- `multi_select`：默认 `false`（单选），`true` 时允许多选
- `options`：2-4 个选项；每个问题还自带文本输入框，用户可自由填写

**返回格式：**

- 单问题：直接返回所选选项（多选用 `,` 拼接）或自定义文本
- 多问题：`[问: header]\n回答: value\n\n[问: header]\n回答: value`

### SubAgents（子 Agent 委派）

`launch_agent` 工具允许 LLM 将子任务委派给 `.claude/agents/{agent_id}/agent.md` 定义的专门 agent 执行。

**工具参数：**

| 参数 | 类型 | 说明 |
|------|------|------|
| `agent_id` | string（必填） | agent 目录名，如 `code-reviewer` |
| `task` | string（必填） | 委派给子 agent 的任务描述 |
| `cwd` | string（可选） | 子 agent 工作目录，默认继承父 agent cwd |

**工具过滤规则：**

- `tools` 字段为空 → 子 agent 继承所有父工具（排除 `launch_agent` 自身，防递归）
- `tools` 字段有值 → 仅保留允许列表中的工具
- `disallowedTools` 字段 → 额外排除指定工具

**返回值格式：**

```
[子 agent 执行了 N 个工具调用: tool1, tool2, tool3]

Final response text here
```

**Agent 定义文件结构：**

```
.claude/agents/{agent_id}.md           # 扁平格式
.claude/agents/{agent_id}/agent.md     # 目录格式
```

两种格式等效，支持的 frontmatter 字段：

| 字段 | 说明 |
|------|------|
| `name` | Agent 唯一标识符 |
| `description` | Agent 用途描述 |
| `tools` | 允许的工具列表（逗号分隔或数组） |
| `disallowedTools` | 拒绝的工具列表 |
| `maxTurns` | 最大迭代轮数 |
| `skills` | 预加载的 skills 列表 |
| `tone` | 输出风格覆盖 |
| `proactiveness` | 主动性覆盖 |
| `model` | 使用的模型（sonnet/opus/haiku/inherit） |

## TUI 命令

输入 `/` 前缀触发，支持前缀唯一匹配（如 `/m` 匹配 `/model`）：

| 命令 | 说明 |
|------|------|
| `/login` | 管理 Provider 配置（新建/编辑/删除），表单包含 API Key/Base URL/三级别模型名 |
| `/model` | 打开模型选择面板（Provider 选择 + 级别切换 + Thinking 配置） |
| `/model <alias>` | 直接切换激活别名（`opus` / `sonnet` / `haiku`） |
| `/history` | 打开历史对话浏览面板（↑↓ 导航，`d` 删除，`Enter` 打开） |
| `/agents` | 打开 SubAgent 定义管理面板 |
| `/compact` | 触发上下文压缩 |
| `/clear` | 清空当前消息列表 |
| `/help` | 列出所有命令 |

输入 `#` 前缀触发 Skills 浮层，`Tab` 导航，`Enter` 补全为 `#skill-name`。

## TUI Headless 测试模式

`rust-agent-tui` 支持无真实终端的 headless 集成测试。

```rust
#[tokio::test]
async fn test_example() {
    let (mut app, mut handle) = App::new_headless(120, 30);

    // 必须在发送事件前注册监听
    let notified = handle.render_notify.notified();

    app.push_agent_event(AgentEvent::AssistantChunk("Hello".into()));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();

    notified.await;  // 等待渲染线程处理完成

    handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
    assert!(handle.contains("Hello"));
}
```

**注意事项：**

- `notified()` 必须在 `process_pending_events()` **之前**调用
- `AssistantChunk` 事件会发送 2 个 `RenderEvent`
- CJK 字符在 `TestBackend` 中有宽字符填充，断言应使用 ASCII 内容
- 测试位于 `rust-agent-tui/src/ui/headless.rs`

## 关键模式

```rust
// 组装 agent（系统提示词通过 PrependSystemMiddleware 注入）
ReActAgent::new(BaseModelReactLLM::new(model))
    .max_iterations(50)
    .add_middleware(Box::new(FilesystemMiddleware::new()))
    .add_middleware(Box::new(PrependSystemMiddleware::new(prompt)))
    .register_tool(Box::new(AskUserTool::new(invoker)))
    .with_event_handler(Arc::new(FnEventHandler(move |ev| { tx.try_send(ev); })))
    .execute(AgentInput::text(input), &mut AgentState::new(cwd))
```

**SubAgent 委派：**

```rust
let parent_tools: Arc<Vec<Arc<dyn BaseTool>>> = Arc::new(
    FilesystemMiddleware::new().tools(cwd)
        .into_iter()
        .map(|t| Arc::new(BoxToolWrapper(t)) as Arc<dyn BaseTool>)
        .collect()
);
let llm_factory = Arc::new(move || {
    Box::new(BaseModelReactLLM::new(model.clone())) as Box<dyn ReactLLM + Send + Sync>
});
let system_builder = Arc::new(|overrides: Option<&AgentOverrides>, cwd: &str| {
    build_system_prompt(overrides, cwd)
});
ReActAgent::new(llm)
    .add_middleware(Box::new(
        SubAgentMiddleware::new(parent_tools, Some(event_handler), llm_factory)
            .with_system_builder(system_builder)
    ))
```

## 环境变量

| 变量 | 说明 |
|------|------|
| `ANTHROPIC_API_KEY` | Anthropic API Key |
| `OPENAI_API_KEY` | OpenAI 兼容 API Key |
| `OPENAI_BASE_URL` | API Base URL |
| `OPENAI_MODEL` | 模型名称 |
| `YOLO_MODE=true` | 默认行为，跳过 HITL 审批（不影响 ask_user_question） |
| `YOLO_MODE=false` | 启用 HITL 审批 |
| `RUST_LOG` | 日志级别（默认 `info`） |
| `RUST_LOG_FILE` | 日志文件路径 |
| `RUST_LOG_FORMAT=json` | 使用 JSON 格式输出日志 |
| `LANGFUSE_*` | Langfuse 追踪配置 |

`.env` 文件已 gitignore，本地开发配置在 `rust-agent-tui/.env`。

## CLI 参数

| 参数 | 说明 |
|------|------|
| `-y, --yolo` | 已废弃（YOLO 已是默认行为） |
| `-a, --approve` | 启用 HITL 审批（设置 `YOLO_MODE=false`） |

## 开发注意事项

- **新增弹窗面板**：`Event::Paste` 独立于 key event 链，必须在该分支单独拦截；`Ctrl+V` 需在 `handle_xxx_panel` 内单独处理。
- **EditField 导航**：`next()/prev()` 链必须与表单实际渲染字段一致。

## 面板快捷键设计规范

所有面板遵循统一的按键约定：

| 按键 | 行为 |
|------|------|
| `↑` / `↓` | 竖向列表导航（Browse 模式切换光标，Edit 模式切换字段） |
| `←` / `→` | 横向切换（仅限 Type 等枚举字段，编辑模式下） |
| `Enter` | 确认/进入（Browse 模式进入编辑，Edit 模式保存，确认面板确认操作） |
| `Space` | 选中/切换（Browse 模式激活 Provider，Edit 模式切换 Type） |
| `Esc` | 关闭/取消（关闭面板、退出编辑回到 Browse、取消确认） |
| `Ctrl+V` | 粘贴剪贴板内容到当前编辑字段 |
