# 移除 /split 多 Session 分屏功能 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 完全移除 TUI 中的多 session 分屏功能（/split 命令、Ctrl+N/P/W 快捷键、多列布局渲染），保留 SessionManager 结构但限制 len=1。/clear 仍支持（销毁旧 session + 创建新 ACP session）。

**Architecture:** 分 4 个任务执行：① 删除 SplitCommand 和注册 ② SessionManager 限制 len=1 + 删除多 session 操作方法 ③ UI 布局简化为纯单列 ④ 全局搜索替换 `sessions[active]` → `current()` 并删除多 session 事件处理 + 清理测试和文档。

**Tech Stack:** Rust, ratatui, tokio

**Spec:** `spec/issues/2026-06-01-remove-split-multi-session.md`

---

## File Structure

| 操作 | 文件 | 职责 |
|------|------|------|
| 删除 | `peri-tui/src/command/session/split.rs` | SplitCommand 定义 |
| 修改 | `peri-tui/src/command/session/mod.rs` | 移除 split 模块导出 |
| 修改 | `peri-tui/src/command/mod.rs` | 移除 SplitCommand 注册 |
| 修改 | `peri-tui/src/app/session_manager.rs` | 移除 session_areas，添加 len=1 断言 |
| 修改 | `peri-tui/src/app/mod.rs` | 重写 new_session（先删再建），删除 switch/close_session |
| 修改 | `peri-tui/src/ui/main_ui/mod.rs` | 删除多列布局分支，简化 render_session_column |
| 修改 | `peri-tui/src/event/keyboard/normal_keys.rs` | 移除 Ctrl+N/P/W 处理 |
| 修改 | `peri-tui/src/event/mod.rs` | 移除鼠标点击切换 session |
| 修改 | `peri-tui/src/ui/main_ui/status_bar.rs` | 移除 Ctrl+N/P/W 提示 |
| 修改 | `peri-tui/src/event/keyboard/normal_keys.rs` | 移除 session_idx 保存/恢复（/split 不再存在） |
| 修改 | `peri-acp/src/dispatch/commands.rs` | 移除 "split" AvailableCommand |
| 修改 | `peri-tui/locales/en/main.ftl` | 移除 command-split-description |
| 修改 | `peri-tui/locales/zh-CN/main.ftl` | 移除 command-split-description |
| 修改 | `peri-tui/src/ui/headless_test.rs` | 删除 split_panel_tests 模块 |
| 修改 | `CLAUDE.md` | 更新中间件链/TUI 描述（移除 split 相关） |
| 修改 | `README.md` | 移除 "Built-in split screen" |
| 修改 | `TUI-STYLE.md` | 移除 /split 条目 |
| 修改 | ~70 个文件 | `sessions[active]` → `current()` 简化（机械替换） |

---

### Task 1: 删除 SplitCommand 命令

**Files:**
- 删除: `peri-tui/src/command/session/split.rs`
- 修改: `peri-tui/src/command/session/mod.rs:10,20`
- 修改: `peri-tui/src/command/mod.rs:26`
- 修改: `peri-acp/src/dispatch/commands.rs:31`
- 修改: `peri-tui/locales/en/main.ftl:35`
- 修改: `peri-tui/locales/zh-CN/main.ftl:34`

- [ ] **Step 1: 删除 SplitCommand 文件**

```bash
rm peri-tui/src/command/session/split.rs
```

- [ ] **Step 2: 从 session/mod.rs 移除 split 模块**

`peri-tui/src/command/session/mod.rs` — 移除以下两行：
```rust
pub mod split;       // 第 10 行
pub use split::SplitCommand;  // 第 20 行
```

- [ ] **Step 3: 从命令注册表移除 SplitCommand**

`peri-tui/src/command/mod.rs` — 移除第 26 行：
```rust
r.register(Box::new(session::split::SplitCommand));
```

- [ ] **Step 4: 从 ACP AvailableCommands 移除 split**

`peri-acp/src/dispatch/commands.rs` — 移除第 31 行：
```rust
AvailableCommand::new("split", "Manage split session layouts"),
```

- [ ] **Step 5: 移除 i18n 翻译**

`peri-tui/locales/en/main.ftl` — 移除：
```
command-split-description = Create a new split session
```

`peri-tui/locales/zh-CN/main.ftl` — 移除：
```
command-split-description = 新建分栏会话
```

- [ ] **Step 6: 构建验证**

Run: `cargo build -p peri-tui -p peri-acp`
Expected: 编译成功（无 unresolved import 错误）

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "refactor: remove /split command and registrations"
```

---

### Task 2: SessionManager 限制 len=1 + 重写 new_session

**Files:**
- 修改: `peri-tui/src/app/session_manager.rs`
- 修改: `peri-tui/src/app/mod.rs:295-371`

- [ ] **Step 1: 简化 SessionManager**

`peri-tui/src/app/session_manager.rs` — 替换为：

```rust
use super::ChatSession;

/// 会话管理器：管理单个 ChatSession 实例（保留结构以便将来扩展）。
pub struct SessionManager {
    session: ChatSession,
}

impl SessionManager {
    pub fn new(initial_session: ChatSession) -> Self {
        Self {
            session: initial_session,
        }
    }

    pub fn current(&self) -> &ChatSession {
        &self.session
    }

    pub fn current_mut(&mut self) -> &mut ChatSession {
        &mut self.session
    }

    /// 替换当前 session（用于 /clear 新建对话）
    pub fn replace(&mut self, new_session: ChatSession) {
        self.session = new_session;
    }
}
```

注意：删除 `session_areas: Vec<Rect>` 字段、`active: usize` 字段、`session_at`/`session_at_mut`/`len`/`is_empty` 方法。

- [ ] **Step 2: 重写 App 中的 session 方法**

`peri-tui/src/app/mod.rs` — 修改 3 个方法：

**2a. 重写 `new_session`（先销毁旧 session 再创建新的，用于 /clear 内部调用）：**
```rust
pub fn new_session(&mut self) {
    // 取消旧 session 的 agent
    if let Some(token) = &self.session_mgr.current().agent.cancel_token {
        token.cancel();
    }
    let diff_visible = self.session_mgr.current().ui.diff_visible;
    let mut command_registry = crate::command::default_registry();
    let mut skills = {
        let mut dirs = Vec::new();
        if let Some(home) = dirs_next::home_dir() {
            dirs.push(home.join(".claude").join("skills"));
        }
        if let Some(global_dir) = peri_middlewares::skills::load_global_skills_dir() {
            dirs.push(global_dir);
        }
        if let Ok(cwd) = std::env::current_dir() {
            dirs.push(cwd.join(".claude").join("skills"));
        }
        peri_middlewares::skills::list_skills(&dirs)
    };
    if let Some(pd) = &self.services.plugin_data {
        let plugin_skills = peri_middlewares::skills::list_skills(&pd.all_skill_dirs);
        let existing_names: std::collections::HashSet<String> =
            skills.iter().map(|s| s.name.clone()).collect();
        for skill in plugin_skills {
            if !existing_names.contains(&skill.name) {
                skills.push(skill);
            }
        }
        command_registry.register_plugin_commands(pd.all_commands.clone());
    }
    let session = ChatSession::new(
        self.services.cwd.clone(),
        command_registry,
        skills,
        &self.services.lc,
        diff_visible,
    );
    self.session_mgr.replace(session);
}
```

**2b. 删除 `close_session` 方法**（第 336-351 行）

**2c. 删除 `switch_next_session` 和 `switch_prev_session` 方法**（第 354-371 行）

- [ ] **Step 3: 构建验证**

Run: `cargo build -p peri-tui 2>&1 | head -50`
Expected: 大量编译错误（因 `sessions` 字段不再存在），这正常。后续 Task 4 会批量修复。但 session_manager.rs 和 app/mod.rs 中的这 3 个方法应无内部错误。

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "refactor: simplify SessionManager to single session"
```

---

### Task 3: UI 布局简化为纯单列

**Files:**
- 修改: `peri-tui/src/ui/main_ui/mod.rs`
- 修改: `peri-tui/src/ui/main_ui/status_bar.rs:411-424`

- [ ] **Step 1: 简化 render 函数**

`peri-tui/src/ui/main_ui/mod.rs` — 将整个 `render` 函数（第 20-87 行）替换为：

```rust
pub fn render(f: &mut Frame, app: &mut App) {
    // Setup 向导：全屏覆盖，优先于所有正常界面
    if app.global_ui.setup_wizard.is_some() {
        popups::setup_wizard::render_setup_wizard(f, app);
        return;
    }

    let area = f.area();
    render_session_column(f, app, area);
}
```

- [ ] **Step 2: 简化 render_session_column 签名和实现**

将函数签名从 `fn render_session_column(f, app, session_idx, area, is_active)` 改为 `fn render_session_column(f, app, area)`。

在函数内部：
1. **删除** `prev_active` 保存/恢复逻辑（第 98、412 行）
2. **删除** 多 session 边框渲染块（第 102-116 行的 `if sessions.len() > 1` 分支），只保留 `area` 直接使用
3. **将** `app.session_mgr.sessions[session_idx]` **全部替换为** `app.session_mgr.current()`（此步骤在 Task 4 批量执行，此处先改签名和删除多列逻辑）
4. **删除** `status_bar_height` 中 `if sessions.len() > 1 { 0 } else { 3 }` 分支，直接 `let status_bar_height = 3;`（第 164-168 行）
5. **删除** 第 396-409 行的 `if sessions.len() == 1` 条件判断（已恒为 true），直接渲染 status_bar 和 bg_agent_bar
6. **删除** 第 348-360 行中 `!is_active` 的光标隐藏逻辑（恒为 active）
7. **删除** 第 373 行 `!is_active` 的 prompt 颜色判断（恒为 active）

简化后的关键变化：
```rust
fn render_session_column(f: &mut Frame, app: &mut App, area: Rect) {
    // 直接使用 area，不再切分多列
    let line_count = app.session_mgr.current().ui.textarea.lines().len() as u16;
    let input_height = (line_count + 2).min(area.height * 2 / 5).max(3);
    // ... 其余计算使用 app.session_mgr.current() ...

    let status_bar_height: u16 = 3; // 不再区分多 session
    // ...

    // 输入框：不再有 is_active 判断
    let bar_focused = app.session_mgr.current().ui.bg_bar_cursor.is_some();
    // ...

    // 光标：始终可见（不再检查 is_active）
    f.render_widget(&app.session_mgr.current().ui.textarea, chunks[5]);

    // 状态栏：始终渲染
    status_bar::render_status_bar(f, app, chunks[6]);
    if bg_bar_height_val > 0 {
        bg_agent_bar::render_bg_agent_bar(f, app, chunks[7]);
        app.session_mgr.current_mut().ui.bg_bar_area = Some(chunks[7]);
    } else {
        app.session_mgr.current_mut().ui.bg_bar_area = None;
    }
}
```

注意：由于 `sessions[session_idx]` 引用极多（此文件约 48 处），建议先完成签名和布局结构简化，将 `sessions[session_idx]` 暂改为 `current()` 的同时确保编译通过。实际替换在 Task 4 用全局搜索批量完成。

- [ ] **Step 3: 移除状态栏 Ctrl+N/P/W 提示**

`peri-tui/src/ui/main_ui/status_bar.rs` — 找到 `format_hints` 函数中多 session 分支（约第 411-424 行），删除 `if app.session_mgr.sessions.len() > 1` 分支，只保留单 session 的 hints。

- [ ] **Step 4: 构建验证**

Run: `cargo build -p peri-tui 2>&1 | head -80`
Expected: 可能仍有编译错误（因 sessions 字段不存在），但 ui/main_ui/mod.rs 内部结构应正确

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "refactor: simplify main_ui to single-column layout"
```

---

### Task 4: 全局搜索替换 + 事件处理清理 + 测试清理

这是最大的任务——批量将 `session_mgr.sessions[session_mgr.active]` 替换为 `session_mgr.current()`，删除多 session 事件处理，清理测试。

**Files:**
- 修改: ~70 个文件（机械替换 `sessions[...]` → `current()`/`current_mut()`）
- 修改: `peri-tui/src/event/keyboard/normal_keys.rs`（Ctrl+N/P/W + session_idx 保存）
- 修改: `peri-tui/src/event/mod.rs`（鼠标点击切换 session）
- 修改: `peri-tui/src/event/macros.rs`（with_session_panels! 简化）
- 修改: `peri-tui/src/ui/headless_test.rs`（删除 split_panel_tests）
- 修改: `peri-tui/src/event/mouse.rs`（session_areas 引用）

- [ ] **Step 1: 批量替换 `sessions[session_mgr.active]` → `current()` / `current_mut()`**

使用全局搜索替换。注意区分不可变和可变上下文：

**只读访问**（`&` 场景）→ `session_mgr.current()`
**写入访问**（赋值、方法调用修改字段）→ `session_mgr.current_mut()`

关键替换模式：
```
app.session_mgr.sessions[app.session_mgr.active]        → app.session_mgr.current() 或 current_mut()
app.session_mgr.sessions[session_idx]                    → app.session_mgr.current() 或 current_mut()
app.session_mgr.sessions[self.session_mgr.active]        → self.session_mgr.current() 或 current_mut()
self.session_mgr.sessions[self.session_mgr.active]       → self.session_mgr.current() 或 current_mut()
```

> **注意**：由于 `current()` 返回 `&ChatSession` 而 `current_mut()` 返回 `&mut ChatSession`，需要根据上下文判断。如果一个函数中既有读又有写，应先写后读，或使用 `current_mut()` 同时满足。

> **特殊文件**：`render_session_column` 在 Task 3 已处理签名简化，此处确认所有 `sessions[session_idx]` 都已替换。

- [ ] **Step 2: 删除 Ctrl+N/P/W 快捷键处理**

`peri-tui/src/event/keyboard/normal_keys.rs` — 删除第 299-330 行的 Ctrl+N/P/W 匹配分支：
```rust
// Ctrl+N/P: cycle session focus — 删除
// Ctrl+W: close current session — 删除
```

Ctrl+W 空出来后可作为其他用途（或直接忽略）。

- [ ] **Step 3: 移除命令 dispatch 中的 session_idx 保存/恢复**

`peri-tui/src/event/keyboard/normal_keys.rs` — 简化 `/` 命令 dispatch 路径（约第 153-168 行）。原来需要保存 `session_idx` 是因为 `/split` 会改变 `active`，现在不再需要：

```rust
// 替换前:
let session_idx = app.session_mgr.active;
let registry = std::mem::take(&mut app.session_mgr.sessions[session_idx].commands.command_registry);
let known = registry.dispatch(app, &text);
app.session_mgr.sessions[session_idx].commands.command_registry = registry;

// 替换后:
let registry = std::mem::take(&mut app.session_mgr.current_mut().commands.command_registry);
let known = registry.dispatch(app, &text);
app.session_mgr.current_mut().commands.command_registry = registry;
```

- [ ] **Step 4: 删除鼠标点击切换 session**

`peri-tui/src/event/mod.rs` — 删除约第 537-550 行的多 session 鼠标点击处理：
```rust
// Multi-session: clicking a non-active session column switches focus — 整段删除
if app.session_mgr.sessions.len() > 1 {
    for (i, area) in app.session_mgr.session_areas.iter().enumerate() {
        // ...
    }
}
```

- [ ] **Step 5: 简化 event/macros.rs**

`peri-tui/src/event/macros.rs` — `with_session_panels!` 宏简化，移除 `active_idx` 保存/恢复逻辑：

```rust
#[macro_export]
macro_rules! with_session_panels {
    ($app:expr, |$sp:ident, $ctx:ident| $body:expr) => {{
        let mut $sp = std::mem::take(&mut $app.session_mgr.current_mut().session_panels);
        let mut $ctx = $crate::app::panel_manager::PanelContext {
            services: &mut $app.services,
            session_mgr: &mut $app.session_mgr,
            acp_client: $app.acp_client.clone(),
        };
        let result = { $body };
        $app.session_mgr.current_mut().session_panels = $sp;
        result
    }};
}
```

- [ ] **Step 6: 删除 headless_test 中的 split_panel_tests 模块**

`peri-tui/src/ui/headless_test.rs` — 删除 `split_panel_tests` 模块（约第 2456-2750 行），包含以下测试函数：
- `test_split_session_hint_shows_for_both_columns`
- `test_split_session_both_have_slash_hint_shows_on_both`
- `test_split_session_left_inactive_shows_model_with_m_prefix`
- `test_split_command_preserves_session0_command_registry`
- `test_split_session_panel_independence`
- `test_split_session_global_panel_closes_all_session_panels`

同时检查其他测试中的 `app.new_session()` 调用，如果用于测试多 session 场景则删除；如果用于测试 /clear 场景则保留。

- [ ] **Step 7: 清理 mouse.rs 中的 session_areas 引用**

`peri-tui/src/event/mouse.rs` — 搜索 `session_areas` 引用并删除。鼠标事件处理中的 `sessions[active]` 替换为 `current()`。

- [ ] **Step 8: 构建验证**

Run: `cargo build -p peri-tui`
Expected: 编译成功，无错误

- [ ] **Step 9: 运行测试**

Run: `cargo test -p peri-tui --lib 2>&1 | tail -30`
Expected: 所有测试通过（删除的测试不再存在，其他测试不受影响）

- [ ] **Step 10: Commit**

```bash
git add -A && git commit -m "refactor: replace sessions[active] with current(), remove multi-session events"
```

---

### Task 5: 清理文档和注释

**Files:**
- 修改: `CLAUDE.md`（更新多 session 相关描述）
- 修改: `README.md:24`（移除 "Built-in split screen"）
- 修改: `TUI-STYLE.md:514`（移除 /split 条目）
- 修改: `peri-tui/src/event/keyboard/normal_keys.rs`（删除 `/split` 相关注释）

- [ ] **Step 1: 更新 CLAUDE.md**

搜索 `split` 相关段落，移除：
- 中间件链描述中的 `/split` 相关说明
- `[TRAP]` 中关于 `session_mgr.active` 竞态的描述（不再有 `/split` 改变 active 的风险）
- TUI 描述中"多 Session 水平分栏"的说明，改为"单 Session 垂直布局"

- [ ] **Step 2: 更新 README.md**

移除第 24 行：
```
  - Built-in split screen.
```

- [ ] **Step 3: 更新 TUI-STYLE.md**

移除命令表中 `/split` 条目。

- [ ] **Step 4: 清理代码注释**

`normal_keys.rs` 第 158 行注释：
```rust
// (e.g. /split) may change app.session_mgr.active
```
此注释已无意义，删除。

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "docs: remove /split references from documentation"
```

---

### Task 6: 最终验证

- [ ] **Step 1: 全量构建**

Run: `cargo build`
Expected: 全部 crate 编译成功

- [ ] **Step 2: 全量测试**

Run: `cargo test 2>&1 | tail -30`
Expected: 所有测试通过

- [ ] **Step 3: Lint 检查**

Run: `cargo clippy --workspace 2>&1 | grep -E "warning|error" | head -20`
Expected: 无新增 warning（可能有既有的，确认无新增即可）

- [ ] **Step 4: 搜索残留引用**

Run: `grep -rn "session_areas\|switch_next_session\|switch_prev_session\|close_session\|SplitCommand\|split_panel_tests" peri-tui/src/`
Expected: 无结果（所有引用已清理）

Run: `grep -rn "sessions\[" peri-tui/src/ | grep -v "test\|//"`
Expected: 无结果（所有 `sessions[idx]` 已替换为 `current()`）

- [ ] **Step 5: 手动运行验证**

Run: `cargo run -p peri-tui`
Expected: TUI 正常启动，单 session 布局，/clear 创建新对话正常工作，Ctrl+N/P/W 无响应（无报错）

- [ ] **Step 6: Final commit (if any fixes)**

```bash
git add -A && git commit -m "fix: cleanup after split removal"
```
