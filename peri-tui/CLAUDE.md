# peri-tui

TUI 应用，纯 ACP client 前端。运行时仅通过 `peri-acp` 的 `MpscTransport`（in-memory channel pair）与 ACP Server 通信，不直接依赖 `peri-agent`/`peri-middlewares` 的运行时路径。

## 依赖说明

`Cargo.toml` 保留 `peri-agent`/`peri-middlewares` 作为**类型依赖**（UI 渲染所需的 `BaseMessage`/`ContentBlock` 等类型），运行时通信仅通过 `peri-acp`。

## 核心文件

| 文件 | 职责 |
|------|------|
| `src/acp_client/client.rs` | ACP client 封装，`AcpNotification` 变体定义 |
| `src/acp_server/requests.rs` | ACP 请求路由 |
| `src/app/agent.rs` | `ExecutorEvent → AgentEvent` 映射（`map_executor_event`） |
| `src/app/agent_ops/acp_bridge.rs` | `AcpNotification → AgentEvent` 桥接 |
| `src/app/agent_ops/lifecycle.rs` | Agent 生命周期处理 |
| `src/app/agent_submit.rs` | 用户输入提交入口 |
| `src/app/agent_compact.rs` | Compact 事件处理：pipeline 清理 + UI 通知 |
| `src/app/message_pipeline.rs` | `MessagePipeline`：规范状态维护 + `messages_to_view_models()` |
| `src/ui/main_ui/mod.rs` | 主布局 |
| `src/i18n/` | 国际化模块（`LcRegistry` + Fluent） |

## ACP 数据流

```
TUI 输入 → AcpTuiClient.new_session() / .prompt()
         → MpscClientTransport.send_request/notification()
         → MpscServerTransport.recv() (ACP Server, tokio::spawn)
         → ExecutorEvent → TransportEventSink.push_event()
         → AcpTuiClient.pump_notifications() → AcpNotification::AgentEvent
         → agent_ops::acp_bridge::handle_acp_notification()
         → map_executor_event() → AgentEvent
         → handle_agent_event() → UI 更新
```

**[TRAP]** TUI 层数据必须通过 ACP 协议到达 ACP 层，禁止直连。所有 TUI → ACP Server 的状态变更必须通过 `acp_client` 的协议方法。TUI 本地清空状态（如 `new_thread()`）不等于 ACP Server 端状态同步——必须同时通过 ACP 协议通知 Server 侧。（详见 spec/global/domains/agent.md#issue_2026-05-29-clear-keeps-acp-server-history）

## 消息渲染

所有消息更新通过统一 `RebuildAll` 路径触发（无增量更新）。`MessagePipeline` 维护规范状态，`build_tail_vms()` 构建尾部 VMs，`messages_to_view_models()` 是唯一转换入口。流式文本通过 16ms 间隔 + 自适应分块策略触发 RebuildAll。独立 `RenderThread` 处理渲染，通过 `RenderCache(RwLock)` 与 UI 线程同步。

**[TRAP]** Ephemeral VM（SystemNote/CacheWarning）依赖锚点机制：`ephemeral_notes: Vec<(usize, MessageViewModel)>` 记录插入时的 `view_messages.len()` 作为位置索引（非 MessageId）。RebuildAll 时通过 `(anchor - prefix_len).min(tail_len) + prefix_len` 计算插入位置。`retain()` 路径通过 `anchor >= prefix_len` 过滤过期锚点。新增 ephemeral VM 类型必须同步更新过滤逻辑。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-12-systemnote-position-drift-on-rebuild）

**[TRAP]** BaseMessage vs MessageViewModel 维度混淆：`completed_len_at_round_start` 是 BaseMessage 长度，`prefix_len` 是 VM 索引，两者非 1:1。`prefix_len` 必须用 `round_start_vm_idx`，`drain` 必须钳位。**禁止 Pipeline 内部返回 `RebuildAll`**——Pipeline 不拥有 `round_start_vm_idx`。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-20-llm-error-message-area-clear-flicker）

**[INFO]** `MessageViewModel` 已不再包含 `message_id` 字段。SubAgentGroup 使用 `instance_id: Option<String>` 标识。

**[TRAP]** frozen_subagent_vms 按 agent_id + 位置匹配（先 instance_id 精确匹配，失败后按顺序 agent_id 匹配）。`begin_round()` 清空 frozen_vms 和 ephemeral_notes，但 `done()` 不清空 frozen_subagent_vms（允许 Done→下一轮之间消费）。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-16-frozen-subagent-vms-cross-round-accumulation-duplication）

## 主布局

单 Session 垂直切分（Sticky Header → Messages → Attachment Bar → Panel Area → Input → Status Bar → BG Agent Bar）。高度优先级：Status Bar 固定 3 行 → Input 动态（3~40% 屏幕）→ 面板（60-75% 屏幕）→ 其余分配给消息区。

### 界面组件

| 组件 | 文件 | 说明 |
|------|------|------|
| Welcome Card | `ui/welcome.rs` | 空消息时替代显示，ASCII Art + 功能要点 + 命令提示 |
| Sticky Header | `ui/main_ui/sticky_header.rs` | 滚动时顶部固定显示最后 Human 消息摘要 |
| Attachment Bar | `ui/main_ui/attachment.rs` | 图片附件标签列表，Input 正上方 |
| Input Area | `edit_utils.rs` | `tui_textarea::TextArea` 封装，高度动态 |
| Hints 浮层 | `ui/main_ui/popups/hints.rs` | `/` 前缀命令匹配，输入框上方 |
| @提及弹窗 | `app/at_mention/mod.rs` | `@` 触发文件搜索，200ms 节流 |
| BG Agent Bar | `ui/main_ui/bg_agent_bar.rs` | 后台 Agent 列表，8 色循环 |

### 弹窗系统

统一通过 `InteractionPrompt` 枚举互斥管理。4 种弹窗：
- **HITL 审批**（`popups/hitl.rs`）：批量工具调用逐个审批
- **AskUser 问答**（`popups/ask_user.rs`）：Tab 栏切换 + 选项列表 + 自定义输入
- **OAuth 授权**（`popups/oauth.rs`）：URL 显示 + 浏览器打开
- **Setup Wizard**（`popups/setup_wizard.rs`）：首次配置向导

### 面板系统

12 种 `PanelKind`（分 Session/Global 作用域）：ModelPanel、LoginPanel、ConfigPanel、AgentPanel、HooksPanel、ThreadBrowser（Session）；McpPanel、PluginPanel、CronPanel、TasksPanel、StatusPanel、MemoryPanel（Global）。

互斥组（`MutexGroup`）：Settings（Model/Login/Config）、Agent（Agent/Hooks）、Tools（MCP/Plugin/Cron/Tasks）、Info（Status/Memory）、Thread（ThreadBrowser 独占）。

`PanelManager` + `PanelComponent` trait（`panel_manager.rs`/`panel_component.rs`），新增面板只需定义变体 + 实现 trait。面板内禁止渲染提示行，由 `status_bar_hints()` 统一描述。

### Status Bar

双行布局（`ui/main_ui/status_bar.rs`）：
- **第一行**：权限模式 → 工作目录 → 模型名 → CPU% → MEM → 上下文使用率
- **第二行**：左侧瞬时状态（复制提示/后台 agent/LLM 重试/MCP/LSP）→ 右侧快捷键 hints

瞬时提示用 `Instant` + Duration 控制消失；颜色分级用 `theme::ERROR`/`WARNING`/`SAGE`；面板 hints 通过 `PanelComponent::status_bar_hints()` trait 注入。

### 消息区

Welcome Card 或消息列表 + 滚动条 + spinner。视口裁剪渲染（`viewport_clip`）。`MessageViewModel` 7 种变体：`UserBubble` / `AssistantBubble`（含 Text/Reasoning/ToolUse） / `ToolBlock` / `SystemNote` / `CacheWarning` / `ToolCallGroup` / `SubAgentGroup`。

## i18n

`LcRegistry` 存储在 `ServiceRegistry.lc` 中，翻译资源通过 `include_str!` 编译时嵌入 `locales/{lang}/main.ftl`。

**[TRAP]** `FluentBundle::get_message` 返回的 `FluentMessage` 生命周期绑定在 bundle 上，`tr()` 方法必须返回 `String` 而非 `&str`。

Command trait 的 `description()` 接收 `&LcRegistry` 参数并返回 `String`。`CommandRegistry::match_prefix()` 和 `list()` 均需 `&LcRegistry`。

## 状态管理

**`ServiceRegistry` 与 `GlobalUiState`**：`App` 状态拆分为 `ServiceRegistry`（跨会话共享：config/MCP/cron/provider）和 `GlobalUiState`（纯 UI 临时状态：高亮计时器/弹窗/鼠标检测）。面板 dispatch 宏（`with_global_panels!`/`with_session_panels!`）位于 `event/macros.rs`。

**`CommandRegistry::dispatch` 借用限制 [TRAP]**：`&self` + `&mut App` 冲突，用 `std::mem::take` + put-back 解决。dispatch 期间不可改变 `app.session_mgr` 的 session 实例。

## Compact 事件处理

**[TRAP]** `handle_compact_completed` 必须三步清理：① `pipeline.clear()` ② `pipeline.restore_completed(messages)` ③ `RebuildAll { prefix_len: 0 }`。缺少任一步都会导致旧消息残留或 system 消息泄漏。禁止在 TUI 层触发 auto-compact——所有触发判断在 executor 内部。（详见 spec/global/domains/compact.md#issue_2026-05-20-compact-command-not-triggering）

**[TRAP]** `restore_completed(messages)` 会把 system 消息放入 completed 列表。re_inject 产生的 System 消息不应渲染。`round_start_vm_idx` 和 `completed_len_at_round_start` 必须正确设置。（详见 spec/global/domains/message-pipeline.md#issue_2026-05-20-session-restore-renders-system-prompt）
