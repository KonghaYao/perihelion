# TUI Headless 测试模式 执行计划

**目标:** 为 rust-agent-tui 实现无 Terminal 的 headless 测试模式，渲染管道完全统一

**技术栈:** Rust, ratatui TestBackend, tokio::test, parking_lot::RwLock, tokio::sync::Notify

**设计文档:** ./spec-design.md

---

### Task 1: App 事件处理提取与测试注入方法

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 提取 `handle_agent_event()` 私有方法
  - 将 `poll_agent()` 中 `match rx.try_recv()` 各分支的处理逻辑提取为独立方法 `fn handle_agent_event(&mut self, event: AgentEvent) -> bool`
  - `poll_agent()` 改为调用此方法，保持行为完全一致
  - 方法返回 `bool` 表示是否需要中断消费循环（ApprovalNeeded/AskUserBatch/Done/Error）
- [x] 在 `#[cfg(test)]` 块中添加 `push_agent_event()` 和 `process_pending_events()`
  - `App` 结构体在 `#[cfg(test)]` 下添加字段 `agent_event_queue: Vec<AgentEvent>`
  - `push_agent_event(&mut self, event: AgentEvent)` 向队列推入事件
  - `process_pending_events(&mut self)` 逐个调用 `handle_agent_event()`，遇到返回 `true` 的事件立即停止（模拟 poll_agent 的 break/return 行为）
- [x] 确认 `#[cfg(test)]` 字段不污染生产结构体
  - `agent_event_queue` 字段和相关 impl 块均在 `#[cfg(test)]` 内
  - `App::new()` 的初始化代码不涉及该字段（条件编译字段 Rust 不要求在 struct literal 中出现）

**检查步骤:**
- [x] 编译通过（release 模式）
  - `cargo build -p rust-agent-tui --release 2>&1 | tail -5`
  - 预期: 输出包含 "Compiling" 或 "Finished"，无 error
- [x] poll_agent 行为不变（现有功能回归）
  - `cargo test -p rust-agent-tui --lib 2>&1 | tail -10`
  - 预期: 输出 "test result: ok" 或 "0 failed"

---

### Task 2: HeadlessHandle 模块实现

**涉及文件:**
- 新建: `rust-agent-tui/src/ui/headless.rs`
- 修改: `rust-agent-tui/src/ui/mod.rs`

**执行步骤:**
- [x] 新建 `rust-agent-tui/src/ui/headless.rs`，内容全部在 `#[cfg(any(test, feature = "headless"))]` 下
  - 定义 `HeadlessHandle` 结构体，包含 `terminal: Terminal<TestBackend>` 和 `render_notify: Arc<Notify>`
  - 实现 `snapshot(&self) -> Vec<String>`：遍历 `self.terminal.backend().buffer().content`，按宽度分行，每行 `cell.symbol()` 拼接后 `trim_end()` 去尾空格
  - 实现 `contains(&self, text: &str) -> bool`：调用 `self.snapshot().iter().any(|l| l.contains(text))`
  - 实现 `wait_for_render(&self) async`：调用 `self.render_notify.notified().await`
- [x] 在 `App` impl 块内添加 `new_headless()` 构造函数（在 `#[cfg(any(test, feature = "headless"))]` 下）
  - 逻辑：创建 `TestBackend::new(width, height)`，`Terminal::new(backend)`，调用 `spawn_render_thread(width)`，构造 `App`（复用 `App::new()` 的字段初始化逻辑，跳过真实终端相关代码）
  - `App` 构造时不启动 SQLite thread store（用 in-memory 或 temp 路径避免副作用），`cwd` 设为 `/tmp`
  - 返回 `(App, HeadlessHandle)`
- [x] 在 `rust-agent-tui/src/ui/mod.rs` 中添加 `#[cfg(any(test, feature = "headless"))] pub mod headless;`
- [x] 在 `rust-agent-tui/src/app/mod.rs` 中 pub use `headless::HeadlessHandle`（条件编译）

**检查步骤:**
- [x] 编译通过（debug 模式，带 test cfg）
  - `cargo test -p rust-agent-tui --no-run 2>&1 | tail -10`
  - 预期: 无编译错误，输出 "Compiling" 或 "Finished"
- [x] snapshot() 返回正确行数（基于 TestBackend 宽高）
  - 在测试中断言：`let (_, handle) = App::new_headless(80, 24); assert_eq!(handle.snapshot().len(), 24);`
  - 预期: 不 panic，返回 24 行（TestBackend buffer 固定行数）

---

### Task 3: 集成测试文件

**涉及文件:**
- 新建: `rust-agent-tui/tests/headless_render.rs`

**执行步骤:**
- [x] 新建 `rust-agent-tui/tests/headless_render.rs`，添加以下 4 个测试用例（实际写在 src/ui/headless.rs 的 #[cfg(test)] 模块中，因 bin crate 不支持外部集成测试）
- [x] 测试 1：AssistantChunk 流式消息渲染
  ```rust
  #[tokio::test]
  async fn test_assistant_chunk_renders() {
      let (mut app, mut handle) = App::new_headless(120, 30);
      app.push_agent_event(AgentEvent::AssistantChunk("Hello world".into()));
      app.push_agent_event(AgentEvent::Done);
      app.process_pending_events();
      handle.wait_for_render().await;
      handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
      assert!(handle.contains("Agent"), "应显示 Agent 标头");
      assert!(handle.contains("Hello world"), "应显示消息内容");
  }
  ```
- [x] 测试 2：ToolCall 工具块渲染
  ```rust
  #[tokio::test]
  async fn test_tool_call_renders() {
      let (mut app, mut handle) = App::new_headless(120, 30);
      app.push_agent_event(AgentEvent::ToolCall {
          tool_call_id: "t1".into(),
          name: "read_file".into(),
          display: "读取 src/main.rs".into(),
          is_error: false,
      });
      app.process_pending_events();
      handle.wait_for_render().await;
      handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
      assert!(handle.contains("read_file"));
  }
  ```
- [x] 测试 3：用户消息渲染（通过 view_messages 直接写入）
  ```rust
  #[tokio::test]
  async fn test_user_message_renders() {
      let (mut app, mut handle) = App::new_headless(120, 30);
      let vm = MessageViewModel::user("你好世界".into());
      app.view_messages.push(vm.clone());
      let _ = app.render_tx.try_send(RenderEvent::AddMessage(vm));
      handle.wait_for_render().await;
      handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
      assert!(handle.contains("你好世界"));
  }
  ```
- [x] 测试 4：Clear 后屏幕内容清空
  ```rust
  #[tokio::test]
  async fn test_clear_empties_messages() {
      let (mut app, mut handle) = App::new_headless(120, 30);
      app.push_agent_event(AgentEvent::AssistantChunk("Some content".into()));
      app.process_pending_events();
      handle.wait_for_render().await;
      // 清空
      app.view_messages.clear();
      let _ = app.render_tx.try_send(RenderEvent::Clear);
      handle.wait_for_render().await;
      handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
      assert!(!handle.contains("Some content"), "清空后不应包含之前的内容");
  }
  ```

**检查步骤:**
- [x] 全部 4 个测试通过
  - `cargo test -p rust-agent-tui 2>&1 | tail -20`
  - 预期: 输出 "test result: ok. 4 passed"（实际 20 passed，含已有测试）
- [x] 无 sleep 调用
  - `grep -n "sleep" rust-agent-tui/tests/headless_render.rs`
  - 预期: 无输出（零 sleep）
- [x] release 编译不包含 headless 代码
  - `cargo build -p rust-agent-tui --release 2>&1 | tail -5`
  - 预期: 编译成功，无 error

---

### Task 4: TUI Headless 测试模式 Acceptance

**Prerequisites:**
- 启动命令: 无（纯测试，不需要运行 TUI）
- 确认 Cargo.toml 无新增外部依赖（`TestBackend` 来自已有 `ratatui` crate）
- 确认 `rust-agent-tui/tests/` 目录已创建

**End-to-end verification:**

1. [x] 全量测试通过（含新增 4 个集成测试）
   - `cargo test -p rust-agent-tui 2>&1 | grep -E "test result|FAILED|error"`
   - Expected: 输出 "test result: ok"，无 FAILED，无 error
   - On failure: 检查 Task 3（测试用例实现）或 Task 1/2（App/HeadlessHandle 实现）

2. [x] Release 编译不引入 headless 代码膨胀
   - `cargo build -p rust-agent-tui --release 2>&1 | grep -E "error|warning.*headless"`
   - Expected: 无 error，无 headless 相关 warning
   - On failure: 检查 Task 2（headless.rs 的 cfg 属性是否正确）

3. [x] 渲染管道统一验证：snapshot 返回真实 draw 结果
   - `cargo test -p rust-agent-tui test_assistant_chunk_renders -- --nocapture 2>&1 | tail -5`
   - Expected: 测试通过，无 panic
   - On failure: 检查 Task 2（HeadlessHandle::snapshot 实现）或 Task 1（handle_agent_event 提取是否正确）

4. [x] wait_for_render 无 sleep 同步正确性
   - `grep -rn "sleep" rust-agent-tui/tests/ rust-agent-tui/src/ui/headless.rs`
   - Expected: 无输出（零 sleep 调用）
   - On failure: 检查 Task 2（wait_for_render 应使用 notify.notified().await）

5. [x] 整个 workspace 编译无新增 warning
   - `cargo build --workspace 2>&1 | grep "^warning" | wc -l`
   - Expected: warning 数量不多于实施前（基线可先 `cargo build --workspace 2>&1 | grep "^warning" | wc -l` 记录）
   - On failure: 检查 Task 1/2（是否引入了未使用的 import 或 dead_code）
