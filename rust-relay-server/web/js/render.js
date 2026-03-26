// render.js — 渲染层（DOM 操作）
import { state, getAgent } from './state.js';
import { showHitlDialog, showAskUserDialog, closeDialog } from './dialog.js';
import { sendMessage } from './connection.js';
import { assignAgentToPane } from './layout.js';

// ─── XSS 安全转义 ─────────────────────────────────────────────

export function escHtml(str) {
  if (str === null || str === undefined) return '';
  const div = document.createElement('div');
  div.textContent = String(str);
  return div.innerHTML;
}

function safeMarkdown(html) {
  if (typeof window.DOMPurify !== 'undefined') {
    return window.DOMPurify.sanitize(html, { USE_PROFILES: { html: true } });
  }
  return escHtml(html);
}

// ─── marked.js 配置 ────────────────────────────────────────────

export function initMarked() {
  if (typeof window.marked === 'undefined') return;
  window.marked.setOptions({
    breaks: true,
    gfm: true,
  });

  // 代码高亮钩子
  window.marked.use({
    renderer: {
      code(code, lang) {
        const langLabel = lang || 'code';
        const validLang = lang && window.hljs.getLanguage(lang) ? lang : 'plaintext';
        let highlighted;
        try {
          highlighted = window.hljs.highlight(code, { language: validLang }).value;
        } catch {
          highlighted = escHtml(code);
        }
        return `<div class="code-block-wrapper">
          <span class="code-lang-label">${escHtml(langLabel)}</span>
          <pre><code class="hljs language-${escHtml(validLang)}">${highlighted}</code></pre>
        </div>`;
      },
    },
  });
}

// ─── 工具：计算输出行数 ─────────────────────────────────────────

function countLines(str) {
  return (str || '').split('\n').length;
}

// ─── 渲染侧边栏 ───────────────────────────────────────────────

export function renderSidebar() {
  const listEl = document.getElementById('agent-list');
  if (!listEl) return;

  listEl.innerHTML = '';
  state.agents.forEach((agent, sessionId) => {
    const item = document.createElement('div');
    item.className = 'agent-item';

    const dotClass = agent.status === 'online' ? 'dot-online' : 'dot-offline';
    const hasNotification = agent.pendingHitl || agent.pendingAskUser;
    const badge = hasNotification ? '<span class="badge">🔔</span>' : '';

    item.innerHTML = `
      <span class="dot ${dotClass}"></span>
      <span class="agent-name">${escHtml(agent.name)}</span>
      ${badge}
    `;

    // 点击将 agent 绑定到当前 pane
    item.addEventListener('click', () => {
      import('./layout.js').then(({ assignAgentToPane }) => {
        assignAgentToPane(state.activePane, sessionId);
      });
    });

    listEl.appendChild(item);
  });
}

// ─── 渲染单个消息 ─────────────────────────────────────────────

function renderSingleMessage(msg, paneId) {
  const div = document.createElement('div');

  switch (msg.type) {
    case 'user':
      div.className = 'message msg-user';
      div.textContent = msg.text;
      break;

    case 'assistant': {
      div.className = 'message msg-assistant';
      const content = document.createElement('div');
      content.className = 'md-content';

      if (msg.streaming && typeof window.marked !== 'undefined') {
        const html = window.marked.parse(msg.text || '');
        content.innerHTML = safeMarkdown(html);
        if (msg.isStreamingDone !== true) {
          const cursor = document.createElement('span');
          cursor.className = 'cursor-blink';
          cursor.textContent = '｜';
          content.appendChild(cursor);
        }
      } else if (typeof window.marked !== 'undefined') {
        const html = window.marked.parse(msg.text || '');
        content.innerHTML = safeMarkdown(html);
      } else {
        content.textContent = msg.text || '';
      }

      div.appendChild(content);
      break;
    }

    case 'tool': {
      div.className = 'message tool-card';
      const isError = msg.isError;

      // INPUT 区
      const inputJson =
        typeof msg.input === 'string'
          ? msg.input
          : JSON.stringify(msg.input, null, 2);

      const outputStr =
        typeof msg.output === 'string'
          ? msg.output
          : JSON.stringify(msg.output, null, 2);

      const outputLines = countLines(outputStr);
      const collapsed = outputLines > 20;

      div.innerHTML = `
        <div class="tool-header" data-tool-card="${paneId}">
          <span class="tool-name">🔧 ${escHtml(msg.name || 'tool')}</span>
          <span class="tool-toggle">${collapsed ? '▶ 展开' : '▼ 折叠'}</span>
        </div>
        <div class="tool-body">
          <div class="tool-section">
            <div class="tool-section-label">INPUT</div>
            <div class="tool-input">${escHtml(inputJson)}</div>
          </div>
          ${outputStr ? `
          <div class="tool-section">
            <div class="tool-section-label">OUTPUT</div>
            <div class="tool-output${isError ? ' tool-error' : ''}${collapsed ? ' tool-output-collapsed' : ''}">${escHtml(outputStr)}</div>
            ${collapsed ? '<button class="expand-btn">▶ 展开全部</button>' : ''}
          </div>` : ''}
        </div>
      `;

      // 折叠交互
      const header = div.querySelector('.tool-header');
      const body = div.querySelector('.tool-body');
      const toggle = div.querySelector('.tool-toggle');
      const expandBtn = div.querySelector('.expand-btn');
      const outputSection = div.querySelector('.tool-output');

      header.addEventListener('click', () => {
        const isHidden = body.style.display === 'none';
        body.style.display = isHidden ? '' : 'none';
        toggle.textContent = isHidden ? '▼ 折叠' : '▶ 展开';
      });

      if (expandBtn) {
        expandBtn.addEventListener('click', (e) => {
          e.stopPropagation();
          outputSection.classList.remove('tool-output-collapsed');
          expandBtn.remove();
        });
      }
      break;
    }

    case 'error':
      div.className = 'message msg-error';
      div.textContent = msg.text;
      break;

    default:
      div.className = 'message';
      div.textContent = JSON.stringify(msg);
  }

  return div;
}

// ─── 渲染消息列表 ─────────────────────────────────────────────

export function renderMessages(paneId, agent) {
  const container = document.getElementById(`messages-${paneId}`);
  if (!container) return;

  // 记录是否在底部（50px 容差），以便更新后决定是否自动滚动
  const wasAtBottom =
    container.scrollTop + container.clientHeight >= container.scrollHeight - 50;

  container.innerHTML = '';
  agent.messages.forEach(msg => {
    container.appendChild(renderSingleMessage(msg, paneId));
  });

  // Loading 态：追加到消息列表末尾
  if (agent.isRunning) {
    const loadingEl = document.createElement('div');
    loadingEl.className = 'message msg-loading';
    loadingEl.innerHTML = `
      <div class="loading-dots">
        <span></span><span></span><span></span>
      </div>
    `;
    container.appendChild(loadingEl);
  }

  if (wasAtBottom) {
    container.scrollTop = container.scrollHeight;
  }
}

// ─── 渲染 TODO 面板 ───────────────────────────────────────────

export function renderTodoPanel(paneId, todos) {
  const panel = document.getElementById(`todo-panel-${paneId}`);
  const list = document.getElementById(`todo-list-${paneId}`);
  if (!panel || !list) return;

  if (!todos || todos.length === 0) {
    panel.classList.add('hidden');
    return;
  }

  panel.classList.remove('hidden');
  list.innerHTML = '';
  todos.forEach(item => {
    const li = document.createElement('li');
    const status = item.status || 'pending';
    if (status === 'in_progress') {
      li.className = 'todo-in-progress';
      li.textContent = `→ ${item.title || item.content || ''}`;
    } else if (status === 'done' || status === 'completed') {
      li.className = 'todo-done';
      li.textContent = `✓ ${item.title || item.content || ''}`;
    } else {
      li.className = 'todo-pending';
      li.textContent = `○ ${item.title || item.content || ''}`;
    }
    list.appendChild(li);
  });
}

// ─── 渲染状态文字（loading 已移至消息列表末尾，此函数保留供兼容调用）─────

export function renderStatus(_paneId, _agent) {
  // no-op：loading 态由 renderMessages 内的 msg-loading 气泡承担
}

// ─── 渲染单个面板 ─────────────────────────────────────────────

export function renderPane(paneId, sessionId) {
  const container = document.getElementById(`pane-${paneId}`);
  if (!container) return;

  if (!sessionId) {
    renderEmptyPane(paneId);
    return;
  }

  const agent = getAgent(sessionId);
  if (!agent) {
    renderEmptyPane(paneId);
    return;
  }

  // 面板根元素
  container.innerHTML = '';
  container.className = 'pane';

  // TODO 面板
  const todoPanel = document.createElement('div');
  todoPanel.id = `todo-panel-${paneId}`;
  todoPanel.className = 'pane-todo hidden';
  todoPanel.innerHTML = `
    <div class="todo-header" data-todo-toggle="${paneId}">
      <span>📋 TODO</span>
      <span class="todo-toggle-icon">▼</span>
    </div>
    <ul id="todo-list-${paneId}" class="todo-list"></ul>
  `;

  // TODO 折叠
  todoPanel.querySelector(`[data-todo-toggle="${paneId}"]`).addEventListener('click', () => {
    const body = todoPanel.querySelector('.todo-list');
    const icon = todoPanel.querySelector('.todo-toggle-icon');
    body.style.display = body.style.display === 'none' ? '' : 'none';
    icon.textContent = body.style.display === 'none' ? '▶' : '▼';
  });

  // 消息列表
  const messages = document.createElement('div');
  messages.id = `messages-${paneId}`;
  messages.className = 'messages';

  // 输入栏
  const inputBar = document.createElement('div');
  inputBar.className = 'pane-input';
  inputBar.innerHTML = `
    <input type="text" id="input-${paneId}" placeholder="输入消息..." autocomplete="off" />
    <button class="send-btn" data-pane="${paneId}">发送</button>
  `;

  const inputEl = inputBar.querySelector(`#input-${paneId}`);
  const sendBtn = inputBar.querySelector('.send-btn');

  const doSend = () => {
    const text = inputEl.value.trim();
    if (!text) return;
    if (text === '/clear') {
      sendMessage(sessionId, { type: 'clear_thread' });
      agent.messages = [];
      agent.todos = [];
      renderMessages(paneId, agent);
    } else {
      sendMessage(sessionId, { type: 'user_input', text });
    }
    inputEl.value = '';
  };

  sendBtn.addEventListener('click', doSend);
  inputEl.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      doSend();
    }
  });

  container.appendChild(todoPanel);
  container.appendChild(messages);
  container.appendChild(inputBar);

  renderTodoPanel(paneId, agent.todos);
  renderMessages(paneId, agent);

  // HITL / AskUser 弹窗
  if (agent.pendingHitl) {
    setTimeout(() => showHitlDialog(agent, sessionId), 0);
  }
  if (agent.pendingAskUser) {
    setTimeout(() => showAskUserDialog(agent, sessionId), 0);
  }
}

// ─── 空面板占位 ─────────────────────────────────────────────

export function renderEmptyPane(paneId) {
  const container = document.getElementById(`pane-${paneId}`);
  if (!container) return;

  container.innerHTML = '';
  container.className = 'pane';

  const empty = document.createElement('div');
  empty.className = 'pane-empty';

  // Agent 选择下拉
  const select = document.createElement('select');
  select.innerHTML = '<option value="">— 选择 Agent —</option>';
  state.agents.forEach((agent, sid) => {
    const opt = document.createElement('option');
    opt.value = sid;
    opt.textContent = `${agent.name} ${agent.status === 'online' ? '🟢' : '⚪'}`;
    select.appendChild(opt);
  });

  select.addEventListener('change', () => {
    if (select.value) {
      import('./layout.js').then(({ assignAgentToPane }) => {
        assignAgentToPane(paneId, select.value);
      });
    }
  });

  empty.innerHTML = '<span style="color: var(--text-muted); font-size: 13px;">此栏未分配 Agent</span>';
  empty.appendChild(select);
  container.appendChild(empty);
}

// ─── 渲染布局（分屏容器）──────────────────────────────────────

export function renderLayout() {
  const container = document.getElementById('pane-container');
  if (!container) return;

  container.innerHTML = '';
  const { cols, panes } = state.layout;

  for (let i = 0; i < cols; i++) {
    const paneEl = document.createElement('div');
    paneEl.id = `pane-${i}`;
    paneEl.className = 'pane';
    container.appendChild(paneEl);

    // paneEl 先加入 DOM，再渲染内容（需挂载 DOM ID）
    renderPane(i, panes[i]);

    // 分隔线（最后一栏不加）
    if (i < cols - 1) {
      const divider = document.createElement('div');
      divider.className = 'pane-divider';
      container.appendChild(divider);
    }
  }
}
