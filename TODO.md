## TODO

**第一层：基础能力**

- [x] 并行工具调用（多个工具同时执行，而非串行）
- [x] 断点续跑（Agent 中途中断后从某步恢复）
- [x] ot 需要直接打包进去,不需要 --features otel,只是没有配置的时候,不需要进行 ot 的行为
- [x] 支持 thinking 模式
- [x] 替换默认提示词
- [x] Model 定位 Opus\Sonnet\Haiku -> provider -> model
- [ ] Token 用量追踪与预算控制
- [ ] 结构化输出（强制 Agent 按 JSON Schema 返回）

**第二层：Agent 能力**

- [x] AgentDefineMiddleware
- [x] SubAgents
- [x] /compact 指令
- [ ] MCP Server 接入（Model Context Protocol）
- [ ] 系统提示词中需要添加更多的 cli 的信息, 比如现在的模型,等
- [x] Subagent 的 Skill 预加载功能
- [ ] Sandbox 抽象,提供文件系统抽象,从而使得我们的 agent middleware 可以在远程有一个服务器,然后能够简单通过 --remote xxx 来替换掉原有的 LocalFileSystem 相关的 middleware <https://docs.langchain.com/oss/python/deepagents/backends>
- [x] 写一个 Explorer Agent
- [x] 写一个 Web Research Agent

**第三层：用户界面**

- [x] 渲染线程分离
- [x] Headless 模式
- [x] 粘贴换行符,会导致 enter 事件被唤醒
- [x] loading 状态, 输入框应该可以输入, 输入之后进入缓冲区, 消息完成之后直接发送
- [x] 工具调用显示的颜色调整, 工具名称一个颜色,然后工具内的描述通统一使用 dimColor.
- [x] 工具内的描述文本需要 replace 掉 pwd 的路径,保证足够短小(Bash 和 search 不需要,仅仅显示层)
- [x] TODOWrite 只显示占位, TODO 的状态由全数据计算出来,然后显示到输入框的上面
- [x] 弹窗面板里面的内容超长会有问题
- [x] 输入框粘贴图片功能
- [x] 接入 langfuse
  - [ ] Langfuse Client 有问题, 现在发送不了 tool 观测暂时无法解决
- [ ] 多 Agent 并发面板（同时跑多个任务）
- [ ] Web UI（浏览器端对话界面）
- [ ] Tarui 整合
- [ ] i18n 文件整合替换能力; 默认中文, 但是检测到 ./i18n/ 里面的 json 时, 会进行替换. /i18n 会进入选择面板, 即文件, 选中即可替换.
- [ ] /loop 命令 和 /cron, cron 可以看到定时任务, loop 是一个 command 指示 ai 如何添加 cron , cron 只存储在内存中; cron 会定时 新建会话, 然后把用户的提示词作为用户输入开始执行
- [x] remote control panel: 能够配置远程地址和密钥,然后存储到 settings.json, 命令只需要 --remote-control 即可
- [x] AIMessage 输出时, 会显示两遍文本信息
- [x] 滚动区域有时候到不了底部, 我们加 10 行空行到末尾
- [x] status bar 增加现有消息数,与消息窗口同步
- [x] 没有 --remote-control 参数时, 就算有配置也不进行远程链接
- [x] Relay server 添加日志打印
- [ ] /clear relay serve 的前端没有进行清理
- [ ] 架构更改到 prereact
