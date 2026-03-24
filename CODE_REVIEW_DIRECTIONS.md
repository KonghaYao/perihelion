# 代码审查方向

- [ ] **核心框架正确性**
  - [x] ReAct 循环边界与退出条件
  - [x] 消息双写一致性
  - [x] HITL 四种决策路径
- [ ] **LLM 适配层**
  - [x] Anthropic / OpenAI 消息格式差异
  - [x] Prompt Cache 标注
  - [ ] MockLLM 测试覆盖
- [x] **中间件与工具安全**
  - [x] 文件路径遍历防护
  - [x] bash 超时与子进程清理
  - [x] HITL 工具白名单
- [x] **异步与并发**
  - [x] 跨 `.await` 持有锁
  - [x] channel 满时的错误处理
  - [x] oneshot 发送端 drop 不阻塞
- [ ] **TUI 事件流**
  - [x] 审批/AskUser 事件后 UI 阻塞等待
  - [x] Done/Error 后停止轮询
  - [ ] headless 测试通知顺序
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
  - [ ] 大文件扫描与拆分
  - [ ] 风格 lint 问题修复
  