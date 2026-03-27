# 代码审查方向

- [x] **核心框架正确性**
  - [x] ReAct 循环边界与退出条件
  - [x] 消息双写一致性
  - [x] HITL 四种决策路径
- [x] **LLM 适配层**
  - [x] Anthropic / OpenAI 消息格式差异
  - [x] Prompt Cache 标注
  - [x] MockLLM 测试覆盖
- [x] **中间件与工具安全**
  - [x] 文件路径遍历防护
  - [x] bash 超时与子进程清理
  - [x] HITL 工具白名单
- [x] **异步与并发**
  - [x] 跨 `.await` 持有锁
  - [x] channel 满时的错误处理
  - [x] oneshot 发送端 drop 不阻塞
- [x] **TUI 事件流**
  - [x] 审批/AskUser 事件后 UI 阻塞等待
  - [x] Done/Error 后停止轮询
  - [x] headless 测试通知顺序
- [x] **持久化与状态**
  - [x] SQLite WAL + 事务写入
  - [x] 消息幂等性
  - [x] 用户消息立即持久化
- [x] **SubAgent 委派**
  - [x] 防递归（排除 launch_agent 自身）
  - [x] tools / disallowedTools 过滤
  - [x] LLM 工厂独立实例

- [x] 常规代码风格与架构维护
  - [x] 模块化维护：langfuse/mod.rs（432行）拆分为 session.rs + tracer.rs + mod.rs（纯重导出）；AgentEvent 从 app/mod.rs 提取到 app/events.rs，提升可发现性
  - [x] 大文件扫描与拆分
  - [x] 风格 lint 问题修复

- [x] **错误处理质量**
  - [x] 生产路径 `unwrap()`/`expect()` 扫描：修复三处真实风险——anthropic 消息适配器 `as_array_mut().unwrap()`（改 `if let`）、LLM 响应解析 `as_text().unwrap()` × 2（改 slice pattern + `if let`）；event.rs 模型面板 mode 提取改为只读借用；其余 `unwrap()` 均在测试代码或逻辑安全路径中
  - [x] `tokio::spawn` 子任务中 `let _ = ...` 静默忽略错误（Langfuse 所有后台 spawn 改为 `tracing::warn` 可观测）
  - [x] SQLite 操作中 `unwrap` 是否覆盖所有写失败场景（sqlite_store.rs 生产路径全部使用 `?` 传播，thread_ops.rs 使用 `unwrap_or_else` 降级，已覆盖）
  - [x] relay.rs 生产路径 `serde_json::to_string().unwrap()` 三处改为 `if let Ok` 防 panic
  - [x] `poll_agent` 的 `Disconnected` 通道断开路径缺少 Langfuse/弹窗/计时清理（与 Error 路径对齐）

- [x] **Langfuse 追踪集成**
  - [x] FIFO span 配对正确性：HITL 拒绝路径导致批次 span 树断裂（改为延迟提交策略：批次 Span 在下轮 on_llm_start 或 on_trace_end 时统一提交，彻底解决分裂问题）
  - [x] Batcher flush 时机：Agent 结束时若 `flush_interval=10s` 未到，最后一批事件是否丢失（改用 JoinHandle 等待，移除 sleep(200ms) 竞态）
  - [x] `LangfuseSession` 与 `LangfuseTracer` 生命周期是否与 Thread/Turn 严格对齐（Error 路径补调 on_trace_end，清理 langfuse_tracer）
  - [x] `input_json` 序列化失败时降级为 `Null`（改为显式 `to_value()` + `tracing::warn`，失败时降级为描述性错误对象而非静默 null）
  - [x] final_answer 从 UI 截断视图提取（改为 TextChunk 事件累积，避免 60 字符截断）

- [x] **Relay Server 安全与健壮性**
  - [x] `validate_token` 使用字符串直接比较，存在 timing attack 风险（改用 `subtle::ConstantTimeEq` 常量时间比较）
  - [x] `broadcast_txs` 为 `Vec<UnboundedSender>`，Web 客户端异常断开后仅靠 `is_closed()` retain 清理，高并发下是否有累积泄漏（广播时顺带 retain 清理）
  - [x] Session 双重清理竞态：30 分钟延迟任务与 5 分钟周期清理可能同时 `sessions.remove`（延迟任务补充 elapsed 双重条件检查，与周期清理对齐）
  - [x] `handle_web_session_ws` 中同一 `text_str` 执行两次 `serde_json::from_str`（合并为一次解析，直接解构已有结果，复用 `forward_to_web` 辅助方法）
  - [x] Relay Server 无速率限制和连接数上限（添加 agent/web 并发连接计数与上限：MAX_AGENT_CONNECTIONS=50、MAX_WEB_CONNECTIONS=200、每 session 最多 10 个 web 连接；超限返回 429，日志可观测）

- [x] **日志质量与可观测性**
  - [x] 日志级别滥用：relay.rs session 清理（5 分钟周期触发）、agent_ops.rs Langfuse session 诊断日志（每次 submit_message 可能触发）由 `info` 降为 `debug`；成功创建 session 保留 `info`
  - [x] `spawn_session_cleanup` 与 `handle_agent_ws` 延迟清理任务的 spawn 错误未传播：`spawn_session_cleanup` 改为返回 JoinHandle，main.rs 添加监控 task；`handle_agent_ws` 延迟清理添加 watcher spawn 感知 panic
  - [x] `agent.rs` Todo channel 和 HITL 审批转发 spawn 中 `let _ = ...` 静默忽略发送失败：改为 `is_err()` + `tracing::warn!` + `break`，channel 关闭时可观测且正确退出

- [x] **配置校验**
  - [x] `budget_tokens` Anthropic 最小值 1024：`with_extended_thinking` 补充 `.max(1024)` 守卫，config/types.rs 注释补充 Anthropic 语义说明
  - [x] `ANTHROPIC_MODEL` / `OPENAI_MODEL` 空字符串 fallback：`from_env()` 改为 `.ok().filter(!empty).unwrap_or(default)`，避免空模型名送到 API

- [x] **WebSocket 安全与健壮性**
  - [x] 反序列化缺少消息大小限制：Agent→Relay 限 16MB，Web→Relay 限 1MB，超限记 warn 并 continue 丢弃
  - [x] 服务端无主动心跳探测：web management 与 web session send task 改用 tokio::select! 每 30s 发送 RelayMessage::Ping JSON，写失败时自动退出并触发连接清理
  - [x] 客户端 `connect_async` 无连接超时：添加 tokio::time::timeout(10s) 包装，超时返回可观测错误
  - [x] 接收消息缺乏字段合法性校验：handle_web_session_ws 对 UserInput（空文本拦截）和 HitlDecision（空 decisions、空 tool_call_id 拦截）添加前置校验，非法消息记 debug 并 continue

- [x] **内存无界增长**
  - [x] `AgentState.messages` 无数量/大小上限：`add_message` 在消息数超过 100 条后每 100 条打 `tracing::warn!`，提示使用 /compact 降低内存占用
  - [x] Relay `history` 仅限条目数，未限制单条字节数：`send_with_seq` 添加单条 512KB 字节上限，超限条目跳过历史缓存但仍正常发送，并记录 `tracing::debug!`

- [x] **生产路径 panic! 调用**
  - [x] `protocol.rs` 存在非测试 `panic!`：全面审查确认所有 `panic!` 均在 `#[cfg(test)]` 内，无生产路径风险；将 6 处测试 match 穷举臂从 `panic!` 改为语义更准确的 `unreachable!`（protocol.rs ×2、hitl/mod.rs、skill_preload.rs、message.rs、openai.rs 各 ×1）

- [x] **spawn 任务错误可观测性（续）**
  - [x] relay.rs 多处 `tokio::spawn` 未 await 也未记录错误：client/mod.rs Ping 响应路径 `to_string().unwrap()` 改为 `if let Ok`；handle_web_session_ws `agent_tx.send()` 失败从静默 `let _ =` 改为 `tracing::debug! + break`，agent 断开时正确终止 web session 循环

- [x] **LangfuseTracer JoinHandle 泄漏**
  - [x] `pending_handles` 依赖 `on_trace_end` 清空，异常退出时 handles 未被等待：`on_trace_end` 改为返回 `JoinHandle<()>`；App 新增 `langfuse_flush_handle` 字段存储该 handle；`run_app` 事件循环退出后 await flush handle 确保 batcher flush 在 runtime drop 前完成；为 `LangfuseTracer` 实现 `Drop` 在 `pending_handles` 非空时打 warn

- [x] **forward_to_web 锁与清理**
  - [x] `forward_to_web` 持有 DashMap shard Ref 跨 `.await` 点（反模式，可能死锁）：改为在 match 时立即 clone `Arc<SessionEntry>` 释放 shard lock，再做异步操作
  - [x] `forward_to_web` 缺少 retain 清理（与 `broadcast` 不一致）：改用 write lock + `retain(|tx| !tx.is_closed())`，避免 Web 客户端异常断开后 stale sender 持续积累
  - [x] `handle_agent_ws` 延迟清理任务同样持有 DashMap Ref 跨两次 `.await`：改为 match 时立即 clone Arc，移除不再必要的手动 `drop(entry)`
  - [x] `spawn_session_cleanup` 循环中 `iter()` Ref 跨两次 `.await`：改为先同步收集所有 `(key, Arc<SessionEntry>)` 再做异步 read，彻底释放所有 shard lock

- [x] **TodoTool notify 可观测性**
  - [x] `TodoWriteTool.invoke` 通知 TUI 时 `let _ = tx.send(...).await` 静默忽略失败：改为 `is_err()` + `tracing::warn!`，channel 关闭时可感知；rust-agent-middlewares 添加 tracing 依赖
