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

- [ ] **日志质量与可观测性**
  - [x] 日志级别滥用：relay.rs session 清理（5 分钟周期触发）、agent_ops.rs Langfuse session 诊断日志（每次 submit_message 可能触发）由 `info` 降为 `debug`；成功创建 session 保留 `info`
  - [ ] `spawn_session_cleanup` 与 `handle_agent_ws` 延迟清理任务的 spawn 错误未传播（静默退出无感知）
  - [ ] `agent.rs` Todo channel 和 HITL 审批转发 spawn 中 `let _ = ...` 静默忽略发送失败

- [x] **配置校验**
  - [x] `budget_tokens` Anthropic 最小值 1024：`with_extended_thinking` 补充 `.max(1024)` 守卫，config/types.rs 注释补充 Anthropic 语义说明
  - [x] `ANTHROPIC_MODEL` / `OPENAI_MODEL` 空字符串 fallback：`from_env()` 改为 `.ok().filter(!empty).unwrap_or(default)`，避免空模型名送到 API
