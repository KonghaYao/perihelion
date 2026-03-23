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

- [x] AgentDefineMiddleware
- [ ] Subagent 的 Skill 预加载功能
- [ ] Sandbox 抽象,提供文件系统抽象,从而使得我们的 agent middleware 可以在远程有一个服务器,然后能够简单通过 --remote xxx 来替换掉原有的 LocalFileSystem 相关的 middleware <https://docs.langchain.com/oss/python/deepagents/backends>
- [x] SubAgents
- [ ] MCP Server 接入（Model Context Protocol）
- [ ] /compact 指令
- [ ] 系统提示词中需要添加更多的 cli 的信息, 比如现在的模型,等

**第三层：用户界面**

- [x] 渲染线程分离
- [ ] loading 状态,缓冲区输入
- [ ] Web UI（浏览器端对话界面）
- [ ] 工具调用显示的颜色调整, 工具名称一个颜色,然后工具内的描述通统一使用 dimColor.
- [ ] 工具内的描述文本需要 replace 掉 pwd 的路径,保证足够短小(Bash 和 search 不需要,仅仅显示层)
- [ ] TODOWrite 只显示占位, TODO 的状态由全数据计算出来,然后显示到输入框的上面
- [ ] Tarui 整合
- [ ] 多 Agent 并发面板（同时跑多个任务）
- [ ] 添加一个会话内的数据统计 status bar
