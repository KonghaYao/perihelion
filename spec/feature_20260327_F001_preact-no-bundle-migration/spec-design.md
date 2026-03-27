# Feature: 20260327_F001 - preact-no-bundle-migration

## 需求背景

`rust-relay-server/web/` 目前是纯原生 ES Modules 前端，渲染层全部依赖手动 DOM 操作（`createElement`、`innerHTML`、`appendChild`）。随着功能增加，`render.js`（450+ 行）、`layout.js`、`dialog.js` 的命令式代码越来越难以维护：

- 新增 UI 功能需要手写大量 DOM 操作，容易引入 bug
- 分屏面板、弹窗、侧边栏等 UI 单元无法复用，相似代码大量重复
- 状态变化时必须手动找到正确 DOM 节点更新，容易遗漏
- 添加新组件（如 Reasoning 展示、流式 Markdown 渲染）需要重写大段 DOM 逻辑

目标是迁移到 **Preact + Signals + htm**，以声明式 UI 替换命令式 DOM 操作，同时保持"无打包工具、纯 JS ES Modules"的部署约束（前端文件由 `rust-embed` 内嵌到 Relay Server 二进制中）。

## 目标

- 将 `web/` 目录下所有 JS 渲染逻辑迁移到 Preact 组件体系
- 使用 Preact Signals 替代手动 Map 状态，实现最小粒度自动重渲染
- 使用 htm 标签模板语法（无需 JSX 编译器）
- 保持所有依赖通过 CDN 加载，不引入任何构建工具（vite/webpack/rollup）
- 保留所有现有功能：分屏布局、侧边栏、HITL/AskUser 弹窗、TODO 面板、Markdown 渲染、移动端响应式

## 方案设计

### 技术选型

| 依赖 | 来源 | 用途 |
|------|------|------|
| `preact` | `https://esm.sh/preact` | 核心 VDOM 渲染 |
| `htm` | `https://esm.sh/htm` | 类 JSX 标签模板（无需编译） |
| `@preact/signals` | `https://esm.sh/@preact/signals` | 响应式状态管理 |
| `marked.js` | cdnjs CDN（UMD script） | Markdown 解析 |
| `highlight.js` | cdnjs CDN（UMD script） | 代码高亮 |
| `DOMPurify` | cdnjs CDN（UMD script） | XSS 净化 |

> marked/hljs/DOMPurify 不提供 ES Module 格式，通过动态 `loadScript()` 注入 `<script>` 标签加载为全局变量，加载完成后 `markedReady` signal 置为 `true`，触发消息组件重渲染。

### 文件结构

```
rust-relay-server/web/
├── index.html              # 最小骨架：<div id="app"> + CDN 样式链接
├── style.css               # 保留现有样式（无需修改）
├── app.js                  # Preact render 入口：加载 CDN 脚本 → mount <App />
├── state.js                # 全局 Signals 定义（agents、layout、activePane 等）
├── connection.js           # WebSocket 连接逻辑（直接操作 signals）
├── events.js               # 服务端消息处理（直接操作 signals）
└── components/
    ├── App.js              # 根组件：整体布局 + 初始化连接
    ├── Sidebar.js          # 左侧 Agent 列表（读 agents signal）
    ├── PaneContainer.js    # 分屏容器（1/2/3 列，读 layout signal）
    ├── Pane.js             # 单面板（TODO + 消息列表 + 输入栏）
    ├── MessageList.js      # 消息渲染（Markdown + 代码高亮 + 工具折叠卡片）
    ├── TodoPanel.js        # TODO 状态面板
    ├── HitlDialog.js       # 工具审批弹窗
    └── AskUserDialog.js    # 用户问答弹窗
```

![前端组件架构图](./images/01-architecture.png)

### Signals 状态设计

```js
// state.js
import { signal, computed } from 'https://esm.sh/@preact/signals'

export const agents = signal(new Map())
// sessionId → {
//   name, status,
//   messages: [{ type, text, name, input, output, isError, streaming }],
//   todos: [{ status, title }],
//   ws, pendingHitl, pendingAskUser, maxSeq, isRunning
// }

export const layout = signal({ cols: 1, panes: [null, null, null] })
export const activePane = signal(0)
export const activeMobilePane = signal(0)
export const markedReady = signal(false)   // marked/hljs/DOMPurify 加载完毕标记

// 派生计算
export const activePaneSessionId = computed(
  () => layout.value.panes[activePane.value] ?? null
)
```

**状态变更方式**：`connection.js` / `events.js` 直接 mutate signals（`agents.value = new Map(agents.value)`），Preact 自动检测依赖并触发最小范围重渲染。无需手动调用任何 render 函数。

![状态流与组件依赖图](./images/02-flow.png)

### index.html 骨架

```html
<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Agent Remote Control</title>
  <link rel="stylesheet"
    href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github.min.css">
  <link rel="stylesheet" href="style.css">
</head>
<body>
  <div id="app"></div>
  <script type="module" src="app.js"></script>
</body>
</html>
```

`index.html` 不再包含任何 HTML 结构，所有 DOM 由 Preact 运行时生成。HITL/AskUser 弹窗移入 `HitlDialog.js` / `AskUserDialog.js` 组件，不再写在 HTML 里。

### app.js 入口

```js
import { h, render } from 'https://esm.sh/preact'
import htm from 'https://esm.sh/htm'
import { App } from './components/App.js'
import { markedReady } from './state.js'

const html = htm.bind(h)

function loadScript(src) {
  return new Promise((res, rej) => {
    const s = document.createElement('script')
    s.src = src
    s.onload = res
    s.onerror = rej
    document.head.appendChild(s)
  })
}

// 并行加载 UMD 脚本（不影响 Preact 初始化）
Promise.allSettled([
  loadScript('https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js'),
  loadScript('https://cdn.jsdelivr.net/npm/marked@15/marked.min.js'),
  loadScript('https://cdnjs.cloudflare.com/ajax/libs/dompurify/3.0.6/purify.min.js'),
]).then(() => {
  markedReady.value = true   // 触发依赖该 signal 的 MessageList 重渲染
})

// 立即 mount，不等待 CDN（文本消息先显示纯文本，CDN 就绪后自动升级为 Markdown）
render(html`<${App} />`, document.getElementById('app'))
```

### 关键组件交互

```
App
├── useEffect → checkToken() → connectManagement()
├── 读 layout signal → 决定移动端/桌面端布局
├── <Sidebar />
├── <PaneContainer />
├── <HitlDialog />      （全局唯一，读 agents signal 查找 pendingHitl）
└── <AskUserDialog />   （全局唯一，读 agents signal 查找 pendingAskUser）

Pane（paneId prop）
├── 从 layout.panes[paneId] 读取 sessionId
├── 从 agents.get(sessionId) 读取数据
├── <TodoPanel todos={agent.todos} />
├── <MessageList messages={agent.messages} />
└── 输入栏（发送 user_input / clear_thread / compact_thread）

MessageList
├── 读 markedReady signal（CDN 就绪后升级渲染质量）
└── 渲染 user / assistant（Markdown） / tool（折叠卡片） / error 气泡
```

### htm 写法示例

```js
// components/Sidebar.js
import { h } from 'https://esm.sh/preact'
import htm from 'https://esm.sh/htm'
import { agents, activePane } from '../state.js'
import { assignAgentToPane } from '../connection.js'

const html = htm.bind(h)

export function Sidebar() {
  return html`
    <aside id="sidebar">
      <div class="sidebar-header">
        <div class="text-sm font-bold" style="color: var(--accent)">在线 Agent</div>
      </div>
      <div id="agent-list" class="flex-1 overflow-y-auto">
        ${[...agents.value].map(([sid, agent]) => html`
          <div
            key=${sid}
            class="agent-item"
            onClick=${() => assignAgentToPane(activePane.value, sid)}
          >
            <span class=${'dot ' + (agent.status === 'online' ? 'dot-online' : 'dot-offline')} />
            <span class="agent-name">${agent.name}</span>
            ${(agent.pendingHitl || agent.pendingAskUser)
              ? html`<span class="badge">🔔</span>` : null}
          </div>
        `)}
      </div>
    </aside>
  `
}
```

### 旧文件处置

| 旧文件 | 处置 |
|--------|------|
| `web/app.js` | 删除（已被新 app.js 替代） |
| `web/js/main.js` | 删除（逻辑合并到 App.js + app.js） |
| `web/js/render.js` | 删除（拆分到各 Component） |
| `web/js/layout.js` | 删除（逻辑移到 PaneContainer.js + App.js） |
| `web/js/dialog.js` | 删除（迁移到 HitlDialog.js + AskUserDialog.js） |
| `web/js/state.js` | 迁移为 Signals 版 state.js（根目录） |
| `web/js/connection.js` | 迁移到根目录 connection.js（操作 signals） |
| `web/js/events.js` | 迁移到根目录 events.js（操作 signals） |

## 实现要点

1. **htm 共享实例**：每个文件都需要 `import htm` 并 `htm.bind(h)`。可抽出 `utils/html.js` 导出 `html` 减少重复。
2. **Signal Map 更新**：Map 是引用类型，修改后必须 `agents.value = new Map(agents.value)` 替换引用才能触发 Signals 依赖追踪。
3. **UMD 脚本时序**：marked/hljs/DOMPurify 通过动态 script 加载，MessageList 需要 `markedReady.value` 来判断是否可以调用 `window.marked`，避免 undefined 错误。
4. **Preact Keys**：列表渲染必须提供 `key` 属性（sessionId 作为 key），防止分屏面板切换时 DOM 复用错误。
5. **弹窗全局唯一**：HitlDialog/AskUserDialog 挂载在 App 根层，通过 signals 感知当前活跃会话的 pending 状态，不重复 mount。
6. **style.css 不动**：现有 CSS 变量和类名保持不变，组件的 `class` 属性沿用旧命名，零样式改动。

## 约束一致性

- **无构建工具约束**（constraints.md）：所有依赖通过 CDN ES Module 加载，无 npm install / vite / webpack，符合"纯静态文件，rust-embed 内嵌"的部署要求。
- **前端 CDN 约束**（architecture.md `Web 前端 CDN（relay-server）`）：现有约束列出了 Tailwind/marked/hljs/DOMPurify，本方案新增 preact + htm + @preact/signals（均为 esm.sh CDN）。需同步更新 architecture.md 的 CDN 列表。
- **安全约束**：保留 DOMPurify 净化 Markdown HTML 输出，XSS 防护不变。

## 验收标准

- [ ] 前端页面正常加载，无 JS 错误
- [ ] 单栏 / 双栏 / 三栏分屏布局切换正常
- [ ] 侧边栏显示在线 Agent，点击可绑定到面板
- [ ] WebSocket 连接正常（管理端 + 会话端），消息实时显示
- [ ] Markdown 消息正确渲染（含代码高亮）
- [ ] HITL 弹窗正常弹出、审批/拒绝后正确关闭
- [ ] AskUser 弹窗正常弹出，单选/多选/文本输入均可提交
- [ ] TODO 面板折叠/展开正常
- [ ] 移动端响应式布局正常（单面板 + 顶部导航栏）
- [ ] 消息中工具卡片折叠/展开、超长输出折叠正常
- [ ] `/clear` 和 `/compact` 命令正常触发
- [ ] Agent 断线/重连后 Tab 状态正确更新
