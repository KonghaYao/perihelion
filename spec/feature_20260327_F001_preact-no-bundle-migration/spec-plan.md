# Preact 无打包迁移 执行计划

**目标:** 将 `rust-relay-server/web/` 前端从手动 DOM 操作迁移到 Preact + Signals + htm，无构建工具

**技术栈:** Preact (esm.sh)、htm、@preact/signals、CDN UMD 脚本（marked/hljs/DOMPurify）

**设计文档:** [spec-design.md](./spec-design.md)

---

### Task 1: 基础层 — utils/html.js + state.js (Signals 版)

**涉及文件:**
- 新建: `rust-relay-server/web/utils/html.js`
- 新建: `rust-relay-server/web/state.js`

**执行步骤:**
- [x] 新建 `utils/html.js`，统一导出 htm 绑定后的 `html` 函数，避免每个组件文件重复 `htm.bind(h)`
  - ```js
    import { h } from 'https://esm.sh/preact'
    import htm from 'https://esm.sh/htm'
    export const html = htm.bind(h)
    ```
- [x] 新建 `state.js`（根目录），将原 `web/js/state.js` 的数据结构转换为 Preact Signals
  - 用 `signal(new Map())` 替代 `state.agents = new Map()`
  - 用 `signal({ cols: 1, panes: [null,null,null] })` 替代 `state.layout`
  - 新增 `connectionStatus` signal（值：`'connected'|'reconnecting'|'disconnected'`）
  - 新增 `markedReady` signal（值：`false`，CDN 加载后置为 `true`）
  - 保留 `upsertAgent`、`getAgent`、`upsertMessage`、`setPaneAgent`、`clearPane` 辅助函数，但内部改为 `agents.value = new Map(agents.value)` 写法触发 Signals 依赖追踪
  - 导出 `computed(() => layout.value.panes[activePane.value] ?? null)` 作为 `activePaneSessionId`

**检查步骤:**
- [x] 语法检查 state.js 无错误
  - `node --input-type=module < rust-relay-server/web/state.js 2>&1 | head -5`
  - 预期: 无错误输出（或仅 `Cannot use import statement...` 说明需浏览器环境，不算错误）
- [x] utils/html.js 文件存在且包含 htm.bind
  - `grep -c 'htm.bind' rust-relay-server/web/utils/html.js`
  - 预期: `1`

---

### Task 2: connection.js 迁移

**涉及文件:**
- 新建: `rust-relay-server/web/connection.js`

**执行步骤:**
- [x] 将 `web/js/connection.js` 复制到 `web/connection.js`，修改 import 路径（`./state.js` 替代 `./state.js` 中的旧路径）
- [x] 移除 `import { renderSidebar, renderLayout } from './render.js'`，改为直接操作 signals
- [x] 将 `updateConnectionIndicator` 函数替换为 `connectionStatus.value = 'connected'` 等 signal 赋值
  - `ws.onopen` → `connectionStatus.value = 'connected'`
  - `ws.onclose` → `connectionStatus.value = 'reconnecting'`
- [x] 在 `ws.onopen`、`ws.onclose` 中移除 `renderSidebar()` 和 `renderLayout()` 调用（Preact 组件自动响应 signals 变化）
- [x] `import { handleAgentEvent, handleBroadcast } from './events.js'` 路径改为新位置

**检查步骤:**
- [x] 新 connection.js 中不含任何 renderSidebar/renderLayout 调用
  - `grep -c 'renderSidebar\|renderLayout\|renderPane' rust-relay-server/web/connection.js`
  - 预期: `0`
- [x] 新 connection.js 中包含 connectionStatus signal 赋值
  - `grep -c 'connectionStatus.value' rust-relay-server/web/connection.js`
  - 预期: 不少于 `2`（connected + reconnecting）

---

### Task 3: events.js 迁移

**涉及文件:**
- 新建: `rust-relay-server/web/events.js`

**执行步骤:**
- [x] 将 `web/js/events.js` 复制到 `web/events.js`，更新所有 import 路径
- [x] 移除 `import { renderSidebar, renderMessages, renderTodoPanel, renderPane } from './render.js'`
- [x] 移除 `import { showHitlDialog, showAskUserDialog, closeDialog } from './dialog.js'`
- [x] 删除 `renderPaneForAllPanes()` 函数（Preact 自动重渲染，不需要手动触发）
- [x] 在所有原本调用 `renderSidebar()` / `renderPaneForAllPanes()` 的位置，改为 signal 触发：
  - `agents.value = new Map(agents.value)` —— 替换原来的 render 调用，让 Preact 感知 Map 变化
- [x] 移除 `approval_needed` / `ask_user_batch` 中的 `showHitlDialog` / `showAskUserDialog` 调用（弹窗组件自行读 pending 状态响应）
- [x] `agent_running` / `agent_done` 事件中，将 `agent.isRunning` 赋值后补充 signal 刷新

**检查步骤:**
- [x] 新 events.js 中不含 renderSidebar/renderLayout/renderPane 等 render 调用
  - `grep -c 'render\(Sidebar\|Layout\|Pane\|Messages\|TodoPanel\)' rust-relay-server/web/events.js`
  - 预期: `0`
- [x] 新 events.js 中不含 showHitlDialog/showAskUserDialog 调用
  - `grep -c 'showHitlDialog\|showAskUserDialog\|closeDialog' rust-relay-server/web/events.js`
  - 预期: `0`
- [x] 新 events.js 中包含 agents signal 刷新
  - `grep -c 'agents.value = new Map' rust-relay-server/web/events.js`
  - 预期: 不少于 `3`

---

### Task 4: index.html 最小骨架 + app.js 入口

**涉及文件:**
- 修改: `rust-relay-server/web/index.html`
- 修改: `rust-relay-server/web/app.js`（重写）

**执行步骤:**
- [x] 重写 `index.html` 为最小骨架
  - 保留 highlight.js CSS CDN 链接和 `style.css` 链接
  - 移除所有手写 HTML 结构（侧边栏、主内容、弹窗 HTML 模板全部删除）
  - 只保留 `<div id="app"></div>` 和 `<script type="module" src="app.js"></script>`
  - 移除 Tailwind CSS CDN（`cdn.tailwindcss.com`）——所有样式改由 style.css 和 CSS 变量提供
- [x] 重写 `app.js` 为 Preact 入口
  - 实现 `loadScript(src)` 工具函数（动态注入 `<script>` 返回 Promise）
  - `Promise.allSettled()` 并行加载 highlight.js、marked.js、DOMPurify 三个 UMD 脚本
  - 加载完成后 `markedReady.value = true`
  - 立即调用 `render(html\`<${App} />\`, document.getElementById('app'))`（不等待 CDN）

**检查步骤:**
- [x] index.html 不含旧 HTML 弹窗模板
  - `grep -c 'hitl-modal\|askuser-modal' rust-relay-server/web/index.html`
  - 预期: `0`
- [x] index.html 不含 Tailwind CDN
  - `grep -c 'cdn.tailwindcss.com' rust-relay-server/web/index.html`
  - 预期: `0`
- [x] app.js 包含 Preact render 调用
  - `grep -c "render(" rust-relay-server/web/app.js`
  - 预期: 不少于 `1`
- [x] app.js 包含 markedReady signal 赋值
  - `grep -c 'markedReady.value = true' rust-relay-server/web/app.js`
  - 预期: `1`

---

### Task 5: App.js + Sidebar.js 组件

**涉及文件:**
- 新建: `rust-relay-server/web/components/App.js`
- 新建: `rust-relay-server/web/components/Sidebar.js`

**执行步骤:**
- [x] 创建 `components/App.js`，实现根布局组件
  - `useEffect` 里调用 `connectManagement()`（仅在 token 存在时），token 从 `new URLSearchParams(location.search).get('token')` 读取
  - 读取 `layout.value` 决定是否移动端布局（`window.matchMedia('(max-width: 768px)').matches`）
  - 渲染结构：`<div id="app">` → `<aside>（Sidebar）` + `<main>（PaneContainer）` + `<HitlDialog>` + `<AskUserDialog>`
  - 移动端：汉堡按钮、顶部导航栏、遮罩层全部在此组件内
  - 若无 token，渲染错误提示，不调用 connect
- [x] 创建 `components/Sidebar.js`，实现 Agent 列表侧边栏
  - 读取 `agents` signal，映射为列表项
  - 每项显示在线状态点（`dot-online` / `dot-offline`）、Agent 名、通知 badge（pendingHitl 或 pendingAskUser 时显示 🔔）
  - 点击 agent 项：调用 `assignAgentToPane(activePane.value, sessionId)` 并在移动端关闭侧边栏
  - 底部连接状态指示器读取 `connectionStatus` signal

**检查步骤:**
- [x] 两个文件存在且不为空
  - `wc -l rust-relay-server/web/components/App.js rust-relay-server/web/components/Sidebar.js`
  - 预期: 两个文件各超过 20 行
- [x] App.js 包含 useEffect
  - `grep -c 'useEffect' rust-relay-server/web/components/App.js`
  - 预期: 不少于 `1`
- [x] Sidebar.js 读取 agents signal
  - `grep -c 'agents.value' rust-relay-server/web/components/Sidebar.js`
  - 预期: 不少于 `1`

---

### Task 6: PaneContainer.js + Pane.js 组件

**涉及文件:**
- 新建: `rust-relay-server/web/components/PaneContainer.js`
- 新建: `rust-relay-server/web/components/Pane.js`

**执行步骤:**
- [x] 创建 `components/PaneContainer.js`，实现分屏容器
  - 读取 `layout` signal，按 `cols` 渲染 1~3 个 `<Pane>` 和 `<div class="pane-divider">`
  - 布局工具栏按钮（1/2/3）：点击调用 `setCols(n)`，当前 cols 对应按钮加 `active` class
  - 移动端：仅渲染 `activeMobilePane` 对应的单面板；渲染移动端 Tab 栏（有多个绑定面板时）
- [x] 创建 `components/Pane.js`，实现单面板
  - 接受 `paneId` prop，从 `layout.value.panes[paneId]` 获取 `sessionId`
  - 若无 sessionId：渲染空占位（含 Agent 下拉选择）
  - 若有 sessionId：渲染 `<TodoPanel>` + `<MessageList>` + 输入栏
  - 输入栏逻辑：普通消息 → `sendMessage(sessionId, { type: 'user_input', text })`；`/clear` → `sendMessage(...clear_thread)` + 清空 agent.messages；`/compact` → `sendMessage(...compact_thread)`
  - 输入栏 Enter 键发送（非 Shift+Enter，非 isComposing）

**检查步骤:**
- [x] 两个文件存在
  - `wc -l rust-relay-server/web/components/PaneContainer.js rust-relay-server/web/components/Pane.js`
  - 预期: 两个文件各超过 30 行
- [x] PaneContainer.js 包含 layout signal 读取
  - `grep -c 'layout.value' rust-relay-server/web/components/PaneContainer.js`
  - 预期: 不少于 `1`
- [x] Pane.js 包含 /clear 处理
  - `grep -c 'clear_thread' rust-relay-server/web/components/Pane.js`
  - 预期: `1`

---

### Task 7: MessageList.js + TodoPanel.js 组件

**涉及文件:**
- 新建: `rust-relay-server/web/components/MessageList.js`
- 新建: `rust-relay-server/web/components/TodoPanel.js`

**执行步骤:**
- [x] 创建 `components/MessageList.js`，实现消息渲染组件
  - 接受 `messages`（数组）和 `paneId` prop
  - 读取 `markedReady` signal：为 `false` 时降级为纯文本，为 `true` 时用 `window.marked.parse()` + `DOMPurify.sanitize()` 渲染 Markdown
  - 消息类型处理：
    - `user`：`<div class="message msg-user">` 纯文本
    - `assistant`：`<div class="message msg-assistant">` Markdown 内容，streaming 时显示光标
    - `tool`：`<div class="message tool-card">` 折叠卡片（header 点击展开/折叠，超过 20 行的 output 折叠）
    - `error`：`<div class="message msg-error">` 纯文本
  - `isRunning` 状态时，在消息列表末尾追加 loading 气泡（三点动画 + 停止按钮）
  - 消息更新后自动滚动到底部（仅当原本在底部时，50px 容差）
- [x] 创建 `components/TodoPanel.js`，实现 TODO 状态面板
  - 接受 `todos` 数组 prop
  - `todos` 为空时隐藏面板
  - 点击 header 可折叠/展开列表
  - 三种状态样式：`in_progress`（`todo-in-progress`）/ `done|completed`（`todo-done`）/ 其他（`todo-pending`）

**检查步骤:**
- [x] MessageList.js 包含 markedReady signal 读取
  - `grep -c 'markedReady.value' rust-relay-server/web/components/MessageList.js`
  - 预期: 不少于 `1`
- [x] MessageList.js 包含四种消息类型处理
  - `grep -c "case 'user'\|case 'assistant'\|case 'tool'\|case 'error'" rust-relay-server/web/components/MessageList.js`
  - 预期: `4`
- [x] TodoPanel.js 存在且包含 todo-in-progress class
  - `grep -c 'todo-in-progress' rust-relay-server/web/components/TodoPanel.js`
  - 预期: 不少于 `1`

---

### Task 8: HitlDialog.js + AskUserDialog.js 组件

**涉及文件:**
- 新建: `rust-relay-server/web/components/HitlDialog.js`
- 新建: `rust-relay-server/web/components/AskUserDialog.js`

**执行步骤:**
- [x] 创建 `components/HitlDialog.js`，实现 HITL 审批弹窗（全局唯一）
  - 读取 `agents` signal，查找当前活跃 session 的 `pendingHitl` 状态
  - `pendingHitl` 为 null 时渲染 `null`（不渲染弹窗）
  - 弹窗内容：遍历 `requests` 列表，每个 request 显示工具名和格式化后的 input JSON
  - "全部批准"按钮：构建 `decisions` 数组（`Approve`）→ `sendMessage(sessionId, { type: 'hitl_decision', decisions })` → 清除 `pendingHitl` → signal 刷新
  - "全部拒绝"按钮：同上，decision 改为 `Reject`
  - 关闭按钮和遮罩点击：清除 `pendingHitl`（不发送 WS 消息）
- [x] 创建 `components/AskUserDialog.js`，实现 AskUser 问答弹窗（全局唯一）
  - 读取 `agents` signal，查找 `pendingAskUser` 状态
  - `pendingAskUser` 为 null 时渲染 `null`
  - 问题类型：有 options → radio（单选）/ checkbox（多选，`multi_select: true`）；无 options → text input
  - 支持 `allow_custom_input`：在选项后追加自由文本输入框
  - "提交"按钮：收集 `answers` → `sendMessage(sessionId, { type: 'ask_user_response', answers })` → 清除 `pendingAskUser` → signal 刷新
  - key 用 `q.tool_call_id || q.description || q.question || 'q{i}'`

**检查步骤:**
- [x] HitlDialog.js 包含全部批准和全部拒绝逻辑
  - `grep -c 'Approve\|Reject' rust-relay-server/web/components/HitlDialog.js`
  - 预期: 不少于 `2`
- [x] AskUserDialog.js 包含多选 checkbox 逻辑
  - `grep -c 'multi_select\|checkbox' rust-relay-server/web/components/AskUserDialog.js`
  - 预期: 不少于 `2`
- [x] 两个弹窗组件均读取 agents signal
  - `grep -rn 'agents.value' rust-relay-server/web/components/HitlDialog.js rust-relay-server/web/components/AskUserDialog.js | wc -l`
  - 预期: 不少于 `2`

---

### Task 9: 旧文件清理 + architecture.md 更新

**涉及文件:**
- 修改: `spec/global/architecture.md`（更新 CDN 列表）
- 删除: `rust-relay-server/web/js/` 目录下全部文件
- 删除: 旧 `rust-relay-server/web/app.js`（已在 Task 4 重写，此步骤确认清理旧版 js/ 目录）

**执行步骤:**
- [x] 删除 `web/js/` 目录下 7 个旧文件（main.js、render.js、layout.js、dialog.js、state.js、connection.js、events.js）
  - `rm -rf rust-relay-server/web/js/`
- [x] 更新 `spec/global/architecture.md` 中 "Web 前端 CDN（relay-server）" 一行，在 Tailwind/marked/hljs/DOMPurify 基础上追加 preact、htm、@preact/signals（均 esm.sh CDN）
- [x] 确认 `rust-relay-server/src/static_files.rs` 中的文件引用仍然有效（`rust-embed` 扫描的是 `web/` 目录，新增文件会自动包含；旧 js/ 目录删除后不影响编译）

**检查步骤:**
- [x] web/js/ 目录已删除
  - `ls rust-relay-server/web/js/ 2>&1`
  - 预期: `No such file or directory`
- [x] architecture.md 已更新 CDN 列表
  - `grep -c 'esm.sh' spec/global/architecture.md`
  - 预期: 不少于 `1`
- [x] Relay Server 可正常编译
  - `cargo build -p rust-relay-server --features server 2>&1 | tail -3`
  - 预期: 输出包含 `Finished` 且无 `error`

---

### Task 10: Preact 迁移验收

**Prerequisites:**
- 启动命令: `cargo run -p rust-relay-server --features server`
- Relay Server 默认监听: `http://localhost:8080`
- 测试用 token: 在命令行参数或环境变量中配置，假设为 `test-token`
- 浏览器访问: `http://localhost:8080/web/?token=test-token`

**端对端验证:**

1. 页面加载无 JS 错误
   - `curl -s http://localhost:8080/web/ | grep -c 'id="app"'`
   - 预期: `1`（只含挂载点，无旧 HTML 结构）
   - On failure: 检查 Task 4 index.html 重写

2. app.js 文件存在且可被服务端提供
   - `curl -sI http://localhost:8080/web/app.js | grep -i 'content-type'`
   - 预期: 响应头包含 `javascript` 或 `text/plain`
   - On failure: 检查 Task 4 app.js 是否被 rust-embed 正确内嵌

3. Preact 依赖 CDN 可访问
   - `curl -sI https://esm.sh/preact | grep -i 'HTTP'`
   - 预期: `HTTP/2 200` 或 `HTTP/1.1 200`
   - On failure: 检查网络连接或 CDN 地址

4. 旧 js/ 目录文件不再被提供
   - `curl -s http://localhost:8080/web/js/main.js | wc -c`
   - 预期: 返回 404 页面（小于 100 字节内容）或 `0`
   - On failure: 检查 Task 9 旧文件清理

5. state.js 包含 Signals 导出
   - `curl -s http://localhost:8080/web/state.js | grep -c 'signal('`
   - 预期: 不少于 `3`
   - On failure: 检查 Task 1 state.js

6. 组件目录文件均可被服务端提供
   - `curl -sI http://localhost:8080/web/components/App.js | grep -i 'HTTP'`
   - 预期: `HTTP/2 200` 或 `HTTP/1.1 200`
   - On failure: 检查 Task 5 组件文件 + rust-embed 编译

7. architecture.md CDN 列表已更新
   - `grep 'esm.sh' spec/global/architecture.md`
   - 预期: 包含 preact / htm / @preact/signals 字样
   - On failure: 检查 Task 9 architecture.md 更新

8. Relay Server 编译无 warning（embed 文件引用完整）
   - `cargo build -p rust-relay-server --features server 2>&1 | grep -c 'warning\|error'`
   - 预期: `0`（或仅有已知无关 warning）
   - On failure: 检查 static_files.rs 引用

9. 新旧文件行数对比（迁移完整性）
   - `find rust-relay-server/web/components -name '*.js' | wc -l`
   - 预期: `8`（App、Sidebar、PaneContainer、Pane、MessageList、TodoPanel、HitlDialog、AskUserDialog）
   - On failure: 检查 Task 5~8 各组件是否全部创建

10. state.js 包含 connectionStatus signal
    - `grep -c 'connectionStatus' rust-relay-server/web/state.js`
    - 预期: 不少于 `1`
    - On failure: 检查 Task 1 state.js

11. connection.js 已迁移（无旧 render 调用）
    - `grep -c 'renderSidebar\|renderLayout' rust-relay-server/web/connection.js`
    - 预期: `0`
    - On failure: 检查 Task 2 connection.js

12. events.js 已迁移（无旧 render 调用）
    - `grep -c 'renderSidebar\|renderPaneForAllPanes\|showHitlDialog' rust-relay-server/web/events.js`
    - 预期: `0`
    - On failure: 检查 Task 3 events.js
