// connection.js — WebSocket 连接管理（Signals 版）
import {
  agents,
  layout,
  activeMobilePane,
  upsertAgent,
  getAgent,
  setPaneAgent,
  connectionStatus,
  refreshAgents,
} from './state.js'
import { handleAgentEvent, handleBroadcast } from './events.js'

let managementWs = null
const RECONNECT_DELAY = 3000

function wsUrl(path) {
  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:'
  return `${proto}//${location.host}${path}`
}

// ─── 辅助函数 ───────────────────────────────────────────────

export function sendMessage(sessionId, msg) {
  const agent = getAgent(sessionId)
  if (!agent || !agent.ws || agent.ws.readyState !== WebSocket.OPEN) return
  agent.ws.send(JSON.stringify(msg))
}

// ─── 管理 WS ────────────────────────────────────────────────

export function connectManagement() {
  const token = new URLSearchParams(location.search).get('token') || ''
  const url = wsUrl(`/web/ws?token=${token}`)
  managementWs = new WebSocket(url)

  managementWs.onopen = () => {
    connectionStatus.value = 'connected'
  }

  managementWs.onmessage = (e) => {
    try {
      const msg = JSON.parse(e.data)
      handleBroadcast(msg)
    } catch (err) {
      console.error('[connection] Failed to parse broadcast msg:', err)
    }
  }

  managementWs.onclose = () => {
    connectionStatus.value = 'reconnecting'
    setTimeout(connectManagement, RECONNECT_DELAY)
  }

  managementWs.onerror = () => managementWs.close()
}

// ─── Session WS ──────────────────────────────────────────────

export function connectSession(sessionId) {
  const agent = getAgent(sessionId)
  if (!agent) return
  if (agent.ws && agent.ws.readyState === WebSocket.OPEN) return

  const token = new URLSearchParams(location.search).get('token') || ''
  const url = wsUrl(`/web/ws?token=${token}&session=${sessionId}`)
  const ws = new WebSocket(url)
  agent.ws = ws

  ws.onopen = () => {
    const since = agent.maxSeq || 0
    ws.send(JSON.stringify({ type: 'sync_request', since_seq: since }))
    upsertAgent(sessionId, { status: 'online' })
  }

  ws.onmessage = (e) => {
    try {
      const msg = JSON.parse(e.data)
      handleAgentEvent(sessionId, msg)
    } catch (err) {
      console.error('[connection] Failed to parse agent msg:', err)
    }
  }

  ws.onclose = () => {
    const a = getAgent(sessionId)
    if (a) {
      a.ws = null
    }
    if (agents.value.has(sessionId)) {
      upsertAgent(sessionId, { status: 'offline' })
      setTimeout(() => connectSession(sessionId), RECONNECT_DELAY)
    }
  }

  ws.onerror = () => ws.close()
}

// ─── 分屏布局工具 ──────────────────────────────────────────────

export function setCols(n) {
  if (n < 1 || n > 3) return
  const l = layout.value
  const oldCols = l.cols
  const panes = [...l.panes]

  if (n < oldCols) {
    for (let i = n; i < oldCols; i++) panes[i] = null
    panes.splice(n)
    if (activeMobilePane.value >= n) {
      activeMobilePane.value = 0
    }
  } else if (n > oldCols) {
    for (let i = oldCols; i < n; i++) {
      if (panes.length <= i) panes.push(null)
    }
  }

  layout.value = { cols: n, panes }
}

export function assignAgentToPane(paneIdx, sessionId) {
  if (paneIdx < 0 || paneIdx >= layout.value.cols) return
  setPaneAgent(paneIdx, sessionId)
}

export function initPaneBindings() {
  const agentIds = Array.from(agents.value.keys())
  for (let i = 0; i < Math.min(agentIds.length, layout.value.cols); i++) {
    setPaneAgent(i, agentIds[i])
  }
}
