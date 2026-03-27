// main.js — 模块入口
import { state } from './state.js';
import { connectManagement } from './connection.js';
import { renderSidebar, renderLayout, initMarked } from './render.js';
import { initLayout, initMobile } from './layout.js';

// ─── 检查 Token ───────────────────────────────────────────────

function checkToken() {
  const token = new URLSearchParams(location.search).get('token') || '';
  const messagesEl = document.getElementById('pane-container');
  if (!token) {
    if (messagesEl) {
      messagesEl.innerHTML =
        '<div style="display:flex;align-items:center;justify-content:center;height:100%;color:var(--text-muted);font-size:14px;">请在 URL 中提供 token 参数，如 ?token=your-token</div>';
    }
    return false;
  }
  return true;
}

// ─── 初始化 ───────────────────────────────────────────────────

function init() {
  if (!checkToken()) return;

  // 初始化 marked.js
  initMarked();

  // 初始化布局
  initLayout();

  // 初始化移动端交互
  initMobile();

  // 渲染初始状态
  renderSidebar();
  renderLayout();

  // 连接 WebSocket
  connectManagement();
}

document.addEventListener('DOMContentLoaded', init);
