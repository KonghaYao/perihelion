# Feature: 20260326_F001 - subagent-message-hierarchy

## 需求背景

当前 TUI 中，SubAgent（`launch_agent` 工具触发的子 Agent）执行的工具调用和回复消息与父 Agent 的消息完全扁平地混合在一起，无法从视觉上区分哪些消息属于子 Agent、哪些属于父 Agent。用户难以跟踪子 Agent 的执行进度，也无法折叠/展开子 Agent 的执行详情。

## 目标

- 将 SubAgent 的所有消息（工具调用、AI 回复）包裹在一个可折叠的层级块中
- 实时显示子 Agent 的执行状态，但只保留最近 4 步（避免信息过载）
- 以总步数概括 SubAgent 的工作量（"已执行 N 步"）
- 完成后可折叠，只显示摘要结果

## 方案设计

### 整体策略

采用纯 TUI 层感知方案（方案 A）：利用现有 `ExecutorEvent::ToolStart/ToolEnd { name: "launch_agent" }` 事件作为子 Agent 生命周期的边界信号，无需修改 `rust-create-agent` 或 `rust-agent-middlewares`。所有变更限于 `rust-agent-tui` crate。

![事件流数据流图](./images/01-flow.png)

### 新增 TUI 事件变体

在 `rust-agent-tui/src/app/events.rs` 中新增两个事件变体：

```rust
pub enum AgentEvent {
    // ... 现有变体 ...

    /// SubAgent 开始执行（由 launch_agent ToolStart 映射而来）
    SubAgentStart {
        agent_id: String,
        task_preview: String,   // task 参数的前 40 个字符
    },

    /// SubAgent 执行结束（由 launch_agent ToolEnd 映射而来）
    SubAgentEnd {
        result: String,         // 工具返回值（格式化后的执行摘要）
        is_error: bool,
    },
}
```

### 新增 ViewModel 变体

在 `rust-agent-tui/src/ui/message_view.rs` 中新增 `SubAgentGroup` 变体：

```rust
pub enum MessageViewModel {
    // ... 现有变体 ...

    /// SubAgent 执行块（可折叠，含滑动窗口消息）
    SubAgentGroup {
        agent_id: String,
        task_preview: String,
        total_steps: usize,                    // 总步数（工具调用 + AI 回复）
        recent_messages: Vec<MessageViewModel>, // 滑动窗口，最多 4 条
        is_running: bool,                      // 执行中为 true
        collapsed: bool,                       // 默认展开，完成后可折叠
        final_result: Option<String>,          // SubAgentEnd 携带的结果摘要
    },
}
```

### 事件映射（agent.rs）

在 `FnEventHandler` 的事件映射逻辑中，为 `launch_agent` 添加专属处理分支：

```rust
// launch_agent ToolStart → SubAgentStart
ExecutorEvent::ToolStart { name, input, .. } if name == "launch_agent" => {
    let agent_id = input["agent_id"].as_str().unwrap_or("unknown").to_string();
    let task_preview = input["task"].as_str().unwrap_or("")
        .chars().take(40).collect();
    AgentEvent::SubAgentStart { agent_id, task_preview }
}

// launch_agent ToolEnd（成功或失败）→ SubAgentEnd
ExecutorEvent::ToolEnd { name, output, is_error, .. } if name == "launch_agent" => {
    AgentEvent::SubAgentEnd { result: output, is_error }
}
```

### App 状态管理与消息路由

在 `App` 结构体中新增追踪字段：

```rust
/// 当前活跃 SubAgentGroup 在 view_messages 中的下标（执行中时有值）
subagent_group_idx: Option<usize>,
```

在 `handle_agent_event` 中，根据 `subagent_group_idx` 决定消息的路由目标：

| 事件 | `subagent_group_idx == None` | `subagent_group_idx == Some(i)` |
|------|------|------|
| `SubAgentStart` | 创建 SubAgentGroup，push 到末尾，记录 idx | 不会发生（只有一层） |
| `ToolCall` | 正常创建 ToolBlock（父 Agent 层） | 路由进 SubAgentGroup.recent_messages（total_steps+1，超 4 条时 pop 最早一条） |
| `AssistantChunk` | 正常 append_chunk 到父 Agent AssistantBubble | 路由进 SubAgentGroup 内最后一条 AssistantBubble |
| `MessageAdded` | 正常处理（父 Agent AI/Tool 消息渲染） | 忽略（不影响父 Agent 消息历史） |
| `SubAgentEnd` | 不会发生 | 设置 is_running=false，写入 final_result，清空 idx |

每次 SubAgentGroup 内部更新，发送 `RenderEvent::UpdateLastMessage(vm.clone())`，令渲染线程重绘最后一条消息。

### 渲染线程扩展

在 `render_thread.rs` 中新增：

```rust
pub enum RenderEvent {
    // ... 现有变体 ...
    /// 替换最后一条消息并重新渲染（SubAgentGroup 更新专用）
    UpdateLastMessage(MessageViewModel),
}
```

`RenderTask::run` 处理 `UpdateLastMessage`：

1. `messages.last_mut()` 替换为新 ViewModel
2. 重新渲染该消息的行（与 `AppendChunk` 路径相同）
3. 替换缓存中对应区间的行，version 自增

### 渲染样式

![TUI 渲染示意图](./images/02-wireframe.png)

**运行中（展开）：**
```
▾ 🤖 code-reviewer  「审查 src/ 目录下的代码...」  [运行中 · 已执行 5 步]
  ▸ read_file  src/main.rs
  ▸ bash  cargo test --lib
  3 分析完成...
  ▸ read_file  src/lib.rs
  [仅显示最近 4/5 步]
```

**完成（展开）：**
```
▾ 🤖 code-reviewer  「已完成 12 步」
  ▸ bash  cargo test
  2 发现 2 处问题
  ▸ edit_file  src/main.rs
  [仅显示最近 4/12 步]
  结果: 共修复 2 处类型错误…
```

**折叠（完成后用户可折叠）：**
```
▸ 🤖 code-reviewer  「已完成 12 步」  共修复 2 处类型错误…
```

颜色方案：Agent 头行使用 `Color::Rgb(129, 199, 132)`（绿色系），内嵌工具消息保持原有颜色不变，"运行中"指示用 `Color::Yellow`。

## 实现要点

1. **滑动窗口管理**：每次向 `recent_messages` push 新消息前，检查长度是否 ≥ 4，若是则 `remove(0)` 移除最老一条，`total_steps` 单独累计不受影响
2. **AssistantChunk 路由**：当 `subagent_group_idx.is_some()` 时，`AssistantChunk` 需路由到 SubAgentGroup 内的最后一条 `AssistantBubble`；若最后一条不是 AssistantBubble，则先在 `recent_messages` 中创建新的 AssistantBubble
3. **渲染保证**：SubAgentGroup 在子 Agent 执行期间始终是 `view_messages` 的最后一条消息（父 Agent 阻塞等待 `launch_agent` 返回），因此 `UpdateLastMessage` 操作安全且高效
4. **折叠交互**：SubAgentGroup 的 `collapsed` 字段接入现有键盘事件处理（Enter 键 toggle_collapse），渲染时只显示头行和 final_result 摘要
5. **错误路径**：`SubAgentEnd { is_error: true }` 时头行颜色改为 `Color::Red`，final_result 显示错误信息

## 约束一致性

- **事件驱动 TUI 通信**（architecture.md 架构决策）：本方案完全遵守 mpsc channel + oneshot 通信模式，SubAgentGroup 通过 `RenderEvent::UpdateLastMessage` 驱动渲染，不引入共享可变状态
- **消息不可变历史**（architecture.md）：SubAgent 内部消息不写入父 Agent 的 `AgentState`，不影响 LLM 上下文
- **Workspace 分层**（constraints.md）：所有变更限于 `rust-agent-tui`，不依赖下层 crate 的上层模块
- **Middleware Chain 模式**：无需修改 SubAgentMiddleware 或其他中间件

## 验收标准

- [ ] SubAgent 执行时，TUI 消息列表中出现 `▾ 🤖 {agent_id}` 头行，包裹其内部消息
- [ ] SubAgent 执行期间，最多显示最近 4 条内部消息，头行实时更新"已执行 N 步"计数
- [ ] SubAgent 完成后，头行显示"已完成 N 步"，展示最终结果摘要
- [ ] 用户可通过 Enter 键折叠/展开 SubAgentGroup
- [ ] 折叠状态下，只显示头行和 final_result 单行摘要
- [ ] 父 Agent 的工具调用和 AI 回复仍在 SubAgentGroup 外正常显示
- [ ] SubAgent 执行出错时，头行变为红色，错误信息显示在 final_result 位置
- [ ] 已有 headless 测试基础设施可覆盖 SubAgentGroup 渲染路径（注入 SubAgentStart/ToolCall/SubAgentEnd 事件序列）
