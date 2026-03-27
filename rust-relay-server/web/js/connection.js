// connection.js — WebSocket 连接管理
import { state, upsertAgent, getAgent, setPaneAgent } from './state.js';
import { handleAgentEvent } from './events.js';
import { handleBroadcast } from './events.js';
import { renderSidebar, renderLayout } from './render.js';

let managementWs = null;
const RECONNECT_DELAY = 3000;

function wsUrl(path) {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  return `${proto}//${location.host}${path}`;
}

// ─── 辅助函数 ───────────────────────────────────────────────

export function sendMessage(sessionId, msg) {
  const agent = getAgent(sessionId);
  if (!agent || !agent.ws || agent.ws.readyState !== WebSocket.OPEN) return;
  agent.ws.send(JSON.stringify(msg));
}

export function sendBroadcast(msg) {
  if (!managementWs || managementWs.readyState !== WebSocket.OPEN) return;
  managementWs.send(JSON.stringify(msg));
}

// ─── 管理 WS ────────────────────────────────────────────────

export function connectManagement() {
  const token = new URLSearchParams(location.search).get('token') || '';
  const url = wsUrl(`/web/ws?token=${token}`);
  managementWs = new WebSocket(url);

  managementWs.onopen = () => {
    updateConnectionIndicator('connected', '已连接');
  };

  managementWs.onmessage = (e) => {
    try {
      const msg = JSON.parse(e.data);
      handleBroadcast(msg);
    } catch (err) {
      console.error('[connection] Failed to parse broadcast msg:', err);
    }
  };

  managementWs.onclose = () => {
    updateConnectionIndicator('reconnecting', '重连中...');
    setTimeout(connectManagement, RECONNECT_DELAY);
  };

  managementWs.onerror = () => managementWs.close();
}

// ─── Session WS ──────────────────────────────────────────────

export function connectSession(sessionId) {
  const agent = getAgent(sessionId);
  if (!agent) return;
  if (agent.ws && agent.ws.readyState === WebSocket.OPEN) return;

  const token = new URLSearchParams(location.search).get('token') || '';
  const url = wsUrl(`/web/ws?token=${token}&session=${sessionId}`);
  const ws = new WebSocket(url);
  agent.ws = ws;

  ws.onopen = () => {
    // 首次连接 since_seq=0，重连时用已知最大 seq 实现增量 sync
    const since = agent.maxSeq || 0;
    ws.send(JSON.stringify({ type: 'sync_request', since_seq: since }));
    agent.status = 'online';
    upsertAgent(sessionId, { status: 'online' });
    renderSidebar();
    renderLayout();
  };

  ws.onmessage = (e) => {
    try {
      const msg = JSON.parse(e.data);
      handleAgentEvent(sessionId, msg);
    } catch (err) {
      console.error('[connection] Failed to parse agent msg:', err);
    }
  };

  ws.onclose = () => {
    agent.ws = null;
    agent.status = 'offline';
    // 只在 session 仍存在于 map 时才更新状态，避免重建已被 addAgent 迁移删除的旧 session
    if (state.agents.has(sessionId)) {
      upsertAgent(sessionId, { status: 'offline' });
      renderSidebar();
      renderLayout();
      // 重连
      setTimeout(() => connectSession(sessionId), RECONNECT_DELAY);
    }
  };

  ws.onerror = () => ws.close();
}

// ─── 连接状态指示器 ───────────────────────────────────────────

function updateConnectionIndicator(status, text) {
  const dot = document.querySelector('#connection-indicator .status-dot');
  const txt = document.getElementById('connection-text');
  if (!dot) return;
  dot.className = 'status-dot ' + (status === 'connected' ? 'connected' : status === 'reconnecting' ? 'reconnecting' : 'disconnected');
  if (txt) txt.textContent = text;
}

// ─── 初始化分屏绑定 ──────────────────────────────────────────

export function initPaneBindings() {
  // 初始：按 agents 顺序填充前 N 栏
  const agents = Array.from(state.agents.keys());
  for (let i = 0; i < Math.min(agents.length, state.layout.cols); i++) {
    setPaneAgent(i, agents[i]);
  }
}
