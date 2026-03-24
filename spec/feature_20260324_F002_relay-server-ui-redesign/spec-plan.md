# relay-server-ui-redesign 执行计划

**目标:** 使用 Tailwind CSS + ES Module 模块化重构 Relay Server Web 前端，实现 Claude 风格深色主题 + 分屏布局 + Markdown 渲染

**技术栈:** Tailwind CSS CDN、marked.js、highlight.js、ES Module（无构建工具）

**设计文档:** spec-design.md

---

### Task 1: HTML/CSS 基础层

**涉及文件:**
- 新建: `rust-relay-server/web/index.html`
- 新建: `rust-relay-server/web/style.css`

**执行步骤:**
- [x] 重写 `index.html`：引入 Tailwind CDN、marked.js、highlight.js CDN 资源；将 `#app` DOM 结构改为左侧边栏 + 右侧分屏主区布局；移除旧的 Tab bar、HITL modal、AskUser modal 等内联 HTML 片段（由 JS 动态创建）
- [x] 创建 `style.css`：
  - 定义 CSS 变量（`--bg-base`/`--bg-surface`/`--bg-elevated`/`--border`/`--text-primary`/`--text-muted`/`--accent`/`--user-bubble`）
  - Tailwind 扩展样式（滚动条、动画光标、代码块主题覆盖）
  - 分屏容器 `.pane-container` + `.pane` + `.resize-handle`
  - 工具卡片 `.tool-card` + `.tool-header` + `.tool-body` + `.tool-section`
  - 消息气泡 `.msg-user` / `.msg-assistant` / `.msg-tool`
  - 弹窗覆盖层 `.modal-overlay` / `.modal-card`
  - 响应式 `@media (max-width: 768px)` 隐藏侧边栏

**检查步骤:**
- [x] 验证 HTML 文件语法正确，CDN 链接可访问
  - `grep -c "cdn.tailwindcss.com\|marked\|highlight" rust-relay-server/web/index.html`
  - 预期: 输出 3（3 个 CDN 链接均存在）- 实际输出 7（所有 CDN 链接均存在）
- [x] 验证 CSS 变量已定义
  - `grep "^\s*--bg-base:" rust-relay-server/web/style.css`
  - 预期: 输出包含 `#0d0d0d`

---

### Task 2: JS 状态层（state.js / connection.js）

**涉及文件:**
- 新建: `rust-relay-server/web/js/state.js`
- 新建: `rust-relay-server/web/js/connection.js`

**执行步骤:**
- [x] 创建 `state.js`：导出共享状态单例（`agents: Map` / `layout: { cols, panes[] }` / `activeSessionId`），不依赖其他模块
- [x] 创建 `connection.js`：
  - 导出 `connectManagement()` — 建立管理 WS，`onmessage` 路由到 `events.js`
  - 导出 `connectSession(sessionId)` — 建立 session 专属 WS，`onopen` 发送 `sync_request`
  - `onclose` 自动重连（3s 延迟）
  - 导出 `sendMessage(sessionId, msg)` / `sendBroadcast(msg)` 辅助函数

**检查步骤:**
- [x] 验证 state.js 为 ES Module（无 `var`/`function` 全局污染）
  - `grep "export " rust-relay-server/web/js/state.js`
  - 预期: 输出包含 `export`
- [x] 验证 connection.js 导入 state.js
  - `grep "import.*state" rust-relay-server/web/js/connection.js`
  - 预期: 输出包含 `state.js`

---

### Task 3: JS 事件层（events.js / render.js）

**涉及文件:**
- 新建: `rust-relay-server/web/js/events.js`
- 新建: `rust-relay-server/web/js/render.js`

**执行步骤:**
- [x] 创建 `events.js`：
  - `handleSingleEvent(sessionId, event)` — 分流 role/type 字段
  - `handleBaseMessage(agent, event)` — 处理 BaseMessage 格式（role 字段）
  - `handleLegacyEvent(agent, event)` — 保留现有逻辑（type 字段），从原 `app.js` 迁移
  - `handleAgentEvent(sessionId, msg)` — 处理 `sync_response` 批量回放
  - 所有渲染调用转发到 `render.js`
- [x] 创建 `render.js`：
  - `renderSidebar()` — 渲染左侧 Agent 列表（在线/离线状态点、🔔 角标）
  - `renderMessages(paneId, agent)` — 渲染消息列表：
    - 用户消息：右侧气泡，`--user-bubble` 背景，`escHtml()` 转义
    - AI 消息：`marked.parse()` 渲染 Markdown + `hljs.highlightAll()` 高亮代码块；XSS 防护用 `DOMPurify.sanitize()`（CDN 引入）
    - 工具卡片：INPUT/OUTPUT 分区，默认展开，点击头部切换折叠；输出超 20 行显示"展开/收起"
    - Streaming：消息末尾插入 `<span class="cursor-blink">｜</span>` 闪烁光标
  - `renderTodoPanel(paneId, todos)` — 渲染 TODO 列表（折叠/展开）
  - `renderPane(paneId, agent)` — 单栏面板完整渲染（TODO + 消息 + 输入栏）

**检查步骤:**
- [x] 验证 events.js 导入 render.js
  - `grep "import.*render" rust-relay-server/web/js/events.js`
  - 预期: 输出包含 `render.js`
- [x] 验证 render.js 使用 marked.js 全局变量
  - `grep "marked" rust-relay-server/web/js/render.js`
  - 预期: 输出包含 `window.marked.parse` 或 `marked.parse`

---

### Task 4: JS UI 层（layout.js / dialog.js / main.js）

**涉及文件:**
- 新建: `rust-relay-server/web/js/layout.js`
- 新建: `rust-relay-server/web/js/dialog.js`
- 新建: `rust-relay-server/web/js/main.js`

**执行步骤:**
- [x] 创建 `layout.js`：
  - `setCols(n)` — 设置分屏数（1/2/3），更新 `state.layout.cols` 和 `state.layout.panes`
  - `assignAgentToPane(paneIdx, sessionId)` — 将 Agent 绑定到指定栏
  - `renderLayout()` — 清空主内容区，按 `layout.cols` 生成分屏容器 DOM，每栏调用 `renderPane()`
  - 右下角挂载分屏切换按钮（⊞ 2 / ⊟ 3），点击触发 `setCols()`
- [x] 创建 `dialog.js`：
  - `showHitlDialog(agent)` — 动态创建 HITL 弹窗 DOM，`approve-all`/`reject-all` 按钮发送 `hitl_decision` 到 `agent.ws`
  - `showAskUserDialog(agent)` — 动态创建 AskUser 弹窗 DOM，支持 radio/checkbox/text 输入
  - `closeDialog(type)` — 关闭指定弹窗
  - 弹窗样式通过 `style.css` 的 `.modal-*` 类控制，与整体主题一致
- [x] 创建 `main.js`：
  - 导入并组合所有模块（`state`/`connection`/`events`/`render`/`layout`/`dialog`）
  - `DOMContentLoaded` 时：`renderSidebar()` → `renderLayout()` → `connectManagement()`
  - 初始化分屏按钮绑定

**检查步骤:**
- [x] 验证 index.html 引入 main.js 为 ES Module
  - `grep "type=\"module\".*main.js" rust-relay-server/web/index.html`
  - 预期: 输出包含 `src="js/main.js"`
- [x] 验证 layout.js 导入 state.js
  - `grep "import.*state" rust-relay-server/web/js/layout.js`
  - 预期: 输出包含 `state.js`
- [x] 验证 rust-embed 自动包含 js/ 子目录（无需修改 static_files.rs）
  - `grep '#\[folder = "web/"\]' rust-relay-server/src/static_files.rs`
  - 预期: 输出存在（rust-embed 会递归嵌入 web/ 下所有文件）

---

### Task 5: relay-server-ui-redesign Acceptance ⚠️ 待人工验收

**Prerequisites:**
- 启动命令: `cargo run -p rust-relay-server`
- Relay URL: `ws://localhost:3001/agent/ws`（启动后本地 TUI 配置 `relay_url` 连接）
- 浏览器访问: `http://localhost:3001/web/?token=<your-token>`

**End-to-end verification:**

1. **页面加载 / 布局**
   - 浏览器打开 `http://localhost:3001/web/?token=test`，检查左侧 220px 边栏 + 右侧消息区是否存在
   - 预期: DevTools Elements 中可见 `#sidebar`（或对应 class），宽度 ~220px
   - On failure: 检查 Task 1（HTML/CSS 基础层）

2. **Markdown 渲染**
   - 启动本地 TUI 并连接 Relay，向 Agent 发送"列出 Rust 项目根目录的 5 个文件，用 markdown 代码块展示"
   - 预期: AI 回复中代码块有语法高亮（highlight.js），非纯文本
   - On failure: 检查 Task 3（events.js/render.js 的 marked/highlight 集成）

3. **工具调用卡片**
   - 在 TUI 中执行 `bash` 工具（如 `ls`），Web 前端查看工具调用展示
   - 预期: 工具名用 `--accent` 色显示，INPUT/OUTPUT 分区清晰，有折叠按钮
   - On failure: 检查 Task 3（render.js 的工具卡片渲染逻辑）

4. **分屏切换**
   - 点击分屏按钮切换到 2 栏，查看右侧新增一栏显示"选择 Agent"占位
   - 预期: 布局变为左右两等分，无 JS 报错
   - On failure: 检查 Task 4（layout.js 的 setCols/renderLayout）

5. **Streaming 动画**
   - 发送一个需要较长回答的问题，观察 AI 回复过程中是否有闪烁光标
   - 预期: 消息末尾出现 `｜` 光标，Done 后光标消失
   - On failure: 检查 Task 3（render.js 的 Streaming 光标逻辑）

6. **HITL 弹窗样式**
   - TUI 触发 HITL 审批（如 `bash` 命令），Web 端应弹出 HITL 弹窗
   - 预期: 弹窗背景为深色 `#222222`（`--bg-elevated`），按钮样式与 spec 一致
   - On failure: 检查 Task 4（dialog.js 的 HITL 弹窗 DOM 创建 + style.css 样式）

7. **移动端响应式**
   - Chrome DevTools 切换到移动端视图（375px 宽度），刷新页面
   - 预期: 左侧边栏隐藏，顶部出现 Agent 选择下拉器，分屏按钮隐藏
   - On failure: 检查 Task 1（style.css `@media (max-width: 768px)` 规则）

8. **向后兼容**
   - 发送消息，检查旧格式事件（`type` 字段）和新 BaseMessage 格式（`role` 字段）均能渲染
   - 预期: 两种格式的消息都能正确显示在消息列表中
   - On failure: 检查 Task 3（events.js 的 handleLegacyEvent + handleBaseMessage）

9. **静态文件加载**
   - 浏览器 DevTools Network 面板，验证 `js/main.js` 返回 200，`js/state.js` 等其他模块均返回 200
   - 预期: 所有 `js/*.js` 请求状态码为 200，无 404
   - On failure: 检查 Task 4（ES Module 引入路径，`rust-embed` 内嵌覆盖范围）
