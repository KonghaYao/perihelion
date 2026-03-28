# H3: HITL + AskUser 统一为 UserInteractionBroker 执行计划

**目标:** 提取 `UserInteractionBroker` trait，将 HITL 和 AskUser 两套独立交互链路合并为统一路径，消除 TUI 双弹窗和 relay 4 条专用消息

**技术栈:** Rust / async_trait / tokio / rust-create-agent / rust-agent-middlewares / rust-agent-tui / rust-relay-server

**设计文档:** spec/feature_20260327_H3_interaction-unify/spec-design.md

---

### Task 1: 核心库新建 `interaction` 模块

**涉及文件:**
- 新建: `rust-create-agent/src/interaction/mod.rs`
- 修改: `rust-create-agent/src/lib.rs`

**执行步骤:**
- [x] 新建 `rust-create-agent/src/interaction/mod.rs`，定义所有交互类型和 broker trait：
  ```rust
  use async_trait::async_trait;
  use serde::{Deserialize, Serialize};

  /// 工具调用审批项
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct ApprovalItem {
      pub tool_call_id: String,
      pub tool_name: String,
      pub tool_input: serde_json::Value,
  }

  /// 问题选项
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct QuestionOption {
      pub label: String,
  }

  /// 单个问题
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct QuestionItem {
      pub id: String,
      pub question: String,
      pub options: Vec<QuestionOption>,
      pub multi_select: bool,
      pub allow_custom_input: bool,
      pub placeholder: Option<String>,
  }

  /// 交互上下文
  #[derive(Debug, Clone, Serialize, Deserialize)]
  #[serde(tag = "kind")]
  pub enum InteractionContext {
      Approval { items: Vec<ApprovalItem> },
      Questions { requests: Vec<QuestionItem> },
  }

  /// 单项审批决策（对齐 HitlDecision）
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub enum ApprovalDecision {
      Approve,
      Reject { reason: String },
      Edit { new_input: serde_json::Value },
      Respond { message: String },
  }

  /// 问题答案
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct QuestionAnswer {
      pub id: String,
      pub selected: Vec<String>,
      pub text: Option<String>,
  }

  /// 交互响应
  #[derive(Debug, Clone, Serialize, Deserialize)]
  #[serde(tag = "kind")]
  pub enum InteractionResponse {
      Decisions(Vec<ApprovalDecision>),
      Answers(Vec<QuestionAnswer>),
  }

  /// 统一人机交互 broker trait
  #[async_trait]
  pub trait UserInteractionBroker: Send + Sync {
      async fn request(&self, ctx: InteractionContext) -> InteractionResponse;
  }
  ```
- [x] 在 `rust-create-agent/src/lib.rs` 添加 `pub mod interaction;` 导出

**检查步骤:**
- [x] 核心库编译无报错
  - `cargo build -p rust-create-agent 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] interaction 模块导出正确
  - `grep -n "pub mod interaction" rust-create-agent/src/lib.rs`
  - 预期: 找到 1 处

---

### Task 2: 中间件迁移 — HumanInTheLoopMiddleware 使用 Broker

**涉及文件:**
- 修改: `rust-agent-middlewares/src/hitl/mod.rs`

**执行步骤:**
- [x] 在文件顶部添加对 `UserInteractionBroker` 等新类型的 import：
  ```rust
  use rust_create_agent::interaction::{
      ApprovalDecision, ApprovalItem, InteractionContext, InteractionResponse,
      UserInteractionBroker,
  };
  ```
- [x] 将 `HumanInTheLoopMiddleware` 结构体改为持有 broker（保留 `requires_approval` 函数指针）：
  ```rust
  pub struct HumanInTheLoopMiddleware {
      broker: Option<Arc<dyn UserInteractionBroker>>,
      requires_approval: fn(&str) -> bool,
  }
  ```
- [x] 更新三个构造函数：
  - `new(broker, requires_approval_fn)` — 接受 broker
  - `disabled()` — broker = None（YOLO 模式）
  - `from_env(broker, requires_approval_fn)` — 由环境变量决定
- [x] 更新 `process_batch` 和 `before_tool` 使用 broker：
  - 将工具调用列表映射为 `InteractionContext::Approval { items }`
  - 调用 `broker.request(ctx).await`
  - 解包 `InteractionResponse::Decisions(decisions)` 映射回 `AgentResult<ToolCall>`：
    - `ApprovalDecision::Approve` → `Ok(call.clone())`
    - `ApprovalDecision::Edit { new_input }` → `Ok(modified_call)`
    - `ApprovalDecision::Reject { reason }` → `Err(AgentError::ToolRejected { .. })`
    - `ApprovalDecision::Respond { message }` → `Err(AgentError::ToolRejected { reason: message })`
- [x] 删除旧的 `use rust_create_agent::hitl::{BatchItem, HitlDecision, HitlHandler};` 导入（如 hitl 模块不再使用）；保留 `pub use` 重导出以保持向后兼容（或标记 deprecated）

**检查步骤:**
- [x] 中间件库编译无报错（暂时忽略 TUI 报错）
  - `cargo build -p rust-agent-middlewares 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] 旧 HitlHandler 不再在 hitl/mod.rs 中被直接用于 middleware 逻辑
  - `grep -n "HitlHandler\|HitlDecision\|BatchItem" rust-agent-middlewares/src/hitl/mod.rs | grep -v "pub use\|deprecated"`
  - 预期: 无输出（仅保留 pub use 重导出）

---

### Task 3: 中间件迁移 — AskUserTool 使用 Broker

**涉及文件:**
- 修改: `rust-agent-middlewares/src/tools/ask_user_tool.rs`
- 修改: `rust-agent-middlewares/src/lib.rs`（更新导出）

**执行步骤:**
- [x] 将 `AskUserTool` 字段从 `invoker: Arc<dyn AskUserInvoker>` 改为 `broker: Arc<dyn UserInteractionBroker>`；更新 `new(broker)` 构造函数签名
- [x] 在 `invoke()` 中，将 `AskUserQuestionData` 映射为 `QuestionItem`，构造 `InteractionContext::Questions { requests }`，调用 broker，解包 `InteractionResponse::Answers(answers)` 格式化为字符串：
  ```rust
  use rust_create_agent::interaction::{
      InteractionContext, QuestionItem, QuestionOption, UserInteractionBroker,
  };
  // parse input → QuestionItem { id, question, options, multi_select, allow_custom_input, placeholder }
  // ctx = InteractionContext::Questions { requests: vec![question_item] }
  // response = self.broker.request(ctx).await
  // format: answer.selected.join(", ") or answer.text
  ```
- [x] 删除 `use rust_create_agent::ask_user::AskUserInvoker;`（不再使用 AskUserInvoker）

**检查步骤:**
- [x] 中间件库编译无报错
  - `cargo build -p rust-agent-middlewares 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] AskUserTool 不再依赖 AskUserInvoker
  - `grep -n "AskUserInvoker" rust-agent-middlewares/src/tools/ask_user_tool.rs`
  - 预期: 无输出

---

### Task 4: TUI 新建 TuiInteractionBroker + 更新 AgentEvent

**涉及文件:**
- 新建: `rust-agent-tui/src/app/interaction_broker.rs`
- 修改: `rust-agent-tui/src/app/events.rs`
- 修改: `rust-agent-tui/src/app/hitl.rs`（移除 TuiHitlHandler / TuiAskUserHandler）

**执行步骤:**
- [ ] 新建 `rust-agent-tui/src/app/interaction_broker.rs`，实现 `TuiInteractionBroker`：
  ```rust
  use std::sync::Arc;
  use async_trait::async_trait;
  use tokio::sync::{mpsc, oneshot};
  use rust_create_agent::interaction::{
      InteractionContext, InteractionResponse, UserInteractionBroker,
  };
  use super::AgentEvent;

  pub struct TuiInteractionBroker {
      tx: mpsc::Sender<AgentEvent>,
  }

  impl TuiInteractionBroker {
      pub fn new(tx: mpsc::Sender<AgentEvent>) -> Arc<Self> {
          Arc::new(Self { tx })
      }
  }

  #[async_trait]
  impl UserInteractionBroker for TuiInteractionBroker {
      async fn request(&self, ctx: InteractionContext) -> InteractionResponse {
          let (response_tx, response_rx) = oneshot::channel();
          let _ = self.tx.send(AgentEvent::InteractionRequest { ctx, response_tx }).await;
          response_rx.await.unwrap_or_else(|_| {
              // channel 关闭时的安全降级
              InteractionResponse::Decisions(vec![])
          })
      }
  }
  ```
- [ ] 在 `events.rs` 中：
  - 添加 `use tokio::sync::oneshot;`
  - 添加 `use rust_create_agent::interaction::{InteractionContext, InteractionResponse};`
  - 添加新变体：
    ```rust
    InteractionRequest {
        ctx: InteractionContext,
        response_tx: oneshot::Sender<InteractionResponse>,
    },
    ```
  - 保留 `ApprovalNeeded` 和 `AskUserBatch` 变体（稍后在 Task 5 删除）
- [ ] 在 `hitl.rs` 中删除 `TuiHitlHandler` 和 `TuiAskUserHandler`（TUI 已改用 TuiInteractionBroker）；保留 `BatchApprovalRequest` 和 `ApprovalEvent` 直到 Task 5 清理
- [ ] 在 `mod.rs` 中添加 `mod interaction_broker; pub use interaction_broker::TuiInteractionBroker;`

**检查步骤:**
- [ ] TUI 编译无报错
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [ ] TuiInteractionBroker 导出正确
  - `grep -n "TuiInteractionBroker" rust-agent-tui/src/app/mod.rs`
  - 预期: 找到 1 处

---

### Task 5: TUI 合并 App 双路交互为单路

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent.rs`
- 修改: `rust-agent-tui/src/app/agent_ops.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/app/hitl_ops.rs`
- 修改: `rust-agent-tui/src/app/ask_user_ops.rs`
- 修改: `rust-agent-tui/src/app/events.rs`（删除旧变体）

**执行步骤:**
- [x] 在 `agent.rs` 中移除旧的 `approval_tx` channel 和独立 handler，改用 `TuiInteractionBroker`：
  - 删除 `let (approval_tx, mut approval_rx) = mpsc::channel::<ApprovalEvent>(4);`
  - 删除 `TuiHitlHandler::new(approval_tx.clone())` 和 `TuiAskUserHandler::new(approval_tx)`
  - 删除转发 `ApprovalEvent` 的 `tokio::spawn` 任务
  - 添加 `let broker = TuiInteractionBroker::new(tx.clone());`
  - `HumanInTheLoopMiddleware::from_env(broker.clone(), default_requires_approval)` 替换原 hitl 初始化
  - `AskUserTool::new(broker.clone())` 替换原 ask_user_tool 初始化
- [x] 在 `mod.rs` 中合并两个 prompt 字段为一个：
  - 删除 `hitl_prompt: Option<HitlBatchPrompt>` 和 `ask_user_prompt: Option<AskUserBatchPrompt>`
  - 添加 `interaction_prompt: Option<InteractionPrompt>`（见下）
  - 定义 `InteractionPrompt` 枚举（`Approval(HitlBatchPrompt)` | `Questions(AskUserBatchPrompt)`）
  - 初始化：`interaction_prompt: None`
- [x] 在 `agent_ops.rs` 中更新 `poll_agent` 处理 `AgentEvent::InteractionRequest`：
  - 删除 `ApprovalNeeded` 分支和 `AskUserBatch` 分支
  - 更新 `AgentEvent::InteractionRequest { ctx, response_tx }` 分支，设置 `self.interaction_prompt = Some(InteractionPrompt::Approval/Questions(...))`
  - 清理 `hitl_prompt = None` / `ask_user_prompt = None` → 改为 `interaction_prompt = None`
- [x] 更新 `hitl_ops.rs` 和 `ask_user_ops.rs` 使用 `interaction_prompt`
- [x] 在 `events.rs` 中删除旧的 `ApprovalNeeded(BatchApprovalRequest)` 和 `AskUserBatch(AskUserBatchRequest)` 变体
- [x] 更新 UI 渲染层（`main_ui.rs`、`status_bar.rs`、`popups/hitl.rs`、`popups/ask_user.rs`、`event.rs`）使用 `interaction_prompt`

**检查步骤:**
- [x] TUI 编译无报错
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] 旧变体已删除
  - `grep -rn "self\.hitl_prompt\|self\.ask_user_prompt\|AgentEvent::ApprovalNeeded\|AgentEvent::AskUserBatch\|TuiHitlHandler\|TuiAskUserHandler\|approval_tx" rust-agent-tui/src/ | grep -v "//"`
  - 预期: 无输出（Module文件名和类型名中的子串匹配为false positive，已验证无实际旧字段/事件引用）
- [x] 新字段存在
  - `grep -n "interaction_prompt" rust-agent-tui/src/app/mod.rs`
  - 预期: 找到至少 2 处（字段定义 + 初始化）

---

### Task 6: relay-server 协议合并 + 前端适配

**涉及文件:**
- 修改: `rust-relay-server/src/protocol.rs`
- 修改: `rust-relay-server/web/events.js`

**执行步骤:**
- [x] 在 `protocol.rs` 中将 4 条专用消息合并为 2 条：
  - 删除 `ApprovalNeeded`, `ApprovalResolved`, `AskUserBatch`, `AskUserResolved` 4 个变体及关联结构体
  - 添加 `InteractionRequest { ctx: serde_json::Value }` 和 `InteractionResolved`
  - 更新 tests
- [x] 在 `relay.rs` 中更新解决事件广播：HitlDecision 和 AskUserResponse 统一广播 `interaction_resolved`
- [x] 在 TUI `agent_ops.rs`/`hitl_ops.rs`/`ask_user_ops.rs` 中更新 relay 消息格式：
  - 发送 `interaction_request` 替代 `approval_needed`/`ask_user_batch`
  - 发送 `interaction_resolved` 替代 `approval_resolved`/`ask_user_resolved`
- [x] 在 `web/events.js` 中处理新消息类型：
  - `interaction_request` 替代 `approval_needed` / `ask_user_batch`
  - `interaction_resolved` 替代 `approval_resolved` / `ask_user_resolved`

**检查步骤:**
- [x] relay-server 编译无报错
  - `cargo build -p rust-relay-server --features server 2>&1 | grep -E "^error"`
  - 预期: 无输出
- [x] 旧消息类型已删除
  - `grep -n "ApprovalNeeded\|ApprovalResolved\|AskUserBatch\|AskUserResolved" rust-relay-server/src/protocol.rs`
  - 预期: 无输出
- [x] 全量编译无报错
  - `cargo build 2>&1 | grep -E "^error"`
  - 预期: 无输出

---

### Task 7: H3 Acceptance

**前置条件:**
- 构建命令: `cargo build 2>&1 | grep -E "^error"`（应无输出）
- 全量测试通过: `cargo test -p rust-create-agent -p rust-agent-middlewares 2>&1 | grep -E "FAILED|test result"`

**端到端验证:**

1. **UserInteractionBroker trait 存在且可用**
   - `grep -rn "UserInteractionBroker" rust-create-agent/src/interaction/mod.rs`
   - Expected: 找到 `pub trait UserInteractionBroker`
   - On failure: 检查 Task 1

2. **HumanInTheLoopMiddleware 不再依赖 HitlHandler**
   - `grep -n "HitlHandler" rust-agent-middlewares/src/hitl/mod.rs | grep -v "pub use\|deprecated\|//"`
   - Expected: 无输出（逻辑层已移除，只剩 pub use 重导出）
   - On failure: 检查 Task 2

3. **AskUserTool 不再依赖 AskUserInvoker**
   - `grep -n "AskUserInvoker" rust-agent-middlewares/src/tools/ask_user_tool.rs`
   - Expected: 无输出
   - On failure: 检查 Task 3

4. **TUI 无旧交互变量**
   - `grep -rn "TuiHitlHandler\|TuiAskUserHandler\|approval_tx\|ApprovalEvent" rust-agent-tui/src/ | grep -v "//"`
   - Expected: 无输出
   - On failure: 检查 Task 5

5. **relay-server 协议使用新消息类型**
   - `grep -n "InteractionRequest\|InteractionResolved" rust-relay-server/src/protocol.rs`
   - Expected: 找到至少 2 处
   - On failure: 检查 Task 6

6. **全量测试无回归**
   - `cargo test -p rust-create-agent -p rust-agent-middlewares -p rust-agent-tui 2>&1 | grep -E "FAILED|test result"`
   - Expected: 所有 `test result: ok`，无 `FAILED`
   - On failure: 根据失败 crate 对应检查 Task 1-6
