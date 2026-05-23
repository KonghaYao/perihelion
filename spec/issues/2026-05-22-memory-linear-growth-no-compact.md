# 长对话内存持续增长，无自动释放机制

**状态**：Open
**优先级**：高
**类型**：性能
**创建日期**：2026-05-22

## 问题描述

Agent 对话过程中，内存（RSS）随对话轮数线性增长，每轮约增长几十 MB，且不会自动下降。持续跑 50-100 轮对话后可达数 GB，最终导致 OOM。**debug 和 release 模式下均表现相同**：`/clear` 后 RSS 不会下降，说明不是简单的分配器缓存行为，而是存在真正的内存未释放问题。对话过程中缺乏自动的上下文压缩（compact）机制来限制内存使用。

## 症状详情

| 维度 | 观察 |
|------|------|
| 增长模式 | 对话轮数相关，非时间相关 |
| 增长速度 | ~几十 MB/轮 |
| 是否自动下降 | 否，只增不减 |
| 触发场景 | 各类操作均有（SubAgent/大文件读取/纯文本） |
| 手动缓解 | `/clear` (new_thread) **无法释放**（debug/release 均如此） |

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启动 TUI，正常对话
  2. 每发一轮消息，观察 RSS 增长
  3. 持续对话数轮后，RSS 持续上升
  4. `/clear` 后 RSS 不下降
- **环境**：macOS，Rust 2021，任何模型下均出现
- **诊断工具**：`/heapdump` 命令（已集成，输出 `.tmp/heapdump-*.txt`）

### 现象 2（2026-05-23）：debug 模式下 `/clear` 后 RSS 不下降

| 维度 | 观察 |
|------|------|
| 编译模式 | debug（`./dev.sh` 启动） |
| `/clear` 前 RSS | 几百 MB |
| `/clear` 后 RSS | 无明显变化，仍在几百 MB |
| 与 release 对比 | 未对比，待确认 release 下 `/clear` 是否能正常释放 |

**推测**：debug 模式下无优化，Rust 全局分配器（jemalloc/system allocator）倾向于保留已释放的内存页不归还 OS，导致 RSS 数值不降。~~需对比 release 模式确认是否为 debug 专属现象~~。**已确认 release 也有同样问题**（见现象 3），推测已推翻。

### 现象 3（2026-05-23）：release 模式下 `/clear` 后 RSS 也不下降

| 维度 | 观察 |
|------|------|
| 编译模式 | release（`--release` 构建） |
| 增长速度 | 比 debug 慢，但仍然持续线性增长 |
| `/clear` 后 RSS | 无效果，不下降 |
| 测量方式 | 内部内存记录工具 |

**意义**：此前推测"debug 模式分配器不归还内存"已被推翻——release 下 `/clear` 同样无法释放，说明存在真正的内存持有问题（数据结构引用未释放、缓存未清理、或循环引用等），而非单纯的分配器缓存行为。优先级从「中」提升至「高」。

### 现象 4（2026-05-23）：jemalloc profiling 定量分析

使用 `/heapdump` 对一轮典型对话前后进行对比（debug 模式，macOS）：

| 指标 | 对话前 | 对话后 | 增长 |
|------|--------|--------|------|
| **RSS** | 54.4 MB | 93.1 MB | **+38.7 MB** |
| jemalloc allocated | 11.1 MB | 23.4 MB | +12.3 MB |
| jemalloc active | 17.5 MB | 37.2 MB | +19.7 MB |
| jemalloc resident | 24.8 MB | 51.8 MB | +27.0 MB |
| jemalloc mapped | 68.8 MB | 95.5 MB | +26.7 MB |
| huge allocations | 0 | 0 | 0 |
| non_arena (mapped-active) | 51.3 MB | 58.4 MB | +7.1 MB |
| RSS - resident（非 jemalloc） | 29.6 MB | 41.4 MB | **+11.8 MB** |

**TUI 组件数据**（/clear 后采样）：agent_state_messages=0, pipeline_completed=0, view_messages=0 — TUI 前端已完全释放。

**jemalloc 分配统计**：

| 指标 | 增长 |
|------|------|
| small malloc 次数 | **+786,935**（80 万次小对象分配/轮） |
| large malloc 次数 | +294 |
| 768KB large class 存活数 | 0 → 6（**4.5 MB**，推测为 LLM streaming response body buffer） |
| arena dirty pages | 1.2 MB → 9.0 MB（+7.8 MB，已 free 未 purge） |

**三大泄漏源定位**：

1. **arena dirty pages（+7.8 MB）**：jemalloc 已释放但未 purge 的 page。`dirty_decay_ms=1000` 配置已确认写入成功，但 decay 在 macOS 上效果有限
2. **arena live objects（+12.3 MB allocated）**：Rust 堆上的活跃对象。`/clear` 后 TUI 前端数据归零，但这些对象在 ACP Server / Agent Executor 侧仍被持有
3. **非 jemalloc 内存（+11.8 MB RSS-resident）**：tokio runtime stack / reqwest TLS buffer / HTTP body buffer，不受 jemalloc 管理

## 根因分析

### 泄漏层级

```
RSS 增长 (+38.7 MB)
├── jemalloc resident (+27.0 MB)
│   ├── allocated (+12.3 MB)  ← Rust 堆活跃对象
│   │   ├── ACP SessionState.history（prompt 返回后整体替换，旧 Vec 随 executor tokio::spawn 闭包持有）
│   │   ├── Agent State.messages（execute_prompt 内部 State，随 spawn 闭包生命周期）
│   │   ├── LLM streaming JSON 解析临时 buffer（768KB × 6 = 4.5 MB）
│   │   └── serde_json::from_value 反序列化中间对象（大量 String clone）
│   └── dirty pages (+7.8 MB) ← jemalloc arena 已释放未 purge
│       └── macOS 上 dirty_decay 效果有限，需要主动 arena.purge
└── 非 jemalloc (+11.8 MB)
    ├── reqwest/hyper TLS connection buffer（每轮 LLM 调用建立新连接？）
    ├── tokio task stack growth
    └── macOS VM system caching
```

### `/clear` 后不释放的原因

`/clear` 清理了 TUI 前端数据（agent_state_messages、pipeline、view_messages），但：
1. **ACP executor 的 `tokio::spawn` 闭包**持有 Agent State、event channel、history 的引用——这些闭包在 `execute_prompt` 返回后应该 drop，但可能被 channel 或其它 Arc 引用钉住
2. **tokio runtime 的 task 缓存**不释放已完成的 task 的 stack memory
3. **reqwest 连接池**保持 TLS session 和 buffer

## 修复方向

### P0：减少每轮分配量（治本）

1. **消除 serde JSON 双重解析**：`run_pump` 中 `serde_json::from_value(event_value.clone())` 先 clone 再反序列化，改为 `serde_json::from_reader` 或零拷贝解析
2. **LLM response body buffer 复用**：768KB × 6 的 large class 分配表明每轮 LLM 调用分配多个大 buffer，考虑用 `Bytes` pool 或复用已有 buffer
3. **减少 String clone**：80 万次 small malloc 中大量是字符串克隆，审计 `AcpNotification::AgentEvent` 构造路径中的 clone

### P1：ACP executor 生命周期管理（治标）

4. **确保 executor spawn 闭包在 prompt 完成后 drop**：检查 `execute_prompt()` 返回后 event_tx/event_rx/channel 是否被其它引用钉住
5. **`/clear` 时主动触发 `arena.purge`**：当前 `jemalloc_decay()` 遍历 arena purge，但可能时机不对（在 ACP server 还没清完 history 时就执行了）
6. **bounded notification channel**：`AcpTuiClient` 的 `unbounded_channel` 改为 `channel(256)`，防止 pump 产出 > TUI 消费时的无限积压

### P2：分配器调优

7. **配置 `dirty_decay_ms`** 在运行时生效（已验证 write 成功，但 macOS decay 效果差，可尝试更激进的值如 100ms）
8. **减小 tcache**：当前 tcache_bytes ~7MB，可配置 `lg_tcache_max` 限制 thread cache 大小
9. **考虑 `background_thread:true`** 让 jemalloc 后台线程主动 purge dirty pages

### 诊断工具

- **`/heapdump`** 已集成（`peri-tui/src/command/core/heapdump.rs`），输出 jemalloc 完整统计 + TUI 组件大小到 `.tmp/heapdump-*.txt`
- **`tikv-jemalloc-ctl`** 已启用 `stats` + `use_std` features

## 涉及文件

- `peri-tui/src/acp_server/mod.rs` —— ACP 服务器端 SessionState.history
- `peri-tui/src/app/agent_comm.rs` —— TUI 端 agent_state_messages
- `peri-tui/src/app/agent_submit.rs` —— submit_message 流程
- `peri-tui/src/app/thread_ops.rs` —— new_thread（/clear）释放逻辑
- `peri-tui/src/acp_server/prompt.rs` —— 每轮执行后 state.history 更新
- `peri-tui/src/acp_client/client.rs` —— notification channel（unbounded → bounded 候选）
- `peri-acp/src/session/executor.rs` —— execute_prompt 内 event channel + spawn 闭包生命周期
- `peri-tui/src/command/core/heapdump.rs` —— `/heapdump` 诊断命令
