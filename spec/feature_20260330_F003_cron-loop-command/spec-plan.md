# Cron Loop Command 执行计划

**目标:** 在 TUI 和 middlewares 层实现定时任务（cron）注册、管理和自动触发执行

**技术栈:** Rust, tokio, croner (cron 表达式解析), ratatui (TUI)

**设计文档:** ../spec-design.md

---

### Task 1: 添加 croner 依赖 + CronScheduler 核心数据结构

**涉及文件:**
- 修改: `rust-agent-middlewares/Cargo.toml`
- 新建: `rust-agent-middlewares/src/cron/mod.rs`

**执行步骤:**
- [x] 在 `rust-agent-middlewares/Cargo.toml` 的 `[dependencies]` 中添加 `croner` crate
  - `croner = "2"`
- [x] 新建 `rust-agent-middlewares/src/cron/mod.rs`
  - 定义 `CronTask` 结构体：`id`（String）、`expression`（String）、`prompt`（String）、`next_fire`（`Option<chrono::DateTime<chrono::Utc>>`）、`enabled`（bool）
  - 定义 `CronTrigger` 结构体：`task_id`（String）、`prompt`（String），derive `Clone`
  - 定义常量 `MAX_CRON_TASKS: usize = 20`
  - 定义 `CronScheduler` 结构体：`tasks: HashMap<String, CronTask>`、`trigger_tx: mpsc::UnboundedSender<CronTrigger>`
  - 实现 `CronScheduler::new(trigger_tx)` 构造函数
  - 实现 `CronScheduler::register(&mut self, expression: &str, prompt: &str) -> Result<String, String>`
    - 使用 `croner::Cron::new(expression).parse()` 解析表达式
    - 解析失败返回 `Err("cron 表达式无效: ...")`
    - 检查任务数上限（`tasks.len() >= MAX_CRON_TASKS`），超限返回错误
    - 使用 `uuid::Uuid::now_v7()` 生成 ID
    - 计算下次触发时间 `cron.schedule_from_now().next()` 存入 `next_fire`
    - 存入 HashMap，返回 `Ok(id)`
  - 实现 `CronScheduler::remove(&mut self, id: &str) -> bool`（从 HashMap 移除）
  - 实现 `CronScheduler::toggle(&mut self, id: &str) -> bool`（切换 enabled）
  - 实现 `CronScheduler::tick(&mut self)`
    - 获取 `Utc::now()`，遍历 tasks
    - 对 enabled 且 `next_fire <= now` 的任务：通过 `trigger_tx.send(CronTrigger)` 发送
    - 更新 `next_fire` 为下一个触发时间
  - 实现 `CronScheduler::list_tasks(&self) -> &[CronTask]`
  - 实现 `CronScheduler::get_task(&self, id: &str) -> Option<&CronTask>`
  - `CronScheduler` 内部使用 `chrono::Utc` 做时间计算

**检查步骤:**
- [x] CronScheduler 单元测试通过
  - `cd rust-agent-middlewares && cargo test -p rust-agent-middlewares --lib -- cron`
  - 预期: 测试注册、删除、tick 触发、上限检查、无效表达式拒绝

---

### Task 2: CronMiddleware + 三个工具实现

**涉及文件:**
- 新建: `rust-agent-middlewares/src/cron/middleware.rs`
- 新建: `rust-agent-middlewares/src/cron/tools.rs`
- 修改: `rust-agent-middlewares/src/cron/mod.rs`（添加 pub mod 声明 + re-export）
- 修改: `rust-agent-middlewares/src/lib.rs`（添加 `pub mod cron` + re-export）
- 修改: `rust-agent-middlewares/src/prelude` 部分（添加 CronMiddleware re-export）

**执行步骤:**
- [x] 新建 `cron/tools.rs`，实现三个 `BaseTool`
  - `CronRegisterTool`：持有 `Arc<Mutex<CronScheduler>>`
    - `name() → "cron_register"`
    - `parameters()` → JSON Schema: `{ expression: string (required), prompt: string (required) }`
    - `invoke()` → 获取 scheduler lock，调用 `register(expression, prompt)`，返回结果字符串
  - `CronListTool`：持有 `Arc<Mutex<CronScheduler>>`
    - `name() → "cron_list"`
    - `parameters()` → `{ type: "object", properties: {} }`
    - `invoke()` → 获取 scheduler lock，格式化任务列表为可读文本返回
  - `CronRemoveTool`：持有 `Arc<Mutex<CronScheduler>>`
    - `name() → "cron_remove"`
    - `parameters()` → JSON Schema: `{ id: string (required) }`
    - `invoke()` → 获取 scheduler lock，调用 `remove(id)`，返回结果字符串
- [x] 新建 `cron/middleware.rs`，实现 `CronMiddleware`
  - 持有 `Arc<Mutex<CronScheduler>>`
  - `pub fn new(scheduler: Arc<Mutex<CronScheduler>>) -> Self`
  - `impl<S: State> Middleware<S> for CronMiddleware`
    - `collect_tools()` → 返回三个工具实例
    - `name() → "CronMiddleware"`
- [x] 在 `cron/mod.rs` 中添加 `pub mod middleware; pub mod tools;`
  - Re-export: `pub use middleware::CronMiddleware;` 和 `pub use tools::*;`
  - 将 `CronTask`, `CronTrigger`, `CronScheduler` 加 pub 可见性
- [x] 在 `lib.rs` 添加 `pub mod cron;`
  - 在 `pub use` 区添加 `pub use cron::{CronMiddleware, CronScheduler, CronTask, CronTrigger};`
- [x] 在 `prelude` 中添加 cron 类型 re-export
- [x] 注意：`Arc<Mutex<CronScheduler>>` 使用 `std::sync::Mutex`（非 tokio::sync::Mutex），因为 tick 和工具调用都是短临界区

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-middlewares`
  - 预期: 无错误
- [x] 工具单元测试通过
  - `cargo test -p rust-agent-middlewares --lib -- cron::tools`
  - 预期: register/list/remove 工具测试通过

---

### Task 3: TUI CronState + CronManager Task

**涉及文件:**
- 新建: `rust-agent-tui/src/app/cron_state.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`（添加 `mod cron_state` + CronState 字段到 App）
- 修改: `rust-agent-tui/src/app/core.rs`（无需修改，cron 面板状态放 CronState）

**执行步骤:**
- [x] 新建 `cron_state.rs`
  - 定义 `CronState` 结构体：
    - `scheduler: Arc<parking_lot::Mutex<CronScheduler>>`
    - `trigger_rx: Option<mpsc::UnboundedReceiver<CronTrigger>>`
    - `cron_panel: Option<CronPanel>`（面板状态）
  - 定义 `CronPanel` 结构体：
    - `tasks: Vec<CronTask>`（快照）
    - `cursor: usize`
    - `scroll_offset: u16`
  - 实现 `CronState::new() -> (Self, Arc<parking_lot::Mutex<CronScheduler>>)`
    - 创建 unbounded channel
    - 创建 CronScheduler
    - 用 `Arc<parking_lot::Mutex<CronScheduler>>` 包装
    - 返回 `(CronState { scheduler, trigger_rx, cron_panel: None }, scheduler_clone)`
  - 实现 `CronPanel` 的 `move_cursor(delta: i32)`、`refresh(scheduler)`
- [x] 在 `App` 结构体中添加 `pub cron: CronState` 字段
- [x] 在 `App::new()` 中初始化 cron state 并 spawn CronManager task
  ```rust
  let (cron_state, scheduler_arc) = CronState::new();
  {
      let sched = scheduler_arc.clone();
      tokio::spawn(async move {
          let mut interval = tokio::time::interval(Duration::from_secs(1));
          loop {
              interval.tick().await;
              sched.lock().tick();
          }
      });
  }
  ```
- [x] 在 headless 测试的 `new_headless()` 中也初始化 `CronState`（不 spawn task）

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui`
  - 预期: 无错误
- [x] CronState 初始化无 panic
  - `cargo test -p rust-agent-tui --lib -- headless`
  - 预期: 已有测试不受影响

---

### Task 4: TUI CronTrigger 消费 + submit_message 触发

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent_ops.rs`（`poll_agent()` 末尾消费 CronTrigger）
- 修改: `rust-agent-tui/src/app/agent.rs`（`run_universal_agent` 中插入 CronMiddleware）

**执行步骤:**
- [x] 在 `agent_ops.rs` 的 `poll_agent()` 函数末尾（`loop` 结束前、return updated 前），添加 CronTrigger 消费：
  ```rust
  // 检查 cron 触发
  if let Some(ref mut rx) = self.cron.trigger_rx {
      while let Ok(trigger) = rx.try_recv() {
          if !self.core.loading {
              self.submit_message(trigger.prompt);
          }
          // Agent 正忙时静默跳过
      }
  }
  ```
- [x] 在 `agent.rs` 的 `run_universal_agent()` 中组装 CronMiddleware
  - 从 `AgentRunConfig` 获取 `scheduler: Arc<parking_lot::Mutex<CronScheduler>>`（新增字段）
  - 用 `CronMiddleware::new(scheduler)` 创建中间件
  - 插入到中间件链中，位于 HITL 之前、TodoMiddleware 之后
- [x] 在 `AgentRunConfig` 结构体中添加 `cron_scheduler: Arc<parking_lot::Mutex<CronScheduler>>` 字段
- [x] 在 `submit_message()` 中传递 `self.cron.scheduler.clone()` 到 `AgentRunConfig`
- [x] 注意：需要在 `CronScheduler` 的 tick 中使用 `parking_lot::Mutex` 而非 `std::sync::Mutex`（TUI 已全局使用 parking_lot）
  - 对应调整 Task 1/2 中的 Mutex 类型

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui`
  - 预期: 无错误
- [x] 现有测试不受影响
  - `cargo test -p rust-agent-tui`
  - 预期: 全部通过

---

### Task 5: /loop 和 /cron 命令注册

**涉及文件:**
- 新建: `rust-agent-tui/src/command/loop_cmd.rs`
- 新建: `rust-agent-tui/src/command/cron.rs`
- 修改: `rust-agent-tui/src/command/mod.rs`（注册两个命令）
- 修改: `rust-agent-tui/src/app/mod.rs`（添加 cron panel 操作方法）

**执行步骤:**
- [x] 新建 `command/loop_cmd.rs`
  - `pub struct LoopCommand;`
  - `impl Command for LoopCommand`
    - `name() → "loop"`
    - `description() → "注册定时任务（/loop <cron表达式> <提示词>）"`
    - `execute(app, args)`
      - 将 args 按空格分为 tokens，前 5 个 token 组成 cron 表达式，剩余为 prompt
      - 如果 prompt 为空，显示错误消息并 return
      - 调用 `app.cron.scheduler.lock().register(expression, prompt)`
      - 成功：在 view_messages 中添加 `MessageViewModel::system("⏰ 已注册定时任务 {id}（{expression}）")`
      - 失败：在 view_messages 中添加 `MessageViewModel::system("❌ {error}")`
- [x] 新建 `command/cron.rs`
  - `pub struct CronCommand;`
  - `impl Command for CronCommand`
    - `name() → "cron"`
    - `description() → "查看和管理定时任务"`
    - `execute(app, _args)`
      - 从 scheduler 获取任务列表快照
      - 如果为空，显示 "无定时任务" 系统消息
      - 否则打开 CronPanel：`app.cron.cron_panel = Some(CronPanel::new(tasks))`
- [x] 在 `command/mod.rs` 的 `default_registry()` 中注册两个命令
  - `r.register(Box::new(loop_cmd::LoopCommand));`
  - `r.register(Box::new(cron::CronCommand));`
  - 添加 `pub mod loop_cmd; pub mod cron;` 声明
- [x] 在 `app/mod.rs` 或新建 `app/cron_ops.rs` 中添加 CronPanel 操作方法
  - `cron_panel_move_up/down`
  - `cron_panel_toggle`（Enter 切换 enabled/disabled）
  - `cron_panel_delete`（d 键删除）
  - `cron_panel_close`

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui`
  - 预期: 无错误
- [ ] /loop 和 /cron 命令出现在帮助列表
  - `cargo test -p rust-agent-tui --lib -- command`
  - 预期: `list()` 包含 "loop" 和 "cron"
- [ ] 命令逻辑单元测试
  - `cargo test -p rust-agent-tui --lib -- loop_cmd`
  - 预期: register 成功/失败路径测试通过

---

### Task 6: CronPanel UI 渲染 + 键盘事件处理

**涉及文件:**
- 新建: `rust-agent-tui/src/ui/main_ui/panels/cron.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`（面板渲染分发 + 高度计算）
- 修改: `rust-agent-tui/src/ui/main_ui/panels/mod.rs`（添加 pub mod cron）
- 修改: `rust-agent-tui/src/event.rs`（CronPanel 键盘处理分支）

**执行步骤:**
- [ ] 新建 `panels/cron.rs`
  - `pub(crate) fn render_cron_panel(f: &mut Frame, app: &App, area: Rect)`
  - 风格参照 `agent.rs` 面板渲染
  - 标题：`" ⏰ 定时任务 "`，MUTED 色
  - 每个任务一行：
    - 光标指示 `▶` / ` `
    - 状态图标 `✓启用` / `✗禁用`
    - cron 表达式
    - 下次触发时间（本地时间格式化）
    - prompt 截断（30 字）
  - 底部提示行：`Enter:切换  d:删除  Esc:关闭`
  - 使用 `Paragraph` + `scroll((panel.scroll_offset, 0))`
- [ ] 在 `panels/mod.rs` 添加 `pub mod cron;`
- [ ] 在 `main_ui.rs` 的 `render()` 中
  - 在 panel 渲染区域添加 CronPanel 分支：
    ```rust
    if app.cron.cron_panel.is_some() {
        panels::cron::render_cron_panel(f, app, panel_area);
    }
    ```
  - 在 `active_panel_height()` 中添加 CronPanel 高度计算：
    ```rust
    } else if app.cron.cron_panel.is_some() {
        (tasks_count as u16 * 1 + 4).max(6)
    ```
- [ ] 在 `event.rs` 的 `next_event()` 中添加 CronPanel 键盘处理（放在 thread_browser 之后、agent_panel 之前）：
  ```rust
  if app.cron.cron_panel.is_some() {
      handle_cron_panel(app, input);
      return Ok(Some(Action::Redraw));
  }
  ```
- [ ] 在 `event.rs` 中实现 `handle_cron_panel()`
  - `Up/k` → `cron_panel_move_up`
  - `Down/j` → `cron_panel_move_down`
  - `Enter` → `cron_panel_toggle`（切换 enabled，刷新面板快照）
  - `d` → `cron_panel_delete`（删除当前任务，刷新面板快照，空列表时关闭面板）
  - `Esc` → `cron_panel_close`
  - `Ctrl+C` → `Action::Quit`

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui`
  - 预期: 无错误
- [ ] Headless UI 测试：CronPanel 渲染
  - 创建 headless app → 注入 cron 任务 → 设置 cron_panel → render → assert 包含 "⏰ 定时任务"
  - 预期: 渲染输出包含面板标题

---

### Task 7: Cron Loop Command Acceptance

**Prerequisites:**
- Start command: `cargo run -p rust-agent-tui`
- Test data setup: 无需额外数据，使用内置工具

**End-to-end verification:**

1. `/loop` 注册定时任务
   - 在 TUI 输入框输入 `/loop * * * * * hello cron test`
   - 预期: 消息流显示 `⏰ 已注册定时任务 {id}（* * * * *）`
   - 等待 1 分钟
   - 预期: Agent 自动提交 "hello cron test" 并开始执行
   - On failure: check Task 3 (CronManager tick) 和 Task 4 (trigger 消费)

2. Agent 正忙时跳过触发
   - 注册一个每分钟触发的 cron 任务
   - 在 Agent 执行任务期间等待 cron 触发
   - 预期: 不崩溃不卡顿，Agent 完成后下一个 cron 周期正常触发
   - On failure: check Task 4 (poll_agent 中 loading 检查)

3. `/cron` 面板浏览
   - 注册 2-3 个 cron 任务
   - 输入 `/cron`
   - 预期: 面板显示所有任务，`↑/↓` 导航高亮变化
   - On failure: check Task 6 (CronPanel 渲染)

4. `/cron` 面板操作
   - 在 `/cron` 面板中按 `Enter` 切换任务启用/禁用
   - 按 `d` 删除任务
   - 预期: 操作生效，面板刷新
   - On failure: check Task 5 (cron_ops) 和 Task 6 (handle_cron_panel)

5. AI 通过工具创建定时任务
   - 向 Agent 发送消息："帮我注册一个每 5 分钟检查磁盘空间的定时任务"
   - 预期: AI 调用 `cron_register` 工具，工具返回成功消息，无需 HITL 审批
   - On failure: check Task 2 (CronMiddleware 工具) 和 Task 4 (中间件链集成)

6. AI 查看和删除任务
   - 向 Agent 发送消息："列出所有定时任务"
   - 预期: AI 调用 `cron_list` 工具，展示任务列表
   - 发送："删除第一个任务"
   - 预期: AI 调用 `cron_remove` 工具成功删除
   - On failure: check Task 2 (cron_list/cron_remove 工具)

7. 任务上限
   - 连续注册 21 个 cron 任务
   - 预期: 第 21 个返回错误 "已达到定时任务上限（20）"
   - On failure: check Task 1 (MAX_CRON_TASKS 检查)

8. 无效 cron 表达式
   - 输入 `/loop invalid test` 或让 AI 尝试无效表达式
   - 预期: 返回 "❌ cron 表达式无效: ..."
   - On failure: check Task 1 (croner 解析) 和 Task 5 (LoopCommand)
