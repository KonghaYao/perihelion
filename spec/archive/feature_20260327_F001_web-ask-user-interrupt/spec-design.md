# Feature: 20260327_F001 - web-ask-user-interrupt

## 需求背景

Web 远程控制端目前存在两个缺陷：

1. **AskUser 弹窗功能残缺**：relay 协议中的 `AskUserQuestion` 结构体字段不完整，缺少 `multi_select`、`allow_custom_input`、`placeholder` 等核心字段，导致 Web 端弹窗无法正确渲染单选/多选、自由输入、占位提示等交互，体验显著落后于 TUI 端。

2. **Web 端无法中断 Agent**：`WebMessage` 没有中断指令类型，TUI 的 `App::interrupt()` 能力未通过 Relay 链路暴露给 Web 端，导致用户一旦在 Web 端发起任务便无法取消，只能等待超时或断线。

## 目标

- 将 relay 协议的 `AskUserQuestion` 字段与核心层 `AskUserQuestionData` 完全对齐
- Web 端 AskUser 弹窗支持完整的单选/多选/自由输入/副标题渲染
- Web 端 Agent 运行时显示"停止"按钮，点击后中断 Agent 并自动关闭待回答弹窗

## 方案设计

### 架构总览

本次改动跨越三层：协议层（`rust-relay-server`）、TUI 应用层（`rust-agent-tui`）、Web 前端（`web/js/`）。各层职责独立，互不侵入。

![完整数据流：AskUser 与 CancelAgent 流向](./images/01-flow.png)

### 一、协议层扩展（rust-relay-server/src/protocol.rs）

#### AskUserOption 结构体（新增）

用结构体替代原有的 `Vec<String>`，支持选项 label 和 description：

```rust
pub struct AskUserOption {
    pub label: String,
    pub description: Option<String>,  // 选项副标题，可选
}
```

#### AskUserQuestion 字段补全

对齐核心层 `AskUserQuestionData` 的所有字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `tool_call_id` | `String` | 对应工具调用 ID |
| `description` | `String` | 问题文本（原 `question` 重命名） |
| `multi_select` | `bool` | 是否多选 |
| `options` | `Vec<AskUserOption>` | 选项列表（结构体替代字符串） |
| `allow_custom_input` | `bool` | 是否允许自由文本输入 |
| `placeholder` | `Option<String>` | 自由输入框占位提示 |

> **向前兼容性**：`description`（原 `question`）字段重命名，需同步修改 relay_ops 中的映射代码和 Web dialog.js 中读取字段名。

#### WebMessage::CancelAgent（新增）

```rust
pub enum WebMessage {
    // ...现有变体...
    CancelAgent,  // Web → Agent：请求中断当前运行的 Agent
}
```

### 二、TUI 应用层改动（rust-agent-tui）

#### relay_ops.rs — CancelAgent 处理

在 `poll_relay` 的 match 分支中新增：

```rust
WebMessage::CancelAgent => {
    self.interrupt();            // 调用已有 App::interrupt()
    self.ask_user_prompt = None; // 清理 AskUser 弹窗状态
    self.hitl_prompt = None;     // 清理 HITL 弹窗状态（非 YOLO 模式）
}
```

#### agent_ops.rs — AskUserBatch 发送映射

修改将 `AskUserQuestionData` 转换为 `AskUserQuestion` 的代码：

```rust
AskUserQuestion {
    tool_call_id: q.tool_call_id.clone(),
    description:  q.description.clone(),
    multi_select: q.multi_select,
    options: q.options.iter().map(|o| AskUserOption {
        label: o.label.clone(),
        description: None,          // 核心层 AskUserOption 当前无 description
    }).collect(),
    allow_custom_input: q.allow_custom_input,
    placeholder: q.placeholder.clone(),
}
```

### 三、Web 前端改动（rust-relay-server/web/js/）

#### dialog.js — AskUser 弹窗增强

![AskUser 弹窗 UI（带选项描述、自由输入框、多选支持）](./images/02-wireframe.png)

每个问题渲染步骤：

1. `description` 作为问题标题（主标签文字）
2. 选项用 radio/checkbox 渲染（`multi_select` 决定类型）
3. 选项的 `description` 字段以灰色小字渲染在 label 下方（若有）
4. `allow_custom_input === true` 时，选项区域下方追加文本 input，`placeholder` 作为提示
5. 读取 `tool_call_id` 作为提交时的 key（替代原来的 `q.question`），确保后端匹配

提交逻辑调整：`answers` 对象改用 `tool_call_id` 为 key（同步修改 TUI relay_ops 中的 `AskUserResponse` 匹配逻辑）。

#### render.js — loading 气泡增加停止按钮

在 `renderMessages` 函数的 loading 态区块内追加：

```javascript
if (agent.isRunning) {
    const loadingEl = document.createElement('div');
    loadingEl.className = 'message msg-loading';
    loadingEl.innerHTML = `
      <div class="loading-dots"><span></span><span></span><span></span></div>
      <button class="stop-btn" data-pane="${paneId}">■ 停止</button>
    `;
    // 绑定停止事件
    loadingEl.querySelector('.stop-btn').addEventListener('click', () => {
        sendMessage(sessionId, { type: 'cancel_agent' });
        // 立即关闭待回答弹窗
        closeDialog('askuser');
        closeDialog('hitl');
    });
    container.appendChild(loadingEl);
}
```

停止按钮仅在 `agent.isRunning === true` 时渲染，无需额外状态管理。

#### 中断状态流程

![中断 Agent 状态机](./images/03-state.png)

中断后状态转换：
- `agent.isRunning = false`（待 `done`/`error` 事件确认）
- AskUser/HITL 弹窗立即关闭
- loading 气泡消失（下次 renderMessages 时不再追加）
- Agent 侧 `CancellationToken` 触发，ReAct 循环在下一轮检测中止

## 实现要点

- **向前兼容**：`AskUserQuestion.description` 是原 `question` 字段重命名，需同时修改 Web dialog.js 读取字段从 `q.question` 改为 `q.description`
- **relay_ops 匹配键同步**：AskUserResponse 当前用 `q.data.description == *q_text` 匹配，Web 端改用 `tool_call_id` 后，relay_ops 也需改为 `q.data.tool_call_id == *tool_call_id`
- **停止按钮幂等性**：多次点击停止应无副作用；`App::interrupt()` 对已取消的 token 调用是安全的
- **HITL 中断注意**：`hitl_prompt.take()` 后若不发送 decision，HITL oneshot channel 的 sender 被 drop，`before_tool` 中的 recv 会得到 `RecvError`，需确保 Agent 能优雅处理（现有 `reject` 或 cancel 语义）

## 约束一致性

- 仅修改 relay 协议 JSON 序列化字段，不新增 crate 依赖，符合现有技术栈约束
- 前端仍为纯 ES Modules，无构建工具，符合 `spec/global/constraints.md`
- `WebMessage::CancelAgent` 通过 TUI 已有 `App::interrupt()` 实现，不侵入核心 ReAct 执行器
- 中断后弹窗自动关闭由前端主动处理，不引入新的 `RelayMessage` 事件类型

## 验收标准

- [ ] `AskUserQuestion` 结构体包含 `tool_call_id`、`description`、`multi_select`、`allow_custom_input`、`placeholder`、`options: Vec<AskUserOption>` 字段
- [ ] Web 端 AskUser 弹窗：多选问题使用 checkbox，单选使用 radio，`allow_custom_input` 为 true 时显示文本框
- [ ] Web 端 AskUser 弹窗：选项 `description` 字段以灰色副标题显示
- [ ] Agent 运行中，Web 面板消息区末尾显示"■ 停止"按钮
- [ ] 点击停止后：AskUser/HITL 弹窗关闭，loading 气泡消失，Agent 执行中止
- [ ] 非运行状态下无停止按钮
- [ ] `WebMessage::CancelAgent` 能被 `relay_ops.rs` 正确处理并调用 `App::interrupt()`
