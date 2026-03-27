// components/Pane.js — 单面板
import { html } from '../utils/html.js'
import { useState } from 'https://esm.sh/preact/hooks'
import { agents } from '../state.js'
import { sendMessage, assignAgentToPane } from '../connection.js'
import { MessageList } from './MessageList.js'
import { TodoPanel } from './TodoPanel.js'
import { useSignalValue } from '../utils/hooks.js'

export function Pane({ paneId, sessionId }) {
  const agentsMap = useSignalValue(agents)
  const agent = sessionId ? agentsMap.get(sessionId) : null

  if (!agent) {
    return html`<${EmptyPane} paneId=${paneId} />`
  }

  return html`
    <div class="pane-content">
      <${TodoPanel} todos=${agent.todos} />
      <${MessageList}
        messages=${agent.messages}
        paneId=${paneId}
        isRunning=${agent.isRunning}
        sessionId=${sessionId}
      />
      <${InputBar} paneId=${paneId} sessionId=${sessionId} agent=${agent} />
    </div>
  `
}

// ─── 输入栏 ────────────────────────────────────────────────────

function InputBar({ paneId, sessionId, agent }) {
  const [value, setValue] = useState('')

  const doSend = () => {
    const text = value.trim()
    if (!text) return

    if (text === '/clear') {
      sendMessage(sessionId, { type: 'clear_thread' })
      agent.messages = []
      agent.todos = []
      agent.maxSeq = 0
      agents.value = new Map(agents.value)
    } else if (text === '/compact') {
      sendMessage(sessionId, { type: 'compact_thread' })
    } else {
      sendMessage(sessionId, { type: 'user_input', text })
    }

    setValue('')
  }

  const onKeyDown = (e) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault()
      doSend()
    }
  }

  return html`
    <div class="pane-input">
      <input
        type="text"
        id=${'input-' + paneId}
        placeholder="输入消息..."
        autocomplete="off"
        value=${value}
        onInput=${(e) => setValue(e.target.value)}
        onKeyDown=${onKeyDown}
      />
      <button class="send-btn" data-pane=${paneId} onClick=${doSend}>发送</button>
    </div>
  `
}

// ─── 空面板占位 ────────────────────────────────────────────────

function EmptyPane({ paneId }) {
  const agentsMap = useSignalValue(agents)
  const agentList = [...agentsMap]

  return html`
    <div class="pane-empty">
      <span style="color: var(--text-muted); font-size: 13px;">此栏未分配 Agent</span>
      <select onChange=${(e) => {
        if (e.target.value) assignAgentToPane(paneId, e.target.value)
      }}>
        <option value="">— 选择 Agent —</option>
        ${agentList.map(([sid, agent]) => html`
          <option key=${sid} value=${sid}>
            ${agent.name} ${agent.status === 'online' ? '🟢' : '⚪'}
          </option>
        `)}
      </select>
    </div>
  `
}
