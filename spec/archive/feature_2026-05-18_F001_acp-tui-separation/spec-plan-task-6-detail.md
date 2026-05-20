# Task 6 细化执行计划 — TUI 接入 ACP 协议

**前置条件**: `peri-acp` crate 已就绪 (transport/session/agent/broker/event/langfuse/prompt/provider)。
`AcpTuiClient` 包装层已创建 (peri-tui/src/acp_client/)。`peri-tui/Cargo.toml` 已添加 `peri-acp` 依赖。

**目标**: TUI 不再直接调用 `peri-agent`/`peri-middlewares` 构建 Agent，改为通过 ACP 协议 (MpscTransport) 与 `peri-acp` 服务层通信。

---

## 改动总览

| 步骤 | 名称 | 概要 |
|------|------|------|
| 6-a | 创建 ACP Server 任务 | main.rs 中 spawn ACP server，创建 transport pair |
| 6-b | 删除已迁移模块 | 移除 acp/ langfuse/ prompt interaction_broker events::AgentEvent |
| 6-c | 重构 App 结构体 | App 增加 AcpTuiClient 字段，AgentComm.agent_rx 改为 acp_notification_rx |
| 6-d | 重构 agent_submit | 从 AgentRunConfig 构建改为 acp_client.prompt() |
| 6-e | 重构 agent_ops | handle_agent_event 改为 handle_acp_notification |
| 6-f | 重构 event 循环 | poll_agent() 改为消费 AcpNotification |
| 6-g | 重构 HITL/AskUser | 弹窗确认改为 acp_client.send_response() |
| 6-h | 重构命令系统 | /model /history /compact 改为 ACP RPC |
| 6-i | 清理 agent.rs | 删除已迁移函数，保留 compact_task (简化) |
| 6-j | 移除旧依赖 | Cargo.toml 删除 peri-agent peri-middlewares peri-lsp langfuse-client |
| 6-k | 编译验证 | cargo build --workspace |
| 6-l | 测试验证 | cargo test --workspace |

**顺序依赖**: 6-a → 6-b → 6-c → (6-d || 6-e || 6-f || 6-g || 6-h 可并行) → 6-i → 6-j → 6-k → 6-l

---

## Step 6-a: 创建 ACP Server 任务

### 背景
`main.rs` 当前通过 `App::new()` 直接初始化 TUI。需要改为：先创建 transport pair，spawn ACP server task 持有 server transport，再将 client transport 传给 App。

### 涉及文件
- `peri-tui/src/main.rs`
- 新建 `peri-tui/src/acp_server.rs` (ACP server spawning + prompt handler)

### 实现要点

**`peri-tui/src/acp_server.rs`** — ACP Server 启动函数：

```rust
use peri_acp::agent::builder::{AcpAgentConfig, AcpAgentOutput, build_agent};
use peri_acp::broker::AcpTransportBroker;
use peri_acp::transport::{AcpTransport, mpsc::MpscServerTransport};
use peri_acp::provider::{LlmProvider, config::PeriConfig};
use peri_acp::prompt::build_system_prompt;
use peri_acp::event::map_executor_to_updates;
use peri_acp::langfuse::LangfuseTracer;
use peri_agent::agent::react::AgentInput;
use peri_agent::agent::state::AgentState;
use peri_agent::agent::AgentCancellationToken;
use peri_agent::llm::BaseModelReactLLM;
use peri_agent::llm::RetryableLLM;
use peri_middlewares::prelude::*;

/// ACP Server 主循环：接收 prompt 请求 → 构建 Agent → 执行 → 推送 SessionNotification
pub async fn run_acp_server(
    transport: MpscServerTransport,
    provider: LlmProvider,
    peri_config: Arc<PeriConfig>,
    thread_store: Arc<dyn peri_agent::thread::ThreadStore>,
    permission_mode: Arc<SharedPermissionMode>,
    cron_scheduler: Option<Arc<parking_lot::Mutex<CronScheduler>>>,
    mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    plugin_skill_dirs: Vec<PathBuf>,
    plugin_agent_dirs: Vec<PathBuf>,
    plugin_hook_groups: Vec<Vec<RegisteredHook>>,
    plugin_lsp_servers: Vec<peri_lsp::config::LspServerConfig>,
    tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    shared_tools: Arc<RwLock<HashMap<String, Arc<dyn peri_agent::tools::BaseTool>>>>,
) {
    let transport = Arc::new(transport);
    let mut session_states: HashMap<String, SessionState> = HashMap::new();
    let mut pending_cancel: HashMap<String, AgentCancellationToken> = HashMap::new();

    while let Some(msg) = transport.recv().await {
        match msg {
            IncomingMessage::Request { id, method, params } => {
                match method.as_str() {
                    "session/new" => { /* 创建 session */ }
                    "session/prompt" => { /* 执行 Agent */ }
                    "session/set_model" => { /* 切换模型 */ }
                    "session/set_mode" => { /* 切换权限模式 */ }
                    "session/list" => { /* 列出 sessions */ }
                    "session/load" => { /* 加载历史 */ }
                    "$/cancel_request" => { /* 取消当前请求 */ }
                    "RequestPermission" => {
                        // 由 broker 内部处理，不应到达这里
                    }
                    _ => { /* 返回 method not found */ }
                }
            }
            IncomingMessage::Response { .. } => {}
            IncomingMessage::Notification { .. } => {}
        }
    }
}
```

**`peri-tui/src/main.rs`** 改动：

```rust
// 在 run_tui() 或 main() 中：
let (client_transport, server_transport) = peri_acp::transport::mpsc::mpsc_transport_pair();

// Spawn ACP server task
tokio::spawn(async move {
    run_acp_server(server_transport, provider, config, ...).await;
});

// 创建 AcpTuiClient 并传给 App
let acp_client = AcpTuiClient::new(client_transport);
let app = App::new(acp_client, config, ...);
```

**关键设计决策**:
- ACP server 任务由 `tokio::spawn` 管理，与 TUI 主循环并发运行
- `AcpTuiClient` 的 `pump_notifications()` 也需要 spawn 为独立任务
- `AgentCancellationToken` 从 Session 级别管理，用户 Ctrl+C 时通过 `acp_client.cancel()` 触发

---

## Step 6-b: 删除已迁移模块

### 背景
`peri-tui/src/acp/`、`peri-tui/src/langfuse/` 的代码已迁移到 `peri-acp`。`peri-tui/src/app/interaction_broker.rs` 由 `AcpTransportBroker` 替代。`AgentEvent` 枚举由 `AcpNotification` 替代。

### 操作清单

1. 删除 `peri-tui/src/acp/` 整个目录 (13 个文件)
2. 删除 `peri-tui/src/langfuse/` 整个目录 (4 个文件)
3. 删除 `peri-tui/src/app/interaction_broker.rs`
4. 删除 `peri-tui/src/prompt.rs` (system prompt 已迁移)
5. 删除 `peri-tui/src/app/events.rs` 中的 `AgentEvent` 枚举
6. 从 `peri-tui/src/lib.rs` 移除:
   - `pub mod acp`
   - `pub mod langfuse`
   - `pub mod prompt`
7. 从 `peri-tui/src/app/mod.rs` 移除:
   - `pub use events::AgentEvent`
   - `pub use interaction_broker::TuiInteractionBroker`
   - `use peri_agent::agent::AgentCancellationToken`

### 风险
这些删除会暂时让 peri-tui 编译失败（大量引用 AgentEvent/AgentCancellationToken 的代码还在）。编译在 Step 6-j 整体修复。

---

## Step 6-c: 重构 App 结构体

### 涉及文件
- `peri-tui/src/app/mod.rs`
- `peri-tui/src/app/agent_comm.rs`

### App 结构体改动

```rust
pub struct App {
    pub services: ServiceRegistry,       // 保留 (config, cron, provider, MCP)
    pub ui: GlobalUiState,              // 保留
    pub session_mgr: SessionManager,     // 保留 (panel 管理)
    pub view: RenderCache,              // 保留
    pub acp_client: AcpTuiClient,       // 新增 — 替代 agent_tx/agent_rx
    pub message_pipeline: MessagePipeline, // 保留
    // ...
}
```

**删除的字段**:
- `App::langfuse` (Option<Arc<Mutex<LangfuseTracer>>>) — 移至 peri-acp
- `App::agent_tx` / `App::agent_rx` — 由 `acp_client` 替代
- `App::compact_tasks` (JoinSet) — compact 由 ACP server 内部处理

**AgentComm 改动**:

```rust
pub struct AgentComm {
    pub agent_id: Option<String>,
    pub agent_turn: u32,
    pub acp_notification_rx: Option<mpsc::UnboundedReceiver<AcpNotification>>,  // 改名
    pub cancel_token: Option<AgentCancellationToken>,
    pub loading_text: Option<String>,
}
```

**App::new() 签名变更**:
```rust
pub fn new(
    acp_client: AcpTuiClient,    // 新增必需参数
    config: Arc<PeriConfig>,
    cron_scheduler: Option<Arc<Mutex<CronScheduler>>>,
    mcp_pool: Option<Arc<McpClientPool>>,
    // ... 其他参数
) -> Self
```

---

## Step 6-d: 重构 agent_submit

### 涉及文件
- `peri-tui/src/app/agent_submit.rs`

### 改动

**当前**:
```rust
pub fn submit_message(&mut self, input_text: String) {
    // 1. 收集上下文 (cwd, history, etc.)
    // 2. 构造 AgentRunConfig
    // 3. spawn run_universal_agent(cfg)
    // 4. 设置 loading 状态
}
```

**新版本**:
```rust
pub fn submit_message(&mut self, input_text: String) -> Result<(), String> {
    // 1. 如果无 active session，调用 acp_client.new_session(cwd, model)
    // 2. 调用 acp_client.prompt(&input_text)
    // 3. 设置 loading 状态 (等待 AcpNotification)
    // 4. 启动 notification pump (如果还没启动)
}
```

**删除的函数**:
- `build_run_config()` — 整个 AgentRunConfig 构造逻辑
- `spawn_agent_task()` — 由 ACP server 的 spawn_blocking 替代

---

## Step 6-e: 重构 agent_ops (事件处理)

### 涉及文件
- `peri-tui/src/app/agent_ops.rs`
- 新建 `peri-tui/src/app/acp_notif_handler.rs`

### 改动

**当前**: `handle_agent_event(&mut self, event: AgentEvent)` — ~20 变体 match
**新**: `handle_acp_notification(&mut self, notif: AcpNotification)` — 3 变体 match

```rust
pub fn handle_acp_notification(&mut self, notif: AcpNotification) {
    match notif {
        AcpNotification::SessionUpdate { session_id, params } => {
            // 解析 SessionUpdate 变体
            use agent_client_protocol::schema::SessionUpdate;
            // 从 params 反序列化 SessionUpdate
            // AgentMessageChunk → 追加文本到 StreamingText
            // ToolCall → 创建 ToolCallGroup VM
            // ToolCallUpdate → 更新已有 ToolCallGroup
            // UsageUpdate → 更新 token 计数
            // TodoList → 更新 Todo 面板
            // StateSync → request_rebuild
        }
        AcpNotification::RequestPermission { id, params } => {
            // 创建 HITL 弹窗
            self.ui.hitl_prompt = Some(HitlBatchPrompt::from_acp(id, params));
        }
        AcpNotification::Elicitation { id, params } => {
            // 创建 AskUser 弹窗
            self.ui.ask_user_prompt = Some(AskUserBatchPrompt::from_acp(id, params));
        }
        AcpNotification::Other { msg } => {
            tracing::warn!("未识别的 ACP 通知: {msg}");
        }
    }
}
```

**映射表** (AgentEvent → SessionUpdate):

| AgentEvent 变体 | SessionUpdate 变体 | 处理逻辑 |
|----------------|-------------------|---------|
| AiReasoning | AgentThoughtChunk | 追加到 reasoning_text |
| AssistantChunk | AgentMessageChunk | 追加到 streaming text |
| ToolStart | ToolCall | 创建 ToolCallGroup VM |
| ToolEnd (成功) | ToolCallUpdate(status=Completed) | 更新 ToolCallGroup |
| ToolEnd (失败) | ToolCallUpdate(status=Incomplete) | 更新 ToolCallGroup + 错误标记 |
| SubAgentStart | ToolCall(name="Agent") | 同上 ToolStart |
| SubAgentEnd | ToolCallUpdate(status=Completed) | 更新 ToolCallGroup |
| StateSnapshot | StateSync | request_rebuild() |
| TokenUsageUpdate | UsageUpdate | 更新 spinner token 计数 |
| ContextWarning | ContextUsage | 触发 auto-compact 检查 |
| LlmRetrying | Status(status=Retrying) | 显示重试提示 |
| Done | Status(status=Done) | 清除 loading, 结束处理 |
| TodoUpdate | TodoList | 更新 Todo 面板 |
| BackgroundTaskCompleted | ToolCallUpdate | 更新后台任务状态 |
| SubagentLifecycle | ToolCallUpdate | 更新子 agent spinner |
| LspDiagnostics | (无直接映射) | 通过 custom _meta 字段 |

---

## Step 6-f: 重构 event 循环

### 涉及文件
- `peri-tui/src/event.rs`

### poll_agent() 改动

**当前**:
```rust
async fn poll_agent(rx: &mut mpsc::Receiver<AgentEvent>) -> Option<AgentEvent> {
    rx.try_recv().ok()  // 非阻塞消费
}
```

**新**:
```rust
async fn poll_acp_notifications(rx: &mut mpsc::UnboundedReceiver<AcpNotification>) -> Option<AcpNotification> {
    rx.try_recv().ok()
}
```

---

## Step 6-g: 重构 HITL/AskUser 弹窗

### 涉及文件
- `peri-tui/src/app/hitl_ops.rs`
- `peri-tui/src/app/ask_user_ops.rs`

### HITL 改动

**当前**: `HitlBatchPrompt::confirm()` → 通过 `oneshot::Sender<InteractionResponse>` 直接返回
**新**: `HitlBatchPrompt::confirm()` → 调用 `acp_client.send_response(request_id, outcome)`

弹窗新增字段:
```rust
pub struct HitlBatchPrompt {
    pub request_id: RequestId,  // 新增 — 对应 ACP 请求 ID
    pub acp_client: Arc<Mutex<AcpTuiClient>>,  // 新增 — 用于 send_response
    // ... 原有字段
}
```

### AskUser 改动

**当前**: `AskUserBatchPrompt::confirm()` → 通过 `oneshot` channel 返回
**新**: `AskUserBatchPrompt::confirm()` → 调用 `acp_client.send_response(request_id, answers)`

---

## Step 6-h: 重构命令系统

### 涉及文件
- `peri-tui/src/command/mod.rs`
- `peri-tui/src/command/model.rs`
- `peri-tui/src/app/command_system.rs`

### 命令映射

| 命令 | 旧实现 | 新实现 |
|------|--------|--------|
| `/model <alias>` | 本地 `provider::switch_model()` | `acp_client.set_model(alias)` |
| Shift+Tab | `provider::cycle_permission_mode()` | `acp_client.set_mode(new_mode)` |
| `/history` | 本地 `SqliteThreadStore::list()` | `acp_client.list_sessions()` |
| `/history Enter` | `agent_ops::open_thread()` | `acp_client.load_session(id)` |
| Ctrl+C | `AgentCancellationToken::cancel()` | `acp_client.cancel()` |
| `/cost` | 本地 `TokenTracker` | `acp_client.send_request("session/usage", ...)` |
| `/compact` | 直接 `compact_task()` | 通过 ACP 协议触发 compact |
| `/config` | 本地直读 `settings.json` | **不变** (UI 配置，不走 ACP) |
| `/memory` | 编辑 CLAUDE.md | **不变** |
| `/mcp` | MCP 面板管理 | **不变** (面板在 TUI 层) |
| `/agents` | SubAgent 列表 | **不变** (UI 本地读取) |
| `/cron` | Cron 面板 | **不变** |

---

## Step 6-i: 清理 agent.rs

### 涉及文件
- `peri-tui/src/app/agent.rs`

### 删除内容

| 行范围 | 函数/类型 | 替代者 |
|--------|----------|--------|
| L1-58 | imports + `AgentRunConfig` | 删除 |
| L60-95 | `BareAgentConfig` + `BareAgentOutput` | peri-acp::agent::builder |
| L96-425 | `build_bare_agent()` | peri-acp::agent::builder::build_agent() |
| L427-656 | `run_universal_agent()` | ACP server prompt handler |
| L660-821 | `map_executor_event()` | peri-acp::event::mapper |
| L824-1005 | `compact_task()` | 简化或删除 |

### 保留内容

- `format_tool_args()` / `format_tool_name()` / `truncate()` — 这些是纯 UI 渲染工具，不依赖 peri-agent
- `default_requires_approval()` 函数 — UI 层仍需要审批判断

---

## Step 6-j: 移除旧依赖

### peri-tui/Cargo.toml

**移除**:
- `peri-agent = { path = "../peri-agent" }`
- `peri-middlewares = { path = "../peri-middlewares" }`
- `peri-lsp = { path = "../peri-lsp" }`
- `langfuse-client = { path = "../langfuse-client" }`

**保留**:
- `peri-acp = { path = "../peri-acp" }` — 通过它间接使用 peri-agent/peri-middlewares
- `agent-client-protocol = ...` — TUI 直接使用 SessionUpdate/SessionNotification 类型
- `agent-client-protocol-schema = ...` — TUI 使用 Elicitation 类型
- 所有 ratatui/widget/UI 依赖

---

## Step 6-k: 编译验证

- `cargo build --workspace`
- `cargo check --workspace`
- 确保 0 errors, 0 warnings (permissive clippy)

---

## Step 6-l: 测试验证

- `cargo test --workspace` — headless 测试应在
- 手动 E2E 验证:
  1. TUI 启动
  2. 输入消息 → Agent 回复
  3. Bash 工具触发 → HITL 弹窗 → Approve → 继续
  4. AskUser 工具触发 → 表单弹窗 → 填写 → Agent 收到答案
  5. Ctrl+C 中断
  6. /model 切换模型
  7. /history 浏览历史

---

## 风险与回退

| 风险 | 缓解 |
|------|------|
| 编译连锁失败 | Step 6-b~6-i 间逐步修复，每 sub-step 后 `cargo check` |
| Headless 测试依赖 AgentEvent | 测试文件同步更新 (app/agent_ops_test.rs 等) |
| MpscTransport 死锁 | 使用 unbounded channel，确认 pump 已 spawn |
| ACP 协议不完整 (session/load, session/resume 等) | 先实现核心路径 (prompt/set_model/cancel)，其他逐步补齐 |
| peri-tui 的 prompt/sections/ 目录删除后 ACP 找不到 | 已用 `concat!(env!("CARGO_MANIFEST_DIR"), "/../peri-tui/prompts/sections/")` 交叉引用 |
