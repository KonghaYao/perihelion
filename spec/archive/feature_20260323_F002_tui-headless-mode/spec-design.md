# Feature: 20260323_F002 - TUI Headless 测试模式

## 需求背景

目前 `rust-agent-tui` 没有任何集成测试覆盖。主要渲染逻辑（`render_view_model`、`RenderCache`、`main_ui`）仅在真实 TUI 终端环境下可观察，无法通过自动化 CI 验证。

特别是 F001（20260323_F001_tui-render-perf）引入双线程渲染架构后，渲染线程已有独立子列，但 `main_ui.rs` 的 `draw()` 调用路径仍未被任何测试覆盖。同时，`App` 的事件处理逻辑（`poll_agent()`、HITL 弹窗状态等）也无法在不启动真实终端的情况下验证。

## 目标

- 实现无 Terminal 的 headless 测试模式
- 渲染管道完全统一：测试与生产走同一代码路径（`main_ui.draw()` → `TestBackend`）
- 支持端到端集成测试：模拟 `AgentEvent` 注入 → 渲染线程处理 → `draw()` → 断言屏幕内容

## 方案设计

### 整体架构

![Headless 测试架构数据流](./images/01-flow.png)

核心思路是用 ratatui 官方内置的 `TestBackend` 替换真实 `CrosstermBackend`，其余渲染管道（`RenderTask`、`main_ui`、`draw()`）一行不改。

```
测试代码                          生产代码
────────────────────────────      ────────────────────────────
App::new_headless(w, h)           App::new(CrosstermBackend)
  └─ Terminal<TestBackend>  ←替换→   └─ Terminal<CrosstermBackend>
  └─ spawn render_thread（不变）      └─ spawn render_thread（不变）
  └─ HeadlessHandle

测试流程：
push_agent_event(event)           poll_agent() 从 mpsc channel 收事件
  └─ 向 agent_event_queue 推入      └─ 相同逻辑处理

process_pending_events()          主循环每帧调用 poll_agent()
  └─ 复用 poll_agent 逻辑           └─ 同

wait_for_render().await           Notify 通知 UI 线程重绘
  └─ notify.notified().await        └─ 同

app.draw(&mut handle.terminal)    terminal.draw(|f| render(f, &app))
  └─ main_ui.rs 同一代码路径         └─ 同

handle.snapshot()                 （无对应，测试专用）
  └─ Vec<String>（buffer cells）
```

### 新增文件与改动范围

| 文件 | 类型 | 说明 |
|------|------|------|
| `rust-agent-tui/src/ui/headless.rs` | 新增 | `HeadlessHandle` 定义，`App::new_headless()` |
| `rust-agent-tui/src/app/mod.rs` | 改动 | 添加 `push_agent_event()`、`process_pending_events()` |
| `rust-agent-tui/src/ui/mod.rs` | 改动 | 新增 `pub mod headless` |
| `rust-agent-tui/tests/headless_render.rs` | 新增 | 平层集成测试 |

**不改动任何生产路径代码**（`main_ui.rs`、`render_thread.rs`、`app/agent.rs` 等）。

### HeadlessHandle 接口

```rust
/// Headless 测试句柄，包含 TestBackend Terminal
pub struct HeadlessHandle {
    pub terminal: Terminal<TestBackend>,
    pub render_notify: Arc<Notify>,
}

impl HeadlessHandle {
    /// 截取当前 buffer 为纯文本行列表（去除尾部空格）
    pub fn snapshot(&self) -> Vec<String>;

    /// 检查任意行是否包含指定文本
    pub fn contains(&self, text: &str) -> bool;

    /// 等待渲染线程完成一次渲染（内部 notify.notified().await）
    pub async fn wait_for_render(&self);
}
```

### App 新增测试方法

```rust
// 仅在 #[cfg(test)] 或 feature = "headless" 下编译
impl App {
    /// 向内部事件队列注入 AgentEvent（测试用）
    pub fn push_agent_event(&mut self, event: AgentEvent);

    /// 批量处理队列中所有待处理事件（复用 poll_agent 逻辑）
    pub fn process_pending_events(&mut self);
}
```

### App::new_headless 构造

```rust
#[cfg(any(test, feature = "headless"))]
pub fn new_headless(width: u16, height: u16) -> (App, HeadlessHandle) {
    let backend = TestBackend::new(width, height);
    let terminal = Terminal::new(backend).unwrap();
    let (render_tx, render_cache, render_notify) = spawn_render_thread(width);
    let app = App {
        // 与 App::new() 相同的初始化，但不持有 terminal
        render_tx,
        render_cache,
        render_notify: Arc::clone(&render_notify),
        agent_event_queue: Vec::new(),
        // ... 其他字段默认值
    };
    let handle = HeadlessHandle { terminal, render_notify };
    (app, handle)
}
```

### 典型测试写法

```rust
#[tokio::test]
async fn test_assistant_message_renders() {
    let (mut app, mut handle) = App::new_headless(120, 30);

    // 注入流式消息事件
    app.push_agent_event(AgentEvent::AssistantChunk("Hello world".into()));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();

    // 等待渲染线程处理完成
    handle.wait_for_render().await;

    // 走完整 draw 路径
    app.draw(&mut handle.terminal);

    // 断言屏幕内容
    let snap = handle.snapshot();
    assert!(handle.contains("Agent"), "应显示 Agent 标头");
    assert!(handle.contains("Hello world"), "应显示消息内容");
}

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
    app.draw(&mut handle.terminal);

    assert!(handle.contains("read_file"));
}
```

### 同步策略

渲染线程是异步的，测试侧通过以下方式同步：

1. `wait_for_render()` — 首选方案，内部 `notify.notified().await`，渲染线程每次写入 cache 后发出通知，零轮询开销
2. 若渲染线程处理多个事件，可多次 `wait_for_render().await`
3. 不使用 `sleep`，避免 CI 不稳定

### Feature Flag 策略

- `#[cfg(test)]`：测试内直接可用，无需显式 feature
- `feature = "headless"`：可选，用于集成测试 binary 或 bench 场景
- Release 二进制中 `HeadlessHandle`、`new_headless()` 不编译，不影响产物大小

## 实现要点

1. **ratatui TestBackend**：`ratatui::backend::TestBackend` 是 ratatui 官方内置，无需额外依赖，`buffer()` 返回 `Buffer`（cell grid）
2. **snapshot() 实现**：遍历 `buffer.content`，按行分组，`cell.symbol()` 拼接，`trim_end()` 去尾空格
3. **process_pending_events() 复用**：直接复用 `poll_agent()` 中的 match 分支，不重复写事件处理逻辑——提取公共 `handle_agent_event(event: AgentEvent)` 方法
4. **draw() 解耦**：现有 `main_ui` 的 `draw()` 接受 `&mut Frame`，与 Backend 类型无关，天然支持 `TestBackend`
5. **agent_event_queue**：仅在测试模式下存在的字段，用 `#[cfg(test)]` 隔离，不污染生产结构体内存布局

## 约束一致性

本方案与项目架构约束的一致性说明：
- **不改动渲染管道**：`RenderTask`、`render_view_model`、`main_ui` 代码零改动，保持 F001 架构完整性
- **无新依赖**：`TestBackend` 来自 `ratatui`（已有依赖），无需引入新 crate
- **测试隔离**：headless 代码通过 `#[cfg(test)]` 隔离，不影响生产二进制

## 验收标准

- [ ] `App::new_headless(w, h)` 正确构造 App 和 HeadlessHandle，渲染线程正常启动
- [ ] `push_agent_event()` + `process_pending_events()` 与生产 `poll_agent()` 走相同代码路径
- [ ] `wait_for_render().await` 能可靠等待渲染线程处理完成，无 sleep
- [ ] `HeadlessHandle::snapshot()` 返回正确的文本行，无乱码
- [ ] `app.draw(&mut handle.terminal)` 调用走完整 `main_ui` draw 路径
- [ ] 至少包含以下集成测试用例：
  - [ ] AssistantChunk 流式消息渲染
  - [ ] ToolCall 工具块渲染
  - [ ] 用户消息渲染
  - [ ] Clear 后屏幕为空
- [ ] `#[cfg(test)]` 隔离生效：`cargo build --release` 不包含 headless 代码
- [ ] `cargo test -p rust-agent-tui` 全量通过
