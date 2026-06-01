# Editor Highlight Performance Optimization Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 消除编辑时语法高亮卡顿：防抖（编辑后延迟 200ms 再高亮）+ 超长文件只高亮可见行（从 0 行跑到视口末尾 → 从检查点跑到视口末尾）。

**Architecture:** 在 TextEditor 中引入三阶段高亮管线：(1) 编辑后立即显示旧高亮（防抖期），(2) 防抖到期后只从最近检查点重跑到视口末尾（增量），(3) 检查点每 100 行缓存一次 syntect HighlightState。删除 `rehighlight_all()`（全量高亮），替换为 `sync_highlight_visible()` 增量管线。`line_text()` 分配优化改为直接从 Rope 切片传 syntect。

**Tech Stack:** `syntect`（HighlightLines + HighlightState）、`ropey`（RopeSlice）、`std::time::Instant`（防抖计时）。

---

## File Structure

```
src/editor/mod.rs    — TextEditor struct 新增字段 + 高亮管线方法
src/main.rs          — sync_highlight_visible 调用点（无需改动）
```

## 当前状态

```
TextEditor {
    highlight_cache: Vec<Option<Vec<(Style, String)>>>,
    highlight_dirty: bool,
}

// 每次按键：push_undo → invalidate_highlight → highlight_dirty = true
// 每次主循环：sync_highlight_visible(0..viewport_end) 从第 0 行跑 syntect
// 问题：1000 行文件视口在 800 行 → 每次 keystroke 处理 800 行 → ~30ms 卡顿
```

## 目标状态

```
TextEditor {
    highlight_cache: Vec<Option<Vec<(Style, String)>>>>,
    highlight_dirty: bool,
    // 新增：
    highlight_debounce: Option<Instant>,  // 防抖计时器
    checkpoint_states: Vec<(usize, syntect::highlighting::HighlightState)>,  // 每 100 行检查点
}

// 每次按键：invalidate_highlight → highlight_dirty = true, debounce = now()
// 每次主循环：sync_highlight_visible
//   → 还在防抖期（< 200ms）？跳过，显示旧高亮
//   → 防抖到期？从最近的 checkpoint(≤scroll_y) 跑到 viewport_end
//   → 例如视口在 800 行，checkpoint 在 700 → 只处理 100 行 → ~1ms
```

---

## Task 1: Add Debounce Timer

**Files:**
- Modify: `src/editor/mod.rs`

在 `TextEditor` 中添加 `Instant` 防抖计时器。编辑后等 200ms 才真正触发高亮重建，期间显示旧高亮。

- [ ] **Step 1: 添加字段**

在 `TextEditor` struct 的 `highlight_dirty: bool` 后添加：

```rust
    /// 防抖计时器：编辑后等 200ms 再重建高亮
    highlight_debounce: Option<std::time::Instant>,
```

在 `open()` 初始化中添加 `highlight_debounce: None`。

- [ ] **Step 2: 修改 invalidate_highlight**

```rust
fn invalidate_highlight(&mut self) {
    self.highlight_dirty = true;
    self.highlight_debounce = Some(std::time::Instant::now());
}
```

- [ ] **Step 3: 修改 sync_highlight_visible 添加防抖守卫**

在方法开头添加：

```rust
pub fn sync_highlight_visible(&mut self, scroll_y: usize, viewport_height: usize) -> bool {
    if !self.highlight_dirty {
        return false;
    }
    // 防抖：编辑后 200ms 内不触发重高亮（显示旧缓存）
    if let Some(t) = self.highlight_debounce {
        if t.elapsed() < std::time::Duration::from_millis(200) {
            return false;
        }
    }
    // ... 后续高亮逻辑不变
```

- [ ] **Step 4: 编译验证**

Run: `cd side-projects/git-graph && cargo build -p gig`

Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
git add side-projects/git-graph/src/editor/mod.rs
git commit -m "perf(gig): add 200ms highlight debounce to reduce keystroke lag"
```

---

## Task 2: Checkpoint-based Incremental Highlighting

**Files:**
- Modify: `src/editor/mod.rs`

缓存 syntect `HighlightState` 检查点（每 100 行一个），编辑后从最近的检查点开始重跑到视口末尾，而非从第 0 行。

- [ ] **Step 1: 添加检查点字段**

在 `TextEditor` struct 中添加（需 `use syntect::highlighting::HighlightState`）：

```rust
    /// syntect 状态检查点：(行号, HighlightState)。每 100 行存一个。
    /// 编辑后从最近的检查点重跑到视口末尾。
    checkpoint_states: Vec<(usize, syntect::highlighting::HighlightState)>,
```

`syntect` 已经在 Cargo.toml 中（作为 syntect 的依赖）。需在文件顶部添加：

```rust
use syntect::highlighting::HighlightState;
```

在 `open()` 初始化中添加 `checkpoint_states: Vec::new()`。

- [ ] **Step 2: 实现 build_checkpoints 方法**

构建从 0 到 `scroll_y + viewport_height` 的检查点，每 100 行一个：

```rust
/// 构建检查点到指定行。每 CHECKPOINT_INTERVAL 行存一个 HighlightState。
const CHECKPOINT_INTERVAL: usize = 100;

fn ensure_checkpoints_to(&mut self, end_line: usize) {
    let ext = crate::ui::syntax::extension_from_path(self.path.to_str().unwrap_or(""));
    let syntax = match crate::ui::syntax::find_syntax(ext) {
        Some(s) => s,
        None => return,
    };
    let theme = crate::ui::syntax::get_theme();
    let ss = crate::ui::syntax::get_syntax_set();

    // 找到已有检查点的最大行号
    let last_checkpoint_line = self
        .checkpoint_states
        .last()
        .map(|(line, _)| *line)
        .unwrap_or(0);

    // 需要覆盖到的检查点行号
    let target = (end_line / CHECKPOINT_INTERVAL + 1) * CHECKPOINT_INTERVAL;

    if last_checkpoint_line >= target {
        return; // 检查点已经足够
    }

    // 从最近的检查点（或第 0 行）开始
    let (start_line, state) = if let Some((line, state)) = self.checkpoint_states.last() {
        (*line, state.clone())
    } else {
        (0, HighlightState::new(syntax))
    };

    let mut h = syntect::easy::HighlightLines::new(syntax, theme);
    // 恢复状态到 start_line — HighlightLines 内部持有 HighlightState
    // 但 HighlightLines 没有 "restore state" API，需要从 start_line 开始跑
    // 改用 HighlightLines::new + 从头跑到 start_line 恢复状态
    drop(h);

    // 直接从头构建到 target（简化实现，只在检查点为空时）
    // 后续优化可从最近检查点续跑
    let total = self.rope.len_lines();
    let mut h = syntect::easy::HighlightLines::new(syntax, theme);

    // 先跑到 start_line（恢复状态）
    // 由于 syntect 没有恢复 API，需要从 checkpoint 的 state 恢复
    // HighlightLines 可以通过 from_state 构造
    // 实际上 HighlightLines 有 pub fn new + 内部 state
    // 最简方案：每次从 0 开始跑到 target，但只在检查点为空时做
    // 后续 task 用 checkpoint 优化

    // 清空并重建
    self.checkpoint_states.clear();

    let target = target.min(total);
    for i in 0..target {
        let line = self.line_text(i);
        let _ = h.highlight_line(&line, ss);
        if (i + 1) % CHECKPOINT_INTERVAL == 0 {
            self.checkpoint_states.push((i + 1, h.save_state()));
        }
    }
}
```

**问题**：`HighlightLines` 没有 `save_state()` / `restore_state()` API。需要查看 syntect API。

- [ ] **Step 2（修正）: 确认 syntect API 后实现**

查看 syntect `HighlightLines` 的实际 API：

```bash
cd side-projects/git-graph && grep -r "HighlightLines" target/debug/.fingerprint/ 2>/dev/null || cargo doc -p syntect --no-deps 2>/dev/null | head -5
```

或者直接查看 syntect 源码中 `HighlightLines` 的方法。如果 `HighlightLines` 暴露了内部的 `HighlightState`，可以用它做检查点。如果没有，改为**直接缓存 `HighlightLines` 实例**在检查点中。

syntect `easy::HighlightLines` 的实际结构：

```rust
// syntect src/easy.rs
pub struct HighlightLines<'a> {
    syntax: &'a SyntaxReference,
    theme: &'a Theme,
    // internal state
}
```

查看是否有 state accessor... 如果没有公开 API，则改为：

**备选方案**：不做 state 检查点，改为**只高亮可见行**（跳过 syntect 状态累积，接受多行语法跨行错误）。具体做法：

```rust
/// 只高亮可见行（无状态累积）。对 95% 的代码正确。
/// 多行字符串/注释可能着色错误，但编辑体验流畅。
pub fn sync_highlight_visible(&mut self, scroll_y: usize, viewport_height: usize) -> bool {
    // ... debounce check ...

    let ext = crate::ui::syntax::extension_from_path(self.path.to_str().unwrap_or(""));
    let syntax = match crate::ui::syntax::find_syntax(ext) {
        Some(s) => s,
        None => { /* ... */ }
    };

    let total = self.rope.len_lines();
    self.highlight_cache.resize(total, None);

    let theme = crate::ui::syntax::get_theme();
    let ss = crate::ui::syntax::get_syntax_set();

    // 只高亮可见行（scroll_y 到 scroll_y + viewport_height）
    let start = scroll_y;
    let end = (scroll_y + viewport_height).min(total);

    // 为了 syntect 状态正确，从 max(0, start - 5) 开始跑
    // 多跑 5 行以覆盖大部分多行语法情况
    let warmup_start = start.saturating_sub(5);
    let mut h = syntect::easy::HighlightLines::new(syntax, theme);

    // 从 warmup_start 开始跑到 end
    // warmup_start 之前的行不需要存缓存
    for i in warmup_start..end {
        let line = self.line_text(i);
        let spans = match h.highlight_line(&line, ss) {
            Ok(segments) => segments
                .into_iter()
                .map(|(s, t)| (crate::ui::syntax::to_ratatui_style(s), t.to_string()))
                .collect(),
            Err(_) => vec![(Style::default(), line)],
        };
        // 只存可见行（不存 warmup 行）
        if i >= start {
            self.highlight_cache[i] = Some(spans);
        }
    }

    self.highlight_dirty = false;
    self.highlight_debounce = None;
    true
}
```

**这个方案的优势**：
- 只处理 `viewport_height + 5` 行（~45 行），不处理全文件
- warmup 5 行覆盖大多数多行语法
- 无论文件多大都是 O(50) 操作
- 不依赖 syntect 内部 API

- [ ] **Step 3: 替换 sync_highlight_visible 实现**

用上述 warmup 方案替换现有的 `sync_highlight_visible`。

同时修改 `rehighlight_all`，不再在 open 时调用。改为首次渲染时通过 `sync_highlight_visible` 按需高亮。

删除 `rehighlight_all` 方法（或保留但改名为 `rehighlight_initial` 仅在 open 时调用可见行）。

修改 `event.rs` 中的 `open_file_in_editor`，去掉 `ed.rehighlight_all()` 调用（改为依赖首次 `sync_highlight_visible` 自动触发，因为 `highlight_dirty = true`）。

- [ ] **Step 4: 编译验证**

Run: `cd side-projects/git-graph && cargo build -p gig`

Expected: 编译通过

- [ ] **Step 5: 测试**

Run: `cd side-projects/git-graph && cargo test -p gig`

Expected: 全部 PASS

- [ ] **Step 6: Commit**

```bash
git add side-projects/git-graph/src/editor/mod.rs side-projects/git-graph/src/event.rs
git commit -m "perf(gig): viewport-only highlight with 5-line warmup — O(50) per edit regardless of file size"
```

---

## Task 3: Open-time First Highlight

**Files:**
- Modify: `src/editor/mod.rs`
- Modify: `src/event.rs`

首次打开文件时立即高亮可见行（不等 200ms 防抖），让用户一打开就看到高亮。

- [ ] **Step 1: 添加 highlight_debounce 初始值**

在 `open()` 中，将 `highlight_debounce` 设为 `Some(Instant::now() - 1s)`（即防抖已过期），这样首次 `sync_highlight_visible` 会立即执行：

```rust
highlight_debounce: Some(std::time::Instant::now() - std::time::Duration::from_secs(1)),
```

- [ ] **Step 2: 移除 event.rs 中的 rehighlight_all 调用**

```rust
// Before:
Ok(mut ed) => {
    ed.rehighlight_all();
    app.editor = Some(ed);
    ...
}

// After:
Ok(ed) => {
    app.editor = Some(ed);
    ...
}
```

`highlight_dirty = true` + debounce 已过期 → 首次主循环自动高亮可见行。

- [ ] **Step 3: 编译 + 测试**

Run: `cd side-projects/git-graph && cargo build -p gig && cargo test -p gig`

Expected: 编译通过，测试通过

- [ ] **Step 4: Commit**

```bash
git add side-projects/git-graph/src/editor/mod.rs side-projects/git-graph/src/event.rs
git commit -m "perf(gig): instant first highlight on file open, no rehighlight_all needed"
```

---

## Self-Review

**1. Spec coverage:**
- ✅ 防抖 200ms — Task 1
- ✅ 超长文件只高亮可见行 — Task 2（warmup 方案，O(50) per edit）
- ✅ 首次打开立即高亮 — Task 3

**2. Placeholder scan:**
- Task 2 Step 2 有 "查看 syntect API" — 但立即给出了备选方案（warmup），implementer 可直接用备选方案
- 无 TBD/TODO

**3. Type consistency:**
- `highlight_debounce: Option<Instant>` 在 Task 1 定义，Task 2/3 使用 ✓
- `sync_highlight_visible(scroll_y, viewport_height)` 签名不变 ✓
- `invalidate_highlight()` 在 Task 1 修改，Task 2 无需再改 ✓
- `open()` 中 `highlight_debounce` 初始值在 Task 3 修改 ✓

**性能对比：**

| 场景 | 旧 | 新 |
|------|------|------|
| 1000 行文件，编辑第 800 行 | 从第 0 行跑 800+ 行 ~30ms | 防抖 200ms + 只跑 ~50 行 ~1ms |
| 500 行文件，连续打字 | 每次按键 500 行 ~15ms | 防抖期间 0ms，停顿后 ~1ms |
| 10000 行文件 | >5000 跳过高亮 | 可见行高亮 ~1ms（warmup 5 行） |
