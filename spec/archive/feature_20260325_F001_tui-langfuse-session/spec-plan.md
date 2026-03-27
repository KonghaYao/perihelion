# TUI Langfuse Session 机制 执行计划

**目标:** 将 `LangfuseClient`/`Batcher` 生命周期从每轮消息提升到 Thread 级别，确保同一对话在 Langfuse 中归属同一 Session

**技术栈:** Rust 2021, tokio, langfuse-ergonomic, parking_lot

**设计文档:** [spec-design.md](./spec-design.md)

---

### Task 1: LangfuseSession 结构 + LangfuseTracer 重构

**涉及文件:**
- 修改: `rust-agent-tui/src/langfuse/mod.rs`

**执行步骤:**

- [x] 在 `mod.rs` 中新增 `LangfuseSession` 结构体，持有 Thread 级别的共享状态
  - 字段：`client: Arc<LangfuseClient>`，`batcher: Arc<Batcher>`，`session_id: String`
  - 实现 `async fn new(config: LangfuseConfig, session_id: String) -> Option<Self>`
  - 构造逻辑与现有 `LangfuseTracer::new()` 中的 client/batcher 创建代码相同，直接迁移

- [x] 修改 `LangfuseTracer` 结构体，移除 `client`/`batcher`/`session_id` 字段，改为持有 `session: Arc<LangfuseSession>`
  - 保留 `trace_id: String`、`generation_data`、`pending_spans` 三个 per-turn 字段
  - 更新注释（生命周期说明），doc 注释中移除对 `Arc<LangfuseClient>` 的引用

- [x] 修改 `LangfuseTracer::new(session: Arc<LangfuseSession>) -> Self`（同步，不再 async）
  - 只生成新的 `trace_id = uuid::Uuid::now_v7().to_string()`，其余字段置空/初始化

- [x] 修改 `on_trace_start(&mut self, input: &str)`，移除 `thread_id: Option<&str>` 参数
  - `session_id` 直接从 `self.session.session_id.clone()` 读取
  - 使用 `Arc::clone(&self.session.client)` 代替原 `Arc::clone(&self.client)`

- [x] 修改 `on_llm_start`/`on_llm_end`/`on_tool_start`/`on_tool_end_by_name_order`/`on_trace_end`
  - 所有方法中将 `Arc::clone(&self.client)` → `Arc::clone(&self.session.client)`
  - 将 `Arc::clone(&self.batcher)` → `Arc::clone(&self.session.batcher)`
  - `self.trace_id.clone()` 不变

**检查步骤:**

- [x] 验证 `langfuse/mod.rs` 单独编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "error\[|^error"`
  - 预期: 无 error 输出（仅此文件改动时可能有 app/mod.rs 编译错误，Task 2 完成后整体通过）

- [x] 验证 `LangfuseSession::new` 签名存在
  - `grep -n "async fn new" rust-agent-tui/src/langfuse/mod.rs`
  - 预期: 输出包含 `async fn new(config: LangfuseConfig, session_id: String)`

- [x] 验证 `LangfuseTracer` 不再持有 `client`/`batcher` 字段
  - `grep -n "client:\|batcher:" rust-agent-tui/src/langfuse/mod.rs | grep -v "session\."`
  - 预期: 仅在 `LangfuseSession` 结构体定义行出现，不在 `LangfuseTracer` 字段中出现

- [x] 验证 `on_trace_start` 签名不含 `thread_id` 参数
  - `grep -n "fn on_trace_start" rust-agent-tui/src/langfuse/mod.rs`
  - 预期: 输出为 `pub fn on_trace_start(&mut self, input: &str)`

---

### Task 2: App Session 生命周期管理

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**

- [x] 在 `App` 结构体中新增字段 `langfuse_session: Option<Arc<crate::langfuse::LangfuseSession>>`
  - 放在 `langfuse_tracer` 字段紧上方，加注释说明生命周期

- [x] 在 `App::new()` 的结构体初始化块中补充 `langfuse_session: None`

- [x] 修改 `submit_message()` 中的 Langfuse 初始化逻辑（原 918-929 行）
  - 原逻辑：每次 `LangfuseConfig::from_env()` → `LangfuseTracer::new(cfg)` → `on_trace_start(input, thread_id)`
  - 新逻辑：
    1. 若 `self.langfuse_session.is_none()`，则 `block_in_place` 创建 `LangfuseSession::new(config, thread_id)`，存入 `self.langfuse_session`
    2. 若 `self.langfuse_session.is_some()`，直接复用
    3. 用 `LangfuseTracer::new(session.clone())` 创建 per-turn tracer（同步调用，不再需要 `block_in_place`）
    4. 调用 `tracer.on_trace_start(input.trim())`（参数移除 `thread_id`）

- [x] 在 `new_thread()` 末尾追加 `self.langfuse_session = None;`
  - 确保新建对话时 Session 重置，下次发消息创建新 Session（对应新 thread_id）

- [x] 在 `open_thread()` 末尾追加 `self.langfuse_session = None;`
  - 确保打开历史对话时 Session 重置，下次发消息按打开的 thread_id 创建 Session

- [x] 在 `App::new_headless()` 测试构造块中补充 `langfuse_session: None`
  - 位置与 `langfuse_tracer: None` 相邻

**检查步骤:**

- [x] 验证整体编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"`
  - 预期: 无 error 输出

- [x] 验证 clippy 无新增 warning
  - `cargo clippy -p rust-agent-tui 2>&1 | grep -E "^warning|^error" | grep -v "Checking\|Compiling\|Finished"`
  - 预期: 输出为空（或只含已有 warning，无新增）

- [x] 验证 `App` 结构体含 `langfuse_session` 字段
  - `grep -n "langfuse_session" rust-agent-tui/src/app/mod.rs`
  - 预期: 至少 5 行（字段声明 + new 初始化 + new_headless 初始化 + new_thread + open_thread + submit_message 懒加载）

- [x] 验证 `new_thread` 包含重置
  - `grep -A5 "fn new_thread" rust-agent-tui/src/app/mod.rs | grep langfuse_session`
  - 预期: 输出 `self.langfuse_session = None;`

- [x] 验证 `open_thread` 包含重置
  - `grep -B2 -A30 "fn open_thread\b" rust-agent-tui/src/app/mod.rs | grep langfuse_session`
  - 预期: 输出 `self.langfuse_session = None;`

- [x] 验证 `on_trace_start` 调用不含 `thread_id` 参数
  - `grep -n "on_trace_start" rust-agent-tui/src/app/mod.rs`
  - 预期: 调用形式为 `on_trace_start(input.trim())`，不含第二个参数

---

### Task 3: Langfuse Session 验收

**前置条件:**
- 编译命令: `cargo build -p rust-agent-tui`
- 测试运行命令: `cargo test -p rust-agent-tui`
- 可选 Langfuse 环境（仅用于端到端验证）: 配置 `LANGFUSE_PUBLIC_KEY`、`LANGFUSE_SECRET_KEY`

**端到端验证:**

1. ~~**编译无错误**~~
   - `cargo build -p rust-agent-tui 2>&1 | tail -3`
   - 结果: `Finished` ✅

2. ~~**所有现有测试通过**~~
   - `cargo test -p rust-agent-tui 2>&1 | tail -10`
   - 结果: `test result: ok. 54 passed; 0 failed` ✅

3. ~~**未配置 Langfuse 时无 panic**~~
   - `LANGFUSE_PUBLIC_KEY="" LANGFUSE_SECRET_KEY="" cargo test -p rust-agent-tui 2>&1 | grep -E "panic|FAILED"`
   - 结果: 无 panic，无 FAILED ✅

4. ~~**验证 `LangfuseSession` 的 `new` 为 async 且接收 session_id**~~
   - 结果: `pub async fn new(config: LangfuseConfig, session_id: String) -> Option<Self>` ✅

5. ~~**验证 `LangfuseTracer::new` 为同步且参数为 `Arc<LangfuseSession>`**~~
   - 结果: `pub fn new(session: Arc<LangfuseSession>) -> Self` ✅
