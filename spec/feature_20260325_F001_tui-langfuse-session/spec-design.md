# Feature: 20260325_F001 - tui-langfuse-session

## 需求背景

TUI 中每次调用 `submit_message()` 都会创建一个全新的 `LangfuseTracer`，而 `LangfuseTracer::new()` 内部会重新构造 `LangfuseClient` + `Batcher` 实例。由于每个 `LangfuseClient` 实例对应 Langfuse 侧的一个独立会话上下文，导致即使同一对话（Thread）内的多轮消息，在 Langfuse 界面中也会显示为多个独立的 Session，而不是同一个 Session 下的多个 Trace。

## 目标

- 保证同一个 Thread（SQLite 对话历史）在 Langfuse 里映射为同一个 Session
- 同一 Thread 内的每一轮消息对应一个独立 Trace，均挂在同一个 Session 下
- 切换或新建 Thread 时自动重置 Session，新 Thread 映射新的 Langfuse Session

## 方案设计

### 核心思路

将 `LangfuseClient` + `Batcher` 的生命周期从「每轮消息」提升到「Thread 级别」，引入 `LangfuseSession` 结构来承载 Thread 生命周期内的共享 Langfuse 连接状态。

```
旧设计：
  submit_message() → LangfuseTracer { client, batcher, trace_id, session_id, ... }
                     ↑ 每轮新建，每轮都有独立 LangfuseClient → Langfuse 视为新 Session

新设计：
  App { langfuse_session: Option<Arc<LangfuseSession>> }
           ↑ Thread 级别，跨多轮共享
  submit_message() → LangfuseTracer { session: Arc<LangfuseSession>, trace_id, ... }
                     ↑ 每轮只创建轻量 per-turn 状态，复用 session 的 client/batcher
```

### 数据模型

**新增 `LangfuseSession`（Thread 级别）：**

```rust
pub struct LangfuseSession {
    pub client: Arc<LangfuseClient>,  // 整个 Thread 生命周期内只初始化一次
    pub batcher: Arc<Batcher>,        // 整个 Thread 生命周期内只初始化一次
    pub session_id: String,           // = thread_id，Thread 内所有 Trace 共享
}

impl LangfuseSession {
    pub async fn new(config: LangfuseConfig, session_id: String) -> Option<Self>;
}
```

**修改 `LangfuseTracer`（Turn 级别）：**

```rust
pub struct LangfuseTracer {
    session: Arc<LangfuseSession>,                        // 引用共享 Session（不再持有 client/batcher）
    trace_id: String,                                     // 当前轮次专属 Trace ID
    generation_data: HashMap<usize, (String, Vec<BaseMessage>)>,
    pending_spans: VecDeque<String>,
}

impl LangfuseTracer {
    pub fn new(session: Arc<LangfuseSession>) -> Self;
    // on_trace_start 不再需要 thread_id 参数，直接从 session.session_id 取
    pub fn on_trace_start(&mut self, input: &str);
    // 其余方法签名不变，内部从 self.session.client / batcher 取
}
```

### 生命周期管理

```
App::new()
  langfuse_session = None  （懒加载）

submit_message():
  1. ensure_thread_id() → thread_id = "UUID_A"
  2. if langfuse_session.is_none():
       langfuse_session = LangfuseSession::new(config, session_id="UUID_A")
                          ↑ 首轮创建，后续复用
  3. LangfuseTracer::new(langfuse_session.clone())  ← 每轮新 trace_id，共享 session

new_thread():
  langfuse_session = None  （下次发消息时按新 thread_id 重建）

open_thread(thread_id):
  langfuse_session = None  （下次发消息时按打开的 thread_id 重建）
```

### Langfuse 数据结构效果

![Langfuse Session-Trace 层次结构](./images/01-flow.png)

```
Langfuse Sessions 列表:
  Session: UUID_A (= thread_id, 对应一段对话)
    ├── Trace: turn-1 (trace_id = UUID_X)
    │     ├── Generation: llm-call-step-0
    │     └── Span: bash tool
    └── Trace: turn-2 (trace_id = UUID_Y)
          ├── Generation: llm-call-step-0
          └── Span: read_file tool

  Session: UUID_B (= 另一个 thread_id, 对应另一段对话)
    └── Trace: turn-1 (trace_id = UUID_Z)
```

### 改动文件

**`rust-agent-tui/src/langfuse/mod.rs`**

| 操作 | 内容 |
|------|------|
| 新增 | `LangfuseSession` 结构体 + `new()` |
| 修改 | `LangfuseTracer`：移除 `client/batcher/session_id` 字段，改为 `session: Arc<LangfuseSession>` |
| 修改 | `LangfuseTracer::new(session)` 构造函数签名 |
| 修改 | `on_trace_start(&mut self, input: &str)`：移除 `thread_id` 参数，从 `session.session_id` 读取 |
| 修改 | 所有方法内部引用从 `self.client` → `self.session.client`，`self.batcher` → `self.session.batcher` |

**`rust-agent-tui/src/app/mod.rs`**

| 操作 | 内容 |
|------|------|
| 新增 | `App.langfuse_session: Option<Arc<LangfuseSession>>` 字段 |
| 修改 | `submit_message()`：懒加载 session，用 `LangfuseTracer::new(session.clone())` |
| 修改 | `new_thread()`：`self.langfuse_session = None` |
| 修改 | `open_thread()`：`self.langfuse_session = None` |
| 修改 | `App::new_headless()` 测试构造：补充 `langfuse_session: None` |

**改动量估计：** ~60 行，纯重构，无新依赖，无 API 变更。

## 实现要点

- `LangfuseSession` 的 `session_id` 在 `submit_message()` 中通过 `ensure_thread_id()` 拿到后传入，确保与 SQLite Thread 一一对应
- Session 是懒加载的（首次发消息时创建），不在 App 初始化时创建，避免启动时不必要的网络连接
- 打开历史 Thread 时，旧 Session 被 None 替换，下一条消息会用历史 Thread 的 `thread_id` 创建新 Session（正确映射 Langfuse Session）
- `LangfuseTracer` 持有 `Arc<LangfuseSession>`，Session 只要有任何 Tracer 还活着就不会被 drop，确保 Batcher 在当前轮次结束前不会提前 flush/drop
- 未配置 Langfuse 环境变量时（`LangfuseConfig::from_env()` 返回 None），`langfuse_session` 保持 None，行为与当前完全一致

## 约束一致性

- 遵守 `architecture.md` 中「事件驱动 TUI 通信」约束：Session 生命周期管理在 App（主线程）上，不引入额外线程共享状态
- 遵守「Workspace 多 crate 分层」约束：改动仅在 `rust-agent-tui`（应用层），不影响 `rust-create-agent` 和 `rust-agent-middlewares`
- 遵守「异步优先」约束：`LangfuseSession::new()` 为 async，通过 `block_in_place` 在同步上下文中调用（与现有 `LangfuseTracer::new()` 调用方式完全一致）
- 无新增依赖，无 API 变更，无破坏性重构

## 验收标准

- [ ] 同一对话（Thread）内连续发送多轮消息，在 Langfuse Sessions 页面能看到它们归属于同一个 Session
- [ ] 新建对话（`new_thread()`）后发消息，在 Langfuse 中生成新的 Session
- [ ] 打开历史 Thread 后发消息，新的 Trace 归入对应历史 Thread 的 Session（不是新 Session，也不是上一个 Thread 的 Session）
- [ ] 未配置 Langfuse 环境变量时，程序正常运行，无 panic 或 warning
- [ ] `cargo build -p rust-agent-tui` 编译通过，`cargo test -p rust-agent-tui` 测试全部通过
