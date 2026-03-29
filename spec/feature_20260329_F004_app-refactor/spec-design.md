# Feature: 20260329_F004 - app-refactor

## 需求背景

架构审查发现 `rust-agent-tui` 的 `App` 结构体包含 40+ 字段，混合了 UI 状态、Agent 通信、Relay 连接、Langfuse 可观测性、Thread 持久化等 5 种职责。每次新增弹窗或面板都需修改此结构体，导致：

1. **编译时间**：修改任何子职责字段都触发 App 全量重编译
2. **认知负担**：开发者需理解 40+ 字段之间的关联才能安全修改
3. **测试困难**：Headless 测试需构造完整 App，无法只测试某一子领域

## 目标

- 将 App 拆分为 4 个子结构体，保持对外 API 不变
- 每个子结构体职责单一，字段数量控制在 15 以内
- 渐进式拆分：先拆字段最多的两个（AgentComm、RelayState），再拆 LangfuseState

## 方案设计

### 拆分后的结构

```
App (公共外壳，保持原有方法签名)
  ├── AppCore          — UI 状态、面板、基础设施、渲染
  ├── AgentComm        — Agent 通信、交互弹窗、取消/计时
  ├── RelayState       — Relay 连接、重连、事件接收
  └── LangfuseState    — Langfuse Session/Tracer/Flush
```

### 各子结构体字段归属

#### AppCore（UI + 基础设施，~20 字段）

| 字段 | 原类型 | 说明 |
|------|--------|------|
| `view_messages` | `Vec<MessageViewModel>` | 消息列表 |
| `textarea` | `TextArea<'static>` | 输入框 |
| `loading` | `bool` | 加载状态 |
| `scroll_offset` | `u16` | 滚动偏移 |
| `scroll_follow` | `bool` | 自动跟随 |
| `show_tool_messages` | `bool` | 工具消息显示开关 |
| `pending_messages` | `Vec<String>` | 缓冲消息 |
| `subagent_group_idx` | `Option<usize>` | SubAgent 分组下标 |
| `render_tx` | `UnboundedSender<RenderEvent>` | 渲染事件 |
| `render_cache` | `Arc<RwLock<RenderCache>>` | 渲染缓存 |
| `render_notify` | `Arc<Notify>` | 渲染通知 |
| `last_render_version` | `u64` | 渲染版本 |
| `command_registry` | `CommandRegistry` | 命令注册表 |
| `command_help_list` | `Vec<(String, String)>` | 帮助文本 |
| `skills` | `Vec<SkillMetadata>` | Skills 列表 |
| `hint_cursor` | `Option<usize>` | 提示浮层光标 |
| `pending_attachments` | `Vec<PendingAttachment>` | 图片附件 |
| `model_panel` | `Option<ModelPanel>` | 模型面板 |
| `agent_panel` | `Option<AgentPanel>` | Agent 面板 |
| `thread_browser` | `Option<ThreadBrowser>` | 历史浏览 |

#### AgentComm（Agent 通信，~10 字段）

| 字段 | 原类型 | 说明 |
|------|--------|------|
| `agent_rx` | `Option<mpsc::Receiver<AgentEvent>>` | Agent 事件接收端 |
| `interaction_prompt` | `Option<InteractionPrompt>` | 当前弹窗 |
| `pending_hitl_items` | `Option<Vec<String>>` | 待解决 HITL 工具名 |
| `pending_ask_user` | `Option<bool>` | AskUser 已提交标记 |
| `agent_state_messages` | `Vec<BaseMessage>` | Agent 消息历史 |
| `agent_id` | `Option<String>` | 当前 Agent ID |
| `cancel_token` | `Option<CancellationToken>` | 取消令牌 |
| `task_start_time` | `Option<Instant>` | 任务开始时间 |
| `last_task_duration` | `Option<Duration>` | 上次任务时长 |
| `agent_event_queue` | `Vec<AgentEvent>` | 测试事件队列 |

#### RelayState（Relay 连接，~4 字段）

| 字段 | 原类型 | 说明 |
|------|--------|------|
| `relay_client` | `Option<Arc<RelayClient>>` | Relay 客户端 |
| `relay_event_rx` | `Option<RelayEventRx>` | 事件接收端 |
| `relay_params` | `Option<(String, String, Option<String>, String)>` | 连接参数缓存 |
| `relay_reconnect_at` | `Option<Instant>` | 重连计划时间 |

> `relay_panel` 留在 AppCore（它是 UI 面板状态，不是连接状态）

#### LangfuseState（可观测性，~3 字段）

| 字段 | 原类型 | 说明 |
|------|--------|------|
| `langfuse_session` | `Option<Arc<LangfuseSession>>` | Thread 级 Session |
| `langfuse_tracer` | `Option<Arc<Mutex<LangfuseTracer>>>` | Turn 级 Tracer |
| `langfuse_flush_handle` | `Option<JoinHandle<()>>` | Flush 句柄 |

### 不变字段（留在 App 顶层）

以下字段是跨子结构体的"胶水"字段，留在 App 顶层：

| 字段 | 说明 |
|------|------|
| `cwd` | 被多处使用 |
| `provider_name` / `model_name` | 状态栏 + Agent 组装 |
| `zen_config` | 配置被多处引用 |
| `thread_store` | Thread 操作 + Agent 组装 |
| `current_thread_id` | Thread 操作 + Agent 组装 |
| `todo_items` | UI 渲染 + Relay 转发 |

### 迁移策略

```rust
// App 内部持有子结构体
pub struct App {
    pub core: AppCore,
    pub agent: AgentComm,
    pub relay: RelayState,
    pub langfuse: LangfuseState,
    // 不变字段
    pub cwd: String,
    pub provider_name: String,
    pub model_name: String,
    pub zen_config: Option<ZenConfig>,
    pub thread_store: Arc<dyn ThreadStore>,
    pub current_thread_id: Option<ThreadId>,
    pub todo_items: Vec<TodoItem>,
    pub relay_panel: Option<RelayPanel>,  // UI 面板，非连接状态
}
```

**对外 API 兼容**：通过 `Deref` 或转发方法保持 `app.loading`、`app.view_messages` 等现有调用方式可用。

方案：不使用 Deref（不习惯），改为**逐字段转发方法**。由于 App 方法本来就以 `app.xxx` 访问，直接在 App 上保留 `loading()` / `set_loading()` / `view_messages` 等常用访问器，内部委托给 `self.core` / `self.agent` 等。较少使用的字段（如 `relay_client`）改为 `app.relay.client`。

### 实现步骤

1. 创建 `app/core.rs` — 定义 `AppCore`，迁移 ~20 个字段
2. 创建 `app/agent_comm.rs` — 定义 `AgentComm`，迁移 ~10 个字段
3. 创建 `app/relay_state.rs` — 定义 `RelayState`，迁移 ~4 个字段
4. 创建 `app/langfuse_state.rs` — 定义 `LangfuseState`，迁移 ~3 个字段
5. 重构 `app/mod.rs` — App 持有 4 个子结构体，保留高频访问器的转发方法
6. 逐步迁移 `app/agent_ops.rs`、`app/hitl_ops.rs`、`app/relay_ops.rs` 等 ops 文件，将 `&mut self` 改为 `&mut self.agent` / `&mut self.relay` 等
7. 运行全量测试 + headless 测试验证无回归

## 实现要点

- **渐进式**：可分两批完成。第一批先拆 `AgentComm` + `RelayState`（字段最多的两个），验证通过后再拆 `LangfuseState` + `AppCore`
- **方法签名不变**：App 上的公共方法保持签名不变，内部委托
- **ops 文件模式**：现有 `agent_ops.rs` 等文件的函数签名 `fn xxx(app: &mut App)` 保持不变，函数内部改为 `app.agent.xxx`
- **不引入 newtype**：子结构体用普通 struct 而非 newtype wrapper

## 约束一致性

- 与 `constraints.md` 的"事件驱动 TUI 通信"约束一致：拆分不改变 mpsc channel 的使用方式
- 与 `architecture.md` 的模块划分一致：`app/` 目录内子模块拆分

## 验收标准

- [ ] App 结构体字段数从 40+ 降低到 ~12（顶层） + 4 个子结构体
- [ ] 每个子结构体字段数 ≤ 20
- [ ] `cargo test` 全量通过（含 headless 测试）
- [ ] `cargo build -p rust-agent-tui` 无新 warning
- [ ] 现有 `app/xxx_ops.rs` 函数签名不变（内部重构）
- [ ] `run_universal_agent()` 调用方式不变
