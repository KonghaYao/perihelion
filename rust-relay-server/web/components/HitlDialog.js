// components/HitlDialog.js — HITL 工具审批弹窗（全局唯一）
import { html } from '../utils/html.js'
import { agents, activePaneSessionId } from '../state.js'
import { sendMessage } from '../connection.js'
import { useSignalValue } from '../utils/hooks.js'

export function HitlDialog() {
  const sessionId = useSignalValue(activePaneSessionId)
  const agentsMap = useSignalValue(agents)
  if (!sessionId) return null

  const agent = agentsMap.get(sessionId)
  if (!agent || !agent.pendingHitl) return null

  const requests = agent.pendingHitl.requests || []

  const onApprove = () => {
    const decisions = requests.map(r => ({
      tool_call_id: r.tool_call_id || r.id || '',
      decision: 'Approve',
    }))
    sendMessage(sessionId, { type: 'hitl_decision', decisions })
    agent.pendingHitl = null
    agents.value = new Map(agents.value)
  }

  const onReject = () => {
    const decisions = requests.map(r => ({
      tool_call_id: r.tool_call_id || r.id || '',
      decision: 'Reject',
    }))
    sendMessage(sessionId, { type: 'hitl_decision', decisions })
    agent.pendingHitl = null
    agents.value = new Map(agents.value)
  }

  const onClose = () => {
    agent.pendingHitl = null
    agents.value = new Map(agents.value)
  }

  return html`
    <div id="hitl-modal" class="modal">
      <div class="modal-overlay" onClick=${onClose} />
      <div class="modal-card">
        <div class="modal-header">
          <h3 class="modal-title">工具审批</h3>
          <button id="hitl-close" class="modal-close-btn" onClick=${onClose}>×</button>
        </div>

        <div id="hitl-items">
          ${requests.map((req, i) => {
            const inputStr =
              typeof req.input === 'string'
                ? req.input
                : JSON.stringify(req.input, null, 2)
            return html`
              <div key=${i} class="hitl-item">
                <div class="tool-info">
                  <span class="tool-name" style="color: var(--accent); font-weight: bold;">
                    ${req.name || 'tool'}
                  </span>
                </div>
                <div class="tool-input">${inputStr}</div>
              </div>
            `
          })}
        </div>

        <div class="modal-actions">
          <button id="hitl-approve-all" class="btn-approve" onClick=${onApprove}>全部批准</button>
          <button id="hitl-reject-all" class="btn-reject" onClick=${onReject}>全部拒绝</button>
        </div>
      </div>
    </div>
  `
}
