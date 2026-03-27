// events.js — 事件解析层
import { state, upsertAgent, getAgent, setPaneAgent } from './state.js';
import {
  renderSidebar,
  renderMessages,
  renderTodoPanel,
  renderPane,
} from './render.js';
import { showHitlDialog, closeDialog } from './dialog.js';

// ─── 广播消息处理 ─────────────────────────────────────────────

export function handleBroadcast(msg) {
  switch (msg.type) {
    case 'agents_list':
      (msg.agents || []).forEach(a =>
        addAgent(a.session_id, a.name, 'online')
      );
      break;
    case 'agent_online':
      addAgent(msg.session_id, msg.name, 'online');
      break;
    case 'agent_offline':
      if (state.agents.has(msg.session_id)) {
        upsertAgent(msg.session_id, { status: 'offline' });
        renderSidebar();
        renderPaneForAllPanes();
      }
      break;
  }
}

// ─── Agent 注册（同名重连合并）────────────────────────────────

export function addAgent(sessionId, name, status) {
  const displayName = name || sessionId.slice(0, 8);

  // 查找同名旧 session（断线重连场景）
  let existingId = null;
  if (name) {
    for (const [id, a] of state.agents) {
      if (a.name === name && id !== sessionId) {
        existingId = id;
        break;
      }
    }
  }

  if (existingId) {
    // 同名 Agent 重连：迁移数据
    const old = state.agents.get(existingId);
    if (old.ws) old.ws.close();
    state.agents.delete(existingId);
    upsertAgent(sessionId, {
      name: displayName,
      status,
      messages: old.messages,
      todos: old.todos,
      ws: null,
      pendingHitl: old.pendingHitl,
      pendingAskUser: old.pendingAskUser,
      maxSeq: old.maxSeq,
    });
    // 迁移面板绑定：将旧 sessionId 替换为新 sessionId
    for (let i = 0; i < state.layout.panes.length; i++) {
      if (state.layout.panes[i] === existingId) {
        setPaneAgent(i, sessionId);
      }
    }
    // 如果没有任何面板绑定，尝试自动分配
    if (!state.layout.panes.includes(sessionId)) {
      autoAssignPane(sessionId);
    }
    // 通知 connection.js 建立 WS
    import('./connection.js').then(({ connectSession }) => {
      connectSession(sessionId);
    });
  } else if (!state.agents.has(sessionId)) {
    upsertAgent(sessionId, { name: displayName, status });
    // 自动分配到第一个空闲面板
    autoAssignPane(sessionId);
    import('./connection.js').then(({ connectSession }) => {
      connectSession(sessionId);
    });
  } else {
    upsertAgent(sessionId, { status });
  }

  renderSidebar();
  renderPaneForAllPanes();
}

// ─── 自动分配 Agent 到空闲面板 ────────────────────────────────

function autoAssignPane(sessionId) {
  for (let i = 0; i < state.layout.cols; i++) {
    if (!state.layout.panes[i]) {
      setPaneAgent(i, sessionId);
      // 移动端：同步刷新 Tab 栏
      import('./layout.js').then(({ renderMobileTabs }) => renderMobileTabs());
      return;
    }
  }
}

// ─── 单事件处理 ──────────────────────────────────────────────

export function handleSingleEvent(sessionId, event) {
  const agent = getAgent(sessionId);
  if (!agent) return;

  // 更新已知最大 seq
  if (event.seq !== undefined && event.seq > agent.maxSeq) {
    agent.maxSeq = event.seq;
  }

  // 基于 seq 去重：如果此 seq 已在 messages 中，跳过
  if (event.seq !== undefined && agent.messages.some(m => m.seq === event.seq)) {
    return;
  }

  // 分流：BaseMessage 格式（role 字段） vs 旧 AgentEvent 格式（type 字段）
  if (event.role !== undefined) {
    handleBaseMessage(agent, event);
  } else {
    handleLegacyEvent(agent, event);
  }
}

// ─── 从 content 字段提取纯文本（支持 string 和 ContentBlock[] 两种格式）────

function extractText(content) {
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) {
    return content
      .filter(b => b.type === 'text')
      .map(b => b.text || '')
      .join('');
  }
  return '';
}

// ─── BaseMessage 格式处理 ────────────────────────────────────

export function handleBaseMessage(agent, event) {
  const text = extractText(event.content);
  const toolCalls = event.tool_calls || [];

  switch (event.role) {
    case 'user':
      agent.messages.push({ type: 'user', text, seq: event.seq });
      break;

    case 'assistant':
      if (toolCalls.length > 0) {
        toolCalls.forEach(tc => {
          agent.messages.push({
            type: 'tool',
            name: tc.name,
            tool_call_id: tc.id,
            input: tc.arguments,
            output: null,
            streaming: false,
          });
        });
      }
      if (text || !agent.messages.length) {
        agent.messages.push({ type: 'assistant', text, streaming: false, id: event.id });
      }
      break;

    case 'tool': {
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
    }

    case 'system':
      // system 消息暂不显示
      break;
  }
}

// ─── 旧 AgentEvent 格式处理 ─────────────────────────────────

export function handleLegacyEvent(agent, event) {
  const eventType = event.type;

  switch (eventType) {
    case 'user_message':
      agent.messages.push({ type: 'user', text: event.text || '' });
      break;

    case 'text_chunk': {
      // 如果已有同 message_id 的完整 BaseMessage，跳过（避免重复）
      const msgId = event.message_id;
      if (msgId && agent.messages.some(m => m.id === msgId)) break;
      agent.messages.push({ type: 'assistant', text: event.chunk || event['0'] || '' });
      break;
    }

    case 'tool_start': {
      // BaseMessage 路径已经从 tool_calls 创建了条目，避免重复
      const alreadyExists = agent.messages.some(
        m => m.type === 'tool' && m.name === event.name && m.output === null
      );
      if (!alreadyExists) {
        agent.messages.push({
          type: 'tool',
          name: event.name,
          input: event.input,
          output: null,
          streaming: false,
        });
      }
      break;
    }

    case 'tool_end':
      for (let i = agent.messages.length - 1; i >= 0; i--) {
        const m = agent.messages[i];
        if (m.type === 'tool' && m.name === event.name && !m.output) {
          m.output = event.output;
          m.isError = event.is_error;
          break;
        }
      }
      break;

    case 'tool_call':
      agent.messages.push({
        type: 'tool',
        name: event.name,
        input: event.args,
        output: null,
        streaming: false,
      });
      break;

    case 'assistant_chunk': {
      const chunkText = event.chunk || event['0'] || '';
      const last = agent.messages[agent.messages.length - 1];
      if (last && last.type === 'assistant' && last.streaming) {
        last.text += chunkText;
      } else {
        agent.messages.push({ type: 'assistant', text: chunkText, streaming: true });
      }
      break;
    }

    case 'done': {
      const lastMsg = agent.messages[agent.messages.length - 1];
      if (lastMsg) {
        lastMsg.streaming = false;
        lastMsg.isStreamingDone = true;
      }
      break;
    }

    case 'llm_call_start':
    case 'llm_call_end':
      // 内部调试事件，不展示
      break;

    case 'agent_running':
      agent.isRunning = true;
      break;

    case 'agent_done':
      agent.isRunning = false;
      break;

    case 'error':
      agent.isRunning = false;
      agent.messages.push({ type: 'error', text: event['0'] || 'Error' });
      break;

    case 'todo_update':
      agent.todos = event.items || [];
      break;

    case 'approval_needed':
      agent.pendingHitl = {
        requests: (event.items || []).map(i => ({
          name: i.tool_name,
          input: i.input,
          tool_call_id: i.tool_call_id,
        })),
      };
      renderPaneForAllPanes();
      break;

    case 'ask_user_batch':
      agent.pendingAskUser = { questions: event.questions || [] };
      renderPaneForAllPanes();
      break;

    case 'approval_resolved':
      // HITL 已解决，清除弹窗状态
      if (agent.pendingHitl) {
        agent.pendingHitl = null;
        // 关闭 HITL 弹窗（如果存在）
        closeDialog('hitl');
        renderPaneForAllPanes();
      }
      break;

    case 'ask_user_resolved':
      // AskUser 已解决，清除弹窗状态
      if (agent.pendingAskUser) {
        agent.pendingAskUser = null;
        closeDialog('askuser');
        renderPaneForAllPanes();
      }
      break;
  }
}

// ─── AgentEvent 主入口 ──────────────────────────────────────

export function handleAgentEvent(sessionId, msg) {
  const agent = getAgent(sessionId);
  if (!agent) return;

  // sync_response：批量回放历史事件
  if (msg.type === 'sync_response') {
    (msg.events || []).forEach(ev => handleSingleEvent(sessionId, ev));
    renderPaneForAllPanes();
    return;
  }

  // 实时单事件
  handleSingleEvent(sessionId, msg);
  renderPaneForAllPanes();
}

// ─── 辅助 ───────────────────────────────────────────────────

// 只更新消息/todo/status 内容，不重建 pane 结构（保持滚动位置）
async function renderPaneForAllPanes() {
  const { renderMessages, renderTodoPanel, renderStatus } = await import('./render.js');
  state.layout.panes.forEach((sessionId, paneIdx) => {
    if (!sessionId) return;
    const agent = getAgent(sessionId);
    if (!agent) return;
    renderMessages(paneIdx, agent);
    renderTodoPanel(paneIdx, agent.todos);
    renderStatus(paneIdx, agent);
  });
}
