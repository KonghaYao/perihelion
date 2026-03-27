# Feature: 20260327_F001 - relay-mobile-layout

## 需求背景

`rust-relay-server` 的 Web 前端（`web/` 目录下的纯 ES Modules 应用）目前仅针对桌面端布局设计，在移动设备上存在以下问题：

1. **侧边栏**：固定宽度 220px 的 `#sidebar` 占据大量屏幕空间，CSS 中虽有 `mobile-visible` class 定义，但 HTML 中缺少汉堡按钮，无法触发。
2. **分屏功能**：1/2/3 栏切换按钮在移动端被隐藏，但没有替代的面板导航机制，多面板时用户无法切换。
3. **`#mobile-agent-selector`**：CSS 中有样式定义，HTML 中完全缺失。
4. **输入框**：虚拟键盘弹起时可能遮挡输入区域（`100vh` 不感知键盘）。
5. **Modal 弹窗**：高度未适配移动端动态视口。

## 目标

- 在移动端（`max-width: 768px`）提供完整可用的 Agent 远程控制体验
- 侧边栏通过汉堡菜单 + 抽屉滑入展示 Agent 列表
- 多面板以标签页（Tab）形式切换，强制单面板显示
- 不破坏桌面端现有布局和功能
- 保持项目纯 ES Modules、无构建工具的技术约束

## 方案设计

### 总体布局对比

![移动端与桌面端布局对比](./images/01-layout.png)

**桌面端（≥769px）：** 保持不变。左侧 220px 固定侧边栏 + 右侧分屏内容区（1/2/3 栏工具栏）。

**移动端（≤768px）：**

```
┌──────────────────────────────────┐
│ ☰  Agent Remote Control  [Agent名] │  ← #mobile-topbar (新增)
├──────────────────────────────────┤
│ [Tab: my-laptop] [Tab: dev-srv]   │  ← #mobile-tabs (新增，多面板时显示)
├──────────────────────────────────┤
│                                  │
│     消息列表（单面板）            │
│                                  │
├──────────────────────────────────┤
│ [ 输入框 ···················] 发送 │
└──────────────────────────────────┘
```

抽屉侧边栏（点击 ☰ 后）：

```
┌──────────────┬────────────────────┐
│ 在线 Agent    │ ░░░░░░░░░░░░░░░░░░ │  ← 遮罩层可点击关闭
│ ● my-laptop  │ ░░░░░░░░░░░░░░░░░░ │
│ ○ dev-server │ ░░░░░░░░░░░░░░░░░░ │
│              │ ░░░░░░░░░░░░░░░░░░ │
│ ● 已连接     │ ░░░░░░░░░░░░░░░░░░ │
└──────────────┴────────────────────┘
```

### HTML 变更（index.html）

新增两个元素，插入在 `<main id="main-content">` 内部顶部：

```html
<!-- 移动端顶部导航栏（桌面端隐藏） -->
<div id="mobile-topbar">
  <button id="hamburger-btn" aria-label="打开 Agent 列表">☰</button>
  <span id="mobile-title">Agent Remote Control</span>
  <span id="mobile-agent-name"></span>
</div>

<!-- 移动端面板标签页（桌面端隐藏，多面板时显示） -->
<div id="mobile-tabs"></div>

<!-- 移动端遮罩层（侧边栏打开时出现） -->
<div id="mobile-overlay"></div>
```

删除 `#mobile-agent-selector` 相关 CSS（原有未实现的占位代码）。

### CSS 变更（style.css）

在已有的 `@media (max-width: 768px)` 块中：

**侧边栏抽屉：**
```css
#sidebar {
  transform: translateX(-100%);
  transition: transform 0.25s ease;
  display: flex; /* 始终保持 flex，通过 transform 控制可见性 */
  z-index: 200;
  position: fixed;
}
#sidebar.mobile-visible {
  transform: translateX(0);
}
```

**遮罩层：**
```css
#mobile-overlay {
  display: none;
  position: fixed;
  inset: 0;
  z-index: 199;
  background: rgba(0, 0, 0, 0.35);
}
#mobile-overlay.visible {
  display: block;
}
```

**顶部导航栏：**
```css
#mobile-topbar {
  display: flex;
  align-items: center;
  padding: 0 12px;
  height: 48px;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border);
  gap: 10px;
  flex-shrink: 0;
}
#hamburger-btn {
  min-width: 44px;
  min-height: 44px; /* iOS HIG 最小点击区域 */
  /* ... */
}
```

**面板 Tab 栏：**
```css
#mobile-tabs {
  display: none; /* 无多面板时隐藏 */
  overflow-x: auto;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
#mobile-tabs.has-tabs {
  display: flex;
}
.mobile-tab {
  min-height: 44px;
  padding: 0 16px;
  /* ... */
}
.mobile-tab.active {
  border-bottom: 2px solid var(--accent);
  color: var(--text-primary);
}
```

**虚拟键盘适配：**
```css
body {
  height: 100dvh; /* 动态视口高度，感知虚拟键盘 */
}
.messages {
  -webkit-overflow-scrolling: touch;
}
.modal-card {
  max-height: 85dvh;
}
```

**桌面端：**
```css
@media (min-width: 769px) {
  #mobile-topbar, #mobile-tabs, #mobile-overlay { display: none !important; }
}
```

### JS 变更（layout.js）

新增 `initMobile()` 函数，在 `initLayout()` 中调用：

```js
export function initMobile() {
  const hamburger = document.getElementById('hamburger-btn');
  const sidebar = document.getElementById('sidebar');
  const overlay = document.getElementById('mobile-overlay');

  hamburger?.addEventListener('click', () => {
    sidebar.classList.add('mobile-visible');
    overlay.classList.add('visible');
  });

  overlay?.addEventListener('click', () => closeMobileSidebar());
}

function closeMobileSidebar() {
  document.getElementById('sidebar')?.classList.remove('mobile-visible');
  document.getElementById('mobile-overlay')?.classList.remove('visible');
}
```

**移动端 Tab 渲染：**

```js
export function renderMobileTabs() {
  if (!isMobile()) return;
  const tabsEl = document.getElementById('mobile-tabs');
  const boundPanes = state.layout.panes.filter(Boolean);
  if (boundPanes.length <= 1) {
    tabsEl.classList.remove('has-tabs');
    return;
  }
  tabsEl.classList.add('has-tabs');
  // 渲染 tab 列表，点击时设置 state.layout.activeMobilePane
}

function isMobile() {
  return window.matchMedia('(max-width: 768px)').matches;
}
```

**面板渲染过滤（render.js 中）：**

在 `renderLayout()` 函数渲染面板时，移动端只渲染 `activeMobilePane` 对应的面板，其他面板 `display: none`。

**Agent 点击关闭侧边栏（events.js 中）：**

在 `addAgent()` / 侧边栏点击绑定面板后，调用 `closeMobileSidebar()`。

### state.js 变更

在 `layout` 对象增加 `activeMobilePane: 0` 字段：

```js
layout: {
  cols: 1,
  panes: [null],
  activeMobilePane: 0, // 移动端当前激活面板序号
}
```

## 实现要点

1. **双重可见性控制**：`#sidebar` 在移动端通过 `transform: translateX(-100%)` 隐藏（而非 `display:none`），保证动画过渡效果正常工作，同时通过 `position: fixed` 脱离文档流不占空间。

2. **`dvh` 兼容性**：`100dvh` 在 iOS Safari 15.4+ 支持。考虑到该页面是本地部署，目标浏览器较新，直接使用。若需兼容旧版可 fallback 到 `100vh`。

3. **Tab 与 state 同步**：`activeMobilePane` 序号始终在 `state.layout.panes` 范围内。若绑定面板减少（Agent 下线），`activeMobilePane` 自动 clamp 到 0。

4. **不侵入桌面端逻辑**：所有移动端专属函数通过 `isMobile()` 守卫，桌面端调用时直接 return，保证零副作用。

5. **静态文件重编译**：前端文件通过 `rust-embed` 打包进二进制，修改 `web/` 后需重新 `cargo build -p rust-relay-server --features server` 才能生效。

## 约束一致性

- **技术栈**：前端保持 Tailwind CSS CDN + 纯 ES Modules，无引入新依赖（满足 constraints.md 约束）。
- **Web 框架**：后端 axum 不受影响，纯前端改动。
- **rust-embed**：`web/` 目录文件变更后需重编译 relay-server binary，符合现有部署方式。

## 验收标准

- [ ] 移动端（≤768px）显示 `#mobile-topbar`，桌面端不显示
- [ ] 点击汉堡按钮，侧边栏从左滑入，并显示遮罩层
- [ ] 点击遮罩层，侧边栏收起
- [ ] 点击侧边栏中的 Agent，绑定到当前面板，侧边栏自动关闭
- [ ] 有多个绑定面板时，`#mobile-tabs` 显示对应 Tab 标签
- [ ] 点击 Tab 切换激活面板，内容区更新为对应 Agent 的消息
- [ ] 只有一个绑定面板时，Tab 栏隐藏
- [ ] 移动端弹出虚拟键盘时，输入框不被遮挡（`dvh` 适配）
- [ ] Modal 弹窗高度适配移动端（`85dvh`）
- [ ] 桌面端布局和功能完全不受影响
