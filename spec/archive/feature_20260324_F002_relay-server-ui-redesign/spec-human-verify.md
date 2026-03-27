# relay-server-ui-redesign 人工验收清单

**生成时间:** 2026-03-24
**关联计划:** spec-plan.md
**关联设计:** spec-design.md

---

## 验收前准备

### 环境要求
- [x] [AUTO] 检查 Rust 工具链: `rustc --version && cargo --version`
  → rustc 1.91.0, cargo 1.91.0
- [x] [AUTO] 编译 rust-relay-server: `cargo build -p rust-relay-server 2>&1 | tail -3`
  → Finished（无 error）
- [x] [AUTO/SERVICE] 启动 Relay Server: `cargo run -p rust-relay-server` (port: 3001)
  → 服务已启动，PID: byb4ksbm1
- [x] [AUTO] 验证 Relay HTTP 接口: `curl -s http://localhost:3001/health`
  → 返回 HTTP 200
- [ ] [MANUAL] 配置 TUI 连接 Relay（可选，如需端到端测试）

### 测试数据准备
- Relay Server 已启动，监听 `http://localhost:3001`
- Token 可任意设置（前端 URL 中传入 `?token=<your-token>`）

---

## 验收项目

### 场景 1：构建与基础设施

#### - [x] 1.1 Rust 编译通过
- **来源:** Task 1-4 实现正确性
- **操作步骤:**
  1. [A] `cd /Users/konghayao/code/ai/perihelion && cargo build -p rust-relay-server 2>&1 | grep -E "(error|warning|Finished)"`
     → 期望: 无 `error`，输出包含 `Finished`
  2. [A] `ls rust-relay-server/web/js/`
     → 期望: 列出 main.js, state.js, connection.js, events.js, render.js, layout.js, dialog.js 共 7 个文件
  3. [A] `wc -l rust-relay-server/web/style.css`
     → 期望: 输出行数 > 200（样式文件有实质内容）
- **异常排查:**
  - 如果编译 error: 检查 `cargo build` 全量输出，定位具体文件错误

#### - [x] 1.2 CDN 资源引入完整（grep 5 处，DOMPurify 已确认）
- **来源:** Task 1 spec-plan.md
- **操作步骤:**
  1. [A] `grep -c "cdn.tailwindcss.com\|jsdelivr.*marked\|cdnjs.*highlight\|dompurify" rust-relay-server/web/index.html`
     → 期望: 输出 4（Tailwind + marked + highlight + DOMPurify 均引入）
  2. [A] `grep "dompurify" rust-relay-server/web/index.html`
     → 期望: 输出包含 `purify.min.js`
- **异常排查:**
  - 如果 CDN 缺失: 对照 spec-design.md 的「CDN 依赖」章节补全

#### - [x] 1.3 CSS 变量体系完整（--bg-base, --accent, --user-bubble 均已定义）
- **来源:** Task 1 spec-design.md
- **操作步骤:**
  1. [A] `grep "^\s*--bg-base:" rust-relay-server/web/style.css`
     → 期望: 输出 `--bg-base: #0d0d0d;`
  2. [A] `grep "^\s*--accent:" rust-relay-server/web/style.css`
     → 期望: 输出 `--accent: #e8975e;`
  3. [A] `grep "^\s*--user-bubble:" rust-relay-server/web/style.css`
     → 期望: 输出 `--user-bubble: #2d4a7a;`
- **异常排查:**
  - 如果变量缺失: 在 style.css `:root` 块中补充

#### - [x] 1.4 ES Module 模块链路（main.js 模块入口 + state.js 导出 + connection.js 导入均已确认）
- **来源:** Task 2-4 spec-plan.md
- **操作步骤:**
  1. [A] `grep "type=\"module\".*main.js" rust-relay-server/web/index.html`
     → 期望: 输出包含 `src="js/main.js"`
  2. [A] `grep "export " rust-relay-server/web/js/state.js`
     → 期望: 输出包含 `export const state` 和 `export function`
  3. [A] `grep "from './state.js'" rust-relay-server/web/js/connection.js`
     → 期望: 输出包含 `from './state.js'`
- **异常排查:**
  - 如果 ES Module 加载失败: 检查浏览器 Console 的 `import` 错误

---

### 场景 2：页面布局与主题

#### - [x] 2.1 页面加载 / 布局结构（5 个关键 ID 均存在，人工确认侧栏和分屏按钮可见）
- **来源:** Task 5 spec-plan.md / spec-design.md
- **操作步骤:**
  1. [A] `curl -s http://localhost:3001/web/ | grep -o 'id="sidebar"\|id="pane-container"\|id="layout-toolbar"\|id="agent-list"\|id="connection-indicator"'`
     → 期望: 输出包含 sidebar、pane-container、layout-toolbar、agent-list、connection-indicator
  2. [H] 打开浏览器访问 `http://localhost:3001/web/?token=test`，按 F12 打开 DevTools → Elements 面板
     → 观察左侧是否有 220px 宽度的 `#sidebar`（或 `aside#sidebar`）元素，侧栏内是否包含 `#agent-list` 和 `#connection-indicator`
     → 是 / 否
  3. [H] 查看右侧主区域是否有 `#layout-toolbar`（右上角有 1/2/3 分屏按钮）和 `#pane-container`（分屏容器）
     → 是 / 否
- **异常排查:**
  - 如果侧栏不存在: 检查 index.html 的 `<aside id="sidebar">` 是否存在
  - 如果布局错乱: 检查 style.css 中 `#app` 的 `display: flex` 和 `#sidebar` 的 `width: 220px`

#### - [x] 2.2 Claude 风格深色主题视觉（body #0d0d0d + 侧栏 #222 人工确认）
- **来源:** spec-design.md
- **操作步骤:**
  1. [H] 在浏览器中（`http://localhost:3001/web/?token=test`）右键页面 → 检查 → 查看 Computed Styles
     → 找到 `body` 元素的 `background-color`，是否为深色（接近 `#0d0d0d`）
     → 是 / 否
  2. [H] 查看左侧边栏背景色是否为深灰（`#222` 或 `#1a1a1a`）
     → 是 / 否
- **异常排查:**
  - 如果颜色不对: 检查 style.css 的 `:root` 变量和 `body` / `#sidebar` 的 `background` 属性

---

### 场景 3：消息渲染与信息流

#### - [x] 3.1 AI 消息 Markdown 渲染（修复 `.messages` 类选择器后通过）
- **来源:** Task 5 spec-plan.md / spec-design.md
- **操作步骤:**
  1. [H] 启动 TUI + Relay，向 Agent 发送一条需要 Markdown 回复的问题（如 "用 Markdown 列出 3 个 Rust 特性，含标题和列表"）
     → 观察 Web 页面中 AI 回复是否包含：
     - Markdown 标题（`<h1>` / `<h2>` 样式，字号明显不同）
     - 列表（`<ul>` / `<ol>` 样式，带圆点或数字）
     → 是 / 否
  2. [H] 在 AI 消息区域右键 → 检查，查看元素是否为 `<div class="md-content">` 包裹的 Markdown 内容
     → 是 / 否
- **异常排查:**
  - 如果 Markdown 未渲染: 检查 `render.js` 的 `marked.parse()` 调用是否在 `window.marked` 加载后执行
  - 如果报错: 打开 Console 面板，查找 `marked is not defined` 或 `DOMPurify is not defined` 错误

#### - [x] 3.2 代码块语法高亮（随 3.1 一起验证通过）
- **来源:** Task 5 spec-plan.md / spec-design.md
- **操作步骤:**
  1. [H] 向 Agent 发送 "用代码块展示一个简单的 Rust fn 示例"
     → 观察 AI 回复中的代码块：
     - 是否显示语法高亮（关键词变色，不是纯白字）
     - 是否有语言标签（如 "rust"）显示在右上角
     → 是 / 否
  2. [H] 右键代码块 → 检查，`<code>` 元素的父元素是否包含 `hljs` 类名
     → 是 / 否
- **异常排查:**
  - 如果无高亮: 检查 `render.js` 的 `initMarked()` 中 `hljs.highlight()` 调用是否正确

#### - [x] 3.3 工具调用卡片展示（INPUT/OUTPUT 分区，橙色工具名确认）
- **来源:** Task 5 spec-plan.md / spec-design.md
- **操作步骤:**
  1. [H] 在 TUI 中执行一个需要 HITL 审批的工具（如 `bash ls`），审批后查看 Web 页面
     → 观察工具调用是否以卡片形式展示：
     - 工具名是否用橙色（`#e8975e`）+ 加粗显示
     - 是否有 "INPUT" 和 "OUTPUT" 标签分区
     → 是 / 否
  2. [H] 点击工具卡片头部，查看是否切换折叠/展开状态
     → 是 / 否
  3. [H] 如果工具输出超过 20 行，查看是否出现 "▶ 展开全部" 按钮
     → 是 / 否
- **异常排查:**
  - 如果格式不对: 检查 `render.js` 中 `renderSingleMessage` 的 `tool` 分支
  - 如果折叠无效: 检查 `.tool-header` 的 `addEventListener` 点击事件

#### - [x] 3.4 Streaming 闪烁光标动画（｜光标动画正常，Done 后消失）
- **来源:** Task 5 spec-plan.md / spec-design.md
- **操作步骤:**
  1. [H] 向 Agent 发送一个需要较长回答的问题，在回答过程中观察
     → 观察 AI 消息末尾是否出现闪烁的 `｜` 光标（CSS animation）
     → 是 / 否
  2. [H] 等待 Agent 回答完成后（收到 `done` 事件），查看光标是否消失
     → 是 / 否
- **异常排查:**
  - 如果无光标: 检查 `render.js` 的 `msg.streaming` 分支是否正确添加 `<span class="cursor-blink">｜</span>`
  - 如果光标不消失: 检查 `handleLegacyEvent` 中 `done` 事件是否正确设置 `lastMsg.streaming = false`

#### - [x] 3.5 向后兼容双格式（无 JS 错误，双格式正常解析）
- **来源:** Task 5 spec-plan.md / spec-design.md
- **操作步骤:**
  1. [H] 确认 TUI 连接 Relay 后，向 Agent 发送消息
     → 查看 DevTools → Console 是否无错误
     → 是 / 否
  2. [H] 检查 `events.js` 中是否存在 `handleLegacyEvent` 和 `handleBaseMessage` 两个函数
     → 是 / 否
- **异常排查:**
  - 如果消息无法显示: 检查 DevTools Console 的 JS 错误

#### - [x] 3.6 静态文件加载（rust-embed）（main.js/state.js 均 HTTP 200，ES Module 内容正确）
- **来源:** Task 5 spec-plan.md
- **操作步骤:**
  1. [A] `curl -s -o /dev/null -w "%{http_code}" http://localhost:3001/web/js/main.js`
     → 期望: HTTP 200
  2. [A] `curl -s -o /dev/null -w "%{http_code}" http://localhost:3001/web/js/state.js`
     → 期望: HTTP 200
  3. [A] `curl -s http://localhost:3001/web/js/main.js | head -5`
     → 期望: 输出包含 `// main.js` 或 `DOMContentLoaded`
- **异常排查:**
  - 如果 404: 确认 `rust-embed` 的 `#[folder = "web/"]` 已正确嵌入所有子目录
  - 检查 `main.rs` 的路由注册，确认 `/web/{*path}` 路由存在

---

### 场景 4：分屏布局与弹窗

#### - [x] 4.1 分屏切换 1/2/3 栏（1/2/3 切换全部正常）
- **来源:** Task 5 spec-plan.md / spec-design.md
- **操作步骤:**
  1. [H] 在浏览器中打开 `http://localhost:3001/web/?token=test`
     → 点击右上角数字按钮 `2`，查看右侧是否变为左右两等分布局（中间有分隔线）
     → 是 / 否
  2. [H] 切换到 `3`，查看是否为三等分布局
     → 是 / 否
  3. [H] 切换回 `1`，查看是否恢复单栏布局
     → 是 / 否
- **异常排查:**
  - 如果分屏无效: 检查 DevTools Console 是否报错；检查 `layout.js` 的 `setCols()` 和 `renderLayout()` 函数

#### - [x] 4.2 HITL / AskUser 弹窗样式一致性（背景 #222，批准绿/拒绝红，overlay 点击关闭确认）
- **来源:** Task 5 spec-plan.md / spec-design.md
- **操作步骤:**
  1. [H] 在 TUI 中触发一个 HITL 审批（如执行 `bash` 命令），查看 Web 页面
     → 观察 HITL 弹窗：
     - 弹窗背景色是否为深灰（`#222`）
     - "全部批准" 按钮是否为绿色（`#4caf50`）
     - "全部拒绝" 按钮是否为红色（`#f44336`）
     → 是 / 否
  2. [H] 点击弹窗外区域，查看弹窗是否关闭
     → 是 / 否
  3. [H] 如果 Agent 发送了 AskUser 问题，查看 AskUser 弹窗是否正确渲染（问题文本 + 选项或输入框）
     → 是 / 否
- **异常排查:**
  - 如果弹窗样式不对: 检查 `dialog.js` 的 `showHitlDialog` / `showAskUserDialog` 和 `style.css` 的 `.modal-*` 类

#### - [x] 4.3 移动端响应式（侧栏/分屏按钮隐藏，主内容单栏确认）
- **来源:** Task 5 spec-plan.md / spec-design.md
- **操作步骤:**
  1. [H] 在 Chrome DevTools 中，点击设备工具栏图标（或按 `Ctrl+Shift+M`），选择移动端设备（如 iPhone 12），刷新页面
     → 观察：
     - 左侧边栏（`#sidebar`）是否隐藏（不再显示）
     - 右上角分屏按钮（`#layout-toolbar`）是否隐藏
     → 是 / 否
  2. [H] 确认主内容区是否正确显示为单栏（宽度 100%）
     → 是 / 否
- **异常排查:**
  - 如果侧栏未隐藏: 检查 style.css 的 `@media (max-width: 768px)` 规则中 `#sidebar` 的 `display: none`

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | Rust 编译通过 | 3 | 0 | ⬜ | |
| 场景 1 | 1.2 | CDN 资源引入完整 | 2 | 0 | ⬜ | |
| 场景 1 | 1.3 | CSS 变量体系完整 | 3 | 0 | ⬜ | |
| 场景 1 | 1.4 | ES Module 模块链路 | 3 | 0 | ⬜ | |
| 场景 2 | 2.1 | 页面加载 / 布局结构 | 1 | 2 | ⬜ | |
| 场景 2 | 2.2 | Claude 风格深色主题视觉 | 0 | 2 | ⬜ | |
| 场景 3 | 3.1 | AI 消息 Markdown 渲染 | 0 | 2 | ⬜ | |
| 场景 3 | 3.2 | 代码块语法高亮 | 0 | 2 | ⬜ | |
| 场景 3 | 3.3 | 工具调用卡片展示 | 0 | 3 | ⬜ | |
| 场景 3 | 3.4 | Streaming 闪烁光标动画 | 0 | 2 | ⬜ | |
| 场景 3 | 3.5 | 向后兼容双格式 | 0 | 2 | ⬜ | |
| 场景 3 | 3.6 | 静态文件加载（rust-embed） | 3 | 0 | ⬜ | |
| 场景 4 | 4.1 | 分屏切换 1/2/3 栏 | 0 | 3 | ⬜ | |
| 场景 4 | 4.2 | HITL / AskUser 弹窗样式一致性 | 0 | 3 | ⬜ | |
| 场景 4 | 4.3 | 移动端响应式 | 0 | 2 | ⬜ | |

**验收结论:** ✓ 全部通过

> **修复记录:** 场景 3 验收中发现 `.messages` 使用了 ID 选择器 `#messages` 而 JS 创建的是类选择器，导致消息区域无法撑开。已修复为 `.messages` 类选择器，刷新后验证通过。
