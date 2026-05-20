# Session Startup System Prompt Freeze Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Freeze all system prompt volatile data (date, CLAUDE.md, skills summary) at session creation, so the system prompt never changes within a session.

**Architecture:** Build the system prompt once at `session/new`, store frozen values in `SessionState`, and pass them through the existing executor/builder pipeline. Middlewares (`AgentsMdMiddleware`, `SkillsMiddleware`) get frozen content via constructor — no new lifecycle hooks needed.

**Tech Stack:** Rust, tokio async, existing peri-agent/peri-acp/peri-middlewares crates.

---

## File Structure

| File | Change | Responsibility |
|------|--------|---------------|
| `peri-acp/src/session/executor.rs` | Modify | Add `FrozenSessionData` struct; accept it in `execute_prompt`; skip prompt rebuild |
| `peri-acp/src/agent/builder.rs` | Modify | `AcpAgentConfig` gets frozen fields; pass to middlewares and `system_builder` |
| `peri-tui/src/acp_server/mod.rs` | Modify | `SessionState` gets frozen fields |
| `peri-tui/src/acp_server/requests.rs` | Modify | `session/new` handler freezes values |
| `peri-tui/src/acp_server/prompt.rs` | Modify | Extract frozen values from sessions, pass to executor |
| `peri-middlewares/src/agents_md/mod.rs` | Modify | Accept frozen content via `with_frozen_content()`; skip disk read when frozen |
| `peri-middlewares/src/skills/mod.rs` | Modify | Accept frozen summary via `with_frozen_summary()`; skip scan when frozen |
| `peri-acp/src/prompt/mod.rs` | Modify | `build_system_prompt` accepts optional frozen date override |
| `CLAUDE.md` | Modify | Update system prompt stability docs and data flow |

---

### Task 1: Add `FrozenSessionData` struct to executor

**Files:**
- Modify: `peri-acp/src/session/executor.rs:1-26` (add struct after imports)

- [ ] **Step 1: Add `FrozenSessionData` struct**

After the existing `PromptResult` struct (line 46), add:

```rust
/// Session-scoped frozen data that locks system prompt stability.
///
/// Populated at session creation time, passed through to every turn's
/// agent build to guarantee the system prompt never changes within a session.
#[derive(Clone)]
pub struct FrozenSessionData {
    /// Full system prompt string built at session creation.
    pub system_prompt: String,
    /// Frozen content of CLAUDE.md (resolved imports), None if no file.
    pub claude_md: Option<String>,
    /// Frozen content of CLAUDE.local.md, None if no file.
    pub claude_local_md: Option<String>,
    /// Frozen skills summary string, None if no skills.
    pub skill_summary: Option<String>,
    /// Session creation date in YYYY-MM-DD format.
    pub date: String,
    /// Whether cwd is a git repo at session creation time.
    pub is_git_repo: bool,
}
```

- [ ] **Step 2: Add `frozen: Option<FrozenSessionData>` param to `execute_prompt`**

Modify the function signature (line 64) — add `frozen` parameter after `content`:

```rust
pub async fn execute_prompt(
    provider: &LlmProvider,
    peri_config: Arc<crate::provider::PeriConfig>,
    cwd: &str,
    content: String,
    frozen: Option<FrozenSessionData>,        // NEW
    history: Vec<BaseMessage>,
    // ... rest unchanged
```

- [ ] **Step 3: Use frozen data instead of rebuilding system prompt**

Replace lines 137-138:

```rust
    let features = PromptFeatures::detect();
    let system_prompt = build_system_prompt(None, cwd, features, &plugin_agent_dirs);
```

With:

```rust
    let (system_prompt, frozen_claude_md, frozen_claude_local_md, frozen_skill_summary, frozen_date) =
        if let Some(ref f) = frozen {
            (
                f.system_prompt.clone(),
                f.claude_md.clone(),
                f.claude_local_md.clone(),
                f.skill_summary.clone(),
                Some(f.date.clone()),
            )
        } else {
            let features = PromptFeatures::detect();
            let sp = build_system_prompt(None, cwd, features, &plugin_agent_dirs, None);
            (sp, None, None, None, None)
        };
```

- [ ] **Step 4: Pass frozen fields into `AcpAgentConfig`**

Add the new fields to the `AcpAgentConfig` construction at line 143-177:

```rust
    let agent_output = builder::build_agent(AcpAgentConfig {
        provider: provider.clone(),
        cwd: cwd.to_string(),
        system_prompt,
        frozen_claude_md,              // NEW
        frozen_claude_local_md,        // NEW
        frozen_skill_summary,          // NEW
        frozen_date,                   // NEW
        event_handler,
        // ... rest unchanged
    });
```

- [ ] **Step 5: Build and verify**

```bash
cargo build -p peri-acp 2>&1 | head -30
```

Expected: COMPILE ERRORS (unknown fields in AcpAgentConfig — will fix in Task 2)

---

### Task 2: Add frozen fields to `AcpAgentConfig` and wire into middlewares

**Files:**
- Modify: `peri-acp/src/agent/builder.rs:36-69` (AcpAgentConfig struct)
- Modify: `peri-acp/src/agent/builder.rs:82-118` (build_agent destructuring)
- Modify: `peri-acp/src/agent/builder.rs:254-259` (Git Attribution)
- Modify: `peri-acp/src/agent/builder.rs:270-278` (AgentsMdMiddleware construction)
- Modify: `peri-acp/src/agent/builder.rs:274-276` (SkillsMiddleware construction)
- Modify: `peri-acp/src/agent/builder.rs:200-209` (system_builder closure)

- [ ] **Step 1: Add frozen fields to `AcpAgentConfig` struct**

After `pub system_prompt: String` (line 39), insert:

```rust
    /// Frozen CLAUDE.md content (None = read from disk each turn, legacy behavior).
    pub frozen_claude_md: Option<String>,
    /// Frozen CLAUDE.local.md content.
    pub frozen_claude_local_md: Option<String>,
    /// Frozen skills summary (None = scan each turn).
    pub frozen_skill_summary: Option<String>,
    /// Frozen session date in YYYY-MM-DD (None = compute fresh each turn).
    pub frozen_date: Option<String>,
```

- [ ] **Step 2: Destructure new fields in `build_agent`**

At line 83-109 (destructuring of AcpAgentConfig), add:

```rust
        frozen_claude_md,
        frozen_claude_local_md,
        frozen_skill_summary,
        frozen_date,
```

After the existing `system_prompt` destructure at ~line 86.

- [ ] **Step 3: Modify `AgentsMdMiddleware` construction**

Replace lines 270-272:

```rust
        .add_middleware(Box::new(
            AgentsMdMiddleware::new().with_excludes(claude_md_excludes),
        ))
```

With:

```rust
        .add_middleware(Box::new({
            let mut mw = AgentsMdMiddleware::new().with_excludes(claude_md_excludes);
            if let (Some(main), local) = (frozen_claude_md, frozen_claude_local_md) {
                mw = mw.with_frozen_content(main, local);
            }
            mw
        }))
```

- [ ] **Step 4: Modify `SkillsMiddleware` construction**

Replace lines 274-276:

```rust
        .add_middleware(Box::new(
            SkillsMiddleware::new().with_extra_dirs(plugin_skill_dirs),
        ))
```

With:

```rust
        .add_middleware(Box::new({
            let mut mw = SkillsMiddleware::new().with_extra_dirs(plugin_skill_dirs);
            if let Some(summary) = frozen_skill_summary {
                mw = mw.with_frozen_summary(summary);
            }
            mw
        }))
```

- [ ] **Step 5: Pass frozen_date into `system_builder` closure**

Replace lines 202-209:

```rust
    let system_builder: Arc<...> = Arc::new(|overrides, cwd_dir| {
        let features = crate::prompt::PromptFeatures::detect();
        crate::prompt::build_system_prompt(overrides, cwd_dir, features, &[])
    });
```

With:

```rust
    let frozen_date_for_sub = frozen_date.clone();
    let system_builder: Arc<...> = Arc::new(move |overrides, cwd_dir| {
        let features = crate::prompt::PromptFeatures::detect();
        crate::prompt::build_system_prompt(
            overrides,
            cwd_dir,
            features,
            &[],
            frozen_date_for_sub.as_deref(),
        )
    });
```

- [ ] **Step 6: Build and verify**

```bash
cargo build -p peri-acp 2>&1 | head -40
```

Expected: COMPILE ERRORS (AgentsMdMiddleware/SkillsMiddleware missing `with_frozen_*` methods, `build_system_prompt` missing `frozen_date` param — will fix in Tasks 3/4/5)

---

### Task 3: Add `frozen_date` parameter to `build_system_prompt`

**Files:**
- Modify: `peri-acp/src/prompt/mod.rs:90-197`

- [ ] **Step 1: Add `frozen_date` parameter to function signature**

Replace line 90-95:

```rust
pub fn build_system_prompt(
    overrides: Option<&AgentOverrides>,
    cwd: &str,
    features: PromptFeatures,
    extra_agent_dirs: &[std::path::PathBuf],
) -> String {
```

With:

```rust
pub fn build_system_prompt(
    overrides: Option<&AgentOverrides>,
    cwd: &str,
    features: PromptFeatures,
    extra_agent_dirs: &[std::path::PathBuf],
    frozen_date: Option<&str>,
) -> String {
```

- [ ] **Step 2: Use frozen_date in PromptEnv construction**

Replace line 96:

```rust
    let env = PromptEnv::detect(cwd);
```

With:

```rust
    let env = if let Some(date) = frozen_date {
        PromptEnv::with_frozen_date(cwd, date)
    } else {
        PromptEnv::detect(cwd)
    };
```

- [ ] **Step 3: Add `PromptEnv::with_frozen_date` constructor**

After `PromptEnv::detect` (line 50-63), add:

```rust
    /// 使用冻结日期构造（跳过 `chrono::Local::now()` 调用）
    pub fn with_frozen_date(cwd: &str, frozen_date: &str) -> Self {
        let is_git_repo = std::path::Path::new(cwd).join(".git").exists();
        let platform = std::env::consts::OS.to_string();
        let os_version = os_version_string();
        Self {
            cwd: cwd.to_string(),
            is_git_repo,
            platform,
            os_version,
            date: frozen_date.to_string(),
        }
    }
```

- [ ] **Step 4: Update test calls to `build_system_prompt`**

In `peri-acp/src/prompt/prompt_test.rs`, update all calls to add `None` as the last argument. Find-replace pattern:

Old: `build_system_prompt(overrides, cwd, features, &[])`  
New: `build_system_prompt(overrides, cwd, features, &[], None)`

Also update `build_system_prompt(None, "/tmp", PromptFeatures::none(), &[])` → `build_system_prompt(None, "/tmp", PromptFeatures::none(), &[], None)`

- [ ] **Step 5: Verify tests pass**

```bash
cargo test -p peri-acp --lib -- prompt_test 2>&1 | tail -20
```

Expected: All prompt tests PASS.

---

### Task 4: Add `with_frozen_content` to `AgentsMdMiddleware`

**Files:**
- Modify: `peri-middlewares/src/agents_md/mod.rs:24-79`

- [ ] **Step 1: Add frozen content fields to struct**

After `excludes: Vec<String>` (line 21), add:

```rust
    /// Frozen CLAUDE.md main content (resolved imports). When set, skip disk read.
    frozen_main: Option<String>,
    /// Frozen CLAUDE.local.md content.
    frozen_local: Option<String>,
```

- [ ] **Step 2: Initialize in `new()`**

In `new()` (line 25-30), add:

```rust
    pub fn new() -> Self {
        Self {
            extra_search_paths: Vec::new(),
            excludes: Vec::new(),
            frozen_main: None,
            frozen_local: None,
        }
    }
```

- [ ] **Step 3: Add `with_frozen_content` builder method**

After `with_excludes` (line 39-42), add:

```rust
    /// 注入冻结的 CLAUDE.md 内容（main）和 CLAUDE.local.md 内容（local）。
    ///
    /// 设置后 `before_agent` 跳过磁盘读取，直接使用冻结内容。
    /// `main` 应为已解析 `@import` 引用的内容。
    pub fn with_frozen_content(mut self, main: String, local: Option<String>) -> Self {
        self.frozen_main = Some(main);
        self.frozen_local = local;
        self
    }
```

- [ ] **Step 4: Short-circuit in `before_agent` when frozen**

At the start of `before_agent` body (line 144), before any disk I/O, add:

```rust
    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        // 使用冻结内容时跳过所有磁盘 I/O
        if let Some(ref main) = self.frozen_main {
            let mut content = main.clone();
            if let Some(ref local) = self.frozen_local {
                if !local.trim().is_empty() {
                    content = format!("{content}\n\n{local}");
                }
            }
            if !content.trim().is_empty() {
                state.prepend_message(BaseMessage::system(content));
            }
            return Ok(());
        }
        // ... existing code unchanged
```

- [ ] **Step 5: Verify tests pass**

```bash
cargo test -p peri-middlewares --lib -- agents_md 2>&1 | tail -20
```

Expected: All agents_md tests PASS.

---

### Task 5: Add `with_frozen_summary` to `SkillsMiddleware`

**Files:**
- Modify: `peri-middlewares/src/skills/mod.rs:135-192`

- [ ] **Step 1: Read the SkillsMiddleware struct**

Read the current struct definition:

```bash
grep -n 'pub struct SkillsMiddleware' peri-middlewares/src/skills/mod.rs
```

- [ ] **Step 2: Add frozen summary field to struct**

```rust
pub struct SkillsMiddleware {
    extra_dirs: Vec<PathBuf>,
    /// Frozen skills summary (None = scan each turn from disk).
    frozen_summary: Option<String>,
}
```

- [ ] **Step 3: Initialize in `new()`**

Set `frozen_summary: None` in the constructor.

- [ ] **Step 4: Add `with_frozen_summary` builder**

```rust
    /// 注入冻结的 skills 摘要。设置后 `before_agent` 跳过目录扫描，
    /// 直接使用冻结内容。
    pub fn with_frozen_summary(mut self, summary: String) -> Self {
        self.frozen_summary = Some(summary);
        self
    }
```

- [ ] **Step 5: Short-circuit in `before_agent`**

At the start of `before_agent`, before the `resolve_dirs` + `list_skills` call:

```rust
    async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
        if let Some(ref summary) = self.frozen_summary {
            if !summary.trim().is_empty() {
                state.prepend_message(BaseMessage::system(summary.clone()));
            }
            return Ok(());
        }
        // ... existing code unchanged
```

- [ ] **Step 6: Verify tests pass**

```bash
cargo test -p peri-middlewares --lib -- skills 2>&1 | tail -20
```

Expected: All skills tests PASS.

---

### Task 6: Add frozen fields to `SessionState` and freeze at `session/new`

**Files:**
- Modify: `peri-tui/src/acp_server/mod.rs:39-46` (SessionState struct)
- Modify: `peri-tui/src/acp_server/requests.rs:60-102` (session/new handler)

- [ ] **Step 1: Add frozen fields to `SessionState`**

```rust
pub(crate) struct SessionState {
    #[allow(dead_code)]
    session_id: String,
    thread_id: String,
    cwd: String,
    history: Vec<BaseMessage>,
    cancel_token: Option<AgentCancellationToken>,
    // ── Frozen session data (populated at creation, never mutated) ──
    pub(crate) frozen_system_prompt: Option<String>,
    pub(crate) frozen_claude_md: Option<String>,
    pub(crate) frozen_claude_local_md: Option<String>,
    pub(crate) frozen_skill_summary: Option<String>,
    pub(crate) frozen_date: Option<String>,
}
```

- [ ] **Step 2: Update session creation in `requests.rs`**

In the `"session/new"` handler (line 60-102), after creating the session state and before sending the response, add freezing logic.

After line 82 (the `sessions.insert(...)`) and before line 83 (`info!(...)`):

```rust
            // ── Freeze system prompt data at session creation ──
            let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
            
            // Read CLAUDE.md content (resolve @import)
            let (frozen_claude_md, frozen_claude_local_md) = {
                let agents_md = peri_middlewares::AgentsMdMiddleware::new();
                // We only need the file content, not the middleware instance.
                // Reuse AgentsMdMiddleware's file-finding logic:
                let cwd_str = cwd.clone();
                let main_content = {
                    let candidates = vec![
                        std::path::Path::new(&cwd_str).join("AGENTS.md"),
                        std::path::Path::new(&cwd_str).join("CLAUDE.md"),
                        std::path::Path::new(&cwd_str).join(".claude").join("AGENTS.md"),
                    ];
                    let found = candidates.into_iter().find(|p| p.is_file());
                    found.map(|path| {
                        let content = std::fs::read_to_string(&path).unwrap_or_default();
                        // Resolve @import for CLAUDE.md
                        if path.file_name()
                            .map(|n| n.to_string_lossy().starts_with("CLAUDE"))
                            .unwrap_or(false)
                        {
                            let dir = path.parent().unwrap_or(std::path::Path::new("."));
                            let mut visited = std::collections::HashSet::new();
                            peri_middlewares::agents_md::resolve_imports(
                                &content, dir, 3, &mut visited,
                            )
                        } else {
                            content
                        }
                    })
                };
                let local_content = {
                    let local_path = std::path::Path::new(&cwd_str).join("CLAUDE.local.md");
                    if local_path.is_file() {
                        let c = std::fs::read_to_string(&local_path).unwrap_or_default();
                        if c.trim().is_empty() { None } else { Some(c) }
                    } else {
                        None
                    }
                };
                (main_content, local_content)
            };
            
            // Scan skills
            let frozen_skill_summary = {
                let skill_dirs = SkillsMiddleware::resolve_dirs_static(&cwd, &cfg.plugin_skill_dirs);
                let skills = peri_middlewares::skills::list_skills(&skill_dirs);
                if skills.is_empty() {
                    None
                } else {
                    Some(SkillsMiddleware::build_summary(&skills))
                }
            };
            
            // Build system prompt once
            let features = peri_acp::prompt::PromptFeatures::detect();
            let system_prompt = peri_acp::prompt::build_system_prompt(
                None, &cwd, features, &cfg.plugin_agent_dirs, Some(&frozen_date),
            );
            
            // Store in session
            let state = sessions.get_mut(&session_id).unwrap();
            state.frozen_system_prompt = Some(system_prompt);
            state.frozen_claude_md = frozen_claude_md;
            state.frozen_claude_local_md = frozen_claude_local_md;
            state.frozen_skill_summary = frozen_skill_summary;
            state.frozen_date = Some(frozen_date);
```

Wait — I'm calling `AgentsMdMiddleware::new()` just to read file content, but the frozen content should use the same resolution logic. Also, I need to expose `resolve_imports` and `build_summary` as public functions.

Let me reconsider. Instead of duplicating the file-reading logic, I should:

1. Expose a static method on `AgentsMdMiddleware` that reads content: `read_frozen_content(cwd: &str) -> (Option<String>, Option<String>)`
2. Expose a static method on `SkillsMiddleware` that scans: `build_frozen_summary(cwd: &str, extra_dirs: &[PathBuf]) -> Option<String>`

This is cleaner. Let me add these to Tasks 4 and 5.

Actually, let me just use helper functions that are already available. `AgentsMdMiddleware` has `find_file` which is private. I'll add a public `read_content` static method.

Let me revise the plan accordingly.

- [ ] **Step 2 (revised): Add public `read_frozen_content` to `AgentsMdMiddleware`**

In Task 4's file (`peri-middlewares/src/agents_md/mod.rs`), add:

```rust
    /// 一次性读取并冻结 CLAUDE.md 内容（解析 @import）。
    ///
    /// 返回 `(main_content, local_content)`，均可能为 `None`。
    /// 供 session 创建时调用。
    pub fn read_frozen_content(cwd: &str) -> (Option<String>, Option<String>) {
        let candidates = vec![
            Path::new(cwd).join("AGENTS.md"),
            Path::new(cwd).join("CLAUDE.md"),
            Path::new(cwd).join(".claude").join("AGENTS.md"),
        ];
        let main_content = candidates.into_iter().find(|p| p.is_file()).and_then(|path| {
            let content = std::fs::read_to_string(&path).ok()?;
            if content.trim().is_empty() {
                return None;
            }
            let is_claude_md = path
                .file_name()
                .map(|n| n.to_string_lossy().starts_with("CLAUDE"))
                .unwrap_or(false);
            if is_claude_md {
                let dir = path.parent().unwrap_or(Path::new("."));
                let mut visited = HashSet::new();
                if let Ok(canonical) = path.canonicalize() {
                    visited.insert(canonical);
                }
                Some(resolve_imports(&content, dir, 3, &mut visited))
            } else {
                Some(content)
            }
        });
        let local_content = {
            let local_path = Path::new(cwd).join("CLAUDE.local.md");
            if local_path.is_file() {
                let c = std::fs::read_to_string(&local_path).unwrap_or_default();
                if c.trim().is_empty() { None } else { Some(c) }
            } else {
                None
            }
        };
        (main_content, local_content)
    }
```

And make `resolve_imports` `pub(crate)` (currently `fn resolve_imports` → `pub(crate) fn resolve_imports`).

- [ ] **Step 2b (revised): Add public `build_frozen_summary` to `SkillsMiddleware`**

In Task 5's file (`peri-middlewares/src/skills/mod.rs`), add:

```rust
    /// 一次性扫描并构建冻结的 skills 摘要。
    ///
    /// 返回 `None` 表示无 skills 可用。
    /// 供 session 创建时调用。
    pub fn build_frozen_summary(cwd: &str, extra_dirs: &[PathBuf]) -> Option<String> {
        let dirs = Self::resolve_dirs_static(cwd, extra_dirs);
        let skills = list_skills(&dirs);
        if skills.is_empty() {
            return None;
        }
        Some(Self::build_summary(&skills))
    }
```

Add a public `resolve_dirs_static`:

```rust
    /// 在无 `&self` 时解析 skills 目录列表。
    pub fn resolve_dirs_static(cwd: &str, extra_dirs: &[PathBuf]) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = vec![
            dirs_next::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".claude")
                .join("skills"),
        ];
        if let Some(global_dir) = load_global_skills_dir() {
            dirs.push(global_dir);
        }
        dirs.push(Path::new(cwd).join(".claude").join("skills"));
        dirs.extend(extra_dirs.iter().cloned());
        dirs
    }
```

- [ ] **Step 2c: Simplify session/new freeze logic**

Now the session/new handler simply calls:

```rust
            // ── Freeze system prompt data at session creation ──
            let frozen_date = chrono::Local::now().format("%Y-%m-%d").to_string();
            
            let (frozen_claude_md, frozen_claude_local_md) =
                peri_middlewares::AgentsMdMiddleware::read_frozen_content(&cwd);
            
            let frozen_skill_summary =
                peri_middlewares::SkillsMiddleware::build_frozen_summary(
                    &cwd, &cfg.plugin_skill_dirs,
                );
            
            let features = peri_acp::prompt::PromptFeatures::detect();
            let system_prompt = peri_acp::prompt::build_system_prompt(
                None, &cwd, features, &cfg.plugin_agent_dirs, Some(&frozen_date),
            );
```

And then assign to session state.

- [ ] **Step 3: Verify compilation**

```bash
cargo build -p peri-tui 2>&1 | tail -20
```

Expected: COMPILE ERRORS (prompt.rs `execute_prompt` call missing `frozen` param — will fix in Task 7)

---

### Task 7: Wire frozen data through TUI prompt handler

**Files:**
- Modify: `peri-tui/src/acp_server/prompt.rs:28-142`

- [ ] **Step 1: Extract frozen fields from session**

After reading `cwd`, `history`, `is_empty` from session (lines 71-82), extract frozen fields:

```rust
    let (cwd, history, is_empty, frozen_system_prompt, frozen_claude_md,
         frozen_claude_local_md, frozen_skill_summary, frozen_date) = {
        let sessions = sessions.lock().await;
        let state = sessions
            .get(&session_id)
            .ok_or_else(|| AcpError::new(-32602, "session not found"))?;
        (
            state.cwd.clone(),
            state.history.clone(),
            state.history.is_empty(),
            state.frozen_system_prompt.clone(),
            state.frozen_claude_md.clone(),
            state.frozen_claude_local_md.clone(),
            state.frozen_skill_summary.clone(),
            state.frozen_date.clone(),
        )
    };
```

- [ ] **Step 2: Construct `FrozenSessionData` and pass to executor**

After constructing the broker and event_sink (lines 85-92), construct frozen data:

```rust
    let frozen = frozen_system_prompt.map(|sp| executor::FrozenSessionData {
        system_prompt: sp,
        claude_md: frozen_claude_md,
        claude_local_md: frozen_claude_local_md,
        skill_summary: frozen_skill_summary,
        date: frozen_date.unwrap_or_default(),
        is_git_repo: std::path::Path::new(&cwd).join(".git").exists(),
    });
```

- [ ] **Step 3: Pass `frozen` to `executor::execute_prompt`**

In the `executor::execute_prompt` call (line 93-113), add `frozen` after `content`:

```rust
    let result = executor::execute_prompt(
        &provider_snapshot,
        peri_config_snapshot,
        &cwd,
        content,
        frozen,                              // NEW — between content and history
        history,
        is_empty,
        // ... rest unchanged
    )
    .await;
```

- [ ] **Step 4: Verify compilation**

```bash
cargo build -p peri-tui 2>&1 | tail -20
```

Expected: SUCCESS or minor warnings only.

---

### Task 8: Update `SessionState` initialization for frozen default

**Files:**
- Modify: `peri-tui/src/acp_server/requests.rs:73-82` (session state creation)

- [ ] **Step 1: Initialize frozen fields as `None` in session creation**

Update the `SessionState` construction in `session/new` handler (line 73-82):

```rust
            sessions.insert(
                session_id.clone(),
                SessionState {
                    session_id: session_id.clone(),
                    thread_id: thread_id.clone(),
                    cwd: cwd.clone(),
                    history: Vec::new(),
                    cancel_token: None,
                    frozen_system_prompt: None,
                    frozen_claude_md: None,
                    frozen_claude_local_md: None,
                    frozen_skill_summary: None,
                    frozen_date: None,
                },
            );
```

- [ ] **Step 2: Build and verify**

```bash
cargo build -p peri-tui 2>&1 | tail -10
```

Expected: SUCCESS.

---

### Task 9: Integration tests and regression verification

**Files:**
- No new files; run existing tests

- [ ] **Step 1: Run full test suite**

```bash
cargo test 2>&1 | tail -30
```

Expected: All existing tests PASS.

- [ ] **Step 2: Run specific affected crate tests**

```bash
cargo test -p peri-acp --lib 2>&1 | tail -15
cargo test -p peri-middlewares --lib 2>&1 | tail -15
```

Expected: All PASS.

- [ ] **Step 3: Verify lefthook pre-commit**

```bash
lefthook run pre-commit 2>&1 | tail -20
```

Expected: fmt, check, clippy all pass.

---

### Task 10: Update CLAUDE.md documentation

**Files:**
- Modify: `CLAUDE.md:65,78-99` (system prompt section)

- [ ] **Step 1: Update system prompt section**

Replace lines 65 (the system prompt description paragraph) with frozen-aware documentation:

```markdown
**系统提示词稳定性（第一优先级）**：会话开始后，系统提示词必须完全稳定、不可变更。`session/new` 时一次性构建完整 system prompt 字符串、冻结日期、读取 CLAUDE.md 内容并解析 `@import`、扫描 skills 摘要，存入 `SessionState` 作为 frozen data。后续所有 `session/prompt` 轮次直接使用 frozen 值，不再重建。唯一例外是 SubAgent 通过 `system_builder` 闭包调用 `build_system_prompt` 时传入 frozen_date 确保日期稳定。

**系统提示词**：`build_system_prompt(overrides, cwd, features, extra_agent_dirs, frozen_date)` 合成。`session/new` 时调用一次，传入 `Some(frozen_date)` 冻结日期和 `is_git_repo`。后续轮次不使用此函数重建，直接使用 `SessionState.frozen_system_prompt`。段落文件位于 `peri-tui/prompts/sections/`（共 11 个：01-07 + 10-13），`peri-acp` 通过 `concat!(env!("CARGO_MANIFEST_DIR"), "/../peri-tui/prompts/sections/")` 交叉引用。`PromptFeatures` 控制条件段落注入。静态段落（01-06）与动态段落（07_env + feature-gated 10-13）通过 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 边界标记分隔。
```

- [ ] **Step 2: Update data flow section**

Add frozen data flow to the ACP/TUI data flow (around line 123):

```markdown
**Frozen Data Flow**：`session/new` → `read_frozen_content` (CLAUDE.md + @import) + `build_frozen_summary` (skills) + `build_system_prompt` (system prompt) → `SessionState.frozen_*` → `TUI prompt::execute_prompt` → `FrozenSessionData` → `executor::execute_prompt` → `AcpAgentConfig.frozen_*` → `AgentsMdMiddleware::with_frozen_content` / `SkillsMiddleware::with_frozen_summary` / `system_builder(frozen_date)`。
```

---

### Task 11: Final verification

- [ ] **Step 1: Full clean build**

```bash
cargo clean && cargo build 2>&1 | tail -10
```

Expected: SUCCESS.

- [ ] **Step 2: Full test suite**

```bash
cargo test 2>&1 | tail -5
```

Expected: `test result: ok.`

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat: freeze system prompt at session creation for stability

Build system prompt once at session/new, store frozen CLAUDE.md content,
skills summary, and date in SessionState. All subsequent turns reuse frozen
values. Middlewares (AgentsMdMiddleware, SkillsMiddleware) accept frozen
content via constructor — no new lifecycle hooks needed.

This guarantees system prompt never changes within a session, fixing:
- Date drift across days breaking OpenAI prompt cache
- CLAUDE.md edits mid-session leaking into agent context
- Skills directory changes mid-session altering behavior

Co-Authored-By: deepseek-v4-pro <deepseek-ai@claude-code-best.win>"
```
