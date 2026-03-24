// Agent Remote Control - Web Frontend
(function() {
  'use strict';

  const params = new URLSearchParams(location.search);
  const TOKEN = params.get('token') || '';

  // State
  const agents = new Map(); // sessionId -> { name, status, messages[], todos[], ws, pendingHitl, pendingAskUser, maxSeq }
  let activeSessionId = null;
  let managementWs = null;

  // DOM refs
  const tabsEl = document.getElementById('tabs');
  const messagesEl = document.getElementById('messages');
  const todoPanel = document.getElementById('todo-panel');
  const todoList = document.getElementById('todo-list');
  const inputEl = document.getElementById('input');
  const sendBtn = document.getElementById('send-btn');
  const statusEl = document.getElementById('connection-status');

  // ─── closeDialog（弹窗管理）────────────────────────────────────
  function closeDialog(type) {
    const modal = document.getElementById(type + '-modal');
    if (modal) modal.classList.add('hidden');
  }
  const hitlModal = document.getElementById('hitl-modal');
  const hitlItems = document.getElementById('hitl-items');
  const askuserModal = document.getElementById('askuser-modal');
  const askuserItems = document.getElementById('askuser-items');

  // ---- WS Connection Management ----

  function wsUrl(path) {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    return `${proto}//${location.host}${path}`;
  }

  function connectManagement() {
    const url = wsUrl(`/web/ws?token=${TOKEN}`);
    managementWs = new WebSocket(url);

    managementWs.onopen = () => {
      statusEl.textContent = '已连接';
      statusEl.className = 'status-connected';
    };

    managementWs.onmessage = (e) => {
      const msg = JSON.parse(e.data);
      handleBroadcast(msg);
    };

    managementWs.onclose = () => {
      statusEl.textContent = '重连中...';
      statusEl.className = 'status-reconnecting';
      setTimeout(connectManagement, 3000);
    };

    managementWs.onerror = () => managementWs.close();
  }

  function connectSession(sessionId) {
    const agent = agents.get(sessionId);
    if (!agent || agent.ws) return;

    const url = wsUrl(`/web/ws?token=${TOKEN}&session=${sessionId}`);
    const ws = new WebSocket(url);
    agent.ws = ws;

    ws.onopen = () => {
      // 首次连接 since_seq=0，重连时使用已知最大 seq 实现增量 sync
      const since = agent ? agent.maxSeq : 0;
      ws.send(JSON.stringify({ type: 'sync_request', since_seq: since }));
    };

    ws.onmessage = (e) => {
      const msg = JSON.parse(e.data);
      handleAgentEvent(sessionId, msg);
    };

    ws.onclose = () => {
      agent.ws = null;
      // Reconnect if still in agents map
      if (agents.has(sessionId)) {
        setTimeout(() => connectSession(sessionId), 3000);
      }
    };

    ws.onerror = () => ws.close();
  }

  // ---- Broadcast Handlers ----

  function handleBroadcast(msg) {
    switch (msg.type) {
      case 'agents_list':
        msg.agents.forEach(a => addAgent(a.session_id, a.name, 'online'));
        break;
      case 'agent_online':
        addAgent(msg.session_id, msg.name, 'online');
        break;
      case 'agent_offline':
        if (agents.has(msg.session_id)) {
          agents.get(msg.session_id).status = 'offline';
          renderTabs();
        }
        break;
    }
  }

  function addAgent(sessionId, name, status) {
    // 按 name 唯一化：同名 Agent 重连时复用旧 Tab
    const displayName = name || sessionId.slice(0, 8);
    let existingId = null;
    if (name) {
      for (const [id, a] of agents) {
        if (a.name === name && id !== sessionId) {
          existingId = id;
          break;
        }
      }
    }
    if (existingId) {
      // 同名 Agent 重连：关闭旧 WS，迁移数据到新 session（保留历史消息和 maxSeq）
      const old = agents.get(existingId);
      if (old.ws) old.ws.close();
      agents.delete(existingId);
      agents.set(sessionId, {
        name: displayName,
        status,
        messages: old.messages,
        todos: old.todos,
        ws: null,
        pendingHitl: null,
        pendingAskUser: null,
        maxSeq: old.maxSeq,  // 保留旧 maxSeq 用于增量 sync
      });
      connectSession(sessionId);
      if (activeSessionId === existingId) activeSessionId = sessionId;
    } else if (!agents.has(sessionId)) {
      agents.set(sessionId, {
        name: displayName,
        status,
        messages: [],
        todos: [],
        ws: null,
        pendingHitl: null,
        pendingAskUser: null,
        maxSeq: 0,
      });
      connectSession(sessionId);
    } else {
      agents.get(sessionId).status = status;
    }
    renderTabs();
    if (!activeSessionId) switchTab(sessionId);
  }

  // ---- Agent Event Handlers ----

  // 处理单条事件（可被实时消息和 sync_response 批量回放共同调用）
  // 支持两种格式：
  // - BaseMessage 格式：{ role: "user"|"assistant"|"tool"|"system", content: "...", tool_calls?: [...] }
  // - 旧 AgentEvent 格式：{ type: "text_chunk"|"tool_start"|"tool_end"|"...", ... }
  function handleSingleEvent(sessionId, event) {
    const agent = agents.get(sessionId);
    if (!agent) return;

    // 更新已知最大 seq
    if (event.seq !== undefined && event.seq > agent.maxSeq) {
      agent.maxSeq = event.seq;
    }

    // BaseMessage 格式（role 字段）
    if (event.role !== undefined) {
      handleBaseMessage(agent, event);
      return;
    }

    // 旧 AgentEvent 格式（type 字段）
    handleLegacyEvent(agent, event);
  }

  // 处理 BaseMessage 格式
  function handleBaseMessage(agent, event) {
    const text = typeof event.content === 'string' ? event.content : '';
    const toolCalls = event.tool_calls || [];

    switch (event.role) {
      case 'user':
        agent.messages.push({ type: 'user', text });
        break;
      case 'assistant':
        // 检查是否有工具调用
        if (toolCalls.length > 0) {
          toolCalls.forEach(tc => {
            agent.messages.push({
              type: 'tool',
              name: tc.name,
              tool_call_id: tc.id,
              input: tc.arguments,
              output: null,
            });
          });
        }
        if (text || !agent.messages.length) {
          agent.messages.push({ type: 'assistant', text, streaming: false });
        }
        break;
      case 'tool':
        // 查找对应的 tool 消息并更新 output
        const tcId = event.tool_call_id;
        if (tcId) {
          for (let i = agent.messages.length - 1; i >= 0; i--) {
            const m = agent.messages[i];
            if (m.type === 'tool' && m.tool_call_id === tcId) {
              m.output = text;
              m.isError = event.is_error || false;
              break;
            }
          }
        }
        break;
      case 'system':
        // system 消息暂不显示
        break;
    }
  }

  // 处理旧 AgentEvent 格式（兼容保留）
  function handleLegacyEvent(agent, event) {
    const eventType = event.type;

    // serde tuple variants serialize as {"type":"text_chunk","0":"text"} — use event["0"] for tuple data
    switch (eventType) {
      case 'user_message':
        agent.messages.push({ type: 'user', text: event.text || '' });
        break;
      case 'text_chunk':
        agent.messages.push({ type: 'assistant', text: event["0"] || '' });
        break;
      case 'tool_start':
        agent.messages.push({ type: 'tool', name: event.name, input: event.input, output: null });
        break;
      case 'tool_end':
        // Find last matching tool message and add output
        for (let i = agent.messages.length - 1; i >= 0; i--) {
          if (agent.messages[i].type === 'tool' && agent.messages[i].name === event.name && !agent.messages[i].output) {
            agent.messages[i].output = event.output;
            agent.messages[i].isError = event.is_error;
            break;
          }
        }
        break;
      case 'tool_call':
        agent.messages.push({ type: 'tool', name: event.name, args: event.args, display: event.display });
        break;
      case 'assistant_chunk':
        // Stream append — not emitted by current AgentEvent enum, kept for future use
        const last = agent.messages[agent.messages.length - 1];
        if (last && last.type === 'assistant' && last.streaming) {
          last.text += (event["0"] || '');
        } else {
          agent.messages.push({ type: 'assistant', text: event["0"] || '', streaming: true });
        }
        break;
      case 'done':
        const lastMsg = agent.messages[agent.messages.length - 1];
        if (lastMsg) lastMsg.streaming = false;
        break;
      case 'error':
        agent.messages.push({ type: 'error', text: event["0"] || 'Error' });
        break;
      case 'todo_update':
        agent.todos = event.items || [];
        break;
      case 'approval_needed':
        // items: [{tool_name, input}] → normalize to requests: [{name, input}]
        agent.pendingHitl = { requests: (event.items || []).map(i => ({ name: i.tool_name, input: i.input })) };
        if (sessionId !== activeSessionId) renderTabs();
        else showHitlDialog(agent.pendingHitl);
        break;
      case 'ask_user_batch':
        // questions: [{question, options}]
        agent.pendingAskUser = { questions: event.questions || [] };
        if (sessionId !== activeSessionId) renderTabs();
        else showAskUserDialog(agent.pendingAskUser);
        break;
      case 'approval_resolved':
        // HITL 已解决，清除弹窗状态
        if (agent.pendingHitl) {
          agent.pendingHitl = null;
          closeDialog('hitl');
          renderMessages();
        }
        break;
      case 'ask_user_resolved':
        // AskUser 已解决，清除弹窗状态
        if (agent.pendingAskUser) {
          agent.pendingAskUser = null;
          closeDialog('askuser');
          renderMessages();
        }
        break;
    }
  }

  function handleAgentEvent(sessionId, msg) {
    const agent = agents.get(sessionId);
    if (!agent) return;

    // 处理 sync_response：批量回放历史事件
    if (msg.type === 'sync_response') {
      (msg.events || []).forEach(ev => handleSingleEvent(sessionId, ev));
      if (sessionId === activeSessionId) {
        renderMessages();
        renderTodoPanel();
      }
      return;
    }

    // 处理单条实时事件（格式已扁平化，直接使用 msg）
    handleSingleEvent(sessionId, msg);

    if (sessionId === activeSessionId) {
      renderMessages();
      renderTodoPanel();
    }
  }

  // ---- Tab Management ----

  function switchTab(sessionId) {
    activeSessionId = sessionId;
    renderTabs();
    renderMessages();
    renderTodoPanel();

    const agent = agents.get(sessionId);
    if (agent) {
      if (agent.pendingHitl) showHitlDialog(agent.pendingHitl);
      if (agent.pendingAskUser) showAskUserDialog(agent.pendingAskUser);
    }
  }

  function renderTabs() {
    tabsEl.innerHTML = '';
    agents.forEach((agent, sid) => {
      const tab = document.createElement('div');
      tab.className = 'tab' + (sid === activeSessionId ? ' active' : '');

      let badge = '';
      if (agent.pendingHitl || agent.pendingAskUser) badge = '<span class="badge">🔔</span>';

      const dotClass = agent.status === 'online' ? 'dot-online' : 'dot-offline';
      tab.innerHTML = `${agent.name} <span class="dot ${dotClass}"></span>${badge}`;
      tab.onclick = () => switchTab(sid);
      tabsEl.appendChild(tab);
    });
  }

  // ---- Message Rendering ----

  function renderMessages() {
    const agent = agents.get(activeSessionId);
    if (!agent) { messagesEl.innerHTML = ''; return; }

    messagesEl.innerHTML = '';
    agent.messages.forEach(msg => {
      const div = document.createElement('div');
      div.className = 'message';

      switch (msg.type) {
        case 'user':
          div.className += ' msg-user';
          div.textContent = msg.text;
          break;
        case 'assistant':
          div.className += ' msg-assistant';
          div.textContent = msg.text;
          break;
        case 'tool':
          div.className += ' msg-tool';
          div.innerHTML = `<span class="tool-name">${escHtml(msg.display || msg.name)}</span>`;
          if (msg.args) div.innerHTML += ` <span class="tool-args">${escHtml(typeof msg.args === 'string' ? msg.args : JSON.stringify(msg.args))}</span>`;
          if (msg.input) div.innerHTML += `<div class="tool-args">${escHtml(typeof msg.input === 'string' ? msg.input : JSON.stringify(msg.input))}</div>`;
          if (msg.output) div.innerHTML += `<div class="tool-output${msg.isError ? ' msg-error' : ''}">${escHtml(msg.output)}</div>`;
          break;
        case 'error':
          div.className += ' msg-error';
          div.textContent = msg.text;
          break;
      }

      messagesEl.appendChild(div);
    });

    messagesEl.scrollTop = messagesEl.scrollHeight;
  }

  // ---- TODO Panel ----

  function renderTodoPanel() {
    const agent = agents.get(activeSessionId);
    if (!agent || !agent.todos || agent.todos.length === 0) {
      todoPanel.classList.add('hidden');
      return;
    }

    todoPanel.classList.remove('hidden');
    todoList.innerHTML = '';
    agent.todos.forEach(item => {
      const li = document.createElement('li');
      const status = item.status || 'pending';
      if (status === 'in_progress') {
        li.className = 'todo-in-progress';
        li.textContent = `→ ${item.title || item.content}`;
      } else if (status === 'done' || status === 'completed') {
        li.className = 'todo-done';
        li.textContent = `✓ ${item.title || item.content}`;
      } else {
        li.className = 'todo-pending';
        li.textContent = `○ ${item.title || item.content}`;
      }
      todoList.appendChild(li);
    });
  }

  // ---- HITL Dialog ----

  function showHitlDialog(data) {
    if (!data) return;
    hitlItems.innerHTML = '';

    const requests = data.requests || data[0] || [];
    (Array.isArray(requests) ? requests : [requests]).forEach(req => {
      const div = document.createElement('div');
      div.className = 'hitl-item';
      div.innerHTML = `
        <div class="tool-info"><span class="tool-name">${escHtml(req.name || 'tool')}</span></div>
        <div class="tool-input">${escHtml(typeof req.input === 'string' ? req.input : JSON.stringify(req.input, null, 2))}</div>
      `;
      hitlItems.appendChild(div);
    });

    hitlModal.classList.remove('hidden');
  }

  document.getElementById('hitl-approve-all').onclick = () => {
    const agent = agents.get(activeSessionId);
    if (!agent || !agent.ws || !agent.pendingHitl) return;

    const requests = agent.pendingHitl.requests || agent.pendingHitl[0] || [];
    const decisions = (Array.isArray(requests) ? requests : [requests]).map(r => ({
      tool_call_id: r.tool_call_id || r.id || '',
      decision: 'Approve',
    }));

    agent.ws.send(JSON.stringify({ type: 'hitl_decision', decisions }));
    agent.pendingHitl = null;
    hitlModal.classList.add('hidden');
    renderTabs();
  };

  document.getElementById('hitl-reject-all').onclick = () => {
    const agent = agents.get(activeSessionId);
    if (!agent || !agent.ws || !agent.pendingHitl) return;

    const requests = agent.pendingHitl.requests || agent.pendingHitl[0] || [];
    const decisions = (Array.isArray(requests) ? requests : [requests]).map(r => ({
      tool_call_id: r.tool_call_id || r.id || '',
      decision: 'Reject',
    }));

    agent.ws.send(JSON.stringify({ type: 'hitl_decision', decisions }));
    agent.pendingHitl = null;
    hitlModal.classList.add('hidden');
    renderTabs();
  };

  // ---- AskUser Dialog ----

  function showAskUserDialog(data) {
    if (!data) return;
    askuserItems.innerHTML = '';

    const questions = data.questions || data[0] || [];
    (Array.isArray(questions) ? questions : [questions]).forEach((q, i) => {
      const div = document.createElement('div');
      div.style.marginBottom = '12px';
      const label = document.createElement('label');
      label.textContent = q.question || q.text || `问题 ${i + 1}`;
      label.style.display = 'block';
      label.style.marginBottom = '4px';
      div.appendChild(label);

      if (q.options && q.options.length > 0) {
        q.options.forEach(opt => {
          const optDiv = document.createElement('div');
          const radio = document.createElement('input');
          radio.type = q.multi_select ? 'checkbox' : 'radio';
          radio.name = `askuser_${i}`;
          radio.value = opt.label || opt;
          const optLabel = document.createElement('span');
          optLabel.textContent = ` ${opt.label || opt}`;
          if (opt.description) {
            optLabel.textContent += ` - ${opt.description}`;
          }
          optDiv.appendChild(radio);
          optDiv.appendChild(optLabel);
          div.appendChild(optDiv);
        });
      } else {
        const input = document.createElement('input');
        input.type = 'text';
        input.name = `askuser_${i}`;
        input.style.cssText = 'width:100%;padding:6px;background:#222;border:1px solid #444;color:#e0e0e0;border-radius:4px;';
        div.appendChild(input);
      }

      askuserItems.appendChild(div);
    });

    askuserModal.classList.remove('hidden');
  }

  document.getElementById('askuser-submit').onclick = () => {
    const agent = agents.get(activeSessionId);
    if (!agent || !agent.ws || !agent.pendingAskUser) return;

    const questions = agent.pendingAskUser.questions || agent.pendingAskUser[0] || [];
    const answers = {};

    (Array.isArray(questions) ? questions : [questions]).forEach((q, i) => {
      const qText = q.question || q.text || `q${i}`;
      const inputs = askuserItems.querySelectorAll(`[name="askuser_${i}"]`);
      if (inputs.length === 1 && inputs[0].type === 'text') {
        answers[qText] = inputs[0].value;
      } else {
        const selected = Array.from(inputs).filter(el => el.checked).map(el => el.value);
        answers[qText] = selected.join(', ');
      }
    });

    agent.ws.send(JSON.stringify({ type: 'ask_user_response', answers }));
    agent.pendingAskUser = null;
    askuserModal.classList.add('hidden');
    renderTabs();
  };

  // ---- Input ----

  function sendMessage() {
    const text = inputEl.value.trim();
    if (!text) return;

    const agent = agents.get(activeSessionId);
    if (!agent || !agent.ws) return;

    if (text === '/clear') {
      agent.ws.send(JSON.stringify({ type: 'clear_thread' }));
      agent.messages = [];
      agent.todos = [];
      renderMessages();
      renderTodoPanel();
    } else {
      agent.ws.send(JSON.stringify({ type: 'user_input', text }));
      // 不要本地 push，等 Agent 广播 MessageAdded 再渲染
    }

    inputEl.value = '';
  }

  sendBtn.onclick = sendMessage;
  inputEl.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  });

  // ---- Util ----

  function escHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
  }

  // ---- Init ----

  if (!TOKEN) {
    messagesEl.innerHTML = '<div class="message msg-error">请在 URL 中提供 token 参数，如 ?token=your-token</div>';
  } else {
    connectManagement();
  }

})();
