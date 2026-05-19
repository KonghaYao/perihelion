# Event 模块三维拆分

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** 将 1447 行的 `event.rs` 拆分为 `event/` 目录：`mod.rs`（入口+分发）、`keyboard.rs`（键盘快捷键） 、`mouse.rs`（鼠标+剪贴板辅助函数）

**Architecture:** 键盘处理与鼠标处理无耦合——各自独立为子模块。`handle_event` 退化为纯分发函数（Focus/Resize 处理 + 委托 Key→keyboard, Mouse→mouse）。两个内部宏保留在 mod.rs（被 keyboard 和 mouse 共用）。外部 API 不变：`Action` enum + `next_event()` 仍由 mod.rs 导出。

**Tech Stack:** Rust 2021, crossterm, tui-textarea, arboard

---

### Task 1: 创建 `event/` 目录结构

**Files:**
- Create: `peri-tui/src/event/`
- Create: `peri-tui/src/event/mod.rs`
- Create: `peri-tui/src/event/keyboard.rs`
- Create: `peri-tui/src/event/mouse.rs`

- [ ] **Step 1: Create directory and empty files**

```bash
mkdir -p peri-tui/src/event
touch peri-tui/src/event/mod.rs
touch peri-tui/src/event/keyboard.rs
touch peri-tui/src/event/mouse.rs
```

---

### Task 2: 移动鼠标辅助函数到 `mouse.rs`

**Files:**
- Create: `peri-tui/src/event/mouse.rs` (完整文件)
- Copy from: `peri-tui/src/event.rs:45-212`

- [ ] **Step 1: Write `mouse.rs` — extract helpers from event.rs lines 45-212**

Copy these functions from `event.rs`:
- `mouse_in_rect` (lines 46-51)
- `display_col_to_char_idx` (lines 53-68)
- `textarea_mouse_to_cursor` (lines 83-149 — includes the large CJK comment block)
- `rgba_to_png_base64` (lines 106-149 if separate, or wherever it is)
- `copy_selection_to_clipboard` (lines 158-184)
- `copy_panel_selection_to_clipboard` (lines 187-212)

Write `peri-tui/src/event/mouse.rs`:

```rust
use arboard;
use base64::Engine as _;
use ratatui::layout::Rect;
use tui_textarea;

use crate::app::App;

/// 检查鼠标事件是否在指定矩形区域内
pub fn mouse_in_rect(mouse: &ratatui::crossterm::event::MouseEvent, area: Rect) -> bool {
    mouse.row >= area.y
        && mouse.row < area.y + area.height
        && mouse.column >= area.x
        && mouse.column < area.x + area.width
}

/// 将终端显示列坐标转换为字符串的字符索引
///
/// 终端中 CJK 等全角字符占 2 列宽，`mouse.column` 是终端列坐标，
/// 但 `CursorMove::Jump(row, col)` 的 `col` 是字符索引。
/// 此函数逐字符累加 `unicode_width`，找到不超过 `display_col` 的最大字符索引。
pub fn display_col_to_char_idx(line: &str, display_col: usize) -> usize {
    let mut col = 0usize;
    for (char_idx, ch) in line.chars().enumerate() {
        if col >= display_col {
            return char_idx;
        }
        col += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
    }
    line.chars().count()
}

/// 将鼠标在 textarea 区域内的坐标转换为 textarea 的 (row, char_idx)
///
/// 需要处理四个偏移：
/// 1. Block border + padding
/// 2. 垂直滚动偏移
/// 3. 水平滚动偏移
/// 4. CJK 字符宽度
pub fn textarea_mouse_to_cursor(
    textarea: &tui_textarea::TextArea<'_>,
    textarea_area: ratatui::layout::Rect,
    mouse: &ratatui::crossterm::event::MouseEvent,
) -> (usize, usize) {
    let inner = textarea
        .block()
        .map(|b| b.inner(textarea_area))
        .unwrap_or(textarea_area);
    let inner_width = inner.width as usize;

    let visual_row = mouse.row.saturating_sub(inner.y) as usize;
    let visual_col = mouse.column.saturating_sub(inner.x) as usize;

    let (cursor_row, cursor_col) = textarea.cursor();
    let top_row = textarea
        .cursor()
        .0
        .saturating_sub(inner.height as usize - 1);
    let top_col = {
        let line = textarea.lines().get(cursor_row).map(|l| l.as_str()).unwrap_or("");
        let cursor_char_idx = cursor_col.min(line.chars().count());
        let cursor_pixels: usize = line
            .chars()
            .take(cursor_char_idx)
            .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
            .sum();
        cursor_pixels.saturating_sub(inner_width)
    };

    let text_row = top_row + visual_row;
    if text_row >= textarea.lines().len() {
        return (textarea.lines().len().saturating_sub(1), 0);
    }
    let line = textarea.lines()[text_row].as_str();
    let char_idx = display_col_to_char_idx(line, top_col + visual_col);
    (text_row, char_idx)
}

/// 将 RGBA 像素缓冲区编码为 PNG base64 字符串
pub fn rgba_to_png_base64(width: u32, height: u32, data: &[u8]) -> Option<String> {
    use std::io::Cursor;
    let mut png_data = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut png_data), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(data).ok()?;
        writer.finish().ok()?;
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_data);
    Some(format!("data:image/png;base64,{b64}"))
}

/// 将选区文本复制到系统剪贴板并更新 UI 提示。返回 true 表示成功复制。
pub fn copy_selection_to_clipboard(app: &mut App) -> bool {
    if let Some(text) = app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .text_selection
        .selected_text
        .take()
    {
        let char_count = text.chars().count();
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(&text);
        }
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .copy_char_count = char_count;
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .copy_message_until =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(2000));
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .text_selection
            .clear();
        return true;
    }
    false
}

/// 将面板选中文本复制到系统剪贴板。返回 true 表示成功复制。
pub fn copy_panel_selection_to_clipboard(app: &mut App) -> bool {
    let selection = app
        .session_mgr
        .sessions
        .get(app.session_mgr.active)
        .and_then(|s| s.ui.panel_selected_text.as_ref())
        .cloned();

    if let Some(text) = selection {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(&text);
        }
        return true;
    }
    false
}
```

- [ ] **Step 2: Remove these functions from `event.rs`**

Delete lines 45-212 from `event.rs` (mouse helpers + clipboard functions).

- [ ] **Step 3: Build to verify**

```bash
cargo build -p peri-tui 2>&1 | head -30
```

Expected: FAIL — `event.rs` still calls removed functions (will be fixed in next task).

---

### Task 3: 移动键盘处理到 `keyboard.rs`

**Files:**
- Create: `peri-tui/src/event/keyboard.rs` (完整文件)
- Copy from: `peri-tui/src/event.rs:271-1056` (Event::Key arm)

- [ ] **Step 1: Write `keyboard.rs`**

Extract the entire `Event::Key` handling block from `event.rs` (lines 271-1056) into a function `pub fn handle_key_event(app: &mut App, key: ratatui::crossterm::event::KeyEvent) -> Result<Option<Action>>`.

```rust
use anyhow::Result;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tui_textarea::{Input, Key};

use crate::app::panel_manager::EventResult;
use crate::app::{App, PendingAttachment};
use super::Action;

pub fn handle_key_event(
    app: &mut App,
    key_event: KeyEvent,
) -> Result<Option<Action>> {
    // Only handle Press events, ignore Release
    if key_event.kind == KeyEventKind::Release {
        return Ok(Some(Action::Redraw));
    }

    // Shift+Tab → cycle permission mode
    if matches!(key_event.code, KeyCode::BackTab) {
        app.session_mgr.sessions[app.session_mgr.active]
            .mode = app.session_mgr.sessions[app.session_mgr.active].mode.next();
        return Ok(Some(Action::Redraw));
    }

    // Alt+M / µ → cycle model
    if key_event.modifiers.contains(KeyModifiers::ALT)
        && matches!(key_event.code, KeyCode::Char('m') | KeyCode::Char('µ'))
    {
        // ... (copy entire Alt+M handler)
    }

    // Loading state: Ctrl+C, Esc → interrupt/cancel
    if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
        // ... (copy loading handlers)
    }

    // Build Input struct from key_event
    let input = Input::from(key_event);

    // Dispatch input
    match input {
        // ... (copy ALL keyboard match arms from event.rs lines ~603-1056) ...
        _ => {
            app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .textarea
                .input(input);
        }
    }

    Ok(Some(Action::Redraw))
}
```

**Critical:** The `with_global_panels!` and `with_session_panels!` macros are used inside keyboard handling. These macros must be available — they're defined in `mod.rs`. Add `use super::with_global_panels;` and `use super::with_session_panels;` reference.

- [ ] **Step 2: Delete keyboard handling from `event.rs`**

Remove lines 271-1056 from `event.rs`.

- [ ] **Step 3: Build to verify**

```bash
cargo build -p peri-tui 2>&1 | head -30
```

Expected: FAIL — references to `mouse_in_rect` etc. still unresolved (fixed in Task 4).

---

### Task 4: 在 `mod.rs` 中重组 handle_event 为纯分发函数

**Files:**
- Create: `peri-tui/src/event/mod.rs`

- [ ] **Step 1: Write `mod.rs` — Action enum + macros + next_event + dispatch-only handle_event**

```rust
pub mod keyboard;
pub mod mouse;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, EventKind, MouseEventKind};
use std::time::Duration;

use crate::app::App;

/// Internal macros used by keyboard and mouse handlers.
/// These are macro_rules! with file scope; re-export via public function wrappers
/// so sub-modules can use them.
#[macro_use]
pub mod macros {
    macro_rules! with_global_panels {
        ($app:expr, |$pm:ident, $ctx:ident| $body:expr) => {{
            let mut $pm = std::mem::take(&mut $app.global_panels);
            let mut $ctx = $crate::app::panel_manager::PanelContext {
                services: &mut $app.services,
                session_mgr: &mut $app.session_mgr,
            };
            let result = { $body };
            $app.global_panels = $pm;
            result
        }};
    }

    macro_rules! with_session_panels {
        ($app:expr, |$sp:ident, $ctx:ident| $body:expr) => {{
            let active_idx = $app.session_mgr.active;
            let mut $sp = std::mem::take(&mut $app.session_mgr.sessions[active_idx].session_panels);
            let mut $ctx = $crate::app::panel_manager::PanelContext {
                services: &mut $app.services,
                session_mgr: &mut $app.session_mgr,
            };
            let result = { $body };
            $app.session_mgr.sessions[active_idx].session_panels = $sp;
            result
        }};
    }
}

pub enum Action {
    Quit,
    Submit(String),
    Redraw,
}

/// Entry point for the main event loop.
/// On first call, probes terminal for pending mouse events to detect the
/// mouse-support capability. Subsequent calls use event::poll → event::read.
pub async fn next_event(app: &mut App) -> Result<Option<Action>> {
    if !app.mouse_probe_done {
        app.mouse_probe_done = true;
        if event::poll(Duration::from_millis(50))? {
            if let Ok(Event::Mouse(_)) = event::read() {
                app.mouse_supported = true;
            }
        }
    }

    if event::poll(Duration::from_millis(50))? {
        let ev = event::read()?;
        handle_event(app, ev).await
    } else {
        Ok(None)
    }
}

/// Dispatch-only event handler. Focus/Resize handled inline,
/// Key → keyboard::handle_key_event, Mouse → inline handler.
async fn handle_event(app: &mut App, ev: Event) -> Result<Option<Action>> {
    match ev {
        Event::FocusGained => {
            app.focused = true;
            Ok(Some(Action::Redraw))
        }
        Event::FocusLost => {
            app.focused = false;
            Ok(Some(Action::Redraw))
        }
        Event::Resize(_, _) => {
            app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .text_selection
                .clear();
            Ok(Some(Action::Redraw))
        }
        Event::Key(key_event) => keyboard::handle_key_event(app, key_event),

        Event::Paste(text) => {
            // ... (copy paste handler from event.rs, ~30 lines) ...
            Ok(Some(Action::Redraw))
        }

        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => {
                // ... (copy scroll handler) ...
                Ok(Some(Action::Redraw))
            }
            MouseEventKind::ScrollDown => {
                // ... (copy scroll handler) ...
                Ok(Some(Action::Redraw))
            }
            MouseEventKind::Down(..) => {
                // ... (copy mouse click handler using mouse::* helpers) ...
                Ok(Some(Action::Redraw))
            }
            MouseEventKind::Drag(..) => {
                // ... (copy mouse drag handler) ...
                Ok(Some(Action::Redraw))
            }
            MouseEventKind::Up(..) => {
                // ... (copy mouse release handler) ...
                Ok(Some(Action::Redraw))
            }
            _ => Ok(Some(Action::Redraw)),
        },
    }
}

/// OAuth prompt input handling.
pub fn handle_oauth_prompt(app: &mut App, input: tui_textarea::Input) {
    // ... (copy from event.rs lines 1405-1447) ...
}
```

**Critical:** The mouse event handlers in `mod.rs` use `mouse::mouse_in_rect()`, `mouse::copy_selection_to_clipboard()`, etc. The keyboard handler in `keyboard.rs` uses `with_session_panels!` macro.

- [ ] **Step 2: Delete old `event.rs`**

```bash
rm peri-tui/src/event.rs
```

- [ ] **Step 3: Build**

```bash
cargo build -p peri-tui 2>&1 | head -30
```

Expected: compilation succeeds.

---

### Task 5: Update `main.rs` import

**Files:**
- Modify: `peri-tui/src/main.rs` (or wherever `use crate::event` is)

- [ ] **Step 1: Verify import path still works**

`use crate::event::Action;` and `crate::event::next_event()` will resolve to `event/mod.rs` — no change needed since we're using `pub mod event;` with the directory pattern.

```bash
cargo build -p peri-tui 2>&1 | head -10
```

Expected: compilation succeeds.

---

### Task 6: Run full tests and commit

- [ ] **Step 1: Run all tests**

```bash
cargo test -p peri-tui --lib 2>&1 | tail -10
```

Expected: All tests pass.

- [ ] **Step 2: Run pre-commit**

```bash
lefthook run pre-commit
```

- [ ] **Step 3: Commit**

```bash
cargo fmt -p peri-tui
git add peri-tui/src/event/ peri-tui/src/event.rs
git commit -m "refactor: split event.rs into keyboard and mouse submodules

- Extract mouse coordinate helpers + clipboard into event/mouse.rs
- Extract keyboard shortcut handling into event/keyboard.rs
- Keep Action enum + next_event + dispatch in event/mod.rs

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```
