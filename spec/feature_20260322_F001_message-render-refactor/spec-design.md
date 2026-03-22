# Feature: 20260322_F001 - message-render-refactor

## 需求背景

当前 TUI 的消息渲染架构存在三个核心问题：

1. **数据与渲染耦合**：`poll_agent()` 同时承担 AgentEvent 数据处理和 UI 状态更新，`ChatMessage` 既是数据容器又是渲染单元，职责混合导致代码难以维护和扩展。
2. **流式输出处理粗糙**：`push_str()` 只做简单的文本追加，无法处理 ContentBlock 级别的流式更新（如 Reasoning block 和 Text block 交替出现的场景）。
3. **历史数据加载不一致**：从 SQLite 加载历史 thread 时，Tool 消息的 `display_name`/`tool_name` 靠 hack 方式从 `tool_call_id` 提取，信息不可靠，且加载后的显示效果与实时对话不一致。

## 目标

- 引入 ViewModel 中间层，将数据处理（BaseMessage/AgentEvent）和 UI 渲染完全解耦
- 支持 Markdown 渲染（使用 tui-markdown crate），包括代码块高亮、列表、粗体等
- 支持 ContentBlock 粒度的独立渲染（Text/Reasoning/ToolUse 各有独立样式）
- 工具调用结果支持折叠/展开，默认折叠以减少屏幕占用
- 流式输出在 ViewModel 层正确处理，markdown 解析支持降频避免卡顿

## 方案设计

### 架构总览

引入三层架构：

```
数据层 (BaseMessage / AgentEvent)
    ↓  转换
视图模型层 (MessageViewModel)
    ↓  渲染
渲染层 (ratatui Widget / tui-markdown)
```

- **数据层**：`BaseMessage`（核心消息）和 `AgentEvent`（运行时事件）保持不变
- **视图模型层**：新增 `MessageViewModel` 枚举，是渲染层唯一的数据源
- **渲染层**：从 ViewModel 读取预渲染数据，不再直接接触 BaseMessage

### ViewModel 数据模型

```rust
/// 渲染层的视图模型，从 BaseMessage/AgentEvent 转换而来
pub enum MessageViewModel {
    /// 用户输入
    UserBubble {
        content: String,
        rendered: Text<'static>,       // tui-markdown 预渲染结果
    },
    /// AI 回复（支持流式追加）
    AssistantBubble {
        blocks: Vec<ContentBlockView>, // Block 级别的渲染单元
        is_streaming: bool,            // 是否正在流式输出
    },
    /// 工具调用结果
    ToolBlock {
        tool_name: String,
        display_name: String,
        content: String,
        is_error: bool,
        collapsed: bool,               // 折叠状态，默认 true
        color: Color,                  // 预计算的颜色
    },
    /// 系统消息
    SystemNote {
        content: String,
    },
    /// Todo 状态（特殊系统消息，支持原地更新）
    TodoStatus {
        rendered: String,
    },
}

/// ContentBlock 的视图化表示
pub enum ContentBlockView {
    /// 文本内容（含 markdown 解析缓存）
    Text {
        raw: String,
        rendered: Text<'static>,       // tui-markdown 解析结果
        dirty: bool,                   // 内容变化标记，true 时需重新解析
    },
    /// 推理/思考过程（仅显示字数摘要）
    Reasoning {
        char_count: usize,
    },
    /// 工具使用请求（AI 发起的调用请求）
    ToolUse {
        name: String,
        input_preview: String,
    },
}
```

### 数据流重构

#### 当前数据流（废弃）

```
AgentEvent → poll_agent() → 直接操作 app.messages: Vec<ChatMessage>
                                         ↓
                             message_to_lines() → 每帧重新解析渲染
```

#### 新数据流

```
AgentEvent → poll_agent() → 转换为 MessageViewModel
                                  ↓
                           app.view_messages: Vec<MessageViewModel>
                                  ↓
                        render_messages() → 使用预渲染的 Text 直接绘制
```

核心变化：

1. **App 字段替换**：`messages: Vec<ChatMessage>` → `view_messages: Vec<MessageViewModel>`
   - `agent_state_messages: Vec<BaseMessage>` 保持不变（LLM 对话历史，不受影响）

2. **poll_agent() 精简为纯转换**：
   - `AssistantChunk(chunk)` → 找到最后一个 `AssistantBubble`，调用 `append_chunk(chunk)`，标记 dirty
   - `ToolCall { name, display, is_error }` → push `ToolBlock { collapsed: true, ... }`
   - `TodoUpdate(todos)` → 更新/创建 `TodoStatus`
   - `StateSnapshot` → 仅更新 `agent_state_messages`，不影响 view_messages
   - `Done` → 将最后一个 `AssistantBubble` 的 `is_streaming` 设为 false

3. **open_thread() 历史加载统一转换**：
   - 遍历 `Vec<BaseMessage>`，每条调用 `MessageViewModel::from_base_message()` 转换
   - Tool 消息的 display_name 提取逻辑封装在 `from_base_message()` 内部，一处维护

### 流式 Markdown 处理策略

流式输出时 markdown 解析的性能策略：

1. **追加阶段**：每个 `AssistantChunk` 到来时，追加到 `ContentBlockView::Text.raw`，设置 `dirty = true`
2. **解析阶段**：每帧渲染前检查 dirty flag，dirty 为 true 时用 tui-markdown 重新解析 `raw → rendered`
3. **降频优化**：如果 tui-markdown 解析耗时成为瓶颈，引入 `last_parse_time` 字段，限制解析频率（例如每 100ms 最多解析一次），中间帧使用旧的 `rendered` 缓存
4. **宽度感知**：终端窗口宽度变化时，invalidate 所有 ViewModel 的缓存，触发全量重新解析

### 工具折叠交互

- 工具调用结果默认 `collapsed = true`，只显示 header 行：`⚙ tool_name`
- 用户可通过键盘操作（如方向键定位 + Enter 切换）展开/折叠
- 折叠状态不持久化，历史加载后默认折叠

### 文件结构

```
rust-agent-tui/src/
├── ui.rs                      # 主渲染入口（保留，精简）
├── ui/
│   ├── mod.rs                 # ui 子模块声明
│   ├── message_view.rs        # MessageViewModel 定义 + from_base_message 转换
│   ├── message_render.rs      # ViewModel → ratatui Widget 渲染逻辑
│   └── markdown.rs            # tui-markdown 封装（解析 + 缓存管理）
├── app/
│   └── mod.rs                 # App 字段从 Vec<ChatMessage> 改为 Vec<MessageViewModel>
```

### 渲染函数重构

```rust
// message_render.rs

/// 将单个 ViewModel 渲染为 Vec<Line>
fn render_view_model(vm: &MessageViewModel, width: usize) -> Vec<Line<'static>> {
    match vm {
        MessageViewModel::UserBubble { rendered, .. } => {
            // "▶ 你  " 前缀 + rendered 内容
        }
        MessageViewModel::AssistantBubble { blocks, .. } => {
            // "◆ Agent  " 前缀
            // 遍历 blocks，每个 ContentBlockView 独立渲染
        }
        MessageViewModel::ToolBlock { collapsed: true, display_name, color, .. } => {
            // 仅显示折叠态 header: "⚙ display_name ▸"
        }
        MessageViewModel::ToolBlock { collapsed: false, content, display_name, color, .. } => {
            // 展开态: header + 内容（"│ " 前缀）
        }
        MessageViewModel::SystemNote { content } => {
            // "ℹ " 前缀
        }
        MessageViewModel::TodoStatus { rendered } => {
            // 直接渲染 todo 文本
        }
    }
}
```

## 实现要点

1. **依赖新增**：`tui-markdown = "0.3"` 加入 `rust-agent-tui/Cargo.toml`
2. **完全替换 ChatMessage**：不做渐进迁移，一次性将 ChatMessage 替换为 MessageViewModel
3. **poll_agent() 精简**：仅负责 `AgentEvent → ViewModel` 转换，不再直接操作 UI 状态逻辑
4. **Markdown 解析降频**：流式输出时不需要每个 chunk 都重新解析，累积后批量解析
5. **折叠状态不持久化**：每次加载默认折叠，无需存储到 SQLite
6. **向后兼容**：
   - `agent_state_messages`（LLM 对话历史）和 SQLite 持久化逻辑完全不变
   - HITL、AskUser、Todo 等弹窗功能不受影响
   - AgentEvent 枚举不变，变化仅在消费端

## 验收标准

- [ ] ChatMessage 完全移除，所有消息通过 MessageViewModel 渲染
- [ ] AI 回复文本支持 Markdown 渲染（代码块高亮、列表、粗体）
- [ ] 工具调用结果默认折叠，可通过键盘展开/折叠
- [ ] 流式输出流畅，markdown 解析不造成明显卡顿
- [ ] 历史 thread 加载后显示效果与实时对话一致
- [ ] 现有功能（HITL 弹窗、AskUser 弹窗、Todo 状态、滚动、中断）不受影响
- [ ] `cargo build` 无 warning，`cargo test` 全部通过
