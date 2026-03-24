# TUI 渲染性能优化（双线程架构）执行计划

**目标:** 将渲染计算移到独立线程，UI 线程只负责从缓存读取可见行并按需绘制，消除消息增多后的卡顿

**技术栈:** Rust, ratatui, tokio, parking_lot::RwLock, tokio::sync::Notify

**设计文档:** spec/feature_20260323_F001_tui-render-perf/spec-design.md

---

### Task 1: RenderCache 与 RenderEvent 数据结构

**涉及文件:**
- 新建: `rust-agent-tui/src/ui/render_thread.rs`
- 修改: `rust-agent-tui/src/ui/mod.rs`

**执行步骤:**
- [x] 在 `rust-agent-tui/src/ui/render_thread.rs` 中定义 `RenderCache` 结构体
  - `lines: Vec<Line<'static>>` — 所有消息渲染后的行
  - `message_offsets: Vec<usize>` — 每条消息在 lines 中的起始索引
  - `total_lines: usize` — 总行数
  - `version: u64` — 版本号，UI 线程用于判断是否需要重绘
  - 实现 `RenderCache::new()` 返回空缓存
- [x] 定义 `RenderEvent` 枚举
  - `AddMessage(MessageViewModel)` — 新增完整消息
  - `AppendChunk(String)` — 流式追加到最后一条 assistant 消息
  - `Resize(u16)` — 终端宽度变化，全量重算
  - `Clear` — 清空所有消息
  - `LoadHistory(Vec<MessageViewModel>)` — 加载历史消息（批量）
- [x] 在 `ui/mod.rs` 中添加 `pub mod render_thread;`

**检查步骤:**
- [x] 模块编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 编译成功，无 error

---

### Task 2: 渲染线程实现

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/render_thread.rs`

**执行步骤:**
- [x] 实现 `RenderTask` 结构体，持有渲染线程的私有状态
  - `messages: Vec<MessageViewModel>` — 消息数据的私有副本
  - `cache: Arc<parking_lot::RwLock<RenderCache>>` — 共享缓存
  - `notify: Arc<tokio::sync::Notify>` — 通知 UI 线程
  - `width: u16` — 当前终端宽度
- [x] 实现 `RenderTask::run(mut self, mut rx: mpsc::Receiver<RenderEvent>)` 异步方法
  - 循环等待 `rx.recv()`，按事件类型处理：
  - `AddMessage`: 调用 `render_view_model` 生成新 lines，追加到内部状态，更新缓存
  - `AppendChunk`: 追加 chunk 到最后一条 assistant 消息的 raw text，标记 dirty，调用 `ensure_rendered`，只重新渲染该消息，替换缓存中对应区间
  - `Resize`: 更新 width，全量重新渲染所有消息
  - `Clear`: 清空 messages 和缓存
  - `LoadHistory`: 批量添加所有消息并全量渲染
  - 每次处理后：短暂加写锁更新 `RenderCache`，递增 version，`notify.notify_one()`
- [x] 提供 `spawn_render_thread(width: u16) -> (mpsc::Sender<RenderEvent>, Arc<RwLock<RenderCache>>, Arc<Notify>)` 工厂函数
  - 创建 channel（容量 64，允许流式 chunk 的高频发送）
  - 创建共享 cache 和 notify
  - `tokio::spawn` 启动 `RenderTask::run`
  - 返回 sender、cache、notify

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 编译成功
- [x] 单元测试：AddMessage 后 cache 的 version 递增且 lines 非空
  - `cargo test -p rust-agent-tui --lib -- render_thread 2>&1 | tail -10`
  - 预期: 测试通过
- [x] 单元测试：AppendChunk 只更新最后一条消息的 lines 区间
  - `cargo test -p rust-agent-tui --lib -- render_thread 2>&1 | tail -10`
  - 预期: 测试通过

---

### Task 3: App 集成渲染线程

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 在 `App` 结构体中添加渲染线程相关字段
  - `render_tx: mpsc::Sender<RenderEvent>` — 发送渲染事件
  - `render_cache: Arc<parking_lot::RwLock<RenderCache>>` — 共享缓存（UI 线程只读）
  - `render_notify: Arc<tokio::sync::Notify>` — 接收渲染完成通知
  - `last_render_version: u64` — UI 线程记录的最后绘制版本
- [x] 在 `App::new()` 中启动渲染线程
  - 调用 `spawn_render_thread(初始宽度)` 获取 tx/cache/notify
  - 初始系统消息通过 `render_tx.try_send(RenderEvent::AddMessage(...))` 发送
- [x] 改造 `poll_agent()` 中的消息写入路径
  - `AgentEvent::ToolCall` → 构建 `MessageViewModel::tool_block` 后，同时 push 到 `view_messages`（保留兼容）并 `render_tx.try_send(AddMessage(vm))`
  - `AgentEvent::AssistantChunk` → 同时维护 `view_messages` 并 `render_tx.try_send(AppendChunk(chunk))`
  - 其他事件（Done/Error/TodoUpdate/SystemNote）类似处理
- [x] 改造 `submit_message()` 中用户消息的写入
  - push 到 `view_messages` 后，同步发送 `render_tx.try_send(AddMessage(vm))`
- [x] 改造 `new_thread()` 和 `open_thread()`
  - `new_thread()`: 发送 `RenderEvent::Clear`
  - `open_thread()`: 发送 `RenderEvent::Clear` 后，发送 `RenderEvent::LoadHistory(vms)`

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 编译成功，无 error
- [x] poll_agent 中所有消息事件分支都有对应的 render_tx 发送
  - `grep -n "render_tx" rust-agent-tui/src/app/mod.rs | wc -l`
  - 预期: 至少 6 处调用（AddMessage×4 + AppendChunk×1 + Clear/LoadHistory）

---

### Task 4: UI 线程按需绘制

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui.rs`
- 修改: `rust-agent-tui/src/main.rs`

**执行步骤:**
- [x] 改造 `render_messages()` 从 RenderCache 读取行
  - 不再遍历 `app.view_messages` 生成 lines
  - 改为：加读锁读取 `render_cache.read()`，获取 `lines` 切片
  - 根据 `scroll_offset` 和 `viewport_height` 计算可见区间
  - 用 `Paragraph::new()` + `scroll()` 绘制可见行（与当前逻辑类似，但数据来源从实时计算改为缓存读取）
  - 更新 `app.last_render_version` 为当前 cache version
- [x] 改造 `main.rs` 的 `run_app()` 主循环实现按需重绘
  - 引入 `needs_redraw` 标志：当 cache version 变化、有用户键盘/鼠标事件、或有弹窗状态变化时为 true
  - 检查 `render_cache.read().version != app.last_render_version` 判断是否有新渲染结果
  - 只在 `needs_redraw` 为 true 时调用 `terminal.draw()`
  - 处理 `Event::Resize` 时发送 `RenderEvent::Resize(new_width)`
- [x] 处理 ensure_rendered 的调用移除
  - `render_messages()` 中不再调用 `ensure_rendered()`，此逻辑已移至渲染线程

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 编译成功
- [x] main_ui.rs 的 render_messages 不再遍历 view_messages
  - `grep -c "app.view_messages" rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 输出 0（render_messages 中不再直接引用 view_messages）
- [x] main.rs 的主循环包含条件重绘逻辑
  - `grep -c "needs_redraw\|last_render_version" rust-agent-tui/src/main.rs`
  - 预期: 至少 2 处

---

### Task 5: TUI 渲染性能优化 Acceptance

**Prerequisites:**
- 启动命令: `cargo run -p rust-agent-tui`
- 确保已配置至少一个 LLM provider（`ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY`）
- 可选：启动 Jaeger 观测 `docker compose -f docker-compose.otel.yml up -d`

**End-to-end verification:**

1. 基本功能：发送消息并接收流式回复
   - 启动 TUI，输入一条消息发送，观察 Agent 流式回复正常显示
   - `cargo run -p rust-agent-tui 2>&1`
   - Expected: 消息正常显示，流式输出逐字渲染，无 panic
   - On failure: check Task 2 [渲染线程 AppendChunk 处理]

2. 滚动流畅性：多条消息时滚动无卡顿
   - 发送多条消息（>10 条），使用鼠标滚轮或 PageUp/PageDown 浏览
   - Expected: 滚动响应即时，无明显掉帧或延迟
   - On failure: check Task 4 [UI 线程按需绘制]

3. 输入响应性：打字输入无延迟
   - 在有多条历史消息时快速打字
   - Expected: 输入框字符即时出现，无可感知延迟
   - On failure: check Task 4 [主循环按需 draw 逻辑]

4. HITL/AskUser 弹窗功能
   - 触发需要审批的工具调用（如 write_file），验证 HITL 弹窗正常弹出和确认
   - Expected: 弹窗正常显示，审批后 Agent 继续执行
   - On failure: check Task 3 [App 集成，弹窗事件未受影响]

5. 清空与历史加载
   - 使用 `/clear` 命令清空对话，验证界面清空；使用 `/history` 加载历史对话，验证消息正确显示
   - Expected: 清空后界面干净，历史消息正确加载和渲染
   - On failure: check Task 3 [new_thread/open_thread 的 RenderEvent 发送]

6. 终端 resize 后渲染正确
   - 在有消息时调整终端窗口大小
   - Expected: 消息重新排版，行包装正确，无显示错乱
   - On failure: check Task 2 [Resize 事件的全量重渲染] 和 Task 4 [Resize 事件转发]
