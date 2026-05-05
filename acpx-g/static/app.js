// ============================================================
// ACPX-G — App Shell (SPA Router + Global State + Toast + Modal)
// ============================================================

const API_WF = '/api/v1/workflows';
const API_TPL = '/api/v1/templates';

// ── Global State ──────────────────────────────────────────
const AppState = {
  currentPage: 'editor',
  templates: [],
  pollTimers: [],
};

// ── API Helper ────────────────────────────────────────────
async function api(path, options = {}) {
  const url = path.startsWith('http') ? path : path;
  const res = await fetch(url, {
    headers: { 'Content-Type': 'application/json', ...options.headers },
    ...options,
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(body.error || body.message || `请求失败 (${res.status})`);
  }
  if (res.status === 204) return null;
  return res.json();
}

// ── Toast System ──────────────────────────────────────────
function showToast(message, type = 'info', duration = 3000) {
  const container = document.getElementById('toastContainer');
  const toast = document.createElement('div');
  toast.className = `toast ${type}`;

  const iconMap = {
    success: 'check-circle',
    error: 'alert-circle',
    warning: 'alert-triangle',
    info: 'info',
  };

  toast.innerHTML = `
    <i data-lucide="${iconMap[type] || 'info'}" class="toast-icon"></i>
    <span class="toast-message">${escapeHtml(message)}</span>
    <button class="toast-close" onclick="dismissToast(this.parentElement)">
      <i data-lucide="x" style="width:14px;height:14px"></i>
    </button>
  `;

  container.appendChild(toast);
  lucide.createIcons({ nodes: [toast] });

  setTimeout(() => dismissToast(toast), duration);
}

function dismissToast(el) {
  if (!el || !el.parentElement) return;
  el.classList.add('exit');
  setTimeout(() => el.remove(), 200);
}

// ── Modal System ──────────────────────────────────────────
function openModal(html, opts = {}) {
  const overlay = document.getElementById('modalOverlay');
  const content = document.getElementById('modalContent');
  content.innerHTML = html;
  overlay.classList.add('open');
  if (opts.onClose) overlay._onClose = opts.onClose;
  lucide.createIcons({ nodes: [content] });
}

function closeModal() {
  const overlay = document.getElementById('modalOverlay');
  overlay.classList.remove('open');
  if (overlay._onClose) {
    overlay._onClose();
    overlay._onClose = null;
  }
}

// ── Confirm Dialog ────────────────────────────────────────
function confirmDialog(title, message, detail, onConfirm) {
  const detailHtml = detail ? `<div class="confirm-detail">${escapeHtml(detail)}</div>` : '';
  openModal(`
    <div class="modal-header">
      <span class="modal-title">${escapeHtml(title)}</span>
      <button class="modal-close" onclick="closeModal()"><i data-lucide="x" style="width:16px;height:16px"></i></button>
    </div>
    <div class="modal-body">
      <div class="confirm-body">${escapeHtml(message)}</div>
      ${detailHtml}
    </div>
    <div class="modal-footer">
      <button class="btn btn-secondary" onclick="closeModal()">取消</button>
      <button class="btn btn-danger" id="confirmBtn">确认</button>
    </div>
  `);
  document.getElementById('confirmBtn').onclick = () => {
    closeModal();
    onConfirm();
  };
}

// ── Router ────────────────────────────────────────────────
function navigate(page, params = {}) {
  // Clear all poll timers
  AppState.pollTimers.forEach(t => clearInterval(t));
  AppState.pollTimers = [];

  AppState.currentPage = page;
  AppState.pageParams = params;

  // Destroy editor when navigating away
  if (page !== 'editor' && typeof destroyEditor === 'function') {
    destroyEditor();
  }

  // Update tab active state
  document.querySelectorAll('.topbar-tab[data-page]').forEach(tab => {
    tab.classList.toggle('active', tab.dataset.page === page);
  });

  // Render page
  const content = document.getElementById('content');

  switch (page) {
    case 'editor':
      content.innerHTML = renderEditorPage();
      if (typeof initEditor === 'function') initEditor();
      break;
    case 'runs':
      content.innerHTML = renderRunsPage();
      if (typeof initRuns === 'function') initRuns();
      break;
    case 'run-detail':
      content.innerHTML = renderRunDetailPage(params.id);
      if (typeof initRunDetail === 'function') initRunDetail(params.id);
      break;
    default:
      content.innerHTML = '<div class="empty-state"><div class="empty-state-icon"><i data-lucide="inbox" style="width:28px;height:28px"></i></div><div class="empty-state-title">页面不存在</div></div>';
  }

  // Re-init lucide icons
  lucide.createIcons();
}

// ── Utility ───────────────────────────────────────────────
function escapeHtml(str) {
  if (!str) return '';
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

function relativeTime(dateStr) {
  if (!dateStr) return '--';
  const now = new Date();
  const date = new Date(dateStr + (dateStr.includes('Z') || dateStr.includes('+') ? '' : 'Z'));
  const diff = (now - date) / 1000;
  if (diff < 60) return '刚刚';
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  if (diff < 604800) return `${Math.floor(diff / 86400)} 天前`;
  return date.toLocaleDateString('zh-CN');
}

function formatDuration(startedAt, finishedAt) {
  if (!startedAt) return '--';
  const start = new Date(startedAt + (startedAt.includes('Z') || startedAt.includes('+') ? '' : 'Z'));
  const end = finishedAt
    ? new Date(finishedAt + (finishedAt.includes('Z') || finishedAt.includes('+') ? '' : 'Z'))
    : new Date();
  const diff = Math.max(0, (end - start) / 1000);
  if (diff < 60) return `${Math.floor(diff)}s`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ${Math.floor(diff % 60)}s`;
  const h = Math.floor(diff / 3600);
  const m = Math.floor((diff % 3600) / 60);
  return `${h}h ${m}m`;
}

function statusText(status) {
  const map = {
    pending: '等待中',
    running: '运行中',
    success: '成功',
    failed: '失败',
    cancelled: '已取消',
    skipped: '已跳过',
  };
  return map[status] || status;
}

function statusClass(status) {
  const map = {
    running: 'running',
    success: 'success',
    idle: 'idle',
    warning: 'warning',
    error: 'error',
    pending: 'pending',
    cancelled: 'cancelled',
    skipped: 'skipped',
  };
  return map[status] || 'pending';
}

function nodeTypeColor(type) {
  const map = {
    shell: 'var(--brand)',
    agent: '#8250DF',
    reference: 'var(--status-active)',
  };
  return map[type] || 'var(--text-dim)';
}

function nodeTypeLabel(type) {
  const map = {
    shell: 'Shell',
    agent: '代理',
    reference: '引用',
  };
  return map[type] || type;
}

function copyToClipboard(text) {
  navigator.clipboard.writeText(text).then(() => {
    showToast('已复制到剪贴板', 'success', 2000);
  }).catch(() => {
    showToast('复制失败', 'error');
  });
}

// ── Init ──────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', () => {
  lucide.createIcons();

  // Tab click handlers
  document.querySelectorAll('.topbar-tab[data-page]').forEach(tab => {
    tab.addEventListener('click', () => {
      location.hash = tab.dataset.page;
    });
  });

  // API Docs button
  document.getElementById('navApiDocs').addEventListener('click', () => {
    if (typeof showApiDocs === 'function') showApiDocs();
  });

  // Close modal on overlay click
  document.getElementById('modalOverlay').addEventListener('click', (e) => {
    if (e.target === e.currentTarget) closeModal();
  });

  // Close API docs modal
  const apiDocsOverlay = document.getElementById('apiDocsOverlay');
  document.getElementById('apiDocsClose').addEventListener('click', () => {
    apiDocsOverlay.classList.remove('open');
  });
  apiDocsOverlay.addEventListener('click', (e) => {
    if (e.target === e.currentTarget) apiDocsOverlay.classList.remove('open');
  });

  // Hash-based routing
  function handleHash() {
    const hash = location.hash.slice(1) || 'editor';
    if (hash.startsWith('run/')) {
      const id = hash.slice(4);
      navigate('run-detail', { id });
    } else {
      navigate(hash);
    }
  }

  window.addEventListener('hashchange', handleHash);
  handleHash();
});
