// state.js — 全局 Preact Signals 状态
import { signal, computed } from 'https://esm.sh/@preact/signals'

// ─── 核心 Signals ─────────────────────────────────────────────

/** sessionId → agent 对象
 * agent: { name, status, messages[], todos[], ws, pendingHitl, pendingAskUser, maxSeq, isRunning }
 */
export const agents = signal(new Map())

/** 分屏布局状态 */
export const layout = signal({
  cols: 1,
  panes: [null, null, null],
})

/** 当前激活的 pane index */
export const activePane = signal(0)

/** 移动端当前激活面板序号 */
export const activeMobilePane = signal(0)

/** WebSocket 连接状态 */
export const connectionStatus = signal('disconnected') // 'connected' | 'reconnecting' | 'disconnected'

/** marked/hljs/DOMPurify CDN 加载完毕标记 */
export const markedReady = signal(false)

// ─── 派生计算 ─────────────────────────────────────────────────

/** 当前激活面板绑定的 sessionId */
export const activePaneSessionId = computed(
  () => layout.value.panes[activePane.value] ?? null
)

// ─── 辅助函数 ─────────────────────────────────────────────────

/**
 * 注册或更新 agent（触发 signals 依赖追踪）
 */
export function upsertAgent(sessionId, data) {
  const map = agents.value
  if (map.has(sessionId)) {
    const existing = map.get(sessionId)
    map.set(sessionId, {
      ...existing,
      ...data,
      messages: data.messages ?? existing.messages,
      todos: data.todos ?? existing.todos,
      maxSeq: data.maxSeq ?? existing.maxSeq ?? 0,
    })
  } else {
    map.set(sessionId, {
      name: data.name || sessionId.slice(0, 8),
      status: data.status || 'offline',
      messages: data.messages || [],
      todos: data.todos || [],
      ws: data.ws || null,
      pendingHitl: data.pendingHitl || null,
      pendingAskUser: data.pendingAskUser || null,
      maxSeq: data.maxSeq || 0,
      isRunning: data.isRunning ?? false,
    })
  }
  // 替换引用触发 Signals 依赖追踪
  agents.value = new Map(map)
}

export function getAgent(sessionId) {
  return agents.value.get(sessionId) || null
}

export function removeAgent(sessionId) {
  const map = new Map(agents.value)
  map.delete(sessionId)
  agents.value = map
}

export function setPaneAgent(paneIdx, sessionId) {
  if (paneIdx < 0 || paneIdx > 2) return
  const l = { ...layout.value, panes: [...layout.value.panes] }
  l.panes[paneIdx] = sessionId
  layout.value = l
}

export function clearPane(paneIdx) {
  if (paneIdx < 0 || paneIdx > 2) return
  const l = { ...layout.value, panes: [...layout.value.panes] }
  l.panes[paneIdx] = null
  layout.value = l
}

/**
 * 按 id 去重地将消息写入 agent.messages。
 * 注意：调用后需手动触发 agents signal 刷新（agents.value = new Map(agents.value)）
 */
export function upsertMessage(agent, msg) {
  if (msg.id) {
    const idx = agent.messages.findIndex(m => m.id === msg.id)
    if (idx !== -1) {
      agent.messages[idx] = { ...agent.messages[idx], ...msg }
      return
    }
  }
  agent.messages.push(msg)
}

/** 刷新 agents signal（强制触发所有依赖组件重渲染） */
export function refreshAgents() {
  agents.value = new Map(agents.value)
}
