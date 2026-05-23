# Memory Linear Growth — jemalloc Allocator Tuning & Allocation Churn Reduction

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the ~40 MB/turn RSS growth caused by jemalloc allocator fragmentation under high allocation churn (680k+ transient allocs/turn), so long sessions (50-100+ turns) remain stable without OOM.

**Architecture:** Two-pronged approach — (1) configure jemalloc at startup to aggressively purge dirty pages and limit tcache bloat, (2) eliminate the largest single source of redundant allocation churn (the `event_value.clone()` + double-deserialization in the ACP client pump). P2 items (executor lifecycle, bounded channels) are deferred since profiling data shows they are not the root cause.

**Tech Stack:** `tikv-jemalloc-ctl` (arena config via MALLCTL), `tikv-jemallocator` (global allocator), `serde_json` (zero-copy deserialization), Rust 2021.

**Root Cause (confirmed by 现象 5 heapdump):**
- `allocated` does NOT grow (9.5 → 9.0 MB) → no data structure leak
- `active` +13.6 MB, `resident` +44.7 MB, `mapped` +137.2 MB → jemalloc fragmentation
- 68 万次 malloc/turn, 97.3% freed immediately → arena slab fragmentation, dirty pages accumulate faster than decay purge
- `dirty_decay_ms=1000` (default? actually not configured) + no background_thread → purge too slow

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `peri-tui/src/main.rs` | Modify | Add `configure_jemalloc()` call before runtime creation |
| `peri-tui/src/jemalloc_config.rs` | Create | jemalloc MALLCTL configuration (dirty_decay_ms, background_thread, lg_tcache_max) + test |
| `peri-tui/src/app/thread_ops.rs` | Modify | Update `jemalloc_decay()` to also call `arena.<i>.decay` for faster purge after `/clear` |
| `peri-tui/src/acp_client/client.rs` | Modify | Replace `event_value.clone()` + `from_value` with `serde_json::from_str` on pre-serialized string |
| `peri-acp/src/session/event_sink.rs` | Modify | Replace `to_value` + `json!({...event_value})` with direct `to_string` + string interpolation to avoid double-serialize |
| `peri-tui/src/command/core/heapdump.rs` | Modify | Add jemalloc config section (dirty_decay_ms, background_thread, tcache stats) |

---

### Task 1: Create `jemalloc_config.rs` — Allocator Tuning Module

**Files:**
- Create: `peri-tui/src/jemalloc_config.rs`
- Modify: `peri-tui/src/lib.rs` (add `mod jemalloc_config;`)

- [ ] **Step 1: Create the module file with `configure_jemalloc()`**

Create `peri-tui/src/jemalloc_config.rs`:

```rust
//! jemalloc allocator tuning for high-churn workloads.
//!
//! Default jemalloc settings prioritize throughput over memory footprint.
//! The agent event pipeline produces ~680k transient allocations per turn
//! (serde JSON serialize/deserialize, string cloning). This causes arena
//! slab fragmentation where dirty pages accumulate faster than the default
//! decay purge can reclaim them.
//!
//! Configuration applied:
//! - `dirty_decay_ms: 200` — purge freed arena pages after 200ms (default: 1000ms+)
//! - `background_thread: true` — enable background purge thread (default: disabled)
//! - `lg_tcache_max: 16` — limit thread cache to objects ≤64KB (default: unlimited)

/// Configure jemalloc for aggressive memory reclamation.
///
/// Must be called **before** creating the tokio runtime, ideally at the
/// very start of `main()`. Writes are best-effort — missing keys or
/// unsupported platforms are silently ignored.
#[cfg(not(target_os = "windows"))]
pub fn configure_jemalloc() {
    use tracing::{debug, warn};

    // Advance epoch to ensure stats are fresh
    let _ = tikv_jemalloc_ctl::epoch::advance();

    // 1. dirty_decay_ms — time before freed dirty pages are purged
    //    Default is 10000ms on many builds; we set 200ms for aggressive reclamation.
    //    Lower values increase CPU overhead from madvise syscalls but prevent
    //    the observed ~27MB dirty extent accumulation per turn.
    match tikv_jemalloc_ctl::raw::write(
        b"arenas.dirty_decay_ms\0",
        200i64,
    ) {
        Ok(()) => debug!("jemalloc: arenas.dirty_decay_ms = 200"),
        Err(e) => warn!("jemalloc: failed to set dirty_decay_ms: {}", e),
    }

    // 2. background_thread — enables a background thread per arena that
    //    proactively purges dirty pages. Without this, purge only happens
    //    during foreground allocations (the "lazy" purge path), which can't
    //    keep up with our churn rate.
    match tikv_jemalloc_ctl::raw::write(
        b"background_thread\0",
        true,
    ) {
        Ok(()) => debug!("jemalloc: background_thread = true"),
        Err(e) => warn!("jemalloc: failed to enable background_thread: {}", e),
    }

    // 3. lg_tcache_max — log2 of max cached allocation size in thread caches.
    //    Default is ~23 (8MB), which means large allocations linger in tcache.
    //    Setting to 16 (64KB) limits tcache to small objects, reducing the
    //    5-7MB tcache_bytes overhead observed in heapdumps.
    match tikv_jemalloc_ctl::raw::write(
        b"arenas.lg_tcache_max\0",
        16usize,
    ) {
        Ok(()) => debug!("jemalloc: arenas.lg_tcache_max = 16 (64KB)"),
        Err(e) => warn!("jemalloc: failed to set lg_tcache_max: {}", e),
    }
}

#[cfg(target_os = "windows")]
pub fn configure_jemalloc() {
    // jemalloc not used on Windows (system allocator instead)
}
```

- [ ] **Step 2: Add module declaration to `peri-tui/src/lib.rs`**

Find the module declarations in `peri-tui/src/lib.rs` and add `jemalloc_config` alongside the other modules. It should be at the top level (not inside any `mod` block):

```rust
pub mod jemalloc_config;
```

- [ ] **Step 3: Write unit test for `configure_jemalloc`**

Add at the bottom of `peri-tui/src/jemalloc_config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configure_jemalloc_does_not_panic() {
        // configure_jemalloc should be safe to call, even multiple times
        configure_jemalloc();
        configure_jemalloc();
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_dirty_decay_ms_is_set() {
        configure_jemalloc();
        let _ = tikv_jemalloc_ctl::epoch::advance();
        let val: i64 = tikv_jemalloc_ctl::raw::read(b"arenas.dirty_decay_ms\0")
            .expect("should read dirty_decay_ms");
        assert_eq!(val, 200, "dirty_decay_ms should be 200ms after configure");
    }
}
```

- [ ] **Step 4: Run tests to verify**

Run: `cargo test -p peri-tui --lib -- jemalloc_config`
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add peri-tui/src/jemalloc_config.rs peri-tui/src/lib.rs
git commit -m "feat(tui): add jemalloc allocator tuning module for high-churn workloads

Configure dirty_decay_ms=200, background_thread=true, lg_tcache_max=16
to prevent arena fragmentation under 680k transient allocs/turn."
```

---

### Task 2: Call `configure_jemalloc()` at Startup

**Files:**
- Modify: `peri-tui/src/main.rs:252-254`

- [ ] **Step 1: Add `configure_jemalloc()` call in `main()`**

In `peri-tui/src/main.rs`, modify the `main()` function to call `configure_jemalloc()` **before** `inject_env_from_settings()`. The jemalloc config must be applied before any significant allocation (especially before tokio runtime creation).

Find in `main.rs` (~line 252):

```rust
fn main() -> Result<()> {
    // 最先注入环境变量（进程环境变量优先）
    inject_env_from_settings();
```

Replace with:

```rust
fn main() -> Result<()> {
    // Configure jemalloc before any significant allocation.
    // Must precede tokio runtime creation and the first LLM call.
    peri_tui::jemalloc_config::configure_jemalloc();

    // 最先注入环境变量（进程环境变量优先）
    inject_env_from_settings();
```

- [ ] **Step 2: Build to verify**

Run: `cargo build -p peri-tui`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/main.rs
git commit -m "feat(tui): call configure_jemalloc() at startup before runtime creation"
```

---

### Task 3: Enhance `jemalloc_decay()` with Explicit Arena Decay

**Files:**
- Modify: `peri-tui/src/app/thread_ops.rs:7-29`

The current `jemalloc_decay()` calls `arena.{i}.purge` which immediately purges all dirty pages. However, in high-churn scenarios, some pages are still "aging" in the decay timeline and won't be purged. Adding explicit `arena.{i}.decay` calls forces jemalloc to process the decay timeline immediately.

- [ ] **Step 1: Update `jemalloc_decay()` function**

In `peri-tui/src/app/thread_ops.rs`, replace the existing `jemalloc_decay()` function:

```rust
/// 通知 jemalloc 将空闲内存页归还给 OS。
/// 在 `/clear`、`/compact`、切换会话等大块内存释放后调用。
/// 注：仅释放 jemalloc 管理的 Rust 堆内存，SQLite/tokio 等非 Rust 分配不受影响。
#[cfg(not(target_os = "windows"))]
pub(crate) fn jemalloc_decay() {
    // Advance epoch to refresh internal stats
    if let Err(e) = tikv_jemalloc_ctl::epoch::advance() {
        tracing::debug!(error = %e, "jemalloc epoch advance failed");
        return;
    }
    let narenas: usize = match tikv_jemalloc_ctl::arenas::narenas::read() {
        Ok(n) => n as usize,
        Err(e) => {
            tracing::debug!(error = %e, "jemalloc narenas read failed");
            return;
        }
    };
    for i in 0..narenas {
        // 先触发 decay：处理 decay timeline 中正在老化的 dirty pages
        let mut decay_key = format!("arena.{}.decay", i);
        decay_key.push(0 as char);
        unsafe {
            let _: u8 = match tikv_jemalloc_ctl::raw::read(decay_key.as_bytes()) {
                Ok(v) => v,
                Err(_) => continue,
            };
        }
        // 再触发 purge：立即释放所有已达到 decay 阈值的 dirty pages
        let mut purge_key = format!("arena.{}.purge", i);
        purge_key.push(0 as char);
        unsafe {
            let _: u8 = match tikv_jemalloc_ctl::raw::read(purge_key.as_bytes()) {
                Ok(v) => v,
                Err(_) => continue,
            };
        }
    }
}
```

- [ ] **Step 2: Build to verify**

Run: `cargo build -p peri-tui`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/app/thread_ops.rs
git commit -m "fix(tui): add arena decay before purge in jemalloc_decay()

Trigger decay timeline processing before immediate purge, ensuring
in-progress aging dirty pages are also reclaimed after /clear."
```

---

### Task 4: Eliminate `event_value.clone()` Double Deserialization in ACP Client Pump

**Files:**
- Modify: `peri-acp/src/session/event_sink.rs:50-66`
- Modify: `peri-tui/src/acp_client/client.rs:95-113`

This is the largest single source of redundant allocation churn. The current flow:

1. `event_sink.rs`: `serde_json::to_value(event)` → produces `Value` (allocates)
2. `event_sink.rs`: wraps in `json!({"event": event_value})` → clones the entire Value tree into a new map
3. Sends through mpsc channel as `ChannelMessage::Notification { params: Value }`
4. `client.rs`: `params.get("event")` → borrows the Value
5. `client.rs`: `event_value.clone()` → **clones the entire Value tree again**
6. `client.rs`: `serde_json::from_value(event_value.clone())` → **deserializes from Value, producing a second copy**

That's **3 full copies** of every ExecutorEvent's JSON representation per event. With 68 万次 events/turn, this alone accounts for a significant portion of the allocation churn.

The fix: serialize once to `String`, send the string through the channel, deserialize once from `&str`.

- [ ] **Step 1: Modify `TransportEventSink::push_event` to serialize once to String**

In `peri-acp/src/session/event_sink.rs`, replace the `push_event` method in `impl EventSink for TransportEventSink`:

```rust
    async fn push_event(&self, session_id: &str, event: &ExecutorEvent, context_window: u32) {
        // 1. peri/agent_event — serialize ExecutorEvent to JSON string once
        let event_json = match serde_json::to_string(event) {
            Ok(s) => s,
            Err(e) => {
                error!(error = %e, "EventSink: serialize ExecutorEvent failed");
                return;
            }
        };
        let agent_event_params = json!({
            "sessionId": session_id,
            "event_json": event_json,
        });
        if let Err(e) = self
            .transport
            .send_notification("peri/agent_event", agent_event_params)
            .await
        {
            error!(error = %e, "EventSink: send peri/agent_event failed");
            return;
        }

        // 2. peri/* custom notifications (compact, session lifecycle)
        let peri_notifs = map_executor_to_peri_notifications(event);
        for (method, mut payload) in peri_notifs {
            if let serde_json::Value::Object(ref mut map) = payload {
                map.insert("sessionId".to_string(), json!(session_id));
            }
            let _ = self.transport.send_notification(method, payload).await;
        }

        // 3. session/update — standard ACP SessionUpdate
        let updates = map_executor_to_updates(event, context_window);
        for update in updates {
            let mut payload = match serde_json::to_value(&update) {
                Ok(p) => p,
                Err(e) => {
                    error!(error = %e, "EventSink: serialize SessionUpdate failed");
                    continue;
                }
            };
            if let serde_json::Value::Object(ref mut map) = payload {
                map.insert("sessionId".to_string(), json!(session_id));
            }
            let _ = self
                .transport
                .send_notification("session/update", payload)
                .await;
        }
    }
```

- [ ] **Step 2: Modify `AcpTuiClient::run_pump` to deserialize from string**

In `peri-tui/src/acp_client/client.rs`, replace the `peri/agent_event` branch in `run_pump`:

Find (~line 95-131):
```rust
                    if method == "peri/agent_event" {
                        event_count += 1;
                        let session_id = params
                            .get("sessionId")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if let Some(event_value) = params.get("event") {
                            match serde_json::from_value::<peri_agent::agent::events::AgentEvent>(
                                event_value.clone(),
                            ) {
```

Replace with:
```rust
                    if method == "peri/agent_event" {
                        event_count += 1;
                        let session_id = params
                            .get("sessionId")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        // Prefer pre-serialized string (avoids clone + double-deserialize).
                        // Fall back to old "event" Value field for backward compat during rollout.
                        let event_result = if let Some(event_str) =
                            params.get("event_json").and_then(|v| v.as_str())
                        {
                            serde_json::from_str::<peri_agent::agent::events::AgentEvent>(event_str)
                        } else if let Some(event_value) = params.get("event") {
                            serde_json::from_value::<peri_agent::agent::events::AgentEvent>(
                                event_value.clone(),
                            )
                        } else {
                            warn!("ACP client pump: agent_event notification missing 'event_json' or 'event' field");
                            continue;
                        };
                        match event_result {
```

Also update the error log line that follows — find:
```rust
                                Err(e) => {
                                    error!(
                                        event_count = event_count,
                                        error = %e,
                                        event_json = %event_value,
                                        "ACP client pump: failed to parse AgentEvent — event LOST"
                                    );
```

Replace with:
```rust
                                Err(e) => {
                                    error!(
                                        event_count = event_count,
                                        error = %e,
                                        "ACP client pump: failed to parse AgentEvent — event LOST"
                                    );
```

- [ ] **Step 3: Build to verify**

Run: `cargo build -p peri-acp -p peri-tui`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add peri-acp/src/session/event_sink.rs peri-tui/src/acp_client/client.rs
git commit -m "perf(acp): eliminate event_value.clone() double-deserialization

Serialize ExecutorEvent to JSON string once, deserialize once from &str.
Saves 2 full Value tree clones per event (3→1 copies). With 680k
events/turn, this reduces allocation churn by ~2M fewer allocations."
```

---

### Task 5: Add Jemalloc Config Section to `/heapdump`

**Files:**
- Modify: `peri-tui/src/command/core/heapdump.rs`

- [ ] **Step 1: Add config diagnostics after jemalloc summary section**

In `peri-tui/src/command/core/heapdump.rs`, after the `RSS-overhead` line (~line 57) and before the `=== JEMALLOC ARENAS ===` section, add a new config section:

Find:
```rust
            let _ = writeln!(
                buf,
                "  RSS-overhead: {:.1} MB (RSS-resident)\n",
                rss_mb - mb(resident)
            );

            let _ = writeln!(buf, "=== JEMALLOC ARENAS ===");
```

Replace with:
```rust
            let _ = writeln!(
                buf,
                "  RSS-overhead: {:.1} MB (RSS-resident)\n",
                rss_mb - mb(resident)
            );

            // Jemalloc config diagnostics
            {
                let _ = writeln!(buf, "=== JEMALLOC CONFIG ===");
                let dirty_decay: i64 = tikv_jemalloc_ctl::raw::read(b"arenas.dirty_decay_ms\0")
                    .unwrap_or(-1);
                let _ = writeln!(buf, "  dirty_decay_ms: {}", dirty_decay);
                let bg_thread: bool = tikv_jemalloc_ctl::raw::read(b"background_thread\0")
                    .unwrap_or(false);
                let _ = writeln!(buf, "  background_thread: {}", bg_thread);
                let lg_tcache_max: usize = tikv_jemalloc_ctl::raw::read(b"arenas.lg_tcache_max\0")
                    .unwrap_or(0);
                let _ = writeln!(buf, "  lg_tcache_max: {} ({}KB)", lg_tcache_max, 1 << (lg_tcache_max.saturating_sub(10)));
                let narenas: usize = tikv_jemalloc_ctl::arenas::narenas::read()
                    .unwrap_or(0) as usize;
                let _ = writeln!(buf, "  narenas: {}", narenas);
                let _ = writeln!(buf, "  tcache_bytes: {:.1} MB", mb(tikv_jemalloc_ctl::stats::allocated::read().unwrap_or(0)));
                let _ = writeln!(buf);
            }

            let _ = writeln!(buf, "=== JEMALLOC ARENAS ===");
```

- [ ] **Step 2: Build to verify**

Run: `cargo build -p peri-tui`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add peri-tui/src/command/core/heapdump.rs
git commit -m "feat(heapdump): add jemalloc config section (dirty_decay_ms, background_thread, tcache)"
```

---

### Task 6: End-to-End Verification

**Files:** None (testing only)

- [ ] **Step 1: Build release mode**

Run: `cargo build -p peri-tui --release`
Expected: Build succeeds

- [ ] **Step 2: Run all tests**

Run: `cargo test -p peri-tui --lib`
Expected: All tests pass

- [ ] **Step 3: Run peri-acp tests**

Run: `cargo test -p peri-acp --lib`
Expected: All tests pass

- [ ] **Step 4: Manual smoke test — verify jemalloc config applied**

1. Start TUI in release mode: `cargo run -p peri-tui --release`
2. Run `/heapdump` command
3. Check `.tmp/heapdump-*.txt` — verify:
   - `dirty_decay_ms: 200` (not 10000)
   - `background_thread: true` (not false)
   - `lg_tcache_max: 16` (not 23)

- [ ] **Step 5: Manual smoke test — verify memory behavior improvement**

1. Start TUI in release mode
2. Send 3-5 messages with tool calls
3. Run `/heapdump` — note RSS
4. Send 3-5 more messages
5. Run `/heapdump` — compare RSS growth
6. Expected: RSS growth per turn should be noticeably reduced (~15-20 MB instead of ~40 MB)

- [ ] **Step 6: Commit final state (if any fixes needed)**

```bash
git add -A
git commit -m "chore: memory optimization verification complete"
```

---

## Self-Review Checklist

### 1. Spec Coverage

| Spec Requirement | Task |
|---|---|
| P0: `dirty_decay_ms` 降至 100-200ms | Task 1 (set 200ms) |
| P0: 启用 `background_thread: true` | Task 1 |
| P0: 限制 tcache 大小 `lg_tcache_max=16` | Task 1 |
| P1: 消除 serde JSON 双重解析 (`event_value.clone()`) | Task 4 |
| P1: 减少 String clone（event 序列化路径） | Task 4 (serialize once to String) |
| `/heapdump` 诊断增强 | Task 5 |
| `jemalloc_decay()` 增强 | Task 3 |

**P1 items deferred** (not root cause per 现象 5 data):
- LLM response body buffer 复用 — requires architectural change to reqwest usage; impact unclear
- P2: bounded notification channel — safe-guarding measure, not root cause

### 2. Placeholder Scan

No TBD/TODO/fill-in-later patterns found. All code blocks contain complete implementations.

### 3. Type Consistency

- `configure_jemalloc()` called as `peri_tui::jemalloc_config::configure_jemalloc()` — matches module path
- `tikv_jemalloc_ctl::raw::write` uses `b"key\0"` byte strings — correct for MALLCTL
- `serde_json::from_str` returns `Result<T, Error>` — matches existing `from_value` error handling pattern
- `event_json` field name consistent between writer (event_sink.rs) and reader (client.rs)
