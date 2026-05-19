# Main.rs ACP Stdio 提取 + Sync 胶水简化

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** 将 952 行的 `main.rs` 中最大的单一函数 `run_acp_stdio()`（430 行）提取到 `acp_stdio.rs`。保留 CLI 定义、环境注入、事件循环入口在 `main.rs`

**Architecture:** `run_acp_stdio` 是独立的 ACP stdio 服务模式——它有自己的 `StdioContext` 结构体、session 管理、中间件初始化。与 TUI 事件循环完全独立。提取后 `main.rs` 保持 CLI 分派入口，每个子命令指向各自的实现文件。

**Tech Stack:** Rust 2021, clap, tokio, ratatui

---

### Task 1: 读取 `run_acp_stdio` 函数边界

**Files:**
- Read: `peri-tui/src/main.rs:169-600`

- [ ] **Step 1: 读取完整函数体**

```bash
sed -n '169,600p' peri-tui/src/main.rs | wc -l
```

Expected: ~430 lines.

---

### Task 2: 创建 `acp_stdio.rs` 并移动 `run_acp_stdio`

**Files:**
- Create: `peri-tui/src/acp_stdio.rs`
- Modify: `peri-tui/src/main.rs:169-600` (delete run_acp_stdio)
- Modify: `peri-tui/src/main.rs:` (add `mod acp_stdio;`)

- [ ] **Step 1: Create `acp_stdio.rs` with `run_acp_stdio` and `StdioContext`**

```rust
//! ACP Stdio 模式：通过 stdin/stdout JSON-RPC 与 IDE client 通信

use std::sync::Arc;

use peri_acp::transport::stdio::stdio_transport_pair;
use peri_acp::transport::types::AcpNotification;
use peri_acp::session::executor;
use peri_acp::session::event_sink::StdioEventSink;
use peri_agent::messages::BaseMessage;
use peri_agent::agent::AgentCancellationToken;

use crate::app::agent::LlmProvider;
use crate::config::PeriConfig;

/// ACP Stdio 模式的 session 信息
struct SessionInfo {
    #[allow(dead_code)]
    session_id: String,
    thread_id: String,
    cwd: String,
    history: Vec<BaseMessage>,
    cancel_token: Option<AgentCancellationToken>,
}

/// ACP Stdio 上下文 — 在 session 间共享的服务
struct StdioContext {
    provider: parking_lot::RwLock<LlmProvider>,
    peri_config: parking_lot::RwLock<PeriConfig>,
    permission_mode: Arc<peri_middlewares::prelude::SharedPermissionMode>,
    cron_scheduler: Arc<parking_lot::Mutex<peri_middlewares::cron::CronScheduler>>,
    mcp_pool: Option<Arc<peri_middlewares::mcp::McpClientPool>>,
    plugin_skill_dirs: Vec<std::path::PathBuf>,
    plugin_agent_dirs: Vec<std::path::PathBuf>,
    hook_groups: Vec<Vec<peri_middlewares::hooks::RegisteredHook>>,
    plugin_lsp_servers: Vec<peri_lsp::config::LspServerConfig>,
    tool_search_index: Arc<peri_middlewares::tool_search::ToolSearchIndex>,
    thread_store: peri_agent::thread::ThreadStore,
}

pub async fn run_acp_stdio(cwd: String) -> anyhow::Result<()> {
    // ... (copy entire existing body from main.rs lines 169-600) ...
```
- [ ] **Step 2: Replace in `main.rs`**

Delete the `run_acp_stdio` function and `SessionInfo`/`StdioContext` structs from `main.rs` (lines 169-600).

Add at the top of `main.rs`:

```rust
mod acp_stdio;
```

Update the `Commands::Acp` arm:

```rust
Some(Commands::Acp { cwd, .. }) => {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(acp_stdio::run_acp_stdio(cwd))
}
```

- [ ] **Step 3: Build to verify**

```bash
cargo build -p peri-tui 2>&1 | head -10
```

---

### Task 3: 提取 `inject_env_from_settings` 到 `env_setup.rs`

**Files:**
- Create: `peri-tui/src/env_setup.rs`
- Modify: `peri-tui/src/main.rs:75-113`

- [ ] **Step 1: Create `env_setup.rs`**

```rust
/// 从 settings.json 读取 env 字段并注入进程环境变量
/// 仅在进程环境变量不存在时设置（进程环境优先）
pub fn inject_env_from_settings() {
    let path = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".peri")
        .join("settings.json");

    if !path.exists() {
        return;
    }

    let Ok(content) = std::fs::read_to_string(&path) else {
        return;
    };

    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return;
    };

    let Some(env_obj) = json.get("config").and_then(|c| c.get("env")) else {
        return;
    };

    let Some(env_map) = env_obj.as_object() else {
        return;
    };

    for (key, value) in env_map {
        if let Some(value_str) = value.as_str() {
            if std::env::var(key).is_err() {
                std::env::set_var(key, value_str);
            }
        }
    }
}
```

- [ ] **Step 2: Replace in `main.rs`**

Delete lines 75-113 from `main.rs`. Add import:

```rust
use peri_tui::env_setup::inject_env_from_settings;
```

Actually, `inject_env_from_settings` can stay as a `pub(crate) fn` in `main.rs` since it's only used there. Skip this extraction if the function is < 40 lines.

---

### Task 4: 验证 `main.rs` 降至 ~470 行

- [ ] **Step 1: Count lines**

```bash
wc -l peri-tui/src/main.rs
```

Expected: ~470 lines (down from 952). Remaining: CLI structs (48 lines), `main()` dispatch (51 lines), `run_tui()` (228 lines), tests (24 lines), other functions.

---

### Task 5: 全量测试和提交

- [ ] **Step 1: Run all tests**

```bash
cargo test -p peri-tui --lib 2>&1 | tail -10
```

- [ ] **Step 2: Commit**

```bash
cargo fmt -p peri-tui
git add peri-tui/src/acp_stdio.rs peri-tui/src/main.rs
git commit -m "refactor: extract run_acp_stdio to acp_stdio.rs

- Move 430-line ACP stdio function + StdioContext + SessionInfo to acp_stdio.rs
- main.rs: 952 → ~470 lines
- Pure code movement, no logic changes

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```
