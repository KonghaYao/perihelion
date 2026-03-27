# relay-mobile-layout 人工验收清单

**生成时间:** 2026-03-27 (本次执行)
**关联计划:** [spec-plan.md](./spec-plan.md)
**关联设计:** [spec-design.md](./spec-design.md)

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 检查 Node.js >= 18（用于自动化脚本）: `node -v`
- [ ] [AUTO] 编译 relay-server（含前端静态文件打包）: `cargo build -p rust-relay-server --features server 2>&1 | tail -5`
- [ ] [AUTO/SERVICE] 启动 relay-server（须提供 token 参数）: `cargo run -p rust-relay-server --features server -- --token test-token` (port: 8080)
- [ ] [AUTO] 确认服务已就绪: `curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/web/?token=test-token`

### 浏览器准备（移动端验证所需）

- [ ] [MANUAL] 打开浏览器 DevTools（F12），进入设备仿真模式（Device Toolbar），选择 iPhone 15 Pro 或类似移动设备（宽度 ≤ 768px）。访问 `http://localhost:8080/web/?token=test-token`。

### 测试 Agent 准备（场景 4/5 所需）

若要测试多面板 Tab 切换（场景 5），需要至少两个 Agent 同时在线。可以在桌面端先设置 2 栏并绑定两个 Agent，再切换到移动端视口。

---

## 验收项目

### 场景 1：代码结构完整性

#### - [x] 1.1 HTML 元素存在性
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -c 'id="mobile-topbar"' rust-relay-server/web/index.html` → 期望: 输出 `1`
  2. [A] `grep -c 'id="mobile-tabs"' rust-relay-server/web/index.html` → 期望: 输出 `1`
  3. [A] `grep -c 'id="mobile-overlay"' rust-relay-server/web/index.html` → 期望: 输出 `1`
  4. [A] `grep -c 'id="hamburger-btn"' rust-relay-server/web/index.html` → 期望: 输出 `1`
- **异常排查:**
  - 如果任意项输出 `0`：检查 `rust-relay-server/web/index.html` 文件，确认 Task 1 的 HTML 变更已保存。

#### - [x] 1.2 CSS 核心规则完整性
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -c '100dvh' rust-relay-server/web/style.css` → 期望: 输出 `3` 或更多（body/app/sidebar 均使用 dvh）
  2. [A] `grep -A5 'max-width: 768px' rust-relay-server/web/style.css | grep -c 'translateX'` → 期望: 输出 `1` 或更多
  3. [A] `grep -c 'min-width: 769px' rust-relay-server/web/style.css` → 期望: 输出 `1`
  4. [A] `grep -c 'mobile-agent-selector' rust-relay-server/web/style.css` → 期望: 输出 `0`（旧样式已清除）
  5. [A] `grep -c 'font-size: 16px' rust-relay-server/web/style.css` → 期望: 输出 `1`
- **异常排查:**
  - translateX 未找到：确认 `@media (max-width: 768px)` 块内 `#sidebar` 使用了 `transform: translateX(-100%)`
  - mobile-agent-selector 仍存在：Task 2 的清理步骤未完成，手动删除该样式块

#### - [x] 1.3 JS 函数与状态完整性
- **来源:** Task 3/4 检查步骤
- **操作步骤:**
  1. [A] `grep -c 'export function initMobile' rust-relay-server/web/js/layout.js` → 期望: 输出 `1`
  2. [A] `grep -c 'export function closeMobileSidebar' rust-relay-server/web/js/layout.js` → 期望: 输出 `1`
  3. [A] `grep -c 'export function renderMobileTabs' rust-relay-server/web/js/layout.js` → 期望: 输出 `1`
  4. [A] `grep -c 'matchMedia' rust-relay-server/web/js/layout.js` → 期望: 输出 `1`
  5. [A] `grep -c 'activeMobilePane' rust-relay-server/web/js/state.js` → 期望: 输出 `1`
  6. [A] `grep -c 'initMobile' rust-relay-server/web/js/main.js` → 期望: 输出 `2`（import 和调用各一处）
  7. [A] `grep -c 'isMobile' rust-relay-server/web/js/render.js` → 期望: 输出 `3` 或更多
  8. [A] `grep -c 'mobile-agent-name' rust-relay-server/web/js/render.js` → 期望: 输出 `2` 或更多
- **异常排查:**
  - 函数不存在：确认 Task 3 执行步骤已应用到 `layout.js`
  - `activeMobilePane` 不存在：确认 `state.js` 的 `layout` 对象已添加该字段

---

### 场景 2：桌面端布局不受影响

#### - [x] 2.1 移动端元素在桌面端强制隐藏
- **来源:** Task 2 检查步骤 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -c 'min-width: 769px' rust-relay-server/web/style.css` → 期望: 输出 `1`（确认隐藏规则存在）
  2. [H] 在桌面浏览器（窗口宽度 > 768px）打开 `http://localhost:8080/web/?token=test-token`。查看页面顶部，确认**没有**出现汉堡菜单（☰ 按钮）或 "Agent Remote Control" 标题栏 → 是/否
- **异常排查:**
  - 桌面端出现移动端元素：检查 `@media (min-width: 769px)` 块中的 `display: none !important` 规则是否存在

#### - [x] 2.2 桌面端侧边栏正常显示
- **来源:** spec-design.md 验收标准（不破坏桌面端布局）
- **操作步骤:**
  1. [H] 在桌面浏览器（窗口宽度 > 768px）打开页面，查看左侧，确认**显示**固定宽度的侧边栏（包含"在线 Agent"标题和 Agent 列表），侧边栏未隐藏或偏移 → 是/否
- **异常排查:**
  - 桌面端侧边栏消失：检查 `@media (max-width: 768px)` 中的 `#sidebar` transform 规则是否被桌面端媒体查询覆盖

---

### 场景 3：移动端顶部导航栏

#### - [x] 3.1 移动端视口下顶部导航栏可见
- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在 DevTools 中切换到移动端仿真模式（宽度 ≤ 768px，如 iPhone 15 Pro = 393px）。刷新页面后，查看页面顶部，确认出现：左侧 ☰ 汉堡按钮、中间"Agent Remote Control"文字标题 → 是/否
- **异常排查:**
  - 顶部栏未出现：检查 `@media (max-width: 768px)` 中 `#mobile-topbar { display: flex; }` 规则是否存在

#### - [x] 3.2 移动端侧边栏不占用屏幕空间
- **来源:** spec-design.md 设计方案（transform 隐藏）
- **操作步骤:**
  1. [H] 移动端视口下（宽度 ≤ 768px），确认页面左侧**没有**占用 220px 的侧边栏，消息区域占据全部屏幕宽度 → 是/否
- **异常排查:**
  - 侧边栏仍然占据空间：检查 `@media (max-width: 768px)` 中 `#sidebar { position: fixed; transform: translateX(-100%); }` 是否正确，`position: fixed` 使侧边栏脱离文档流

---

### 场景 4：抽屉侧边栏交互

（需要移动端仿真模式，且至少有一个 Agent 在线）

#### - [x] 4.1 点击汉堡按钮开启侧边栏抽屉
- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 移动端仿真模式下，点击页面左上角的 ☰ 汉堡按钮。确认：①左侧侧边栏从屏幕左侧滑入（有动画过渡效果）；②右侧区域出现半透明深色遮罩层 → 是/否
- **异常排查:**
  - 侧边栏未滑入：检查 `initMobile()` 是否在 `main.js` 中被调用；检查汉堡按钮的 click 事件是否正确绑定 `sidebar.classList.add('mobile-visible')`
  - 无动画效果：检查 CSS 中 `#sidebar { transition: transform 0.25s ease; }` 是否存在

#### - [x] 4.2 点击遮罩层关闭侧边栏
- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在侧边栏展开状态下，点击右侧的半透明遮罩区域（不是侧边栏本身）。确认：①侧边栏向左滑出消失；②遮罩层消失 → 是/否
- **异常排查:**
  - 点击遮罩无效：检查 `#mobile-overlay` 的 click 事件是否绑定 `closeMobileSidebar()`；检查 `closeMobileSidebar` 是否正确移除 `mobile-visible` 和 `visible` class

#### - [x] 4.3 点击 Agent 后侧边栏自动关闭
- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 展开侧边栏后，点击 Agent 列表中的某个 Agent 名称。确认：①侧边栏关闭（滑出）；②页面顶部右侧显示该 Agent 的名称（`#mobile-agent-name` 更新） → 是/否
- **异常排查:**
  - 点击 Agent 后侧边栏未关闭：检查 `render.js` 的 `renderSidebar()` 中 Agent 点击事件是否调用 `closeDrawer()`（即 `closeMobileSidebar`）
  - 顶部 Agent 名未更新：检查 `renderSidebar()` 中是否有 `mobileAgentName.textContent = agent.name` 赋值

---

### 场景 5：面板 Tab 切换

（需要多个 Agent 在线并绑定到不同面板）

#### - [x] 5.1 单个 Agent 绑定时无 Tab 栏显示
- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 移动端仿真模式下，当只有一个 Agent 绑定到面板时，查看消息列表上方区域，确认**没有**出现 Tab 标签栏（`#mobile-tabs` 不可见） → 是/否
- **异常排查:**
  - Tab 栏意外显示：检查 `renderMobileTabs()` 中 `boundPanes.length <= 1` 时是否正确移除 `has-tabs` class

#### - [x] 5.2 多个 Agent 绑定时显示 Tab 栏
- **来源:** spec-design.md 验收标准
- **前置操作:** 先在桌面端将布局切换为 2 栏，分别绑定两个不同的 Agent，再切换到移动端仿真模式
- **操作步骤:**
  1. [H] 移动端仿真模式下，确认消息列表上方出现 Tab 标签栏，其中显示两个 Agent 名称对应的标签（当前激活的标签下方有橙色底线） → 是/否
- **异常排查:**
  - Tab 栏未出现：检查 `renderMobileTabs()` 是否在 `isMobile()` 为 true 时运行；检查 `state.layout.panes` 中是否有两个非 null 的 sessionId

#### - [x] 5.3 点击 Tab 标签切换面板内容
- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] Tab 栏显示状态下，点击非激活的 Tab 标签（另一个 Agent 名称）。确认：①被点击的 Tab 标签变为激活态（橙色底线）；②下方消息区切换为该 Agent 的消息内容 → 是/否
- **异常排查:**
  - Tab 点击后内容未切换：检查 `renderMobileTabs()` 中 Tab 的 click 事件是否更新 `state.layout.activeMobilePane` 并调用 `renderLayout()`

---

### 场景 6：虚拟键盘与 Modal 适配

#### - [x] 6.1 dvh 动态视口高度规则已应用
- **来源:** Task 2 + spec-design.md 虚拟键盘适配
- **操作步骤:**
  1. [A] `grep 'height: 100dvh' rust-relay-server/web/style.css` → 期望: 返回至少 2 行（body 和 #app）
  2. [A] `grep 'max-height: 85dvh' rust-relay-server/web/style.css` → 期望: 返回 1 行（.modal-card）
- **异常排查:**
  - 未找到 `100dvh`：确认 Task 2 的 `body` 和 `#app` 高度已从 `100vh` 改为 `100dvh`

#### - [x] 6.2 虚拟键盘弹起时输入框可见
- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在 DevTools 移动端模式（或真实手机）下，点击页面底部输入框，弹出系统虚拟键盘。确认输入框**仍然完全可见**，未被键盘覆盖 → 是/否
- **异常排查:**
  - 输入框被遮挡：`dvh` 兼容性问题（iOS Safari < 15.4）；可尝试在 DevTools Console 中运行 `document.documentElement.style.height` 确认高度单位

#### - [x] 6.3 Modal 弹窗高度适配移动端
- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 移动端仿真模式下，触发 HITL 审批弹窗（需要有待审批的 Agent 工具调用）或 AskUser 弹窗。确认：弹窗高度未超出屏幕，"全部批准"/"提交"等操作按钮不需要滚动即可看到 → 是/否
- **异常排查:**
  - 弹窗超出屏幕：检查 `.modal-card` 是否有 `max-height: 85dvh` 且 `overflow-y: auto`

---

### 场景 7：重编译与部署验证

#### - [x] 7.1 前端变更已打包进 relay-server 二进制
- **来源:** spec-design.md 实现要点（rust-embed 重编译）
- **操作步骤:**
  1. [A] `cargo build -p rust-relay-server --features server 2>&1 | tail -5` → 期望: 输出包含 `Finished`，无 `error` 字样
  2. [A] `test -f target/debug/rust-relay-server && echo "EXISTS" || echo "MISSING"` → 期望: 输出 `EXISTS`
  3. [A] `grep -c 'mobile-topbar' rust-relay-server/web/index.html` → 期望: 输出 `1`（源文件存在）
  4. [A] `cargo run -p rust-relay-server --features server -- --help 2>&1 | head -5` → 期望: 显示帮助信息，无编译错误
- **异常排查:**
  - 编译报错：检查 Rust 语法是否有问题（`cargo check -p rust-relay-server --features server`）
  - 二进制不含新前端：前端是通过 `rust-embed` 在编译时打包的，修改后**必须重新编译**才能生效

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | HTML 元素存在性 | 4 | 0 | ✅ | |
| 场景 1 | 1.2 | CSS 核心规则 | 5 | 0 | ✅ | |
| 场景 1 | 1.3 | JS 函数与状态 | 8 | 0 | ✅ | |
| 场景 2 | 2.1 | 移动端元素桌面隐藏 | 1 | 1 | ✅ | |
| 场景 2 | 2.2 | 桌面侧边栏正常 | 0 | 1 | ✅ | |
| 场景 3 | 3.1 | 移动端顶部栏可见 | 0 | 1 | ✅ | |
| 场景 3 | 3.2 | 侧边栏不占空间 | 0 | 1 | ✅ | |
| 场景 4 | 4.1 | 汉堡按钮开启抽屉 | 0 | 1 | ✅ | |
| 场景 4 | 4.2 | 遮罩层关闭抽屉 | 0 | 1 | ✅ | |
| 场景 4 | 4.3 | Agent 点击关闭抽屉 | 0 | 1 | ✅ | |
| 场景 5 | 5.1 | 单 Agent 无 Tab | 0 | 1 | ✅ | |
| 场景 5 | 5.2 | 多 Agent 显示 Tab | 0 | 1 | ✅ | |
| 场景 5 | 5.3 | Tab 切换内容 | 0 | 1 | ✅ | |
| 场景 6 | 6.1 | dvh 规则验证 | 2 | 0 | ✅ | |
| 场景 6 | 6.2 | 键盘不遮挡输入框 | 0 | 1 | ✅ | |
| 场景 6 | 6.3 | Modal 弹窗高度 | 0 | 1 | ✅ | |
| 场景 7 | 7.1 | 重编译成功 | 4 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
