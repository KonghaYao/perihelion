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

- [ ] 常规代码风格与架构维护
  - [ ] 模块化维护
  - [x] 大文件扫描与拆分
  - [x] 风格 lint 问题修复

- [ ] **错误处理质量**
  - [ ] 生产路径 `unwrap()`/`expect()` 扫描（rust-create-agent 71 处，rust-agent-middlewares 73 处，rust-agent-tui 55 处）
  - [ ] `tokio::spawn` 子任务中 `let _ = ...` 静默忽略错误（Langfuse 上报失败无感知）
  - [ ] SQLite 操作中 `unwrap` 是否覆盖所有写失败场景

- [ ] **Langfuse 追踪集成**
  - [ ] FIFO span 配对正确性：`on_tool_start` / `on_tool_end_by_name_order` 在并行工具调用时是否错位（HITL 拒绝路径导致批次 span 树断裂，待修复）
  - [x] Batcher flush 时机：Agent 结束时若 `flush_interval=10s` 未到，最后一批事件是否丢失（改用 JoinHandle 等待，移除 sleep(200ms) 竞态）
  - [x] `LangfuseSession` 与 `LangfuseTracer` 生命周期是否与 Thread/Turn 严格对齐（Error 路径补调 on_trace_end，清理 langfuse_tracer）
  - [ ] `input_json` 序列化失败时降级为 `Null`，是否影响 Langfuse 侧数据质量
  - [x] final_answer 从 UI 截断视图提取（改为 TextChunk 事件累积，避免 60 字符截断）

- [ ] **Relay Server 安全与健壮性**
  - [ ] `validate_token` 使用字符串直接比较，存在 timing attack 风险（应换用 `constant_time_eq`）
  - [ ] `broadcast_txs` 为 `Vec<UnboundedSender>`，Web 客户端异常断开后仅靠 `is_closed()` retain 清理，高并发下是否有累积泄漏
  - [ ] Session 双重清理竞态：30 分钟延迟任务与 5 分钟周期清理可能同时 `sessions.remove`（DashMap 安全但语义重复）
  - [ ] `handle_web_session_ws` 中同一 `text_str` 执行两次 `serde_json::from_str`（检测 hitl_decision 和 ask_user_response 各一次），可合并为一次解析
  - [ ] Relay Server 无速率限制和连接数上限，需评估 DoS 风险
