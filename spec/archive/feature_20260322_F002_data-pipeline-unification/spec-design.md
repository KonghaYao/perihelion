# Feature: 20260322_F002 - data-pipeline-unification

## 需求背景

当前数据管道在实时流式显示和历史恢复方面存在不一致：

- **实时流式**：`ExecutorEvent::ToolStart` 携带 `{ name, input }` → `format_tool_call_display()` 提取参数生成 display（如 `ReadFile(/path/to/file)`）
- **历史恢复**：`BaseMessage::Tool` 只存储 `tool_call_id + content`，丢失参数信息 → display 只显示工具名（如 `ReadFile`）

这导致用户体验不一致：实时对话能看到工具调用参数，但恢复历史时看不到。

## 目标

- 统一实时流式和历史恢复的渲染逻辑
- 工具调用参数在两种场景下显示一致
- 最小化数据结构变更，保持向后兼容

## 方案设计

### 核心思路

**方案 B2：彻底统一** —— 实时和历史走同一渲染路径，工具参数统一从 `Ai.tool_calls` 提取。

### 数据流对比

![数据流对比：变更前 vs 变更后](./images/01-flow.png)

#### 变更前（不一致）

```
实时流式：
  ToolStart { name, input } → format_tool_call_display() → "ReadFile(/path)"
  ToolBlock { display: "ReadFile(/path)" }  ✅ 有参数

历史恢复：
  BaseMessage::Tool { tool_call_id, content } → 查找 name → "ReadFile"
  ToolBlock { display: "ReadFile" }  ❌ 无参数
```

#### 变更后（统一）

```
实时流式：
  ToolStart { tool_call_id, name, input } → 统一格式化函数
  ToolBlock { display: "ReadFile(/path)" }  ✅ 有参数

历史恢复：
  BaseMessage::Tool { tool_call_id } → 从 prev Ai.tool_calls 查找 input
  ToolBlock { display: "ReadFile(/path)" }  ✅ 有参数
```

### 架构变更

#### 1. ExecutorEvent 扩展

**文件**：`rust-create-agent/src/agent/events.rs`

```rust
pub enum ExecutorEvent {
    // 变更前
    ToolStart { name: String, input: Value },
    
    // 变更后
    ToolStart { 
        tool_call_id: String,  // 新增
        name: String, 
        input: Value 
    },
    // ...
}
```

#### 2. AgentEvent 扩展

**文件**：`rust-agent-tui/src/app/mod.rs`

```rust
pub enum AgentEvent {
    ToolCall {
        tool_call_id: String,  // 新增
        name: String,
        display: String,
        is_error: bool,
    },
    // ...
}
```

#### 3. ReActAgent 发出 ToolStart 时携带 tool_call_id

**文件**：`rust-create-agent/src/agent/react.rs`

在 `invoke_tool_call` 函数中，`ToolStart` 事件需要携带 `tool_call.id`：

```rust
// 变更前
event_handler.handle(ExecutorEvent::ToolStart {
    name: tool_call.name.clone(),
    input: tool_call.input.clone(),
});

// 变更后
event_handler.handle(ExecutorEvent::ToolStart {
    tool_call_id: tool_call.id.clone(),
    name: tool_call.name.clone(),
    input: tool_call.input.clone(),
});
```

#### 4. TUI 事件转换

**文件**：`rust-agent-tui/src/app/agent.rs`

```rust
// 变更前
ExecutorEvent::ToolStart { name, input } => AgentEvent::ToolCall {
    display: format_tool_call_display(&name, &input),
    name,
    is_error: false,
},

// 变更后
ExecutorEvent::ToolStart { tool_call_id, name, input } => AgentEvent::ToolCall {
    tool_call_id,
    name,
    display: format_tool_call_display(&name, &input),
    is_error: false,
},
```

#### 5. 历史恢复统一渲染

**文件**：`rust-agent-tui/src/app/mod.rs` → `open_thread()`

变更 `prev_ai_tool_calls` 存储 `(id, name, input)` 而非 `(id, name)`：

```rust
// 变更前
let mut prev_ai_tool_calls: Vec<(String, String)> = Vec::new();

// 变更后
let mut prev_ai_tool_calls: Vec<(String, String, Value)> = Vec::new();
```

`MessageViewModel::from_base_message` 增加 `input` 参数：

```rust
pub fn from_base_message(
    msg: &BaseMessage, 
    prev_ai_tool_calls: &[(String, String, Value)]  // 增加 input
) -> Self {
    // ...
    BaseMessage::Tool { tool_call_id, content, is_error } => {
        let (tool_name, input) = prev_ai_tool_calls
            .iter()
            .find(|(id, _, _)| id == tool_call_id)
            .map(|(_, name, input)| (name.clone(), input.clone()))
            .unwrap_or_else(|| (tool_call_id.clone(), Value::Null));
        
        let display = format_tool_call_display(&tool_name, &input);
        // ...
    }
}
```

#### 6. 统一格式化函数

**文件**：`rust-agent-tui/src/app/agent.rs`

将 `format_tool_call_display` 和 `extract_display_arg` 提取为公共模块：

```rust
// rust-agent-tui/src/app/tool_display.rs (新文件)

pub fn format_tool_call_display(tool: &str, input: &serde_json::Value) -> String {
    let name = to_pascal(tool);
    let arg = extract_display_arg(tool, input);
    match arg {
        Some(a) => format!("{}({})", name, truncate(&a, 60)),
        None => name,
    }
}

pub fn extract_display_arg(tool: &str, input: &serde_json::Value) -> Option<String> {
    // 现有逻辑不变
}
```

### 消息结构保持不变

`BaseMessage::Tool` **不增加 input 字段**，理由：
1. 避免存储膨胀（input 可能很大）
2. Ai.tool_calls 已包含完整参数，无需重复存储
3. 历史恢复时可从相邻 Ai 消息查找

### 边界情况处理

| 场景 | 处理方式 |
|------|----------|
| Tool 消息无匹配的 tool_call_id | 使用 tool_call_id 作为 name，display = name |
| input 为空或 Null | display = 工具名（无参数） |
| 跨 Ai 消息的 Tool 调用 | prev_ai_tool_calls 在每次遇到 Ai 消息时重置 |

## 实现要点

1. **最小变更原则**：只修改事件携带的数据，不改变消息存储结构
2. **向后兼容**：旧的历史消息（无 tool_call_id）仍能正常显示，只是没有参数
3. **统一入口**：`format_tool_call_display` 函数被实时和历史共用
4. **测试覆盖**：
   - 实时流式工具调用显示参数
   - 历史恢复工具调用显示参数
   - 多工具调用顺序正确
   - 无匹配 tool_call_id 时的降级处理

## 约束一致性

本方案不涉及架构变更，与现有约束一致。

## 验收标准

- [ ] 实时流式时 ToolBlock 显示工具调用参数（如 `ReadFile(/path/to/file)`）
- [ ] 历史恢复时 ToolBlock 显示相同格式的参数
- [ ] 多工具调用时参数正确匹配
- [ ] 无匹配 tool_call_id 时不崩溃，降级显示工具名
- [ ] 单元测试覆盖实时和历史两种路径
