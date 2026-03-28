# Plan H3：HITL + AskUser 统一为 UserInteractionBroker

> 优先级：大工作量，建议最后处理
> 涉及 crate：rust-create-agent / rust-agent-middlewares / rust-agent-tui / rust-relay-server

---

## 问题描述

HITL 和 AskUser 都是"暂停执行 → 等人工响应"的交互模式，但实现路径完全独立：

| | HITL | AskUser |
|--|------|---------|
| 触发方式 | `Middleware::before_tool` 拦截 | LLM 调用普通工具 `ask_user` |
| 等待机制 | oneshot channel in `HitlHandler` | oneshot channel in `AskUserTool` |
| TUI 弹窗 | `hitl_prompt: Option<BatchApprovalRequest>` | `ask_user_prompt: Option<AskUserBatchRequest>` |
| relay 消息 | `ApprovalNeeded` + `ApprovalResolved` | `AskUserBatch` + `AskUserResolved` |

结果：TUI 维护两套弹窗、两套 channel 转发逻辑，relay 有 4 条专用消息。

---

## 方案

### 核心：提取 `UserInteractionBroker` trait

**Step 1：新建 `rust-create-agent/src/interaction/mod.rs`**

```rust
// rust-create-agent/src/interaction/mod.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 人机交互上下文（描述需要用户响应的场景）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum InteractionContext {
    /// 工具调用前审批（原 HITL BatchApprovalRequest）
    Approval {
        items: Vec<ApprovalItem>,
    },
    /// 向用户提问（原 AskUserBatchRequest）
    Questions {
        requests: Vec<QuestionItem>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalItem {
    pub tool_call_id: String,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionItem {
    pub id: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    pub multi_select: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

/// 用户响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum InteractionResponse {
    Decisions(Vec<ApprovalDecision>),
    Answers(Vec<QuestionAnswer>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApprovalDecision {
    Approve,
    Reject { reason: String },
    Edit { new_input: serde_json::Value },
    Respond { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionAnswer {
    pub id: String,
    pub selected: Vec<String>,
    pub text: Option<String>,
}

/// 统一的人机交互 broker trait
#[async_trait]
pub trait UserInteractionBroker: Send + Sync {
    async fn request(&self, ctx: InteractionContext) -> InteractionResponse;
}
```

**Step 2：更新 `rust-agent-middlewares/src/hitl/mod.rs`**

`HitlHandler` trait 改为调用 broker：

```rust
// HumanInTheLoopMiddleware 持有 broker，而非独立的 HitlHandler
pub struct HumanInTheLoopMiddleware {
    broker: Arc<dyn UserInteractionBroker>,
    requires_approval: fn(&str) -> bool,
}

impl HumanInTheLoopMiddleware {
    pub fn new(
        broker: Arc<dyn UserInteractionBroker>,
        requires_approval: fn(&str) -> bool,
    ) -> Self {
        Self { broker, requires_approval }
    }
}

// before_tool 实现
async fn before_tool(&self, _state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
    if !(self.requires_approval)(tool_call.name.as_str()) {
        return Ok(tool_call.clone());
    }
    let ctx = InteractionContext::Approval {
        items: vec![ApprovalItem {
            tool_call_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            tool_input: tool_call.input.clone(),
        }],
    };
    let response = self.broker.request(ctx).await;
    match response {
        InteractionResponse::Decisions(decisions) => {
            // 处理 Approve / Reject / Edit / Respond 逻辑（与现在一致）
            ...
        }
        _ => Ok(tool_call.clone()),  // 不应发生
    }
}
```

**Step 3：更新 `rust-agent-middlewares/src/tools/ask_user_tool.rs`**

`AskUserTool` 也改为持有 broker：

```rust
pub struct AskUserTool {
    broker: Arc<dyn UserInteractionBroker>,
}

async fn invoke(&self, input: Value) -> Result<String, ...> {
    let questions = parse_ask_user_input(&input)?;
    let ctx = InteractionContext::Questions { requests: questions };
    let response = self.broker.request(ctx).await;
    match response {
        InteractionResponse::Answers(answers) => format_answers(answers),
        _ => Err("unexpected response type".into()),
    }
}
```

**Step 4：TUI 实现统一 broker `TuiInteractionBroker`**

```rust
// rust-agent-tui/src/app/interaction_broker.rs（新建）
pub struct TuiInteractionBroker {
    tx: mpsc::Sender<AgentEvent>,  // 发给 TUI 主循环
}

#[async_trait]
impl UserInteractionBroker for TuiInteractionBroker {
    async fn request(&self, ctx: InteractionContext) -> InteractionResponse {
        let (response_tx, response_rx) = oneshot::channel();
        // 统一发送 InteractionRequest 事件到 TUI
        self.tx.send(AgentEvent::InteractionRequest { ctx, response_tx }).await.ok();
        response_rx.await.unwrap_or_else(|_| /* 默认拒绝 */ ...)
    }
}
```

TUI 主循环只需处理一种 `AgentEvent::InteractionRequest`，根据 `ctx` 类型决定展示审批弹窗还是问答弹窗。

**Step 5：relay-server 协议简化**

将 4 条消息合并为 2 条：

```
之前：ApprovalNeeded / ApprovalResolved / AskUserBatch / AskUserResolved
之后：InteractionRequest / InteractionResponse
```

---

## 变更文件清单

| 文件 | 操作 | 内容 |
|------|------|------|
| `rust-create-agent/src/interaction/mod.rs` | 新建 | `UserInteractionBroker` trait + 上下文/响应类型 |
| `rust-create-agent/src/lib.rs` | 修改 | 导出 `interaction` 模块 |
| `rust-agent-middlewares/src/hitl/mod.rs` | 重构 | 使用 `UserInteractionBroker` 替换 `HitlHandler` |
| `rust-agent-middlewares/src/tools/ask_user_tool.rs` | 重构 | 使用 `UserInteractionBroker` 替换 `AskUserInvoker` |
| `rust-agent-tui/src/app/interaction_broker.rs` | 新建 | `TuiInteractionBroker` 实现 |
| `rust-agent-tui/src/app/events.rs` | 修改 | 新增 `AgentEvent::InteractionRequest`，删除 `ApprovalNeeded`/`AskUserBatch` |
| `rust-agent-tui/src/app/mod.rs` | 修改 | 合并两套弹窗逻辑为一套 |
| `rust-agent-tui/src/app/agent.rs` | 修改 | 改用 `TuiInteractionBroker` |
| `rust-relay-server/src/protocol.rs` | 修改 | 4 条消息合并为 2 条 |
| `rust-relay-server/web/events.js` | 修改 | 前端适配新协议消息 |

---

## 迁移策略（降低风险）

建议分两阶段：

**阶段一（无破坏性变更）**：
1. 新建 `UserInteractionBroker` trait 和类型
2. `HumanInTheLoopMiddleware` 新增接受 broker 的构造函数（旧接口保留）
3. `AskUserTool` 同上
4. TUI 新建 `TuiInteractionBroker`，并行运行两套逻辑，验证一致性

**阶段二（删除旧实现）**：
1. 删除旧的 `HitlHandler` trait 和 `AskUserInvoker` trait
2. 删除 TUI 中两套独立的 channel 和弹窗
3. relay 协议升级，前端适配

---

## 注意事项

1. **HITL 的 `Edit` 和 `Respond` 语义**：AskUser 没有这两种响应，需确保 `InteractionResponse` 对两者都兼容，
   且 HITL 的 `Middleware` 侧能正确解析。
2. **relay 前端向后兼容**：协议消息改变后，旧版前端无法正确解析。若有多端部署，需协调升级。
3. **AskUserInvoker 对外 API**：外部若有代码依赖 `AskUserInvoker` trait，需同步通知。

---

## 工作量估计

- 新建 interaction/mod.rs：约 80 行
- hitl/mod.rs 重构：约 40 行改动
- ask_user_tool.rs 重构：约 30 行改动
- TUI 合并弹窗逻辑：约 60 行改动
- relay 协议修改：约 20 行
- 前端 events.js：约 20 行
- 合计：**大（1-2 天）**
