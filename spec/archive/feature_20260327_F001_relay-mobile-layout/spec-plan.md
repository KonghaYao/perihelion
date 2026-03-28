# relay-mobile-layout 执行计划

**目标:** 为 relay server Web 前端适配移动端布局（抽屉侧边栏 + 面板 Tab 切换）

**技术栈:** 纯 HTML / CSS / ES Modules（无构建工具），Tailwind CSS CDN

**设计文档:** [spec-design.md](./spec-design.md)

---

### Task 1: HTML 结构更新

**涉及文件:**
- 修改: `rust-relay-server/web/index.html`

**执行步骤:**
- [x] 在 `<main id="main-content">` 内部最顶部插入移动端顶部导航栏 `#mobile-topbar`
  - 包含：`#hamburger-btn`（汉堡按钮 ☰）、`#mobile-title`（标题）、`#mobile-agent-name`（当前 Agent 名）
- [x] 在 `#layout-toolbar` 之后插入移动端面板 Tab 栏 `#mobile-tabs`
  - 初始为空，由 JS 动态填充 Tab 标签
- [x] 在 `#app` 内（`</div>` 结束前）插入移动端遮罩层 `#mobile-overlay`
  - 初始无 `visible` class，通过 JS 控制显示
- [x] 删除 `style.css` 中已有但 HTML 未实现的 `#mobile-agent-selector` 相关样式（后续 Task 2 清理）

**检查步骤:**
- [x] 验证 HTML 中存在 `#mobile-topbar` 元素
  - `grep -c 'id="mobile-topbar"' rust-relay-server/web/index.html`
  - 预期: 输出 `1`
- [x] 验证 HTML 中存在 `#mobile-tabs` 元素
  - `grep -c 'id="mobile-tabs"' rust-relay-server/web/index.html`
  - 预期: 输出 `1`
- [x] 验证 HTML 中存在 `#mobile-overlay` 元素
  - `grep -c 'id="mobile-overlay"' rust-relay-server/web/index.html`
  - 预期: 输出 `1`
- [x] 验证 HTML 中存在 `#hamburger-btn` 按钮
  - `grep -c 'id="hamburger-btn"' rust-relay-server/web/index.html`
  - 预期: 输出 `1`

---

### Task 2: CSS 移动端适配

**涉及文件:**
- 修改: `rust-relay-server/web/style.css`

**执行步骤:**
- [x] 清理旧的 `#mobile-agent-selector` 样式块（已废弃，用新结构替代）
- [x] 将 `body` 高度从 `100vh` 改为 `100dvh`，`#app` 同步更新，适配移动端虚拟键盘
  - `dvh`（dynamic viewport height）在 iOS Safari 15.4+ 支持，感知虚拟键盘弹起
- [x] 在媒体查询外添加 `#mobile-topbar` 基础样式（默认隐藏，高度 48px，flex 布局）
  - `#hamburger-btn` 最小点击区域 44×44px（iOS HIG 标准）
- [x] 在媒体查询外添加 `#mobile-overlay` 基础样式（默认 `display:none`，fixed 定位，z-index 199）
  - `.visible` class：`display:block`
- [x] 在媒体查询外添加 `#mobile-tabs` 基础样式（默认 `display:none`，overflow-x auto）
  - `.has-tabs` class：`display:flex`
  - `.mobile-tab` 最小高度 44px，`.mobile-tab.active` 底部橙色边框
- [x] 更新 `@media (max-width: 768px)` 块：
  - `#sidebar`：改用 `transform: translateX(-100%)` + `transition` 隐藏（不用 `display:none`），`position:fixed`，`z-index:200`
  - `#sidebar.mobile-visible`：`transform: translateX(0)`
  - `#layout-toolbar`：保持 `display:none`
  - `#mobile-topbar`：`display:flex`
  - `.pane-input input`：`font-size: 16px`（防止 iOS Safari 自动缩放）
- [x] 添加 `@media (min-width: 769px)` 块强制隐藏移动端元素：
  - `#mobile-topbar, #mobile-tabs, #mobile-overlay { display: none !important; }`
- [x] `.modal-card` 新增 `max-height: 85dvh`（避免虚拟键盘遮挡提交按钮）
- [x] `.messages` 新增 `-webkit-overflow-scrolling: touch`（iOS 滚动流畅度）

**检查步骤:**
- [x] 验证 `dvh` 已替换 `vh`（body/app 高度）
  - `grep -c '100dvh' rust-relay-server/web/style.css`
  - 预期: 输出 `2`（body 和 #app 各一处）
- [x] 验证移动端媒体查询存在侧边栏 transform 规则
  - `grep -A5 'max-width: 768px' rust-relay-server/web/style.css | grep -c 'translateX'`
  - 预期: 输出 `1` 或更多
- [x] 验证桌面端强制隐藏规则存在
  - `grep -c 'min-width: 769px' rust-relay-server/web/style.css`
  - 预期: 输出 `1`
- [x] 验证 `mobile-agent-selector` 旧样式已删除
  - `grep -c 'mobile-agent-selector' rust-relay-server/web/style.css`
  - 预期: 输出 `0`
- [x] 验证输入框移动端字体大小设置
  - `grep -c 'font-size: 16px' rust-relay-server/web/style.css`
  - 预期: 输出 `1`

---

### Task 3: JS 状态与交互逻辑

**涉及文件:**
- 修改: `rust-relay-server/web/js/state.js`
- 修改: `rust-relay-server/web/js/layout.js`
- 修改: `rust-relay-server/web/js/main.js`

**执行步骤:**
- [x] **state.js**：在 `layout` 对象中添加 `activeMobilePane: 0` 字段
  - 用于记录移动端当前激活的面板序号
- [x] **layout.js**：新增 `isMobile()` 辅助函数
  - `return window.matchMedia('(max-width: 768px)').matches`
- [x] **layout.js**：新增 `closeMobileSidebar()` 函数
  - 移除 `#sidebar.mobile-visible` class，移除 `#mobile-overlay.visible` class
- [x] **layout.js**：新增 `initMobile()` 函数（export）
  - 绑定 `#hamburger-btn` click → 添加 `mobile-visible` + `visible` class
  - 绑定 `#mobile-overlay` click → 调用 `closeMobileSidebar()`
- [x] **layout.js**：新增 `renderMobileTabs()` 函数（export）
  - `isMobile()` 守卫，非移动端直接 return
  - 统计 `state.layout.panes` 中非 null 的 sessionId
  - 若数量 ≤ 1：`#mobile-tabs` 移除 `has-tabs` class，return
  - 若数量 > 1：动态生成 Tab 按钮（显示 Agent 名），激活态对应 `activeMobilePane`
  - Tab 点击：更新 `state.layout.activeMobilePane` → 触发 `renderLayout()`
- [x] **layout.js**：更新 `renderLayout()` export 函数，在 `doRender()` 后调用 `renderMobileTabs()`
- [x] **layout.js**：更新 `setCols()` 函数，调用 `renderMobileTabs()` 保持同步
- [x] **main.js**：在 `init()` 中调用 `initMobile()`（import 来自 `./layout.js`）

**检查步骤:**
- [x] 验证 `state.js` 中 `activeMobilePane` 字段存在
  - `grep -c 'activeMobilePane' rust-relay-server/web/js/state.js`
  - 预期: 输出 `1`
- [x] 验证 `layout.js` 导出 `initMobile` 函数
  - `grep -c 'export function initMobile' rust-relay-server/web/js/layout.js`
  - 预期: 输出 `1`
- [x] 验证 `layout.js` 导出 `renderMobileTabs` 函数
  - `grep -c 'export function renderMobileTabs' rust-relay-server/web/js/layout.js`
  - 预期: 输出 `1`
- [x] 验证 `main.js` 调用了 `initMobile`
  - `grep -c 'initMobile' rust-relay-server/web/js/main.js`
  - 预期: 输出 `1`
- [x] 验证 `isMobile` 使用 `matchMedia` 实现
  - `grep -c 'matchMedia' rust-relay-server/web/js/layout.js`
  - 预期: 输出 `1`

---

### Task 4: JS 渲染层适配

**涉及文件:**
- 修改: `rust-relay-server/web/js/render.js`
- 修改: `rust-relay-server/web/js/events.js`（侧边栏点击联动）

**执行步骤:**
- [x] **render.js**：更新 `renderLayout()` 函数，移动端时仅渲染激活面板
  - 导入 `isMobile` 和 `closeMobileSidebar` from `./layout.js`
  - 在遍历 `cols` 之前：若 `isMobile()` 为 true，只渲染 `state.layout.activeMobilePane` 对应的面板（`cols` 设为 1，paneId 固定为 0，但内容来自 `panes[activeMobilePane]`）
  - 其余面板 `pane-${i}` 设置 `display:none`（通过不渲染或隐藏实现）
- [x] **render.js**：更新 `renderSidebar()` 函数
  - Agent 点击事件中，调用 `closeMobileSidebar()`（import from `./layout.js`）
  - 点击后更新 `#mobile-agent-name` 文本为当前 Agent 名（`document.getElementById('mobile-agent-name')`）
  - 更新 `renderMobileTabs()` 调用（保持 Tab 状态同步）
- [x] **render.js**：更新 `renderPane()` 中 Agent 分配成功后同步 `#mobile-agent-name`
  - 在 `renderPane()` 开头：若 sessionId 存在，更新 `#mobile-agent-name` 为 agent.name
- [x] **events.js**：更新 `autoAssignPane()` 函数，自动分配后同步调用 `renderMobileTabs()`
  - `import('./layout.js').then(({ renderMobileTabs }) => renderMobileTabs())`
- [x] **render.js**：`renderLayout()` 桌面端路径不受影响（`isMobile()` 为 false 时走原逻辑）

**检查步骤:**
- [x] 验证 `render.js` 中导入了 `isMobile`
  - `grep -c 'isMobile' rust-relay-server/web/js/render.js`
  - 预期: 输出 `1` 或更多
- [x] 验证 `render.js` 中 `renderSidebar` 包含 `closeMobileSidebar` 调用
  - `grep -c 'closeMobileSidebar' rust-relay-server/web/js/render.js`
  - 预期: 输出 `1` 或更多
- [x] 验证 `render.js` 中存在 `mobile-agent-name` 的更新逻辑
  - `grep -c 'mobile-agent-name' rust-relay-server/web/js/render.js`
  - 预期: 输出 `1` 或更多
- [x] 验证 `events.js` 中 `renderMobileTabs` 被调用
  - `grep -c 'renderMobileTabs' rust-relay-server/web/js/events.js`
  - 预期: 输出 `1` 或更多
- [x] 验证 `layout.js` 导出了 `closeMobileSidebar`
  - `grep -c 'export function closeMobileSidebar' rust-relay-server/web/js/layout.js`
  - 预期: 输出 `1`

---

### Task 5: Relay Mobile Layout 验收

**前置条件:**
- 启动命令: `cargo run -p rust-relay-server --features server`（默认监听 8080 端口）
- 浏览器打开: `http://localhost:8080/web/?token=<your-token>`
- 验证文件变更已重编译: `cargo build -p rust-relay-server --features server 2>&1 | tail -3`

**端到端验证:**

1. [x] 桌面端布局不受影响
   - `grep -c 'mobile-topbar' rust-relay-server/web/index.html && grep -c 'min-width: 769px' rust-relay-server/web/style.css`
   - 预期: 均输出 `1`（新增元素存在，且有桌面端强制隐藏规则）
   - 失败排查: 检查 Task 1（HTML 结构）和 Task 2（CSS 桌面端规则）

2. [x] 移动端媒体查询正确激活
   - `node -e "const fs=require('fs');const css=fs.readFileSync('rust-relay-server/web/style.css','utf8');console.log(css.includes('max-width: 768px') && css.includes('translateX(-100%)') ? 'PASS' : 'FAIL')"`
   - 预期: 输出 `PASS`
   - 失败排查: 检查 Task 2（CSS 抽屉动画规则）

3. [x] JS 核心函数完整性验证
   - `node /tmp/check_fns.mjs`
   - 预期: 输出 `PASS`
   - 失败排查: 检查 Task 3（JS 函数实现）

4. [x] state.js activeMobilePane 字段完整性
   - `grep -c 'activeMobilePane' rust-relay-server/web/js/state.js`
   - 预期: 输出 `1`
   - 失败排查: 检查 Task 3（state.js 字段添加）

5. [x] 渲染层联动完整性
   - `node /tmp/check_render.mjs`
   - 预期: 输出 `PASS`
   - 失败排查: 检查 Task 4（render.js 和 events.js 联动）
