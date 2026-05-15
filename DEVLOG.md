# Perihelion Devlog — March 20 – May 14, 2026

## March 20 · Hello World

项目第一行代码诞生。agent 框架的骨架开始搭建——ReAct 循环、BaseMessage 消息类型、中间件 trait 雏形。这一天只有一个 commit：把想法变成代码。

---

## March 21 · Agent 定义文件

Agent 配置文件读取能力上线。从这一天起，每个 agent 都可以用自己的 `.claude/agents/` 文件定义行为——model、maxTurns、tools 白名单。

---

## March 22 · SubAgent 诞生 / 存储层设计 / UI 重构

SubAgent 能力第一天就完成了——子 agent 通过 `Agent` 工具触发，继承或覆盖父 agent 的工具配置。

底层存储层（SQLite 持久化）也在今天敲定设计。TUI 前端经历了一次架构重构。**三条线并进，像是第一天铺了三根平行的铁轨。**

---

## March 23 · Remote Control / Headless / 工具展示

Remote Control 面板完工——可以远程连接和控制 agent。Headless 模式就位——不启动 TUI，纯后台运行。工具调用的视觉展示迎来第一次优化。

**今天的关键决策**：HITL 拒绝不再终止 Agent，改为反馈错误结果让 LLM 调整——这个设计选择影响了后面无数轮工具的交互逻辑。

---

## March 24 · Relay Server 样式 / Langfuse 接入 / Compact 指令

Relay Server 的 Web 前端样式更新了一版。Langfuse 追踪集成第一次接通。图片上传支持、markdown 渲染加入。`/compact` 指令诞生。

也是修复日：Bash 超时自动 SIGKILL、WebSocket HITL 补齐 4 种决策路径、渲染 channel 从有界改无界消除静默丢弃、文件路径 canonicalize 防路径遍历……

---

## March 25 · Langfuse 集成 / Web Crawler / 零 Clippy 警告

Langfuse 深度集成——token 统计、session 管理、工具定义上报全链路打通。Web Crawler 能力加入（后来演变成 WebFetch/WebSearch 中间件）。SubAgent 开始共享中间件链。

毫不动摇地把全 workspace 的 clippy 警告清零。**从今天起，"零警告"成为了基线。**

---

## March 26 · Relay Server 加固 / 远程控制面板 / Skill Preload

Relay Server 补了连接数上限防 DoS、spawn 错误可观测性、四处安全漏洞修复。远程控制面板的 TUI 端就位。`#skill-name` 预加载机制上线。

今天还修了 3 处 unwrap panic 风险、2 处 Lagfuse 追踪质量漏洞、1 处 JoinHandle 泄漏。**安全加固日——每一个潜在 crash 都被提前堵上。**

---

## March 27 · 架构债清偿 / Relay 数据同步 / 移动端适配

Arch.md 中标记的 M1/M2/M3/M4 架构问题全部修复。Relay Server 完成了数据同步、弹窗、命令行传递三件套。前端从 React 迁移到 Preact。移动端适配完成。

也是修复日：DashMap 锁跨 await 反模式、WebSocket 消息限制/心跳/超时/字段校验、内存无界增长防护……

---

## March 28 · 跨平台构建 / 交互统一 / Skill 预加载

Cross 编译问题大决战——macOS/Linux 构建修复、rustls-tls 替代 native-tls 消除 OpenSSL 依赖、cross-rs 入 CI。TUI 交互统一化——Skill 发送后自动预加载全文。Relay Server 协议层解耦支持多用户。

样式重构完成。ThreadStore 与 AgentState 合并。

---

## March 29 · peri-cli 发布 / Langfuse Client / 大重构

peri-cli 正式发布——Node.js 安装/更新/卸载工具独立出去。Langfuse Client 作为独立 crate 完成。Cron 面板开发完毕。

Sticky header 上线、颜色系统微调、配置文件 env 注入就位。YOLO 模式设为默认。**今天还是一个"大扫除日"——基础代码大重构，历史遗留代码清理。**

---

## March 30 · Langfuse Client 独立 / Cron 完成 / 颜色收尾

Langfuse Client 作为库独立发布。`/cron` 命令完成。颜色收尾微调。Sticky header 完工。相对平静的一天。

---

## March 31 · Setup 向导 / 历史筛选 / Langfuse 修复

Setup 初始化向导设计完成。历史对话筛选功能上线。Langfuse 的层级关系和 warning 细节修复。OTel 相关代码移除。

---

## April 27 · 组件库诞生 / Token 计数 / 权限模式 / Relay 移除

**休息了近一个月后重新开工**。

`perihelion-widgets` 组件库诞生——独立 TUI 组件只依赖 ratatui + pulldown-cmark。Token 计数和自动压缩功能就位。权限控制模式开发完成——Bypass/Default/AcceptEdit/DontAsk/AutoMode 五种模式。模型选择面板重构。Relay Server 代码移除以简化架构。

---

## April 28 · 消息管线统一 / 压缩完成 / LLM 重试

消息渲染管线彻底统一——`messages_to_view_models` 成为唯一转换入口。压缩（Compact）能力验收完成——micro + full 两层架构就位。LLM 自动重试机制上线。Markdown 表格自适应换行、Spinner/ToolCall/MessageBlock Widget 化、代码块语法高亮用 syntect 实现。输入框历史恢复、浮层 Up/Down 导航。

---

## April 29 · 四十二个修复的马拉松

**当天 48 个 commit，是整个项目最密集的修复日。**

从 SubAgent 栈泄漏到 write_file 并发安全，从 executor 竞态到 HITL 批次一致性，从 SQLite 外键到 ContentBlock::Unknown 序列化透传——42 个逻辑修复覆盖了消息管线、工具系统、安全、UX 的方方面面。

Auto-classifier 的 ALLOW/DENY 否定词误判修复尤其关键——一个 bug 差点让危险命令自动放行。

---

## April 30 · 系统提示词重构 / Prompt Cache 修复 / ContextBudget 连接

系统提示词大重构——从单块文本拆成 11 个段落文件，引入 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 边界标记分隔缓存/非缓存段。Anthropic Prompt Cache 的 `cache_control` 位置修复。ContextBudget 连接到 executor 上下文监控。

批量 `before_tool` 处理优化让 HITL 可以一次审批多个工具。工具名与 Claude Code 原生界面彻底对齐。

---

## May 1 · 修修补补的仪式感

五一劳动节没休息——但是干的都是"舒服的活"。给 Thread Browser 加了删除确认、空列表引导、新建反馈，把颜色系统对齐了 Claude Dark Theme，模型面板塞进了 thinking effort 旋钮。CJK 截断修了又修（这已经是第 N 次被字节/字符混淆坑了）。**今天像是给房子做大扫除——每个角落都擦了一遍，住着舒服多了。**

---

## May 2 · MCP 降临

重头戏：MCP 中间件落地了。外部 MCP 服务器的工具和资源现在可以直接注入对话。ACP 协议也通了第一版。

Thread Browser 彻底重设计——搜索框、两行紧凑列表、内容大小一目了然。还顺手清了全项目的编译警告，补了 widgets 和 TokenTracker 的测试。**今天是"打下两根大桩"的日子——MCP 和 ACP 让 Perihelion 对外部生态张开了双臂。**

---

## May 3 · SubAgent 大爆炸

一天之内，三个重量级能力同时上线：Background Agent（后台执行 + 自动 continuation）、Fork Agent（继承上下文分叉）、多 session 分屏。Alt+M 还能循环切模型了。

peri-dag 工作流引擎也在今天诞生——带 Web UI 的 DAG 编排器。**这一天像是 feature 工厂开足马力，SubAgent、多 session、dag 引擎三线并进，气势很猛。**

---

## May 4 · 疯狂的设计评审马拉松

45 个 commit，整个 sprint 最密集的一天。

acpx-g（原 peri-dag）经历了 **Round 1 到 Round 22+** 的设计评审——从 UX 打磨到架构加固，从 schema 校验到并发限流器，从前端安全到 CSS 变量化。系统提示词也被大刀砍了 51%（281→137 行），把依赖收敛到根 Cargo.toml。rusqlite 迁移到 sqlx，Ctrl+C 中断后能恢复输入文本了。**今天是"量变到质变"的一天——acpx-g 从一个想法变成了一个经得起审视的系统。**

---

## May 5 · 前端炼狱与网络之眼

acpx-g 的设计评审从 Round 2 一路杀到 Round 22——事件委托、内联 handler 消灭、undo/redo 防腐败、CSS 变量替换硬编码色……每一天都在把前端代码从"能跑"推向"体面"。

同时 WebFetch/WebSearch 中间件上线，peri-cli 支持多包安装，双击 Ctrl+C 退出成为了肌肉记忆。**今天大概是这个 sprint 里最像"debug 地狱"的一天——但 acpx-g 终于在审美的意义上站稳了脚跟。**

---

## May 6 · 插件商店开张

插件系统正式兼容 Claude Code Marketplace。MCP 中间件的四阶段流水线重构完成。Marketplace 面板有了安装量排序、自动刷新、状态持久化。

**Perihelion 从此有了自己的"应用商店"。这一天像一个里程碑——生态的种子播下了。**

---

## May 7 · Hook 全家桶与零警示

Claude Code hooks 系统完整实现——4 种执行类型、14 种事件、SSRF 防护。全项目 clippy 警告清零。ContentBlock 测试补全，MCP 工具名 sanitize 到 API 规范。

**如果 May 6 是"开门"，May 7 就是"铺路"——hooks 让插件有了和宿主对话的完整语言。**

---

## May 8 · 架构瘦身日

TUI 层大重构——PanelComponent trait 组件化、core.rs 拆解为独立模块。Setup Wizard 升级了多 provider 迁移和 Browse/Edit 双模式。

**身体变轻了，姿态更好了。从这以后，加新面板只要实现一个 trait。**

---

## May 9 · 改名与延迟加载

`.zen-code` 正式更名为 `.peri`。Tool Search 延迟加载上线——非核心工具按需发现，不再拖慢启动。built-in agents（explore、plan、general-purpose、verification）就位。executor.rs 拆成目录模块。

**今天是"重命名日"——但更重要的是，工具系统从"全家桶"变成了"自助餐"。**

---

## May 10 · 思考的细节

OpenAI 兼容 provider 的 reasoning_content 回传机制踩了一圈坑——DeepSeek thinking 模式、extended thinking 验证、模型能力条件回传。status bar 的计时器换成了进程内存监控。鼠标可用性检测让键盘党也能看到快捷键提示。

**今天在处理"看不见的东西"——token 在网络上来回时，每一个字段的位置都决定了下一个请求能不能命中缓存。**

---

## May 11 · 编译引擎与渲染革命

LSP 中间件落地——10 种代码智能操作。auto-compact 两层架构（micro + full）就位。TUI 渲染管线统一为 RebuildAll 架构——增量渲染的复杂性和各种边界 bug 终于画上句号。agent_ops.rs 从 2158 行拆成 7 个模块。

**今天做了一件勇敢的事——扔掉一个复杂的增量系统，换一个更简单的全量重建。代码行数没变少，但心智负担降了一半。**

---

## May 12 · Anthropic 缓存周 · 第一天

**这是整个 sprint 最高光的一天。**

Anthropic prompt cache 命中率从 ~70% 飙升到 98.5%+——3 断点缓存策略。背后是 cache_control 位置排序、tools 前缀稳定化、skill 预加载改用 add_message、system prompt 边界标记分离缓存块……一堆细节纠缠。

同时修了 3 个 background agent 的展示 bug、SubAgent 事件泄露、CJK 鼠标定位（第三次）、pipeline 切片 panic、compact 后的残留通知。还加上了缓存命中率在 status bar 的实时显示。

**今天像是在和 Anthropic 的缓存机制下棋——每一步都有意为之，最终赢了。98.5% 不是优化，是逆袭。**

---

## May 13 · 缓存稳定与流式新生

缓存战役从"命中率"打到"稳定性"——跨进程工具排序、deferred tools 会话级缓存、fork agent 的缓存破坏问题逐一解决。

流式渲染迎来增量 markdown 解析和前缀缓存复用。多 SubAgent 完成后出现了树形汇总视图。面板的滚轮滚动和鼠标点击终于统一了。

**如果说 May 12 是在"造火箭"，May 13 就是在"调轨道"——让那 98.5% 的缓存率在各种边界条件下依然稳定。**

---

## May 14 · 流式 LLM 与收尾之美

LLM 流式输出支持正式上线——文本逐字出现在 TUI 上。llm-gateway 透明代理诞生、Git 安全提示词就位、GlobalUiState 从 ServiceRegistry 中分离、Grep 工具增强到 8 个新参数。

CLAUDE.md 同步了 29 个 commit 的架构变更和 TRAP。issues 归档了一大批，整个 spec/global 知识库焕然一新。

**最后一天，像写完小说后校对标点——整理文档、归档 issue、补充 TRAP。所有螺丝都拧紧了。**

---

## 八周缩影

| 日期 | 主旋律 | commits |
|------|--------|---------|
| 3/20 | Hello World | 1 |
| 3/21 | Agent 定义文件 | 2 |
| 3/22 | SubAgent + 存储设计 + UI 重构 | 4 |
| 3/23 | Remote Control / Headless / 工具展示 | 11 |
| 3/24 | Relay 样式 / Langfuse / Compact 指令 | 19 |
| 3/25 | Langfuse 深度集成 / 零 Clippy | 20 |
| 3/26 | Relay 加固 / 远程控制面板 / Skill Preload | 24 |
| 3/27 | 架构债清偿 / Relay 同步 / 移动端 | 22 |
| 3/28 | 跨平台构建 / 交互统一 / 样式重构 | 21 |
| 3/29 | peri-cli 发布 / Langfuse Client / 大重构 | 14 |
| 3/30 | Langfuse 独立 / Cron 完成 | 4 |
| 3/31 | Setup 向导 / 历史筛选 | 8 |
| 4/27 | 组件库 / Token 计数 / 权限模式 / Relay 移除 | 10 |
| 4/28 | 消息管线统一 / 压缩完成 / LLM 重试 | 13 |
| 4/29 | **四十二个修复的马拉松** | 48 |
| 4/30 | 系统提示词重构 / Prompt Cache 修复 | 32 |
| 5/1 | TUI 抛光 | 12 |
| 5/2 | MCP + ACP 落地 | 18 |
| 5/3 | SubAgent 三件套 + dag 诞生 | 24 |
| 5/4 | acpx-g 设计马拉松 | 45 |
| 5/5 | WebFetch/Search + acpx-g 收尾 | 38 |
| 5/6 | 插件商店 | 17 |
| 5/7 | Hooks 系统 + 零警示 | 24 |
| 5/8 | TUI 架构重构 | 15 |
| 5/9 | Tool Search + 改名 | 21 |
| 5/10 | Thinking 模式打磨 | 19 |
| 5/11 | LSP + RebuildAll 统一 | 28 |
| 5/12 | **缓存逆袭 70%→98.5%** | 35 |
| 5/13 | 缓存稳定 + 流式渲染 | 31 |
| 5/14 | 流式 LLM + 收官 | 24 |

**总计：522 个 commit，54 天，8 周。**

从 ReAct 循环的第一行代码到 Anthropic 缓存 98.5%，从 Relay Server 的远程控制到插件商店的开张，从 perihelion-widgets 组件库的诞生到 acpx-g 的 22+ 轮设计评审。**这不是一个 sprint——这是一段旅程。Perihelion 从 0 到 1，再到 1.0。**
