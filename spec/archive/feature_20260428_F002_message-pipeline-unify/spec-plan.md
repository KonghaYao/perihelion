# 消息显示管线统一 执行计划

**目标:** 将 MessagePipeline 接入流式对话路径，实现历史恢复与流式对话共享同一转换函数，消除两条路径的不一致

**技术栈:** Rust 2021, ratatui, tokio, parking_lot

**设计文档:** spec/feature_20260428_F002_message-pipeline-unify/spec-design.md

## 改动总览

- 本次改动集中在 `rust-agent-tui/src/app/` 内的 6 个文件，涉及事件定义、管线核心、事件处理和线程操作四个层次
- Task 1 拆分 AgentEvent（事件层）→ Task 2 接入 Pipeline 到 AppCore（结构层）→ Task 3 扩展 Pipeline handle_event（管线层）→ Task 4 重构 agent_ops（处理层）→ Task 5 适配恢复路径（恢复层），严格顺序依赖
- 关键决策：保留 AppendChunk 流式优化，Done 时 reconcile 确保最终一致；ToolStart/ToolEnd 拆分仅在 TUI 内部，不影响核心层 ExecutorEvent

---


### Task 1: AgentEvent 拆分 ToolCall → ToolStart + ToolEnd

**背景:** 当前 `AgentEvent::ToolCall` 混合了 ToolStart 和 ToolEnd 语义（通过 `is_error` 字段区分），导致 Pipeline 无法正确区分工具调用开始和结束两个阶段。拆分后 Pipeline 可以在 ToolStart 时立即显示工具调用信息，在 ToolEnd 时显示结果，同时携带原始 input 供 Pipeline 做 cwd 路径缩短。

---

#### 执行步骤

- [ ] **步骤 1: 在 events.rs 中新增 ToolStart 和 ToolEnd 变体**
  - **目标文件**: `rust-agent-tui/src/app/events.rs`
  - **位置**: 在 `AgentEvent` 枚举定义中（line 7-13），删除 `ToolCall` 变体，在其原位置插入两个新变体
  - **内容**: 
    ```rust
    /// 工具调用开始（参数已就绪）
    ToolStart {
        tool_call_id: String,
        name: String,
        display: String,
        args: String,          // 格式化后的参数（已含 cwd 缩短）
        input: serde_json::Value,  // 原始输入（Pipeline 用于 cwd 路径缩短）
    },
    /// 工具调用结果
    ToolEnd {
        tool_call_id: String,
        name: String,
        output: String,
        is_error: bool,
    },
    ```
  - **原因**: 拆分混合语义为两个独立事件，ToolStart 携带原始 input 供 Pipeline 做路径缩短，ToolEnd 携带输出结果

- [ ] **步骤 2: 在 events.rs 顶部添加 serde_json 导入**
  - **目标文件**: `rust-agent-tui/src/app/events.rs`
  - **位置**: 在文件开头（line 1-4），添加 `use serde_json::Value;` 导入
  - **内容**: 
    ```rust
    use serde_json::Value;
    ```
  - **原因**: ToolStart 变体需要使用 `serde_json::Value` 类型存储原始 input

- [ ] **步骤 3: 在 agent.rs 中调整 map_executor_event 的 ToolStart 分支**
  - **目标文件**: `rust-agent-tui/src/app/agent.rs`
  - **位置**: 在 `map_executor_event` 函数的 `ExecutorEvent::ToolStart` 匹配分支（line 244-250），将 `AgentEvent::ToolCall` 改为 `AgentEvent::ToolStart`
  - **内容**: 
    ```rust
    ExecutorEvent::ToolStart { tool_call_id, name, input, .. } => AgentEvent::ToolStart {
        tool_call_id,
        name: name.clone(),
        display: format_tool_name(&name),
        args: format_tool_args(&name, &input, Some(cwd)),
        input: input.clone(),
    },
    ```
  - **原因**: 将核心层的 ToolStart 事件映射到新的 AgentEvent::ToolStart，携带格式化后的 args 和原始 input

- [ ] **步骤 4: 在 agent.rs 中调整 map_executor_event 的 ask_user ToolEnd 分支**
  - **目标文件**: `rust-agent-tui/src/app/agent.rs`
  - **位置**: 在 `map_executor_event` 函数的 `ExecutorEvent::ToolEnd { name, is_error: false, .. } if name == "ask_user"` 匹配分支（line 256-264），将 `AgentEvent::ToolCall` 改为 `AgentEvent::ToolEnd`
  - **内容**: 
    ```rust
    ExecutorEvent::ToolEnd { tool_call_id, name, output, is_error: false, .. } if name == "ask_user" => {
        AgentEvent::ToolEnd {
            tool_call_id,
            name,
            output: format!("? → {}", truncate(&output, 60)),
            is_error: false,
        }
    }
    ```
  - **原因**: ask_user 成功时映射为 ToolEnd 事件，携带用户回答作为 output

- [ ] **步骤 5: 在 agent.rs 中调整 map_executor_event 的错误 ToolEnd 分支**
  - **目标文件**: `rust-agent-tui/src/app/agent.rs`
  - **位置**: 在 `map_executor_event` 函数的 `ExecutorEvent::ToolEnd { is_error: true, .. }` 匹配分支（line 266-272），将 `AgentEvent::ToolCall` 改为 `AgentEvent::ToolEnd`
  - **内容**: 
    ```rust
    ExecutorEvent::ToolEnd { tool_call_id, name, output, is_error: true, .. } => AgentEvent::ToolEnd {
        tool_call_id,
        name,
        output: format!("✗ {}", truncate(&output, 60)),
        is_error: true,
    },
    ```
  - **原因**: 工具执行错误时映射为 ToolEnd 事件，is_error=true，携带错误信息

- [ ] **步骤 6: 在 headless.rs test_tool_call_renders 中改用 ToolStart**
  - **目标文件**: `rust-agent-tui/src/ui/headless.rs`
  - **位置**: 在 `test_tool_call_renders` 测试函数（line 96-102），将 `AgentEvent::ToolCall` 改为 `AgentEvent::ToolStart`
  - **内容**: 
    ```rust
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "t1".into(),
        name: "read_file".into(),
        display: "ReadFile".into(),
        args: "src/main.rs".into(),
        input: serde_json::json!({"path": "src/main.rs"}),
    });
    ```
  - **原因**: 测试中 `is_error: false` 表示工具调用开始，应使用 ToolStart 事件

- [ ] **步骤 7: 在 headless.rs test_subagent_group_tools 中改用 ToolStart**
  - **目标文件**: `rust-agent-tui/src/ui/headless.rs`
  - **位置**: 在 `test_subagent_group_tools` 测试函数（line 395-408），将两处 `AgentEvent::ToolCall` 改为 `AgentEvent::ToolStart`
  - **内容**: 
    ```rust
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "t1".into(),
        name: "read_file".into(),
        display: "ReadFile".into(),
        args: "src/main.rs".into(),
        input: serde_json::json!({"path": "src/main.rs"}),
    });
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "t2".into(),
        name: "bash".into(),
        display: "Bash".into(),
        args: "cargo test".into(),
        input: serde_json::json!({"command": "cargo test"}),
    });
    ```
  - **原因**: 测试中 SubAgent 内部的工具调用（is_error: false）表示工具开始，应使用 ToolStart 事件

- [ ] **步骤 8: 在 headless.rs test_subagent_group_sliding_window 中改用 ToolStart**
  - **目标文件**: `rust-agent-tui/src/ui/headless.rs`
  - **位置**: 在 `test_subagent_group_sliding_window` 测试函数的循环（line 447-453），将 `AgentEvent::ToolCall` 改为 `AgentEvent::ToolStart`
  - **内容**: 
    ```rust
    for i in 1..=6 {
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: format!("t{}", i),
            name: "read_file".into(),
            display: "ReadFile".into(),
            args: format!("file{}.rs", i),
            input: serde_json::json!({"path": format!("file{}.rs", i)}),
        });
    }
    ```
  - **原因**: 测试中循环创建的工具调用（is_error: false）表示工具开始，应使用 ToolStart 事件

- [ ] **步骤 9: 在 headless.rs test_tool_call_message_visible_when_toggled 中改用 ToolStart**
  - **目标文件**: `rust-agent-tui/src/ui/headless.rs`
  - **位置**: 在 `test_tool_call_message_visible_when_toggled` 测试函数（line 522-528），将 `AgentEvent::ToolCall` 改为 `AgentEvent::ToolStart`
  - **内容**: 
    ```rust
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "bash".into(),
        display: "Bash".into(),
        args: "ls".into(),
        input: serde_json::json!({"command": "ls"}),
    });
    ```
  - **原因**: 测试中 `is_error: false` 表示工具调用开始，应使用 ToolStart 事件

- [ ] **步骤 10: 在 headless.rs test_tool_call_without_assistant_chunk_no_bubble 中改用 ToolStart**
  - **目标文件**: `rust-agent-tui/src/ui/headless.rs`
  - **位置**: 在 `test_tool_call_without_assistant_chunk_no_bubble` 测试函数（line 605-611），将 `AgentEvent::ToolCall` 改为 `AgentEvent::ToolStart`
  - **内容**: 
    ```rust
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "bash".into(),
        display: "Bash".into(),
        args: "ls".into(),
        input: serde_json::json!({"command": "ls"}),
    });
    ```
  - **原因**: 测试中 `is_error: false` 表示工具调用开始，应使用 ToolStart 事件

- [ ] **步骤 11: 在 agent_ops.rs 中临时兼容 ToolStart 事件**
  - **目标文件**: `rust-agent-tui/src/app/agent_ops.rs`
  - **位置**: 在 `handle_agent_event` 函数的 `match event` 分支中，查找 `AgentEvent::ToolCall` 的处理逻辑，在其后添加 `AgentEvent::ToolStart` 和 `AgentEvent::ToolEnd` 的临时兼容分支
  - **内容**: 
    ```rust
    AgentEvent::ToolStart { tool_call_id, name, display, args, input } => {
        // 临时兼容：复用现有 ToolCall 逻辑（忽略 input 字段）
        // TODO: Task 4 时重构为 Pipeline 驱动
        self.handle_tool_start(tool_call_id, name, display, args)
    }
    AgentEvent::ToolEnd { tool_call_id, name, output, is_error } => {
        // 临时兼容：复用现有 ToolCall 逻辑
        // TODO: Task 4 时重构为 Pipeline 驱动
        self.handle_tool_end(tool_call_id, name, output, is_error)
    }
    ```
  - **原因**: 在 Task 4 重构前，确保新事件类型能被正确处理，保持现有行为

- [x] **步骤 12: 运行所有 headless 测试验证事件拆分**
  - **目标文件**: `rust-agent-tui/src/ui/headless.rs`
  - **位置**: 终端执行测试命令
  - **内容**: 
    ```bash
    cargo test -p rust-agent-tui --lib -- headless
    ```
  - **预期输出**: 所有测试通过，无编译错误，输出包含 `test result: ok. X passed`（X 为测试数量）
  - **原因**: 验证事件拆分后测试代码正确适配，现有功能未受影响

---


### Task 2: AppCore 持有 MessagePipeline + 移除 subagent_group_idx

**背景:** 当前 AppCore 没有持有 MessagePipeline 实例，导致 agent_ops.rs 无法通过 Pipeline 统一处理消息事件。本 Task 将 MessagePipeline 实例添加到 AppCore，为后续 Task 3（Pipeline.handle_event）和 Task 4（agent_ops 重构）奠定基础。采用简化方案：本 Task 只添加 pipeline 字段，保留 subagent_group_idx 字段，留待 Task 4 一并移除。

---

#### 执行步骤

- [x] **步骤 1: 在 core.rs 顶部添加 MessagePipeline 导入**
  - **目标文件**: `rust-agent-tui/src/app/core.rs`
  - **位置**: 在文件开头（line 1-16），找到 `use super::agent_panel::AgentPanel;` 等导入语句，在其后添加新的导入
  - **内容**: 
    ```rust
    use super::message_pipeline::MessagePipeline;
    ```
  - **原因**: AppCore 结构体需要使用 MessagePipeline 类型，必须先导入该模块

- [x] **步骤 2: 在 AppCore 结构体中添加 pipeline 字段**
  - **目标文件**: `rust-agent-tui/src/app/core.rs`
  - **位置**: 在 `pub struct AppCore` 结构体定义中（line 19-48），在 `pub view_messages: Vec<MessageViewModel>,` 之后（line 20 之后）插入新字段
  - **内容**: 
    ```rust
    pub pipeline: MessagePipeline,
    ```
  - **原因**: 添加 MessagePipeline 实例到 AppCore，使其成为消息状态管理的核心组件

- [x] **步骤 3: 调整 AppCore::new 方法签名，添加 cwd 参数**
  - **目标文件**: `rust-agent-tui/src/app/core.rs`
  - **位置**: 在 `impl AppCore` 块的 `pub fn new` 方法签名（line 52），在 `command_registry: CommandRegistry,` 参数之后、`skills: Vec<SkillMetadata>)` 参数之前插入新参数
  - **内容**: 
    ```rust
    pub fn new(cwd: String,
               render_tx: mpsc::UnboundedSender<RenderEvent>,
               render_cache: Arc<RwLock<RenderCache>>,
               render_notify: Arc<Notify>,
               command_registry: CommandRegistry,
               skills: Vec<SkillMetadata>) -> Self {
    ```
  - **原因**: MessagePipeline::new() 需要 cwd 参数，必须在 AppCore::new() 中接收并传递

- [x] **步骤 4: 在 AppCore::new 方法体中初始化 pipeline 字段**
  - **目标文件**: `rust-agent-tui/src/app/core.rs`
  - **位置**: 在 `AppCore::new` 方法体的结构体初始化代码中（line 62-88），在 `view_messages: Vec::new(),` 之后（line 63 之后）插入新字段初始化
  - **内容**: 
    ```rust
    pipeline: MessagePipeline::new(cwd),
    ```
  - **原因**: 使用传入的 cwd 参数初始化 MessagePipeline 实例，存储在 AppCore 中

- [x] **步骤 5: 调整 app/mod.rs 的 App::new 方法，传递 cwd 参数给 AppCore::new**
  - **目标文件**: `rust-agent-tui/src/app/mod.rs`
  - **位置**: 在 `impl App` 块的 `App::new` 方法中（line 150），找到 `core: AppCore::new(render_tx, render_cache, render_notify, command_registry, skills),` 调用，在第一个参数位置插入 cwd 参数
  - **内容**: 
    ```rust
    core: AppCore::new(cwd.clone(), render_tx, render_cache, render_notify, command_registry, skills),
    ```
  - **原因**: App::new() 方法已有 cwd 参数（line 153），需要将其传递给 AppCore::new() 以初始化 Pipeline

- [x] **步骤 6: 调整 panel_ops.rs 的 new_headless 方法，传递 cwd 参数给 AppCore::new**
  - **目标文件**: `rust-agent-tui/src/app/panel_ops.rs`
  - **位置**: 在 `pub fn new_headless` 方法中（line 207-213），找到 `let core = super::AppCore::new(...)` 调用，在第一个参数位置插入 cwd 参数
  - **内容**: 
    ```rust
    let core = super::AppCore::new(
        "/tmp".to_string(),
        render_tx,
        render_cache,
        Arc::clone(&render_notify),
        crate::command::default_registry(),
        Vec::new(),
    );
    ```
  - **原因**: new_headless 用于测试，使用固定测试目录 "/tmp" 作为 cwd，确保 AppCore::new() 接收到有效的 cwd 参数

- [x] **步骤 7: 在 core.rs 中添加单元测试验证 pipeline 字段初始化**
  - **目标文件**: `rust-agent-tui/src/app/core.rs`
  - **位置**: 在文件末尾（impl AppCore 块之后），添加新的测试模块
  - **内容**: 
    ```rust
    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_appcore_pipeline_initialized() {
            // 创建必要的依赖
            let (render_tx, _, _) = crate::ui::render_thread::spawn_render_thread(80);
            let render_cache = Arc::new(RwLock::new(Default::default()));
            let render_notify = Arc::new(tokio::sync::Notify::new());
            let command_registry = crate::command::default_registry();
            let skills = Vec::new();
            let cwd = "/test/path".to_string();

            // 创建 AppCore
            let core = AppCore::new(
                cwd.clone(),
                render_tx,
                render_cache,
                render_notify,
                command_registry,
                skills,
            );

            // 验证 pipeline 字段已正确初始化
            assert_eq!(core.pipeline.cwd(), cwd);
            assert_eq!(core.pipeline.completed_messages().len(), 0);
        }
    }
    ```
  - **原因**: 验证 AppCore 正确初始化 MessagePipeline 实例，cwd 参数正确传递，确保后续 Task 可以依赖 core.pipeline 访问 Pipeline 功能

- [x] **步骤 8: 运行 core.rs 单元测试验证 pipeline 初始化**
  - **目标文件**: `rust-agent-tui/src/app/core.rs`
  - **位置**: 终端执行测试命令
  - **内容**: 
    ```bash
    cargo test -p rust-agent-tui --lib -- test_appcore_pipeline_initialized
    ```
  - **预期输出**: 测试通过，输出包含 `test test_appcore_pipeline_initialized ... ok` 和 `test result: ok. 1 passed`
  - **原因**: 验证 pipeline 字段正确初始化，确保 AppCore 持有有效的 MessagePipeline 实例，为后续 Task 奠定基础

- [x] **步骤 9: 运行所有单元测试确保无回归**
  - **目标文件**: `rust-agent-tui/src/app/`
  - **位置**: 终端执行测试命令
  - **内容**: 
    ```bash
    cargo test -p rust-agent-tui --lib
    ```
  - **预期输出**: 所有测试通过，无编译错误，输出包含 `test result: ok. X passed`（X 为测试数量）
  - **原因**: 确保 AppCore::new() 签名变更后，所有现有调用点（app/mod.rs 和 panel_ops.rs）正确适配，无功能回归

---

### Task 3: MessagePipeline 新增 handle_event 统一入口

**背景:** 当前 MessagePipeline 提供了分散的 push_chunk / tool_start / tool_end 等方法，但没有统一的 AgentEvent 处理入口。agent_ops.rs 中的 handle_agent_event() 无法直接委托给 Pipeline。本 Task 新增 `handle_event()` 方法，将所有 AgentEvent 变体路由到 Pipeline 内部方法，返回 PipelineAction 列表供 agent_ops 映射到 RenderEvent。Task 4 将直接调用此方法完成委托。

#### 执行步骤

- [x] **步骤 1: 在 message_pipeline.rs 顶部添加 AgentEvent 导入**
  - 目标文件: `rust-agent-tui/src/app/message_pipeline.rs`
  - 位置: 在现有 `use` 语句之后（~L30），添加 `use crate::app::events::AgentEvent;`
  - 内容: `use crate::app::events::AgentEvent;`
  - 原因: handle_event 方法需要匹配 AgentEvent 枚举变体

- [x] **步骤 2: 在 MessagePipeline impl 块中新增 handle_event 方法**
  - 目标文件: `rust-agent-tui/src/app/message_pipeline.rs`
  - 位置: 在 `pub fn cwd(&self)` 方法之后（~L116），插入新的 `handle_event` 方法
  - 内容:
    ```rust
    /// 统一事件处理入口：将 AgentEvent 转换为 PipelineAction 列表。
    /// agent_ops 通过此方法委托所有消息状态管理逻辑。
    pub fn handle_event(&mut self, event: AgentEvent) -> Vec<PipelineAction> {
        match event {
            AgentEvent::AssistantChunk(chunk) => {
                if chunk.is_empty() {
                    // 空 chunk：不创建新 bubble，仅追加到已有 bubble
                    vec![PipelineAction::None]
                } else if self.in_subagent() {
                    // SubAgent 内部：路由到 subagent_push_chunk
                    self.subagent_push_chunk(&chunk);
                    vec![self.build_subagent_update()
                        .map(PipelineAction::UpdateLast)
                        .unwrap_or(PipelineAction::None)]
                } else {
                    // 父 Agent：流式追加
                    self.push_chunk(&chunk);
                    vec![PipelineAction::AppendChunk(chunk)]
                }
            }
            AgentEvent::ToolStart { tool_call_id, name, display: _, args: _, input } => {
                if self.in_subagent() {
                    // SubAgent 内部工具调用
                    self.subagent_tool_start(&name, input);
                    vec![self.build_subagent_update()
                        .map(PipelineAction::UpdateLast)
                        .unwrap_or(PipelineAction::None)]
                } else {
                    // 父 Agent 工具调用
                    vec![self.tool_start(&tool_call_id, &name, input)]
                }
            }
            AgentEvent::ToolEnd { tool_call_id, name, output, is_error } => {
                if self.in_subagent() {
                    // SubAgent 内部工具结果（仅更新 UI，不影响 completed）
                    vec![self.build_subagent_update()
                        .map(PipelineAction::UpdateLast)
                        .unwrap_or(PipelineAction::None)]
                } else {
                    vec![self.tool_end(&tool_call_id, &name, &output, is_error)]
                }
            }
            AgentEvent::SubAgentStart { agent_id, task_preview } => {
                // SubAgentStart 通过 tool_start("launch_agent", ...) 触发内部逻辑
                let input = serde_json::json!({"agent_id": &agent_id, "task": &task_preview});
                vec![self.tool_start("subagent", "launch_agent", input)]
            }
            AgentEvent::SubAgentEnd { result, is_error } => {
                vec![self.tool_end("subagent", "launch_agent", &result, is_error)]
            }
            AgentEvent::Done => {
                self.done();
                let vms = self.reconcile();
                vec![PipelineAction::RebuildAll(vms)]
            }
            AgentEvent::Interrupted => {
                self.interrupt();
                vec![PipelineAction::None]
            }
            AgentEvent::StateSnapshot(msgs) => {
                self.set_completed(msgs);
                vec![PipelineAction::None]
            }
            // 以下事件由 agent_ops 直接处理，Pipeline 返回 None
            AgentEvent::Error(_)
            | AgentEvent::InteractionRequest { .. }
            | AgentEvent::TodoUpdate(_)
            | AgentEvent::CompactDone { .. }
            | AgentEvent::CompactError(_)
            | AgentEvent::TokenUsageUpdate { .. }
            | AgentEvent::LlmRetrying { .. } => {
                vec![PipelineAction::None]
            }
        }
    }
    ```
  - 原因: 提供统一入口，让 agent_ops 可以一行代码委托所有消息状态管理。每个分支内部调用已有的 Pipeline 方法（push_chunk / tool_start / tool_end / done / reconcile），不重复实现逻辑

- [x] **步骤 3: 新增 handle_event 的单元测试——流式文本路径**
  - 目标文件: `rust-agent-tui/src/app/message_pipeline.rs`
  - 位置: 在现有 `mod tests` 块末尾（~L640），追加测试函数
  - 内容:
    ```rust
    /// 测试：handle_event AssistantChunk 产生 AppendChunk
    #[test]
    fn test_handle_event_assistant_chunk() {
        let mut pipeline = MessagePipeline::new("/tmp".to_string());
        let actions = pipeline.handle_event(AgentEvent::AssistantChunk("hello".into()));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], PipelineAction::AppendChunk(ref c) if c == "hello"));
    }

    /// 测试：handle_event 空 chunk 不产生 AppendChunk
    #[test]
    fn test_handle_event_empty_chunk() {
        let mut pipeline = MessagePipeline::new("/tmp".to_string());
        let actions = pipeline.handle_event(AgentEvent::AssistantChunk(String::new()));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], PipelineAction::None));
    }

    /// 测试：handle_event ToolStart + ToolEnd + Done 产生完整生命周期
    #[test]
    fn test_handle_event_tool_lifecycle() {
        let mut pipeline = MessagePipeline::new("/tmp".to_string());
        // ToolStart
        let actions = pipeline.handle_event(AgentEvent::ToolStart {
            tool_call_id: "tc1".into(),
            name: "read_file".into(),
            display: "ReadFile".into(),
            args: "src/main.rs".into(),
            input: serde_json::json!({"file_path": "/tmp/src/main.rs"}),
        });
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], PipelineAction::AddMessage(_)));
        // ToolEnd
        let actions = pipeline.handle_event(AgentEvent::ToolEnd {
            tool_call_id: "tc1".into(),
            name: "read_file".into(),
            output: "file content".into(),
            is_error: false,
        });
        // ToolEnd 对只读工具返回 None
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], PipelineAction::None));
        // Done → RebuildAll
        let actions = pipeline.handle_event(AgentEvent::Done);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], PipelineAction::RebuildAll(_)));
    }

    /// 测试：handle_event StateSnapshot 更新 completed
    #[test]
    fn test_handle_event_state_snapshot() {
        let mut pipeline = MessagePipeline::new("/tmp".to_string());
        let msgs = vec![BaseMessage::human("hello"), BaseMessage::ai("world")];
        let actions = pipeline.handle_event(AgentEvent::StateSnapshot(msgs.clone()));
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], PipelineAction::None));
        // 验证 completed 已更新
        assert_eq!(pipeline.completed_messages().len(), 2);
    }
    ```
  - 原因: 验证 handle_event 对各种 AgentEvent 变体的路由逻辑正确，覆盖流式文本、空 chunk、工具生命周期、Done reconcile、StateSnapshot 五个核心路径

#### 检查步骤

- [x] 验证 handle_event 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 编译成功，无错误
- [x] 运行 message_pipeline 测试
  - `cargo test -p rust-agent-tui --lib -- message_pipeline`
  - 预期: 所有测试通过（原有 5 个 + 新增 4 个 = 9 个）

---
