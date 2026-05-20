# Theme & Markdown Color Decoupling — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unify the three independent color systems (Theme trait, MarkdownTheme trait, TUI constants) and eliminate hardcoded color values scattered across the codebase.

**Architecture:** Create a `ThemeMarkdownAdapter` bridging `&dyn Theme` → `MarkdownTheme` so markdown rendering derives colors from `Theme`. Add diff/code-syntax colors to `Theme` trait. Align `DarkTheme::thinking()` with TUI `THINKING` (#A2A9E4). Fix spinner defaults and hardcoded arrow/SELECTED colors.

**Tech Stack:** Rust, ratatui style/Color, peri-widgets (Theme + MarkdownTheme traits), peri-tui (theme constants)

**Key Design Decisions:**
- `DarkTheme::thinking()` changes from `#AF87FF` → `#A2A9E4` (matches TUI `THINKING` and existing `DefaultMarkdownTheme::code()`)
- `ThemeMarkdownAdapter<'a>(pub &'a dyn Theme)` wraps `&dyn Theme` as a newtype (orphan rule prevents blanket `impl MarkdownTheme for &dyn Theme`)
- Markdown bridge mappings: heading→warning, text→text, muted→muted, code→thinking, link→success, code_prefix→success, quote_prefix→muted, list_bullet→text, separator→muted
- Diff colors exposed as Theme methods: `diff_add`, `diff_remove`, `diff_hunk`
- `highlight.rs` functions gain a `theme: &dyn Theme` parameter (not a separate trait — keeps it simple)
- TUI `parse_markdown` stays on `DefaultMarkdownTheme` (TUI already has its own color system matching `DefaultMarkdownTheme`; bridging TUI constants → `dyn Theme` is out of scope)
- `SELECTED #B2B9F9` in thread_browser becomes a TUI-level named constant `SELECTED_FG` with a comment tracing its semantic origin

---

### Task 1: Align `DarkTheme::thinking()` color with TUI `THINKING`

**Files:**
- Modify: `peri-widgets/src/theme/presets.rs:25-27`
- Modify: `peri-widgets/src/theme/presets_test.rs` (if needed)

- [ ] **Step 1: Change `DarkTheme::thinking()` from `#AF87FF` to `#A2A9E4`**

```rust
// peri-widgets/src/theme/presets.rs
fn thinking(&self) -> Color {
    Color::Rgb(162, 169, 228)
} // THINKING #A2A9E4（与 TUI theme::THINKING 一致）
```

- [ ] **Step 2: Verify the test still passes**

```bash
cargo test -p peri-widgets -- presets_test
```

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add peri-widgets/src/theme/presets.rs
git commit -m "fix: align DarkTheme::thinking() color with TUI THINKING (#A2A9E4)

DarkTheme::thinking() was #AF87FF but TUI theme::THINKING was #A2A9E4.
DefaultMarkdownTheme::code() also used #A2A9E4. Align on the value
actually used in practice.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 2: Create `ThemeMarkdownAdapter` to bridge `Theme` → `MarkdownTheme`

**Files:**
- Modify: `peri-widgets/src/markdown/mod.rs` — add adapter struct + impl

- [ ] **Step 1: Add `ThemeMarkdownAdapter` struct and `MarkdownTheme` impl**

Before the existing `DefaultMarkdownTheme` struct (after line 34 in mod.rs), insert:

```rust
/// 将 `Theme` trait 适配为 `MarkdownTheme`
///
/// 语义映射：
/// - heading → warning（标题用警告色，增强可读性）
/// - code → thinking（行内代码用思考色，蓝紫调）
/// - link → success（链接用成功色，绿色标识可点击）
/// - code_prefix → success
/// - quote_prefix → muted
/// - list_bullet → text
/// - separator → muted
pub struct ThemeMarkdownAdapter<'a>(pub &'a dyn crate::theme::Theme);

impl MarkdownTheme for ThemeMarkdownAdapter<'_> {
    fn heading(&self) -> Color {
        self.0.warning()
    }
    fn text(&self) -> Color {
        self.0.text()
    }
    fn muted(&self) -> Color {
        self.0.muted()
    }
    fn code(&self) -> Color {
        self.0.thinking()
    }
    fn link(&self) -> Color {
        self.0.success()
    }
    fn code_prefix(&self) -> Color {
        self.0.success()
    }
    fn quote_prefix(&self) -> Color {
        self.0.muted()
    }
    fn list_bullet(&self) -> Color {
        self.0.text()
    }
    fn separator(&self) -> Color {
        self.0.muted()
    }
}
```

- [ ] **Step 2: Build to verify it compiles**

```bash
cargo build -p peri-widgets
```

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add peri-widgets/src/markdown/mod.rs
git commit -m "feat: add ThemeMarkdownAdapter to bridge Theme → MarkdownTheme

Maps Theme semantic colors to MarkdownTheme rendering colors.
Uses newtype wrapper to avoid orphan rule conflicts.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 3: Add diff color methods to `Theme` trait

**Files:**
- Modify: `peri-widgets/src/theme/mod.rs` — add 3 new methods
- Modify: `peri-widgets/src/theme/presets.rs` — implement 3 new methods
- Modify: `peri-widgets/src/theme/presets_test.rs` — add test (if test file exists)

- [ ] **Step 1: Add `diff_add()`, `diff_remove()`, `diff_hunk()` to `Theme` trait defaults**

In `peri-widgets/src/theme/mod.rs`, after the `bash_border()` default method (line 59), add:

```rust
    // ── Diff 高亮色 ─────────────────────────────────────────
    /// Diff 新增行颜色
    fn diff_add(&self) -> Color {
        Color::Rgb(110, 181, 106)
    } // DIFF_ADD #6EB56A
    /// Diff 删除行颜色
    fn diff_remove(&self) -> Color {
        Color::Rgb(204, 70, 62)
    } // DIFF_REMOVE #CC463E
    /// Diff hunk 头部颜色
    fn diff_hunk(&self) -> Color {
        Color::Cyan
    } // DIFF_HUNK 青色
```

These are default methods with the existing hardcoded values, so `DarkTheme` inherits them without changes. Custom themes can override.

- [ ] **Step 2: Build and test**

```bash
cargo build -p peri-widgets && cargo test -p peri-widgets -- presets_test
```

Expected: BUILD PASS, TEST PASS

- [ ] **Step 3: Commit**

```bash
git add peri-widgets/src/theme/mod.rs
git commit -m "feat: add diff color methods (diff_add/diff_remove/diff_hunk) to Theme trait

Default implementations match existing hardcoded values in
message_block/highlight.rs. Custom themes can override.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 4: Update `highlight.rs` to accept theme for diff colors

**Files:**
- Modify: `peri-widgets/src/message_block/highlight.rs` — remove file-level constants, add theme param
- Modify: `peri-widgets/src/message_block/blocks.rs:44` — pass theme to `highlight_diff_line`
- Modify: `peri-widgets/src/message_block/highlight_test.rs` — update tests

- [ ] **Step 1: Change `highlight_diff_line` to accept `&dyn Theme`**

In `peri-widgets/src/message_block/highlight.rs`, replace the file-level constants and function signature:

```rust
use ratatui::style::Color;
use ratatui::text::Span;

use crate::theme::Theme;

pub fn highlight_diff_line(
    line: &str,
    theme: &dyn Theme,
) -> Vec<Span<'static>> {
    if line.starts_with("@@ ") {
        vec![Span::styled(
            line.to_string(),
            ratatui::style::Style::default().fg(theme.diff_hunk()),
        )]
    } else if line.starts_with('+') {
        vec![Span::styled(
            line.to_string(),
            ratatui::style::Style::default().fg(theme.diff_add()),
        )]
    } else if line.starts_with('-') {
        vec![Span::styled(
            line.to_string(),
            ratatui::style::Style::default().fg(theme.diff_remove()),
        )]
    } else {
        vec![Span::raw(line.to_string())]
    }
}

pub fn is_diff_content(text: &str) -> bool {
    // ... unchanged ...
}
```

Remove the 3 old `const DIFF_*_COLOR` lines (lines 4-6).

- [ ] **Step 2: Update `render_block` to pass theme to `highlight_diff_line`**

In `peri-widgets/src/message_block/blocks.rs`, line 44, change:

```rust
// Before:
lines.push(Line::from(super::highlight::highlight_diff_line(line)));
// After:
lines.push(Line::from(super::highlight::highlight_diff_line(line, theme)));
```

- [ ] **Step 3: Update `render_block` to use `ThemeMarkdownAdapter` for markdown parsing**

In `peri-widgets/src/message_block/blocks.rs`, replace the markdown section (lines 36-54):

```rust
#[cfg(feature = "markdown")]
{
    use super::highlight::is_diff_content;
    use crate::markdown::ThemeMarkdownAdapter;

    if is_diff_content(content) {
        let mut lines: Vec<Line<'static>> = Vec::new();
        for line in content.lines() {
            lines.push(Line::from(
                super::highlight::highlight_diff_line(line, theme),
            ));
        }
        if lines.is_empty() {
            lines.push(Line::raw(content.clone()));
        }
        lines
    } else {
        let md_theme = ThemeMarkdownAdapter(theme);
        let text = crate::markdown::parse_markdown(content, &md_theme, width);
        text.lines.into_iter().collect()
    }
}
```

Remove the now-unused `use crate::markdown::DefaultMarkdownTheme;` import.

- [ ] **Step 4: Update the highlight test**

In `peri-widgets/src/message_block/highlight_test.rs`, each test needs a `Theme` instance:

```rust
use super::*;
use crate::theme::DarkTheme;

#[test]
fn test_highlight_diff_added_line() {
    let theme = DarkTheme;
    let spans = highlight_diff_line("+ added line", &theme);
    assert_eq!(spans.len(), 1);
    assert!(spans[0].content.contains("added line"));
    assert_eq!(spans[0].style.fg, Some(DarkTheme.diff_add()));
}

#[test]
fn test_highlight_diff_removed_line() {
    let theme = DarkTheme;
    let spans = highlight_diff_line("- removed line", &theme);
    assert_eq!(spans.len(), 1);
    assert!(spans[0].content.contains("removed line"));
    assert_eq!(spans[0].style.fg, Some(DarkTheme.diff_remove()));
}

#[test]
fn test_highlight_diff_hunk_header() {
    let theme = DarkTheme;
    let spans = highlight_diff_line("@@ -1,3 +1,4 @@", &theme);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].style.fg, Some(DarkTheme.diff_hunk()));
}

#[test]
fn test_is_diff_content_positive() {
    let text = "## some heading\n@@ -1,3 +1,4 @@\n+new\n-old";
    assert!(is_diff_content(text));
}

#[test]
fn test_is_diff_content_negative() {
    let text = "normal text\nno diff here";
    assert!(!is_diff_content(text));
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p peri-widgets -- highlight_test
cargo test -p peri-widgets -- blocks
```

Expected: ALL PASS

- [ ] **Step 6: Commit**

```bash
git add peri-widgets/src/message_block/highlight.rs peri-widgets/src/message_block/blocks.rs peri-widgets/src/message_block/highlight_test.rs
git commit -m "refactor: use Theme trait for diff colors and markdown adapter in render_block

- highlight_diff_line() now takes &dyn Theme instead of hardcoded constants
- render_block uses ThemeMarkdownAdapter instead of DefaultMarkdownTheme
- Removes the last independent color source in message_block

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 5: Update `SpinnerWidget` constructor to accept `Theme`

**Files:**
- Modify: `peri-widgets/src/spinner/mod.rs` — add `with_theme()` builder, update `new()` to use Theme defaults
- Modify: `peri-widgets/src/lib.rs` — re-export `ThemeMarkdownAdapter` if needed (check)

- [ ] **Step 1: Add `with_theme()` builder to `SpinnerWidget`**

In `peri-widgets/src/spinner/mod.rs`, add a builder method and update `new()` to not hardcode:

```rust
use crate::theme::Theme;

impl<'a> SpinnerWidget<'a> {
    pub fn new(state: &'a SpinnerState) -> Self {
        Self {
            state,
            show_elapsed: true,
            show_tokens: true,
            primary_color: Color::Rgb(215, 119, 87), // ACCENT #D77757
            secondary_color: Color::Rgb(153, 153, 153), // MUTED #999999
        }
    }

    /// Use `Theme` trait for spinner colors instead of hardcoded defaults.
    pub fn with_theme(mut self, theme: &dyn Theme) -> Self {
        self.primary_color = theme.accent();
        self.secondary_color = theme.muted();
        self
    }

    pub fn show_elapsed(mut self, show: bool) -> Self {
        // ... unchanged ...
    }
    // ... rest unchanged ...
}
```

Note: Keep the `new()` defaults for backward compatibility — callers that don't use `with_theme()` still get the same colors. Only callers who opt in via `with_theme()` get dynamic theming.

- [ ] **Step 2: Build and test**

```bash
cargo build -p peri-widgets
cargo test -p peri-widgets
```

Expected: BUILD PASS, TESTS PASS

- [ ] **Step 3: Commit**

```bash
git add peri-widgets/src/spinner/mod.rs
git commit -m "feat: add SpinnerWidget::with_theme() builder for dynamic coloring

Keeps new() defaults for backward compat. with_theme()
derives primary/secondary from Theme::accent()/muted().

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 6: Replace hardcoded arrow color in `message_render.rs` with theme constant

**Files:**
- Modify: `peri-tui/src/ui/message_render.rs:367,407` — replace `Color::Rgb(147, 197, 253)` with `theme::LOADING`

- [ ] **Step 1: Replace the two hardcoded arrow color instances**

Line 367 (collapsed state):
```rust
// Before:
let arrow_color = Color::Rgb(147, 197, 253); // 淡蓝紫色 #93C1FD
// After:
let arrow_color = theme::LOADING; // #93A5FF
```

Line 407 (expanded state):
```rust
// Before:
let arrow_color = Color::Rgb(147, 197, 253); // 淡蓝紫色 #93C1FD  
// After:
let arrow_color = theme::LOADING; // #93A5FF
```

Note: `LOADING` is `#93A5FF` vs the old `#93C1FD` — they are visually nearly identical light blue tones. Using the existing theme constant eliminates a standalone hardcoded value.

- [ ] **Step 2: Build and verify**

```bash
cargo build -p peri-tui
cargo test -p peri-tui -- ui::headless_test -- --nocapture
```

Expected: BUILD PASS, TESTS PASS

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/ui/message_render.rs
git commit -m "refactor: use theme::LOADING for subagent arrow color in message_render

Replaces hardcoded #93C1FD with existing theme constant LOADING (#93A5FF).
Visually nearly identical — both light blue tones.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 7: Replace hardcoded `SELECTED` in `thread_browser.rs` with named TUI constant

**Files:**
- Modify: `peri-tui/src/ui/theme.rs` — add `SELECTED_FG` constant
- Modify: `peri-tui/src/ui/main_ui/panels/thread_browser.rs` — use `theme::SELECTED_FG`

- [ ] **Step 1: Add `SELECTED_FG` to TUI theme constants**

In `peri-tui/src/ui/theme.rs`, after the `SELECTION_BG` line (line 64), add:

```rust
/// 选中行前景色（列表高亮文字）#B2B9F9
/// 语义对应 Theme::loading() 的蓝紫色系
pub const SELECTED_FG: Color = Color::Rgb(178, 185, 249);
```

- [ ] **Step 2: Replace hardcoded `SELECTED` in thread_browser.rs**

In `peri-tui/src/ui/main_ui/panels/thread_browser.rs`:

Remove line 19 (`const SELECTED: Color = ...`).

Replace all 4 uses:
```rust
// Line 145:
Style::default().fg(theme::SELECTED_FG).add_modifier(Modifier::BOLD),
// Line 198:
Style::default().fg(if is_cursor { theme::SELECTED_FG } else { theme::MUTED }),
// Line 204:
Style::default().fg(theme::SELECTED_FG).add_modifier(Modifier::BOLD)
// Line 206:
Style::default().fg(theme::SELECTED_FG).add_modifier(Modifier::BOLD)
```

(Same replacements — just `SELECTED` → `theme::SELECTED_FG`.)

- [ ] **Step 3: Build and test**

```bash
cargo build -p peri-tui
cargo test -p peri-tui -- thread_browser
```

Expected: BUILD PASS, TESTS PASS

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/ui/theme.rs peri-tui/src/ui/main_ui/panels/thread_browser.rs
git commit -m "refactor: extract thread_browser SELECTED color to TUI theme constant

Moves #B2B9F9 to theme::SELECTED_FG alongside existing theme constants.
Documents semantic relationship to Theme::loading() blue-purple family.

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 8: Export `ThemeMarkdownAdapter` from `peri-widgets` public API

**Files:**
- Modify: `peri-widgets/src/lib.rs:34` — add re-export

- [ ] **Step 1: Re-export `ThemeMarkdownAdapter`**

In `peri-widgets/src/lib.rs`, change the markdown re-export line:

```rust
#[cfg(feature = "markdown")]
pub use markdown::{DefaultMarkdownTheme, MarkdownTheme, ThemeMarkdownAdapter};
```

- [ ] **Step 2: Build**

```bash
cargo build -p peri-widgets -p peri-tui
```

Expected: BUILD PASS

- [ ] **Step 3: Commit**

```bash
git add peri-widgets/src/lib.rs
git commit -m "feat: export ThemeMarkdownAdapter from peri-widgets public API

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```

---

### Task 9: Full integration test — build + test all crates

**Files:** None (verification only)

- [ ] **Step 1: Full build**

```bash
cargo build
```

Expected: BUILD PASS for all crates

- [ ] **Step 2: Run all tests**

```bash
cargo test
```

Expected: ALL TESTS PASS

- [ ] **Step 3: Run clippy**

```bash
cargo clippy --all-targets
```

Expected: No new warnings (existing warnings acceptable)

---

## Self-Review

### 1. Spec Coverage

| Requirement from issue | Covered by |
|---|---|
| 1. `MarkdownTheme` ↔ `Theme` 联动 | Task 2 (`ThemeMarkdownAdapter`), Task 4 (used in `render_block`) |
| 2. Unify `DarkTheme::thinking()` with TUI `THINKING` | Task 1 |
| 3. Spinner through `Theme` (not hardcoded) | Task 5 (`with_theme()` builder) |
| 4. Diff colors in `Theme` trait | Task 3 (new methods), Task 4 (wired in highlight.rs) |
| 5. Arrow color → theme constant | Task 6 |
| 6. `SELECTED` → `Theme::cursor_bg()` or similar | Task 7 (via new `SELECTED_FG` in TUI theme.rs) |

**Gaps:**
- Code syntax highlighting in `highlight_code_line` (Yellow/Green/DarkGray) — intentionally deferred. This uses standard ANSI colors that are universal across terminal themes. Adding them to `Theme` would require defining 3+ new methods (`code_keyword`, `code_string`, `code_comment`) and adding a `theme` parameter to `highlight_code_line`. Since this function isn't actually called anywhere in the codebase (grep found zero callers), it's dead code — not worth extending the trait for.
- syntect `base16-ocean.dark` syntax theme — third-party theme, intentionally out of scope.
- TUI `parse_markdown` switching from `DefaultMarkdownTheme` → `ThemeMarkdownAdapter` — intentionally deferred. TUI uses its own `theme.rs` constants, not `dyn Theme`. Bridging TUI constants → `dyn Theme` is a separate, larger refactor across 30+ TUI files that use `use crate::ui::theme`. Not in scope.

### 2. Placeholder Scan

✅ No TBD/TODO/fill-in patterns. All steps have concrete code.

### 3. Type Consistency

✅ `ThemeMarkdownAdapter` defined in Task 2, used in Task 4 and Task 8 with same name.
✅ `diff_add()`/`diff_remove()`/`diff_hunk()` defined in Task 3, used in Task 4 with same signatures.
✅ `highlight_diff_line(line, theme)` in Task 4 matches call site signature.
✅ `with_theme(&dyn Theme)` in Task 5 matches the trait used throughout.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-20-theme-markdown-color-decoupling.md`. Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration
2. **Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
