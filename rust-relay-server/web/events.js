// events.js — 事件解析层（Signals 版）
import {
  agents,
  layout,
  activeMobilePane,
  upsertAgent,
  getAgent,
  setPaneAgent,
  upsertMessage,
  refreshAgents,
} from './state.js'
import { connectSession } from './connection.js'

// ─── 广播消息处理 ─────────────────────────────────────────────

export function handleBroadcast(msg) {
  switch (msg.type) {
    case 'agents_list':
      ;(msg.agents || []).forEach(a => addAgent(a.session_id, a.name, 'online'))
      break
    case 'agent_online':
      addAgent(msg.session_id, msg.name, 'online')
      break
    case 'agent_offline':
      if (agents.value.has(msg.session_id)) {
        upsertAgent(msg.session_id, { status: 'offline' })
        // agents signal 已在 upsertAgent 中刷新，Preact 自动重渲染
      }
      break
  }
}

// ─── Agent 注册（同名重连合并）────────────────────────────────

export function addAgent(sessionId, name, status) {
  const displayName = name || sessionId.slice(0, 8)
  const map = agents.value

  // 查找同名旧 session（断线重连场景）
  let existingId = null
  if (name) {
    for (const [id, a] of map) {
      if (a.name === name && id !== sessionId) {
        existingId = id
        break
      }
    }
  }

  if (existingId) {
    // 同名 Agent 重连：迁移数据
    const old = map.get(existingId)
    if (old.ws) old.ws.close()
    const newMap = new Map(map)
    newMap.delete(existingId)
    newMap.set(sessionId, {
      name: displayName,
      status,
      messages: old.messages,
      todos: old.todos,
      ws: null,
      pendingHitl: old.pendingHitl,
      pendingAskUser: old.pendingAskUser,
      maxSeq: old.maxSeq,
      isRunning: false,
    })
    agents.value = newMap

    // 迁移面板绑定
    const l = layout.value
    const panes = [...l.panes]
    let changed = false
    for (let i = 0; i < panes.length; i++) {
      if (panes[i] === existingId) {
        panes[i] = sessionId
        changed = true
      }
    }
    if (changed) layout.value = { ...l, panes }

    if (!layout.value.panes.includes(sessionId)) {
      autoAssignPane(sessionId)
    }
    connectSession(sessionId)
  } else if (!map.has(sessionId)) {
    upsertAgent(sessionId, { name: displayName, status })
    autoAssignPane(sessionId)
    connectSession(sessionId)
  } else {
    upsertAgent(sessionId, { status })
  }
  // agents signal 已在 upsertAgent / 直接赋值中刷新
}

// ─── 自动分配 Agent 到空闲面板 ────────────────────────────────

function autoAssignPane(sessionId) {
  const l = layout.value
  for (let i = 0; i < l.cols; i++) {
    if (!l.panes[i]) {
      setPaneAgent(i, sessionId)
      return
    }
  }
}

// ─── 单事件处理 ──────────────────────────────────────────────

export function handleSingleEvent(sessionId, event) {
  const agent = getAgent(sessionId)
  if (!agent) return

  if (event.seq !== undefined && event.seq > agent.maxSeq) {
    agent.maxSeq = event.seq
  }

  if (event.seq !== undefined && agent.messages.some(m => m.seq === event.seq)) {
    return
  }

  if (event.role !== undefined) {
    handleBaseMessage(agent, event)
  } else {
    handleLegacyEvent(agent, event, sessionId)
  }
}

// ─── 从 content 字段提取纯文本 ───────────────────────────────

function extractText(content) {
  if (typeof content === 'string') return content
  if (Array.isArray(content)) {
    return content
      .filter(b => b.type === 'text')
      .map(b => b.text || '')
      .join('')
  }
  return ''
}

// ─── BaseMessage 格式处理 ────────────────────────────────────

export function handleBaseMessage(agent, event) {
  const text = extractText(event.content)
  const toolCalls = event.tool_calls || []

  switch (event.role) {
    case 'user':
      upsertMessage(agent, { type: 'user', text, id: event.id, seq: event.seq })
      break

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
          })
        })
      }
      if (text || !agent.messages.length) {
        upsertMessage(agent, { type: 'assistant', text, streaming: false, id: event.id })
      }
      break

    case 'tool': {
      const tcId = event.tool_call_id
      if (tcId) {
        for (let i = agent.messages.length - 1; i >= 0; i--) {
          const m = agent.messages[i]
          if (m.type === 'tool' && m.tool_call_id === tcId) {
            m.output = text
            m.isError = event.is_error || false
            break
          }
        }
      }
      break
    }

    case 'system':
      break
  }
}

// ─── 旧 AgentEvent 格式处理 ─────────────────────────────────

export function handleLegacyEvent(agent, event, sessionId) {
  const eventType = event.type

  switch (eventType) {
    case 'user_message':
      agent.messages.push({ type: 'user', text: event.text || '' })
      break

    case 'text_chunk': {
      const msgId = event.message_id
      if (msgId && agent.messages.some(m => m.id === msgId)) break
      agent.messages.push({ type: 'assistant', text: event.chunk || event['0'] || '' })
      break
    }

    case 'tool_start': {
      const alreadyExists = agent.messages.some(
        m => m.type === 'tool' && m.name === event.name && m.output === null
      )
      if (!alreadyExists) {
        agent.messages.push({
          type: 'tool',
          name: event.name,
          input: event.input,
          output: null,
          streaming: false,
        })
      }
      break
    }

    case 'tool_end':
      for (let i = agent.messages.length - 1; i >= 0; i--) {
        const m = agent.messages[i]
        if (m.type === 'tool' && m.name === event.name && !m.output) {
          m.output = event.output
          m.isError = event.is_error
          break
        }
      }
      break

    case 'tool_call':
      agent.messages.push({
        type: 'tool',
        name: event.name,
        input: event.args,
        output: null,
        streaming: false,
      })
      break

    case 'assistant_chunk': {
      const chunkText = event.chunk || event['0'] || ''
      const last = agent.messages[agent.messages.length - 1]
      if (last && last.type === 'assistant' && last.streaming) {
        last.text += chunkText
      } else {
        agent.messages.push({ type: 'assistant', text: chunkText, streaming: true })
      }
      break
    }

    case 'done': {
      const lastMsg = agent.messages[agent.messages.length - 1]
      if (lastMsg) {
        lastMsg.streaming = false
        lastMsg.isStreamingDone = true
      }
      break
    }

    case 'llm_call_start':
    case 'llm_call_end':
      break

    case 'agent_running':
      agent.isRunning = true
      agents.value = new Map(agents.value)
      break

    case 'agent_done':
      agent.isRunning = false
      agents.value = new Map(agents.value)
      break

    case 'error':
      agent.isRunning = false
      agent.messages.push({ type: 'error', text: event['0'] || 'Error' })
      agents.value = new Map(agents.value)
      break

    case 'todo_update':
      agent.todos = event.items || []
      agents.value = new Map(agents.value)
      break

    case 'interaction_request':
      if (event.ctx_type === 'approval') {
        agent.pendingHitl = {
          requests: (event.items || []).map(i => ({
            name: i.tool_name,
            input: i.input,
            tool_call_id: i.tool_call_id,
          })),
        }
      } else if (event.ctx_type === 'questions') {
        agent.pendingAskUser = { questions: event.questions || [] }
      }
      // 弹窗组件自动读取 pendingHitl / pendingAskUser 状态响应
      agents.value = new Map(agents.value)
      break

    case 'interaction_resolved':
      agent.pendingHitl = null
      agent.pendingAskUser = null
      agents.value = new Map(agents.value)
      break

    case 'thread_reset': {
      agent.messages = []
      ;(event.messages || []).forEach(m => handleBaseMessage(agent, m))
      agents.value = new Map(agents.value)
      break
    }

    case 'compact_done': {
      agent.messages = []
      agent.messages.push({ type: 'system', text: '📦 上下文已从旧对话压缩' })
      if (event.summary) {
        agent.messages.push({ type: 'assistant', text: event.summary })
      }
      agents.value = new Map(agents.value)
      break
    }
  }
}

// ─── AgentEvent 主入口 ──────────────────────────────────────

export function handleAgentEvent(sessionId, msg) {
  const agent = getAgent(sessionId)
  if (!agent) return

  if (msg.type === 'sync_response') {
    ;(msg.events || []).forEach(ev => handleSingleEvent(sessionId, ev))
    agents.value = new Map(agents.value)
    return
  }

  handleSingleEvent(sessionId, msg)
  agents.value = new Map(agents.value)
}
