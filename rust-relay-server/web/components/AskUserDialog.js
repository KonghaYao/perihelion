// components/AskUserDialog.js — AskUser 问答弹窗（全局唯一）
import { html } from '../utils/html.js'
import { useState } from 'https://esm.sh/preact/hooks'
import { agents, activePaneSessionId } from '../state.js'
import { sendMessage } from '../connection.js'
import { useSignalValue } from '../utils/hooks.js'

export function AskUserDialog() {
  const sessionId = useSignalValue(activePaneSessionId)
  const agentsMap = useSignalValue(agents)
  if (!sessionId) return null

  const agent = agentsMap.get(sessionId)
  if (!agent || !agent.pendingAskUser) return null

  return html`<${AskUserDialogInner} agent=${agent} sessionId=${sessionId} />`
}

function AskUserDialogInner({ agent, sessionId }) {
  const questions = agent.pendingAskUser.questions || []
  const [answers, setAnswers] = useState(() => {
    const init = {}
    questions.forEach((q) => {
      const key = q.tool_call_id
      init[key] = q.multi_select ? [] : ''
    })
    return init
  })
  const [customInputs, setCustomInputs] = useState(() => {
    const init = {}
    questions.forEach((q) => { init[q.tool_call_id] = '' })
    return init
  })

  const onClose = () => {
    agent.pendingAskUser = null
    agents.value = new Map(agents.value)
  }

  const onSubmit = () => {
    // 合并选项答案和自定义输入
    const merged = {}
    questions.forEach((q) => {
      const key = q.tool_call_id
      const custom = (customInputs[key] || '').trim()
      const sel = answers[key]
      if (q.multi_select) {
        merged[key] = custom ? [...(sel || []), custom] : (sel || [])
      } else {
        merged[key] = custom || sel || ''
      }
    })
    sendMessage(sessionId, { type: 'ask_user_response', answers: merged })
    agent.pendingAskUser = null
    agents.value = new Map(agents.value)
  }

  return html`
    <div id="ask-user-modal" class="modal">
      <div class="modal-overlay" onClick=${onClose} />
      <div class="modal-card">
        <div class="modal-header">
          <h3 class="modal-title">Agent 提问</h3>
          <button id="ask-user-close" class="modal-close-btn" onClick=${onClose}>×</button>
        </div>

        <div id="ask-user-items">
          ${questions.map((q) => {
            const key = q.tool_call_id
            const isMulti = !!q.multi_select
            const hasOptions = q.options && q.options.length > 0

            const updateAnswer = (val) => {
              setAnswers(prev => ({ ...prev, [key]: val }))
            }

            const toggleCheckbox = (opt) => {
              setAnswers(prev => {
                const cur = prev[key] || []
                const next = cur.includes(opt)
                  ? cur.filter(v => v !== opt)
                  : [...cur, opt]
                return { ...prev, [key]: next }
              })
            }

            return html`
              <div key=${key} class="ask-user-item">
                ${q.header && html`<span class="ask-user-header-chip">${q.header}</span>`}
                <div class="ask-user-question">${q.question || ''}</div>
                ${hasOptions ? html`
                  <div class="ask-user-options">
                    ${q.options.map((opt, oi) => {
                      const optVal = typeof opt === 'string' ? opt : (opt.label || String(oi))
                      const optLabel = typeof opt === 'string' ? opt : (opt.label || String(oi))
                      const optDesc = typeof opt === 'object' ? opt.description : null
                      if (isMulti) {
                        const checked = (answers[key] || []).includes(optVal)
                        return html`
                          <label key=${oi} class="ask-user-option">
                            <input
                              type="checkbox"
                              value=${optVal}
                              checked=${checked}
                              onChange=${() => toggleCheckbox(optVal)}
                            />
                            <div class="ask-user-option-label-wrap">
                              <span>${optLabel}</span>
                              ${optDesc && html`<small class="ask-user-opt-desc">${optDesc}</small>`}
                            </div>
                          </label>
                        `
                      } else {
                        return html`
                          <label key=${oi} class="ask-user-option">
                            <input
                              type="radio"
                              name=${key}
                              value=${optVal}
                              checked=${answers[key] === optVal}
                              onChange=${() => updateAnswer(optVal)}
                            />
                            <div class="ask-user-option-label-wrap">
                              <span>${optLabel}</span>
                              ${optDesc && html`<small class="ask-user-opt-desc">${optDesc}</small>`}
                            </div>
                          </label>
                        `
                      }
                    })}
                  </div>
                ` : null}
                <input
                  type="text"
                  class="ask-user-text"
                  placeholder=${hasOptions ? '或输入自定义内容...' : '输入回答...'}
                  value=${customInputs[key] || ''}
                  onInput=${(e) => setCustomInputs(prev => ({ ...prev, [key]: e.target.value }))}
                />
              </div>
            `
          })}
        </div>

        <div class="modal-actions">
          <button id="ask-user-submit" class="btn-approve" onClick=${onSubmit}>提交</button>
          <button class="btn-reject" onClick=${onClose}>取消</button>
        </div>
      </div>
    </div>
  `
}
