# Feature: 20260324_F002 - relay-server-ui-redesign

## 需求背景

`rust-relay-server` 的 Web 前端（`web/` 目录）当前为简单深色主题 HTML + 单文件 Vanilla JS，存在以下问题：

1. **视觉设计粗糙**：无设计系统，元素间距不一致，字体层级不清晰，与现代 AI chat 产品（Claude.ai 等）有较大差距
2. **信息流展示不友好**：AI 回复无 Markdown 渲染（标题/列表/代码块全以纯文本展示），工具调用无结构化展示，无流式输出动画
3. **布局单一**：所有 Agent 通过顶栏 Tab 切换，无法同时查看多个 Agent，不支持多面板对比
4. **代码耦合**：`app.js` 近 600 行单文件，可读性差，维护困难

## 目标

- 引入 Tailwind CSS（CDN）重构样式，达到 Claude.ai 风格视觉品质
- 支持基础分屏：手动切换 1/2/3 栏，每栏独立展示一个 Agent
- AI 消息支持 Markdown 渲染 + 代码语法高亮
- 工具调用改为结构化卡片展示（输入/输出分区、默认展开）
- 将 `app.js` 拆分为 7 个 ES Module 模块，提升可维护性

## 方案设计

### 整体布局

采用左侧边栏 + 右侧分屏主区布局，类似 VS Code 侧边栏 + 编辑器的视觉结构。

![整体布局线框图](./images/01-wireframe.png)

**布局区域说明：**

| 区域 | 宽度 | 内容 |
|------|------|------|
| 左侧边栏 | 220px（固定） | Agent 列表（在线状态）+ 连接状态 |
| 主内容区 | 剩余宽度 | 1/2/3 分屏面板，切换按钮在右上角 |
| 每个分屏面板 | 均分 | TODO 折叠面板 + 消息区 + 输入栏 |

**移动端（< 768px）适配：**
- 左侧边栏自动隐藏，顶部显示 Agent 名称下拉选择器
- 强制单栏模式，分屏按钮隐藏

**分屏模式：**
- 默认单栏：全宽显示一个 Agent
- 双栏：左右各 50%，各自独立显示不同 Agent
- 三栏：三等分，独立显示三个 Agent
- 切换分屏数时，多余面板显示"选择 Agent"占位提示

### 颜色与设计语言

Claude 风格深色主题，通过 Tailwind CSS + 少量自定义 CSS 变量实现：

```css
:root {
  --bg-base:     #0d0d0d;   /* 页面底层背景 */
  --bg-surface:  #1a1a1a;   /* 消息区背景 */
  --bg-elevated: #222222;   /* 卡片、侧边栏背景 */
  --border:      #2e2e2e;   /* 分隔线颜色 */
  --text-primary:#f0ede8;   /* 主文字（暖白） */
  --text-muted:  #8c8c8c;   /* 次要文字 */
  --accent:      #e8975e;   /* 强调色（Claude 暖橙） */
  --user-bubble: #2d4a7a;   /* 用户气泡蓝 */
}
```

### 消息渲染设计

![消息渲染与工具调用卡片设计](./images/02-wireframe.png)

**用户消息：** 右侧气泡，`--user-bubble` 背景，最大宽 75% 宽度。

**AI 消息：** 左对齐，无背景色（直接在 `--bg-surface` 上），`marked.js` 解析 Markdown：
- 标题（h1-h3）：字号递减，`--text-primary` 颜色
- 代码行内：`bg-elevated` 背景 + 等宽字体
- 代码块：`highlight.js` GitHub Dark 主题，带语言标签 + 复制按钮
- 列表、粗体、链接：标准 Markdown 样式

**Streaming 状态：** 消息末尾显示闪烁光标 `｜`（CSS animation），同时侧边栏 Agent 名旁显示旋转加载图标。

**工具调用卡片：** 默认展开，可点击折叠。

```
┌── [🔧] bash ─────────────────────────────── [▼折叠] ┐
│ INPUT                                                │
│  ls -la /home/user                                  │
│ ────────────────────────────────────────────────── │
│ OUTPUT                                               │
│  total 48                                           │
│  drwxr-xr-x 12 user user 4096 Mar 24 10:00 .       │
└─────────────────────────────────────────────────────┘
```

- 工具名：`--accent` 颜色 + BOLD（对应 TUI 规范）
- INPUT 区：`--bg-base` 背景 + 等宽字体
- OUTPUT 区：默认展示全部内容，超过 20 行时出现"展开/收起"按钮
- 错误输出：红色背景 `#2d1111`，文字 `#f87171`

### 分屏管理

![分屏布局切换流程](./images/03-flow.png)

**状态管理：**

```javascript
// state.js 新增
export const layout = {
  cols: 1,          // 1/2/3
  panes: [null, null, null],  // 每栏绑定的 sessionId，null 表示未选择
};
```

**交互流程：**
1. 点击右上角 `⊞ 2` / `⊟ 3` 按钮切换栏数
2. 增加栏时，新栏显示 Agent 选择下拉（列出所有在线 Agent）
3. 减少栏时，最右侧栏移除（不影响其他栏）
4. 左侧边栏 Agent 条目可拖拽到目标栏（可选，Phase 2 实现）
5. 每栏右上角有"关闭/切换"按钮

### 文件模块结构

```
rust-relay-server/web/
├── index.html            # 加载 Tailwind CDN、marked.js、highlight.js、main.js
├── style.css             # 自定义样式（滚动条、动画、代码块主题覆盖）
└── js/
    ├── main.js           # 模块入口：初始化、DOMContentLoaded 事件
    ├── state.js          # 共享状态：agents Map、layout 对象、activeSessionId
    ├── connection.js     # WS 管理：connectManagement、connectSession、自动重连
    ├── events.js         # 事件处理：handleSingleEvent、handleBaseMessage、handleLegacyEvent
    ├── render.js         # 渲染：renderPane、renderMessages、renderSidebar、renderTodoPanel
    ├── layout.js         # 分屏：setCols、assignAgentToPane、renderLayout
    └── dialog.js         # 弹窗：showHitlDialog、showAskUserDialog、closeDialog
```

**ES Module 加载方式（index.html）：**

```html
<script type="module" src="js/main.js"></script>
```

模块间通过 `import/export` 共享 `state.js` 中的单例状态对象，无全局变量污染。

### CDN 依赖

```html
<!-- Tailwind CSS -->
<script src="https://cdn.tailwindcss.com"></script>
<!-- marked.js (Markdown 解析) -->
<script src="https://cdn.jsdelivr.net/npm/marked@15/marked.min.js"></script>
<!-- highlight.js (代码高亮) -->
<link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github-dark.min.css">
<script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js"></script>
```

所有 CDN 资源通过 `defer` / `type="module"` 按序加载，避免阻塞渲染。

## 实现要点

1. **`static_files.rs` 需更新**：`rust-embed` 内嵌路径需新增 `js/` 子目录下的 7 个文件
2. **ES Module + CDN 全局变量共存**：`marked`、`hljs` 为 CDN 全局变量，ES Module 内通过 `window.marked` / `window.hljs` 访问（或在 `main.js` 顶部声明 `const { marked, hljs } = window`）
3. **XSS 防护**：`marked.js` 渲染 AI 消息时需开启 `sanitize: true`，或使用 `DOMPurify` 净化 HTML（CDN 引入）；用户输入和工具输出继续用 `escHtml()`
4. **消息重新渲染性能**：`renderMessages()` 目前每次全量重建 DOM，在消息量大时（>200 条）可能卡顿；本期保持全量重建，Phase 2 可改为增量追加
5. **`static_files.rs` 路由**：`/web/js/main.js` 等路径需在 axum 路由中正确 serve（当前只注册了 `index.html`、`app.js`、`style.css`）
6. **向后兼容**：`events.js` 保留 `handleLegacyEvent` 函数，支持旧 `AgentEvent` 格式（type 字段）和新 BaseMessage 格式（role 字段）

## 约束一致性

- **技术栈约束**：Web 框架继续使用 axum 0.8（`rust-relay-server` crate），`rust-embed` 内嵌静态文件，无破坏性变更
- **无构建工具约束**：使用 ES Module + CDN，无需 webpack/vite 等构建步骤，符合"纯 HTML + Vanilla JS"原则
- **Workspace Crate 层次**：前端改动仅在 `rust-relay-server/web/` 目录，不影响其他 Crate
- **安全约束**：保持 Token 认证机制不变；新增 `marked.js` 渲染时对 AI 输出做 XSS 净化

## 验收标准

- [ ] 页面加载后呈现 Claude 风格深色主题，左侧 Agent 列表栏 + 右侧消息区布局
- [ ] AI 消息中的 Markdown（标题/列表/代码块）正确渲染
- [ ] 代码块显示语言标签 + 语法高亮（GitHub Dark 主题）
- [ ] 工具调用以结构化卡片展示（输入区 + 输出区分隔，可折叠）
- [ ] 点击分屏按钮可切换 1/2/3 栏，每栏独立绑定不同 Agent
- [ ] Streaming 中消息末尾出现闪烁光标，Done 后消失
- [ ] HITL / AskUser 弹窗样式与整体主题一致
- [ ] 移动端（< 768px）退化为单栏 + 顶部 Agent 选择器
- [ ] `static_files.rs` 正确 serve `js/` 子目录下所有 JS 模块
- [ ] 旧格式事件（type 字段）和新 BaseMessage 格式（role 字段）均能正确解析渲染
