# 长对话内存持续增长，无自动释放机制

**状态**：Open
**优先级**：高
**类型**：性能
**创建日期**：2026-05-22
**更新日期**：2026-06-01

## 问题描述

Agent 对话过程中，内存（RSS）随对话轮数线性增长，每轮约增长 40 MB，且不会自动下降。持续 50-100 轮对话后可达数 GB，最终导致 OOM。**debug 和 release 模式下均表现相同**：`/clear` 后 RSS 不会下降。

**已尝试的缓解措施**（均未解决）：
- jemalloc 调优（`dirty_decay_ms=200`, `lg_tcache_max=16`）→ 效果有限
- 切换为 mimalloc（`PAGE_RESET/DECOMMIT/BACKGROUND_THREAD`）→ 现象未改善
- `/clear` 时调用 `alloc_collect()`（`mi_collect(true)` 或 `jemalloc_decay()`）→ RSS 不降
- AgentPool LLM 实例复用 → 减少瞬态分配但 RSS 增长模式不变

**当前结论**：RSS 增长中大部分不是 Rust 堆上的活跃对象（`allocated` 不增长），而是**分配器碎片化 + 运行时基础设施持有**（tokio 线程栈、reqwest HTTP 连接池、TLS session 缓冲）。详见下方根因分析。

## 症状详情

| 维度 | 观察 |
|------|------|
| 增长模式 | 对话轮数相关，非时间相关 |
| 增长速度 | ~40 MB/轮 |
| 是否自动下降 | 否，只增不减 |
| 触发场景 | 各类操作均有（SubAgent/大文件读取/纯文本） |
| 手动缓解 | `/clear` (new_thread) **无法释放**（debug/release 均如此） |
| 分配器历史 | jemalloc → 系统默认 → **mimalloc (当前)**，均表现相同 |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启动 TUI，正常对话
  2. 每发一轮消息，观察 RSS 增长
  3. 持续对话数轮后，RSS 持续上升
  4. `/clear` 后 RSS 不下降
- **环境**：macOS，Rust 2021，任何模型下均出现
- **诊断工具**：无（`/heapdump` 已在 mimalloc 迁移中删除，mimalloc 无等价工具）

### 现象 2（2026-05-23）：debug/release 均无法通过 `/clear` 释放

debug 和 release 模式下 `/clear` 后 RSS 均不下降。排除"debug 模式分配器不归还内存"的推测。初步怀疑数据结构泄漏 → 后被推翻（`allocated` 不增长）。

### 现象 3（2026-05-30）：mimalloc 迁移后问题持续

已从 jemalloc 切换至 mimalloc（`MIMALLOC_PAGE_RESET=1`, `MIMALLOC_DECOMMIT=1`, `MIMALLOC_BACKGROUND_THREAD=1`），并在 `/clear`、`/compact`、切换会话后调用 `alloc_collect()` → `mi_collect(true)`。

**结果**：RSS 线性增长模式未发生变化，`/clear` 后 RSS 仍然不降。详见 `spec/issues/2026-05-30-retry-mimalloc-with-mi-options.md`（已 Fixed）。

### 现象 4（2026-05-30）：问题持续确认

wrap_map 增量优化（Plan 1）实施后，内存增长模式未发生变化。确认与近期渲染优化无关。

## 根因分析

### 核心发现（jemalloc 时代的 heapdump 数据沉淀）

经过多轮 heapdump 对比（详见下方历史附录），关键发现：

1. **`allocated` 不增长**（9.5 → 9.0 MB）：Rust 堆活跃对象并未随对话轮数线性增长。ACP executor / State.messages 不是泄漏源
2. **`/clear` 后 TUI 数据归零**：agent_state_messages=0, pipeline_completed=0, view_messages=0 — TUI 前端完全释放
3. **free/malloc 比 97.3%**：每轮 68 万次分配中绝大部分已释放，不是传统意义的内存泄漏
4. **增长来自两部分**：
   - 分配器碎片化：已 free 但未归还 OS 的页面（jemalloc dirty pages / mimalloc free segments）
   - 运行时基础设施：tokio 线程栈（8MB×N threads）、reqwest HTTP 连接池、TLS session 缓存

### 为什么 mimalloc 也没解决

mimalloc 的 `DECOMMIT` 和 `BACKGROUND_THREAD` 在 macOS 上的实际效果待验证。`mi_collect(true)` 是同步回收但可能需要多次调用才能触发 OS 归还。手动 `/clear` 路径已调用 `alloc_collect()`，但 RSS 未降——说明：

- 要么 mimalloc 在 macOS 上也受限于同样的 OS 层面限制（macOS 的 `madvise(MADV_FREE)` 不立即回收物理页）
- 要么非分配器开销（tokio/reqwest）占比太大，分配器层面的优化无法触及

### 当前 RSS 构成（估算，基于历史数据）

```
RSS 增长/轮 (~40 MB)
├── 分配器碎片化 (~17 MB)        ← mimalloc DECOMMIT 理论上可缓解，实际待验证
├── 非分配器运行时 (~20 MB)       ← tokio 线程栈 + reqwest 连接池 + TLS 缓冲
│   ├── reqwest HTTP 连接池       ← 默认无限制，TLS session 不释放
│   ├── tokio runtime 线程栈      ← 8MB/线程 × worker threads
│   └── hyper 响应体缓冲区        ← streaming response 的 Bytes 积累
└── 分配器元数据 (~3 MB)          ← 不可控
```

## 修复方向

### P0：降低非分配器运行时开销（分配器已无法进一步优化）

1. **限制 reqwest 连接池**：检查 `ClientBuilder` 的 `pool_max_idle_per_host` 和 `pool_idle_timeout`。默认无限制的连接池持续持有 TLS session。建议 `pool_max_idle_per_host(2)` + `pool_idle_timeout(30s)`
2. **减小 tokio 线程栈**：默认 8MB/线程，N 个 worker threads 就有 N×8MB 纯栈开销。检查是否可用 `thread_stack_size` 减半
3. **审计 hyper 响应体缓冲区**：LLM streaming response 的 `Bytes` 是否在 response 完成后及时释放

### P1：减少每轮分配 churn（治本）

4. **消除 serde JSON 双重解析**：`run_pump` 中 `serde_json::from_value(event_value.clone())` 先 clone 再反序列化，改为零拷贝解析
5. **减少 String clone**：68 万次 malloc 中大量是字符串克隆（event 序列化/反序列化路径），审计 `AcpNotification::AgentEvent` 构造路径中的 clone
6. **LLM response body buffer 复用**：考虑用 `Bytes` pool 或复用已有 buffer

### P2：已验证/已否决的方案

7. ✅ **jemalloc 调优**（`dirty_decay_ms=200`, `lg_tcache_max=16`）— 已实施，效果有限
8. ✅ **mimalloc 替代**（`PAGE_RESET/DECOMMIT/BACKGROUND_THREAD`）— 已实施，未改善
9. ✅ **系统分配器对照实验** — 已测试，同样表现，排除分配器独有因素
10. ✅ **AgentPool LLM 复用** — 已实施，减少瞬态分配但 RSS 增长模式不变
11. ❌ **macOS `background_thread`** — jemalloc 的 `background_thread` 和 mimalloc 的 `BACKGROUND_THREAD` 在 macOS 上实际效果待验证（macOS 线程模型限制）
12. ❌ ~~**`/heapdump`**~~ — 已随 jemalloc 一起删除，mimalloc 无等价内置工具

### P3：备选方案

13. **考虑定期重启策略**：对于长时间运行的 TUI 会话，在 N 轮对话后提示用户重启或自动重置 runtime
14. **外部内存 profiling**：使用 macOS Instruments (Allocations/Leaks) 或 `sample` 命令获取非分配器内存分布

## 2026-06-01 深度排查：分配风暴源定位

通过 3 个并行 agent 对 `peri-agent`/`peri-acp`/`peri-tui`/`peri-middlewares` 全量扫描，定位到碎片化的根本原因是**大块临时分配/释放循环**。

### 根因链条

```
RSS ~200MB = 活跃对象(~9MB) + 分配器碎片(~70MB) + 非分配器运行时(~120MB)
                                ↑                        ↑
                        ← 风暴源 A/B →             ← 进程级资源 →
```

### 风暴源 A（最严重）：`before_agent` 每轮 ReAct 迭代全量 clone

**位置**：`peri-middlewares/src/subagent/mod.rs:456-457`

```rust
if let Some(ref pm) = self.parent_messages {
    *pm.write() = state.messages().to_vec();  // 全量深拷贝
}
```

- **触发频率**：每个 ReAct **迭代**（不是 SubAgent 调用），一次对话可能 10-50 次迭代
- **影响**：500 条消息时每次 clone ~1-2MB × 50 次迭代 = 50-100MB 临时分配/释放
- **根因**：这些大型 Vec 跨越多个内存页，释放后页面因相邻存活对象（MCP Pool、ToolSearchIndex 等）无法归还 OS

### 风暴源 B：`state.history.clone()` 每轮 prompt 全量克隆

**位置**：`peri-tui/src/acp_server/prompt.rs:86`

```rust
state.history.clone()  // 可用 std::mem::take 替代
```

- **影响**：1-2MB/轮，但可用 `std::mem::take` 完全消除

### 风暴源 C：`prompt_locks` HashMap 泄漏

**位置**：`peri-tui/src/acp_server/mod.rs:87`

- `session/prompt` 时 `or_insert` 创建 lock，但 `session/close` **不清理**
- 每次 `/clear` = 1 废弃 + 1 新 lock 条目（单条目 ~50 bytes，非主因但需修复）

### 风暴源 D：`Arc::try_unwrap` 失败导致 AgentPool 残留

**位置**：`peri-tui/src/acp_server/mod.rs:165`

- `build_agent` 将 pool Arc clone 到中间件链 → 引用计数 > 1 → `try_unwrap` 失败
- 失败时 AgentPool 中 `reqwest::Client`（~1-2MB）+ LLM 缓存不会被恢复或 invalidate

### 消息历史三重存储（/clear 可清理，但正常使用时占 3x 内存）

| # | 位置 | 生命周期 |
|---|------|----------|
| 1 | `AcpSession.state_messages`（ACP 层） | session 级 |
| 2 | `AgentComm.origin_messages`（TUI 层） | session 级 |
| 3 | `MessagePipeline.completed`（TUI pipeline） | session 级 |

### 修复方向（按影响排序）

| 优先级 | 修复 | 预估效果 | 复杂度 |
|--------|------|----------|--------|
| P0 | `before_agent` 延迟 clone：仅在 SubAgent 工具实际调用时才 clone | 减少 50-100MB/轮 | 中 |
| P0 | `prompt.rs:86` 用 `std::mem::take` 替代 `.clone()` | 减少 1-2MB/轮 | 低 |
| P1 | `session/close` 清理 prompt_locks | 消除 HashMap 泄漏 | 低 |
| P1 | `Arc::try_unwrap` 失败时 `pool.lock().invalidate()` | 释放残留 LLM 实例 | 低 |

## 涉及文件（当前代码库）

| 文件 | 角色 |
|------|------|
| `peri-tui/src/main.rs:266-268` | `#[global_allocator]` mimalloc + tokio runtime 配置 |
| `peri-tui/src/mimalloc_config.rs` | `init_mimalloc_conf()` + `alloc_collect()` |
| `peri-tui/src/app/thread_ops.rs` | `/clear` 路径（new_thread） |
| `peri-tui/src/acp_server/mod.rs:87` | prompt_locks HashMap（泄漏源 C） |
| `peri-tui/src/acp_server/prompt.rs:86` | history.clone()（风暴源 B） |
| `peri-tui/src/acp_server/mod.rs:165` | Arc::try_unwrap 失败（风暴源 D） |
| `peri-middlewares/src/subagent/mod.rs:456-457` | before_agent 全量 clone（风暴源 A） |
| `peri-acp/src/session/executor.rs` | execute_prompt 内 event channel + spawn 闭包 |
| `peri-acp/src/session/agent_pool.rs:47` | subagent_llm_cache |

## 关联 Issue

- `spec/issues/2026-05-30-retry-mimalloc-with-mi-options.md` — mimalloc 迁移方案（Fixed，已实施但未改善）
- `spec/issues/2026-05-30-render-event-unbounded-channel.md` — RenderThread 事件通道（Fixed）
- `spec/issues/2026-05-30-cpu-spike-on-session-restore.md` — 会话恢复 CPU 暴涨（Partial）

---

## 历史附录：jemalloc 时代的诊断数据

以下数据来自 2026-05-23 的 `/heapdump` 定量分析，基于 **jemalloc** 分配器。当前已切换至 mimalloc，这些数据**不可复现**，仅作为历史参考保留。

### jemalloc 现象 A：debug 模式 heapdump

| 指标 | 对话前 | 对话后 | 增长 |
|------|--------|--------|------|
| **RSS** | 54.4 MB | 93.1 MB | **+38.7 MB** |
| jemalloc allocated | 11.1 MB | 23.4 MB | +12.3 MB |
| jemalloc active | 17.5 MB | 37.2 MB | +19.7 MB |
| jemalloc resident | 24.8 MB | 51.8 MB | +27.0 MB |
| RSS - resident（非 jemalloc） | 29.6 MB | 41.4 MB | **+11.8 MB** |

small malloc 次数：+786,935（80 万次小对象分配/轮）

### jemalloc 现象 B：release 模式 heapdump

| 指标 | 空会话 | 5 tool calls 后 | 增长 |
|------|--------|--------|------|
| **RSS** | 52.9 MB | 94.8 MB | **+41.9 MB** |
| jemalloc allocated | 9.5 MB | 9.0 MB | **-0.5 MB** ← 不增长！ |
| jemalloc resident | 23.3 MB | 68.0 MB | +44.7 MB |
| jemalloc mapped | 67.3 MB | 204.5 MB | +137.2 MB |
| total mallocs | — | 700,782 | — |
| total frees | — | 681,795 | free/malloc 比 97.3% |

### jemalloc 现象 C：`/clear` 后 RSS 构成

```
RSS: 81.8 MB
├── jemalloc allocated:  9.3 MB  ← 实际在用极少
├── arena 碎片化空闲:   17.5 MB  ← active - allocated
├── jemalloc metadata:   ~7.6 MB
├── tcache:              4.4 MB
└── 非 jemalloc:        43.4 MB  ← tokio/hyper/reqwest/rustls
```

**关键结论**：`allocated` 不增长说明不是传统泄漏，RSS 增长来自分配器碎片化 + 运行时基础设施持有。这一结论在切换至 mimalloc 后仍然成立。
