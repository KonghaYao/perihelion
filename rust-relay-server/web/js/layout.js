// layout.js — 分屏布局管理
import { state, setPaneAgent, clearPane } from './state.js';
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
  } else if (n > oldCols) {
    // 增加栏：补充 null
    for (let i = oldCols; i < n; i++) {
      state.layout.panes.push(null);
    }
  }

  state.layout.cols = n;
  renderLayout();
  updateLayoutButtons();
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
