# Feature: 20260325_F002 - large-file-refactor

## 需求背景

`rust-agent-tui` 中两个文件已过度膨胀，严重影响可维护性：

- **`src/app/mod.rs`（1931 行）**：混合了 AgentEvent、HitlBatchPrompt、QuestionState、AskUserBatchPrompt 等类型定义，App 结构体声明，以及跨越 agent 事件处理、HITL、AskUser、线程管理、面板管理等多个功能域的 impl 方法块。其中 `handle_agent_event` 单函数超过 350 行，`submit_message` 约 150 行。
- **`src/ui/main_ui.rs`（1305 行）**：将标题栏、消息列表、状态栏、所有弹窗（HITL/AskUser/Command/Skill）、所有面板（Model/ThreadBrowser/Agent）的渲染函数全部集中在一个文件里。`render_model_panel` 单函数约 330 行。

修改某一功能需要跨越数百行无关代码，定位修改点困难。

## 目标

- 将 `app/mod.rs` 拆分为 8 个文件（含已有子文件），单新建文件不超过 600 行
- 将 `ui/main_ui.rs` 拆分为 8 个文件（含 2 个新子目录），单文件不超过 350 行
- 不改变任何外部 API——所有 `pub` 类型和方法的签名、调用路径保持不变
- 拆分完成后所有测试（包括 headless 测试）必须全部通过

## 方案设计

### 核心技术机制

Rust 允许同一模块（`app/`）内的不同文件各自包含独立的 `impl App {}` 块——通过 `mod.rs` 中 `mod <file>;` 声明将各文件组合为同一模块，各文件对 App 的 impl 块共享模块命名空间。这是本次拆分的核心支撑，无需 trait 抽象即可将方法分散到多个文件。

### app/mod.rs 拆分方案

![app 模块拆分结构](./images/01-app-module-split.png)

#### 拆分后目录结构

```
rust-agent-tui/src/app/
├── mod.rs              (~400 行)  App struct 定义 + new() + poll_relay + 基础读写方法 + 模块声明
├── agent.rs            (已有)     run_universal_agent()
├── agent_panel.rs      (已有)     AgentPanel
├── hitl.rs             (已有)     hitl_confirm 相关类型
├── model_panel.rs      (已有)     ModelPanel
├── provider.rs         (已有，私有)
├── tool_display.rs     (已有)
│
├── hitl_prompt.rs      (新建 ~100行)   HitlBatchPrompt struct + impl
├── ask_user_prompt.rs  (新建 ~150行)   QuestionState + AskUserBatchPrompt + impl
├── agent_ops.rs        (新建 ~560行)   impl App: submit_message + handle_agent_event + poll_agent
├── hitl_ops.rs         (新建 ~70行)    impl App: hitl_move/toggle/approve_all/reject_all/confirm
├── ask_user_ops.rs     (新建 ~75行)    impl App: ask_user_next_tab ~ ask_user_confirm
├── thread_ops.rs       (新建 ~200行)   impl App: open_thread + new_thread + start_compact + scroll + attachments
└── panel_ops.rs        (新建 ~250行)   impl App: model_panel_* + agent_panel_* + open/close + headless 辅助
```

#### 新文件内容说明

| 文件 | 迁移内容（行号对应原 mod.rs） |
|------|------------------------------|
| `hitl_prompt.rs` | `HitlBatchPrompt` struct（83-152）、`PendingAttachment`（69-82）可酌情一并移入 |
| `ask_user_prompt.rs` | `QuestionState`（153-235）、`AskUserBatchPrompt`（236-307） |
| `agent_ops.rs` | `submit_message`（818-968）、`handle_agent_event`（969-1326）、`poll_agent`（1327-1372） |
| `hitl_ops.rs` | `hitl_move`（1373）~ `hitl_confirm`（1433） |
| `ask_user_ops.rs` | `ask_user_next_tab`（1435）~ `ask_user_confirm`（1509） |
| `thread_ops.rs` | `ensure_thread_id`（680）、scroll（698-717）、`add_pending_attachment`（1510-1521）、`open_thread`（1522）~ `open_thread_browser`（1633） |
| `panel_ops.rs` | `open_model_panel`（1634）~ `agent_panel_clear`（1711）、`model_panel_*`（1716-1843）、`push_agent_event`（1845）~ `new_headless`（1931） |

#### mod.rs 的 pub use 重导出策略

类型从新文件迁移后，在 `mod.rs` 中补充重导出，保持外部调用路径不变：

```rust
// app/mod.rs 新增声明
mod hitl_prompt;
mod ask_user_prompt;
mod agent_ops;
mod hitl_ops;
mod ask_user_ops;
mod thread_ops;
mod panel_ops;

// 重导出，保持外部路径不变
pub use hitl_prompt::{HitlBatchPrompt, PendingAttachment};
pub use ask_user_prompt::{QuestionState, AskUserBatchPrompt};
```

#### 各 impl 文件头部模板

```rust
// app/agent_ops.rs 示例
use super::*;  // 或具体 use 导入所需类型

impl App {
    pub fn submit_message(&mut self, input: String) {
        // 代码原封不动搬移
    }
    // ...
}
```

### ui/main_ui.rs 拆分方案

![ui 模块拆分结构](./images/02-ui-module-split.png)

#### 拆分后目录结构

```
rust-agent-tui/src/ui/
├── main_ui.rs          (~250 行)  render() 主函数 + 布局 + render_title/messages/attachment_bar/todo_panel
├── message_view.rs     (已有)
├── message_render.rs   (已有)
├── render_thread.rs    (已有)
├── headless.rs         (已有)
│
├── status_bar.rs       (新建 ~130行)  render_status_bar(pub(super)) + format_duration
│
├── popups/
│   ├── mod.rs          (新建 ~10行)   pub(super) mod hitl; pub(super) mod ask_user; pub(super) mod hints;
│   ├── hitl.rs         (新建 ~95行)   pub(super) fn render_hitl_popup(...)
│   ├── ask_user.rs     (新建 ~130行)  pub(super) fn render_ask_user_popup(...)
│   └── hints.rs        (新建 ~140行)  pub(super) fn render_command_hint + render_skill_hint
│
└── panels/
    ├── mod.rs          (新建 ~10行)   pub(super) mod model; pub(super) mod thread_browser; pub(super) mod agent;
    ├── model.rs        (新建 ~330行)  pub(super) fn render_model_panel(...) + fn mask_api_key（私有）
    ├── thread_browser.rs (新建 ~75行) pub(super) fn render_thread_browser(...)
    └── agent.rs        (新建 ~115行)  pub(super) fn render_agent_panel(...) + fn format_input_preview（私有）
```

#### 可见性约束

- 所有 `render_*` 函数均使用 `pub(super)` 可见性（仅对父模块 `main_ui.rs` 可见）
- `main_ui.rs` 中的 `pub fn render(...)` 保持 `pub`，是唯一对外入口
- `mask_api_key`、`format_input_preview`、`format_duration` 等辅助函数保持 `fn`（私有）

#### main_ui.rs 改动后调用骨架

```rust
// ui/main_ui.rs
mod status_bar;
pub mod popups;
pub mod panels;

pub fn render(f: &mut Frame, app: &mut App) {
    // 布局逻辑不变...
    render_title(f, app, title_area);
    render_messages(f, app, msg_area);
    status_bar::render_status_bar(f, app, status_area);

    if app.hitl_prompt.is_some() {
        popups::hitl::render_hitl_popup(f, app);
    }
    if app.ask_user_prompt.is_some() {
        popups::ask_user::render_ask_user_popup(f, app);
    }
    popups::hints::render_command_hint(f, app, input_area);
    popups::hints::render_skill_hint(f, app, input_area);

    if app.model_panel.is_some() {
        panels::model::render_model_panel(f, app);
    }
    panels::thread_browser::render_thread_browser(f, app);  // 条件判断在函数内
    panels::agent::render_agent_panel(f, app);
    // ...
}
```

### 执行顺序

1. **先拆 `app/mod.rs`**（改动更复杂，优先完成）
   - 每建一个新子文件后立即执行 `cargo build -p rust-agent-tui` 验证编译
   - 顺序：hitl_prompt.rs → ask_user_prompt.rs → hitl_ops.rs → ask_user_ops.rs → thread_ops.rs → panel_ops.rs → agent_ops.rs（最后，最大）
2. **再拆 `ui/main_ui.rs`**
   - 先建 `popups/` 和 `panels/` 目录及 `mod.rs`
   - 顺序：status_bar.rs → panels/model.rs（最大）→ panels/thread_browser.rs → panels/agent.rs → popups/hitl.rs → popups/ask_user.rs → popups/hints.rs
3. **最终验证**：`cargo test -p rust-agent-tui`

## 实现要点

1. **纯机械搬移，禁止顺手重构**：只移动代码，不改写逻辑、不重命名变量、不改接口签名，避免引入意外 bug。

2. **循环依赖检查**：`hitl_prompt.rs` / `ask_user_prompt.rs` 只依赖外部 crate（`rust-create-agent`、`tokio::sync::oneshot` 等），不依赖 App 类型，天然无循环依赖。

3. **AgentEvent 位置**：`AgentEvent` 枚举目前在 `mod.rs` 第 37 行，被 `agent_ops.rs` 和 TUI 事件循环大量使用，建议**保留在 `mod.rs`** 中（不单独提取），减少依赖传播。

4. **测试文件位置不变**：`headless.rs` 中的 `#[cfg(test)]` 测试位于 `src/ui/headless.rs`，保持不动。

5. **编译缓存友好**：每个新文件只包含相关逻辑，局部修改只重编译该文件。

## 约束一致性

| 约束 | 符合情况 |
|------|---------|
| Workspace 分层：禁止下层依赖上层 | ✅ 全部改动在 `rust-agent-tui` 内，无跨 crate 依赖变化 |
| 文件组织：每个模块一个目录，`mod.rs` 作为入口 | ✅ `popups/` 和 `panels/` 新建子目录均遵循此约定 |
| 测试：bin crate 集成测试在 `src/` 内 | ✅ `headless.rs` 位置不变 |
| 不引入新外部依赖 | ✅ 纯代码组织，无新 crate 引入 |
| 日志：使用 `tracing` 宏 | ✅ 不改变任何日志调用 |

## 验收标准

- [ ] `cargo build -p rust-agent-tui` 编译无错误、无新 clippy 警告
- [ ] `cargo test -p rust-agent-tui` 所有测试（含 headless 测试）全部通过
- [ ] `app/mod.rs` 行数 ≤ 450 行
- [ ] `ui/main_ui.rs` 行数 ≤ 300 行
- [ ] 单个新建文件行数均 ≤ 600 行
- [ ] 没有任何现有 `pub` 类型或函数的外部调用路径发生变化
- [ ] 新建文件均有对应的 `mod <name>;` 声明在父模块中
- [ ] `popups/` 和 `panels/` 目录各有 `mod.rs` 入口文件
