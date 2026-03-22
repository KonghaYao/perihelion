# Perihelion — Feature Routemap

## 第一层：基础能力

> 框架底层支撑，供上层复用的核心机制

- ReAct Agent 执行循环
  - 多步推理 + 工具调用
  - 最大迭代次数限制
  - 流式事件回调
- 多 LLM Provider
  - OpenAI 兼容 API
    - 自定义 base URL（代理场景）
    - reasoning_content 支持
  - Anthropic API
    - Extended Thinking
    - Prompt Caching（默认开启）
- 多模态消息
  - 图片（Base64 / URL）
  - 文档
  - 推理内容（Reasoning/CoT）
- 可组合中间件系统
  - 生命周期钩子
  - 中间件动态提供工具
- 工具注册与调用系统
  - JSON Schema 参数定义
  - 动态工具提供者
- 会话线程持久化
  - 多会话独立存储
  - 历史会话加载恢复
- 遥测与可观测性
  - tracing 日志（stdout / JSON 格式）
  - OpenTelemetry OTLP 导出
  - Jaeger 可视化
- [ ] i18n 方案
  - 文件系统读取 lang/xxx.json 文件
  - list lang
  - pick lang

---

## 第二层：Agent 能力

> 具体的 Agent 行为能力，通过中间件和工具组合实现

- 文件系统操作
  - 读取文件
  - 写入文件
  - 精确编辑文件
  - Glob 模式匹配
  - Ripgrep 全文搜索
  - 目录操作
- 终端命令执行
  - Shell 命令
  - 超时控制
  - 多平台
- 任务管理
  - Todo 列表增删改
  - 状态追踪
- 项目上下文注入
  - AGENTS.md / CLAUDE.md 自动搜索加载
  - 项目级与用户级分级优先
- Skills 系统
  - 外部 Skill 文件按需加载
  - 渐进式摘要注入
- SubAgents（子 Agent 委派）
  - `launch_agent` 工具：LLM 将子任务委派给专门 agent
  - 读取 `.claude/agents/{agent_id}.md` 定义文件
  - 子 agent 继承父工具集（tools / disallowedTools 过滤）
  - 防递归：子 agent 不含 `launch_agent` 自身
  - 执行摘要：工具调用列表 + 最终回答返回父 agent

---

## 第三层：用户界面

> 面向终端用户的交互体验

- 交互式 TUI
  - 对话界面
  - 多行输入
  - 实时流输出
- Human-in-the-Loop 审批
  - 敏感操作前拦截（bash / write / edit / folder 等）
  - 批准 / 编辑参数 / 拒绝 / 拒绝并说明原因
  - 批量审批面板
  - YOLO 模式（全局跳过审批）
- Ask User
  - Agent 主动向用户提问
  - 单选 / 多选 / 自由输入
  - 多问题 Tab 切换
- 历史会话浏览与恢复
- 内置命令
  - `/model` 切换 LLM provider / 模型
  - `/clear` 清空对话
  - `/help` 帮助
  - `/history` 历史会话
- Skill 补全（`#` 前缀触发）

---

## TODO

**第一层：基础能力**

- [x] 并行工具调用（多个工具同时执行，而非串行）
- [x] 断点续跑（Agent 中途中断后从某步恢复）
- [ ] Token 用量追踪与预算控制
- [ ] 结构化输出（强制 Agent 按 JSON Schema 返回）
- [ ] 更多 LLM Provider
  - [ ] Gemini
  - [ ] 本地 Ollama
- [x] ot 需要直接打包进去,不需要 --features otel,只是没有配置的时候,不需要进行 ot 的行为
- [x] 支持 thinking 模式
- [x] 替换默认提示词
- [ ] Model 定位 Opus\Sonnet\Haiku -> provider -> model

**第二层：Agent 能力**

- [ ] AgentDefineMiddleware
- [ ] Subagent 的 Skill 预加载功能
- [ ] Sandbox 抽象,提供文件系统抽象,从而使得我们的 agent middleware 可以在远程有一个服务器,然后能够简单通过 --remote xxx 来替换掉原有的 LocalFileSystem 相关的 middleware <https://docs.langchain.com/oss/python/deepagents/backends>
- [x] SubAgents
- [ ] MCP Server 接入（Model Context Protocol）
- [ ] 知识库检索 / RAG（向量搜索 + 上下文注入）

**第三层：用户界面**

- [ ] Web UI（浏览器端对话界面）
- [ ] 多 Agent 并发面板（同时跑多个任务）
- [ ] 添加一个会话内的数据统计 status bar
