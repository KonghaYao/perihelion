# Preact 无打包迁移 人工验收清单

**生成时间:** 2026-03-27 12:10
**关联计划:** [spec-plan.md](./spec-plan.md)
**关联设计:** [spec-design.md](./spec-design.md)

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 编译 Relay Server: `cargo build -p rust-relay-server --features server 2>&1 | tail -3`
- [ ] [AUTO/SERVICE] 启动 Relay Server（需要环境变量 RELAY_TOKEN）: `RELAY_TOKEN=test-token cargo run -p rust-relay-server --features server` (port: 8080)
- [ ] [AUTO] 等待服务就绪: `sleep 3 && curl -s --max-time 5 http://localhost:8080/web/ | grep -c 'id="app"'`

### 测试数据准备
- [ ] 需要一个真实 Agent 连接 Relay Server（使用 `--remote-control` 参数启动 `rust-agent-tui`）以测试 WebSocket 交互功能；纯静态文件验收项（场景1-2）无需此步骤

---

## 验收项目

### 场景 1：静态资源与代码结构

#### - [x] 1.1 旧 web/js/ 目录已删除且编译通过

- **来源:** Task 9 检查步骤
- **操作步骤:**
  1. [A] `ls rust-relay-server/web/js/ 2>&1` → 期望: 输出含 `No such file or directory`
  2. [A] `grep -c 'esm.sh' spec/global/architecture.md` → 期望: 不少于 `1`（CDN 列表已更新）
  3. [A] `cargo build -p rust-relay-server --features server 2>&1 | tail -3` → 期望: 输出含 `Finished` 且不含 `error`
- **异常排查:**
  - 若 web/js/ 仍存在: 手动执行 `rm -rf rust-relay-server/web/js/` 再重新编译
  - 若编译失败: 检查 `rust-relay-server/src/static_files.rs` 的 rust-embed 路径引用

#### - [x] 1.2 8 个组件文件全部存在

- **来源:** Task 10 验证项 9
- **操作步骤:**
  1. [A] `find rust-relay-server/web/components -name '*.js' | sort` → 期望: 列出 8 个文件（App.js、Sidebar.js、PaneContainer.js、Pane.js、MessageList.js、TodoPanel.js、HitlDialog.js、AskUserDialog.js）
  2. [A] `find rust-relay-server/web/components -name '*.js' | wc -l` → 期望: `8`
- **异常排查:**
  - 若文件数不足 8: 检查 spec-plan.md 中 Task 5~8 的执行步骤是否完成

#### - [x] 1.3 state.js Signals 结构正确

- **来源:** Task 1 检查步骤 + Task 10 验证项 5, 10
- **操作步骤:**
  1. [A] `grep -c 'signal(' rust-relay-server/web/state.js` → 期望: 不少于 `5`（agents/layout/activePane/activeMobilePane/connectionStatus/markedReady）
  2. [A] `grep -c 'connectionStatus' rust-relay-server/web/state.js` → 期望: 不少于 `1`
  3. [A] `grep -c 'computed(' rust-relay-server/web/state.js` → 期望: 不少于 `1`（activePaneSessionId）
- **异常排查:**
  - 若 signal 计数不足: 查看 `rust-relay-server/web/state.js` 内容确认导出

#### - [x] 1.4 connection.js / events.js 迁移完成（无旧 render 调用）

- **来源:** Task 2/3 检查步骤 + Task 10 验证项 11, 12
- **操作步骤:**
  1. [A] `grep -c 'renderSidebar\|renderLayout\|renderPane' rust-relay-server/web/connection.js` → 期望: `0`
  2. [A] `grep -c 'connectionStatus.value' rust-relay-server/web/connection.js` → 期望: 不少于 `2`
  3. [A] `grep -c 'renderSidebar\|renderPaneForAllPanes\|showHitlDialog' rust-relay-server/web/events.js` → 期望: `0`
  4. [A] `grep -c 'agents.value = new Map' rust-relay-server/web/events.js` → 期望: 不少于 `3`
- **异常排查:**
  - 若旧 render 调用仍存在: 检查文件内容，手动删除对应行

---

### 场景 2：服务端文件服务与 CDN

#### - [x] 2.1 服务端正确内嵌前端静态文件

- **来源:** Task 10 验证项 1, 2, 4, 5, 6（需 Relay Server 已启动）
- **操作步骤:**
  1. [A] `curl -s http://localhost:8080/web/ | grep -c 'id="app"'` → 期望: `1`（index.html 只含挂载点）
  2. [A] `curl -sI http://localhost:8080/web/app.js | grep -i 'content-type'` → 期望: 含 `javascript` 或 `text/plain`
  3. [A] `curl -s http://localhost:8080/web/js/main.js | wc -c` → 期望: 不超过 `100`（旧文件已不存在，返回 404）
  4. [A] `curl -s http://localhost:8080/web/state.js | grep -c 'signal('` → 期望: 不少于 `3`
  5. [A] `curl -sI http://localhost:8080/web/components/App.js | grep -i 'HTTP'` → 期望: 含 `200`
- **异常排查:**
  - 若 App.js 返回 404: 重新编译 `cargo build -p rust-relay-server --features server` 后重启服务

#### - [x] 2.2 Preact 和 UMD CDN 可访问

- **来源:** Task 10 验证项 3
- **操作步骤:**
  1. [A] `curl -sI --max-time 10 https://esm.sh/preact | grep -i 'HTTP' | head -1` → 期望: 含 `200`
- **异常排查:**
  - 若 CDN 不可访问: 检查网络连接；若网络正常但 CDN 故障，页面仍可加载但 Markdown 渲染降级为纯文本

#### - [x] 2.3 浏览器加载无 JS 错误

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在浏览器中打开 `http://localhost:8080/web/?token=test-token`，打开 DevTools（F12）→ Console 面板，查看是否有红色 Error 日志（忽略 CORS warn） → 是/否（是=有错误=失败）
  2. [H] 检查 Console 中是否出现 `Cannot use import statement` 或 `SyntaxError` → 是/否（是=有错误=失败）
  3. [H] 页面主体是否渲染出来（侧边栏 + 主内容区可见，不是空白页） → 是/否（是=渲染成功=通过）
- **异常排查:**
  - 若空白页: 在 Console 查看具体报错，重点检查 esm.sh CDN 是否可访问
  - 若有 SyntaxError: 检查对应组件 JS 文件的语法

---

### 场景 3：分屏布局与侧边栏

#### - [x] 3.1 1/2/3 栏分屏布局切换

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在 `http://localhost:8080/web/?token=test-token`（桌面浏览器，窗口宽度 > 768px），点击布局工具栏的「1」按钮，查看主内容区是否变为单栏 → 是/否
  2. [H] 点击「2」按钮，主内容区是否出现两个面板（有分隔线） → 是/否
  3. [H] 点击「3」按钮，主内容区是否出现三个面板 → 是/否
- **异常排查:**
  - 若按钮无响应: 检查 PaneContainer.js 中 `setCols()` 调用和 `layout` signal 订阅

#### - [x] 3.2 侧边栏 Agent 列表与面板绑定

- **来源:** Task 5 + spec-design.md
- **操作步骤:**
  1. [H] 侧边栏底部连接状态指示器是否显示（「已连接」文字或绿色指示点） → 是/否
  2. [H] 当有 Agent 连接时，侧边栏 Agent 列表是否显示 Agent 名称与在线状态点 → 是/否
  3. [H] 点击侧边栏中的一个 Agent，主内容区当前激活面板是否绑定该 Agent（面板显示消息列表而非「未分配」占位） → 是/否
- **异常排查:**
  - 若无 Agent 显示: 确认 Agent TUI 已通过 `--remote-control` 连接到 Relay Server

---

### 场景 4：消息渲染与 WebSocket

#### - [x] 4.1 WebSocket 管理端/会话端连接

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [A] `curl -s http://localhost:8080/agents` → 期望: 返回 JSON（可为空数组 `[]` 或含 Agent 列表）
  2. [H] 打开 DevTools → Network 面板 → 筛选 WS，刷新页面，查看是否出现 WebSocket 连接（URL 含 `/web/ws`） → 是/否
  3. [H] WebSocket 连接状态是否为「101 Switching Protocols」（不是 4xx/5xx） → 是/否
- **异常排查:**
  - 若 WS 连接失败: 确认 URL 参数包含 `token=test-token`；检查 Relay Server 日志

#### - [x] 4.2 消息类型正确渲染

- **来源:** Task 7 + spec-design.md
- **操作步骤:**
  1. [H] 在输入栏发送一条普通文字（如「你好」），用户消息是否以不同样式显示在消息列表右侧（class 含 `msg-user`） → 是/否
  2. [H] Agent 回复消息是否显示（class 含 `msg-assistant`），流式输出时是否出现光标闪烁 → 是/否
  3. [H] 若 Agent 触发工具调用，工具消息是否以卡片形式折叠显示（标题含工具名和「▶ 展开」） → 是/否
- **异常排查:**
  - 若消息不显示: 检查 events.js 中 `upsertMessage` 调用和 `agents.value = new Map(agents.value)` 刷新

#### - [x] 4.3 Markdown 渲染与代码高亮

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 让 Agent 回复含 Markdown 格式的内容（如含 `**粗体**`、`# 标题`、代码块），检查是否渲染为格式化 HTML（非纯文本） → 是/否
  2. [H] 代码块内容是否有语法高亮颜色（非黑白纯文本） → 是/否
- **异常排查:**
  - 若 Markdown 未渲染（显示原始 * # 字符）: 检查 DevTools Console 中 `markedReady` 是否变为 true；检查 CDN 脚本是否加载成功

#### - [x] 4.4 工具卡片折叠/展开与超长输出截断

- **来源:** Task 7 spec-design.md
- **操作步骤:**
  1. [H] 点击工具消息卡片的标题区域（含工具名的 header），卡片是否展开显示 INPUT 和 OUTPUT → 是/否
  2. [H] 若工具输出超过 20 行，是否显示「▶ 展开全部」按钮，点击后是否显示完整内容 → 是/否（若无超长输出可跳过此步）
- **异常排查:**
  - 若卡片不可展开: 检查 MessageList.js 中 ToolCard 组件的 `expanded` state 逻辑

---

### 场景 5：HITL/AskUser 弹窗

#### - [x] 5.1 HITL 工具审批弹窗

- **来源:** Task 8 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -c 'Approve\|Reject' rust-relay-server/web/components/HitlDialog.js` → 期望: 不少于 `2`
  2. [A] `grep -c 'agents.value' rust-relay-server/web/components/HitlDialog.js` → 期望: 不少于 `1`
  3. [H] 当 Agent 执行需要 HITL 审批的工具时（如 bash），页面是否弹出「工具审批」弹窗，列出工具名和参数 → 是/否
  4. [H] 点击「全部批准」，弹窗是否关闭，Agent 继续执行 → 是/否
  5. [H] 再次触发 HITL，点击「全部拒绝」，弹窗是否关闭，Agent 收到拒绝响应 → 是/否
- **异常排查:**
  - 若弹窗不出现: 检查 events.js `approval_needed` 处理中 `pendingHitl` 赋值和 `agents.value` 刷新
  - 若批准后弹窗不关闭: 检查 HitlDialog.js `onApprove` 中 `agent.pendingHitl = null` 后的 signal 刷新

#### - [x] 5.2 AskUser 问答弹窗（单选/多选/文本）

- **来源:** Task 8 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -c 'multi_select' rust-relay-server/web/components/AskUserDialog.js` → 期望: 不少于 `1`
  2. [A] `grep -c 'checkbox' rust-relay-server/web/components/AskUserDialog.js` → 期望: 不少于 `1`
  3. [H] 当 Agent 调用 `ask_user` 工具时，页面是否弹出「Agent 提问」弹窗，显示问题内容 → 是/否
  4. [H] 若问题有选项（radio 单选），是否可以点选其中一个选项，点「提交」后弹窗关闭 → 是/否
  5. [H] 若问题为 `multi_select: true`（checkbox 多选），是否可以勾选多个选项后提交 → 是/否（若无多选题可跳过）
- **异常排查:**
  - 若弹窗不出现: 检查 events.js `ask_user_batch` 处理中 `pendingAskUser` 赋值
  - 若提交无响应: 检查 AskUserDialog.js `onSubmit` 中 `sendMessage` 和 `pendingAskUser = null`

---

### 场景 6：TODO 面板与命令

#### - [x] 6.1 TODO 面板折叠/展开

- **来源:** Task 7 + spec-design.md 验收标准
- **操作步骤:**
  1. [H] 当 Agent 使用 `todo_write` 工具更新任务后，面板顶部是否出现「📋 TODO」区域 → 是/否
  2. [H] 点击「📋 TODO」标题行，TODO 列表是否收起；再次点击是否展开 → 是/否
- **异常排查:**
  - 若 TODO 面板不显示: 检查 Pane.js 中 `<TodoPanel todos={agent.todos} />` 传递的 todos 数组

#### - [x] 6.2 /clear 命令清空会话

- **来源:** Task 6 + spec-design.md
- **操作步骤:**
  1. [A] `grep -c 'clear_thread' rust-relay-server/web/components/Pane.js` → 期望: `1`
  2. [H] 在输入栏输入 `/clear` 并回车，消息列表是否立即清空（面板变为空状态） → 是/否
- **异常排查:**
  - 若清空无响应: 检查 Pane.js InputBar 的 `/clear` 分支中 `agent.messages = []` 和 signal 刷新

#### - [x] 6.3 /compact 命令触发上下文压缩

- **来源:** Task 6
- **操作步骤:**
  1. [H] 在输入栏输入 `/compact` 并回车，输入栏是否清空（命令已发送），Agent 服务端是否收到 `compact_thread` 消息（可在 Relay Server 日志中确认） → 是/否
- **异常排查:**
  - 若命令无响应: 检查 Pane.js InputBar 的 `/compact` 分支中 `sendMessage(sessionId, { type: 'compact_thread' })`

---

### 场景 7：移动端响应式布局

#### - [x] 7.1 移动端布局与导航

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在 DevTools 中切换为移动端模式（如 iPhone 375px 宽），刷新页面，侧边栏是否隐藏，顶部是否出现导航栏（含汉堡菜单按钮） → 是/否
  2. [H] 点击汉堡按钮，侧边栏是否从左侧滑入显示，同时出现遮罩层 → 是/否
  3. [H] 点击遮罩层或侧边栏外区域，侧边栏是否收起 → 是/否
- **异常排查:**
  - 若移动端仍显示桌面布局: 检查 App.js 中 `window.matchMedia('(max-width: 768px)')` 判断和 CSS 中 `@media` 响应式规则

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景1 | 1.1 | 旧文件清理与编译通过 | 3 | 0 | ⬜ | |
| 场景1 | 1.2 | 8 个组件文件完整存在 | 2 | 0 | ⬜ | |
| 场景1 | 1.3 | state.js Signals 结构正确 | 3 | 0 | ⬜ | |
| 场景1 | 1.4 | connection.js / events.js 迁移完成 | 4 | 0 | ⬜ | |
| 场景2 | 2.1 | 服务端正确内嵌前端静态文件 | 5 | 0 | ⬜ | |
| 场景2 | 2.2 | Preact CDN 可访问 | 1 | 0 | ⬜ | |
| 场景2 | 2.3 | 浏览器加载无 JS 错误 | 0 | 3 | ⬜ | |
| 场景3 | 3.1 | 1/2/3 栏分屏布局切换 | 0 | 3 | ⬜ | |
| 场景3 | 3.2 | 侧边栏 Agent 列表与面板绑定 | 0 | 3 | ⬜ | |
| 场景4 | 4.1 | WebSocket 管理端/会话端连接 | 1 | 2 | ⬜ | |
| 场景4 | 4.2 | 消息类型正确渲染 | 0 | 3 | ⬜ | |
| 场景4 | 4.3 | Markdown 渲染与代码高亮 | 0 | 2 | ⬜ | |
| 场景4 | 4.4 | 工具卡片折叠/展开与超长输出截断 | 0 | 2 | ⬜ | |
| 场景5 | 5.1 | HITL 工具审批弹窗 | 2 | 3 | ⬜ | |
| 场景5 | 5.2 | AskUser 问答弹窗 | 2 | 3 | ⬜ | |
| 场景6 | 6.1 | TODO 面板折叠/展开 | 0 | 2 | ⬜ | |
| 场景6 | 6.2 | /clear 命令清空会话 | 1 | 1 | ⬜ | |
| 场景6 | 6.3 | /compact 命令触发上下文压缩 | 0 | 1 | ⬜ | |
| 场景7 | 7.1 | 移动端响应式布局 | 0 | 3 | ⬜ | |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
