# Review Fix: config_update 迁移 + load/resume/fork frozen data 构建

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 code review 发现的两个 🟡 Major 问题：requests.rs 中 build_config_options 未迁移到共享模块、load/resume/fork 路径 frozen: None 导致系统提示词不稳定。

**Architecture:** 机械替换。Task 1 替换 import 和调用；Task 2 在 3 个 Session 构建点插入 build_frozen_session_data() 调用（与 session/new 相同的模式）。

**Tech Stack:** Rust 2021, peri-acp 共享模块, parking_lot::RwLock

---

## File Structure

### 修改文件
- `peri-tui/src/acp_server/requests.rs` — 替换 4 处 `build_config_options` 调用 + 3 处 `frozen: None` 改为构建
- `peri-tui/src/acp_stdio.rs` — 3 处 `frozen: None` 改为构建

### 不动的文件
- `peri-acp/src/session/frozen.rs` — 共享函数不变
- `peri-acp/src/dispatch/config_update.rs` — 共享函数不变
- `peri-tui/src/acp_server/notify.rs` — 已使用共享模块

---

## Task 1: 迁移 requests.rs 中 build_config_options 到共享模块

**Files:**
- Modify: `peri-tui/src/acp_server/requests.rs`

- [ ] **Step 1: 替换 import**

将 `requests.rs` 顶部的：
```rust
use peri_acp::session::state_builders::build_config_options;
```
替换为：
```rust
use peri_acp::dispatch::config_update::make_config_options;
```

- [ ] **Step 2: 替换 4 处调用**

所有 `build_config_options(&c, &p, ...)` 替换为 `make_config_options(&c, &p, ...)`。模式完全相同，仅函数名变化：

| 行号 | 上下文 |
|------|--------|
| ~L103 | session/new response 构建 |
| ~L214 | session/set_config_option response |
| ~L263 | session/load response 构建 |
| ~L440 | session/update_config response |

每处的替换模式：
```rust
// Before
build_config_options(&c, &p, cfg.permission_mode.load())

// After
make_config_options(&c, &p, cfg.permission_mode.load())
```

- [ ] **Step 3: 验证编译**

Run: `cargo build -p peri-tui 2>&1 | grep -E '^error' | head -5`
Expected: 无 error

- [ ] **Step 4: Commit**

```bash
git add peri-tui/src/acp_server/requests.rs
git commit -m "refactor: migrate requests.rs to dispatch::config_update::make_config_options

Replace direct build_config_options import with shared make_config_options,
consistent with notify.rs and acp_stdio.rs."
```

---

## Task 2: 为 load/resume/fork 构建 frozen data（TUI 路径）

**Files:**
- Modify: `peri-tui/src/acp_server/requests.rs`

在 session/new 的处理模式中，先 insert SessionState（frozen: None），然后构建 frozen data，最后 `state.frozen = Some(frozen_data)` 补回。其他 3 个路径也用相同模式。

- [ ] **Step 1: 修复 session/load（~L247）**

在 `sessions.insert(...)` 之后、response 构建之前，插入：

```rust
// Freeze session data for loaded session
let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
let frozen_language = cfg.peri_config.read().config.language.clone();
let frozen_data = peri_acp::session::frozen::build_frozen_session_data(
    cwd,
    frozen_language.as_deref(),
    &cfg.plugin_skill_dirs,
    &cfg.plugin_agent_dirs,
    &frozen_date,
);
if let Some(s) = sessions.get_mut(&req_session_id.to_string()) {
    s.frozen = Some(frozen_data);
}
```

- [ ] **Step 2: 修复 session/resume（~L346）**

在 `sessions.insert(...)` 之后，插入同样的 frozen data 构建代码。注意 resume 的 cwd 来自哪里——读取代码确认。如果 resume 有 `req.session_id` 对应的 cwd，使用该 cwd；否则用 `cfg.cwd`。

```rust
// Freeze session data for resumed session
let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
let frozen_language = cfg.peri_config.read().config.language.clone();
let frozen_data = peri_acp::session::frozen::build_frozen_session_data(
    &cwd,  // 确认 cwd 来源
    frozen_language.as_deref(),
    &cfg.plugin_skill_dirs,
    &cfg.plugin_agent_dirs,
    &frozen_date,
);
if let Some(s) = sessions.get_mut(&req_session_id.to_string()) {
    s.frozen = Some(frozen_data);
}
```

- [ ] **Step 3: 修复 session/fork（~L389）**

在 `sessions.insert(...)` 之后，插入同样的 frozen data 构建代码。fork 的 cwd 继承自父 session：

```rust
// Freeze session data for forked session
let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
let frozen_language = cfg.peri_config.read().config.language.clone();
let frozen_data = peri_acp::session::frozen::build_frozen_session_data(
    &cwd,  // fork 继承父 session 的 cwd
    frozen_language.as_deref(),
    &cfg.plugin_skill_dirs,
    &cfg.plugin_agent_dirs,
    &frozen_date,
);
if let Some(s) = sessions.get_mut(&new_session_id) {
    s.frozen = Some(frozen_data);
}
```

- [ ] **Step 4: 验证编译**

Run: `cargo build -p peri-tui 2>&1 | grep -E '^error' | head -5`
Expected: 无 error

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/acp_server/requests.rs
git commit -m "fix: build frozen data for load/resume/fork in TUI path

load/resume/fork sessions now freeze system prompt at load time,
ensuring stable prompts across turns (same guarantee as session/new)."
```

---

## Task 3: 为 load/resume/fork 构建 frozen data（Stdio 路径）

**Files:**
- Modify: `peri-tui/src/acp_stdio.rs`

与 Task 2 相同模式，但在 Stdio 的 SessionInfo 构建中。

- [ ] **Step 1: 修复 session/resume（~L708）**

在 SessionInfo insert 之后，插入 frozen data 构建代码。注意 Stdio 路径从 `ctx` 获取参数：

```rust
// Freeze session data for resumed session
let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
let frozen_language = ctx.peri_config.read().config.language.clone();
let frozen_data = peri_acp::session::frozen::build_frozen_session_data(
    &cwd,  // 确认 cwd 来源
    frozen_language.as_deref(),
    &ctx.plugin_skill_dirs,
    &ctx.plugin_agent_dirs,
    &frozen_date,
);
{
    let mut sessions = ctx.sessions.write();
    if let Some(s) = sessions.get_mut(&sid) {
        s.frozen = Some(frozen_data);
    }
}
```

- [ ] **Step 2: 修复 session/load（~L753）**

同上模式。

- [ ] **Step 3: 修复 session/fork（~L850）**

同上模式。

- [ ] **Step 4: 验证编译**

Run: `cargo build -p peri-tui 2>&1 | grep -E '^error' | head -5`
Expected: 无 error

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/acp_stdio.rs
git commit -m "fix: build frozen data for load/resume/fork in Stdio path

Symmetric with TUI path — ensures frozen system prompt for all
session lifecycle paths."
```

---

## Task 4: 验证

- [ ] **Step 1: 全量编译 + 测试**

Run: `cargo build --workspace 2>&1 | grep -E '^error' | head -5`
Expected: 无 error

Run: `cargo test --workspace 2>&1 | grep -E 'test result:' | head -10`
Expected: 全部通过

- [ ] **Step 2: Squash 或保留**

根据偏好将 Task 1-3 的 commit squash 为 1 个，或保留独立 commit。

---

## 实施注意事项

1. **cwd 来源**：每个 handler 的 cwd 来源可能不同——load 从 ThreadStore 读取，resume 从请求参数获取，fork 从父 session 继承。实施时需先 Read 代码确认 cwd 变量名。
2. **sessions 的锁类型不同**：TUI 路径用 `sessions`（可直接 get_mut，因为是 `&mut HashMap` 参数），Stdio 路径用 `ctx.sessions.write()`（parking_lot::RwLock）。
3. **plugin_skill_dirs / plugin_agent_dirs**：TUI 路径从 `cfg.plugin_skill_dirs` / `cfg.plugin_agent_dirs` 获取，Stdio 路径从 `ctx.plugin_skill_dirs` / `ctx.plugin_agent_dirs` 获取。
4. **不影响 session/new**：session/new 已正确构建 frozen data，不需要修改。
