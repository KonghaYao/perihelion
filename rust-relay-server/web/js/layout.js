// layout.js — 分屏布局管理
import { state, setPaneAgent, clearPane, getAgent } from './state.js';
import { renderPane } from './render.js';

export function setCols(n) {
  if (n < 1 || n > 3) return;
  const oldCols = state.layout.cols;

  if (n < oldCols) {
    // 减少栏：清除被移除栏的绑定
    for (let i = n; i < oldCols; i++) {
      clearPane(i);
    }
    // 截断 panes 数组
    state.layout.panes = state.layout.panes.slice(0, n);
    // activeMobilePane 不能超过新的栏数
    if (state.layout.activeMobilePane >= n) {
      state.layout.activeMobilePane = 0;
    }
  } else if (n > oldCols) {
    // 增加栏：补充 null
    for (let i = oldCols; i < n; i++) {
      state.layout.panes.push(null);
    }
  }

  state.layout.cols = n;
  renderLayout();
  updateLayoutButtons();
  renderMobileTabs();
}

export function assignAgentToPane(paneIdx, sessionId) {
  if (paneIdx < 0 || paneIdx >= state.layout.cols) return;
  setPaneAgent(paneIdx, sessionId);
  renderPane(paneIdx, sessionId);
  updateLayoutButtons();
}

export function renderLayout() {
  // 动态导入 render.js，避免循环依赖
  import('./render.js').then(({ renderLayout: doRender }) => {
    doRender();
    renderMobileTabs();
  });
}

function updateLayoutButtons() {
  document.querySelectorAll('.layout-btn').forEach(btn => {
    const n = parseInt(btn.id.replace('btn-cols-', ''), 10);
    btn.classList.toggle('active', n === state.layout.cols);
  });
}

export function initLayout() {
  // 绑定分屏切换按钮
  document.getElementById('btn-cols-1')?.addEventListener('click', () => setCols(1));
  document.getElementById('btn-cols-2')?.addEventListener('click', () => setCols(2));
  document.getElementById('btn-cols-3')?.addEventListener('click', () => setCols(3));
  updateLayoutButtons();
}

// ─── 移动端辅助函数 ──────────────────────────────────────────

/** 判断是否处于移动端视口 */
export function isMobile() {
  return window.matchMedia('(max-width: 768px)').matches;
}

/** 关闭移动端抽屉侧边栏 */
export function closeMobileSidebar() {
  document.getElementById('sidebar')?.classList.remove('mobile-visible');
  document.getElementById('mobile-overlay')?.classList.remove('visible');
}

/** 初始化移动端交互（汉堡按钮 + 遮罩关闭） */
export function initMobile() {
  const hamburger = document.getElementById('hamburger-btn');
  const sidebar = document.getElementById('sidebar');
  const overlay = document.getElementById('mobile-overlay');

  hamburger?.addEventListener('click', () => {
    sidebar?.classList.add('mobile-visible');
    overlay?.classList.add('visible');
  });

  overlay?.addEventListener('click', () => {
    closeMobileSidebar();
  });
}

/** 渲染移动端面板 Tab 栏 */
export function renderMobileTabs() {
  if (!isMobile()) return;

  const tabsEl = document.getElementById('mobile-tabs');
  if (!tabsEl) return;

  // 收集有绑定 session 的面板
  const boundPanes = state.layout.panes
    .map((sessionId, idx) => ({ sessionId, idx }))
    .filter(({ sessionId }) => sessionId);

  if (boundPanes.length <= 1) {
    tabsEl.classList.remove('has-tabs');
    return;
  }

  tabsEl.classList.add('has-tabs');
  tabsEl.innerHTML = '';

  boundPanes.forEach(({ sessionId, idx }) => {
    const agent = getAgent(sessionId);
    const name = agent ? agent.name : sessionId.slice(0, 8);

    const tab = document.createElement('button');
    tab.className = 'mobile-tab' + (idx === state.layout.activeMobilePane ? ' active' : '');
    tab.textContent = name;
    tab.addEventListener('click', () => {
      state.layout.activeMobilePane = idx;
      import('./render.js').then(({ renderLayout: doRender }) => {
        doRender();
        renderMobileTabs();
      });
    });
    tabsEl.appendChild(tab);
  });
}
