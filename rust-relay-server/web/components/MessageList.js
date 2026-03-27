// components/MessageList.js — 消息列表渲染
import { h } from 'https://esm.sh/preact'
import { useEffect, useRef, useState } from 'https://esm.sh/preact/hooks'
import { html } from '../utils/html.js'
import { markedReady } from '../state.js'
import { sendMessage } from '../connection.js'
import { useSignalValue } from '../utils/hooks.js'

// ─── XSS 安全转义 ──────────────────────────────────────────────

function escHtml(str) {
  if (str === null || str === undefined) return ''
  const div = document.createElement('div')
  div.textContent = String(str)
  return div.innerHTML
}

function safeMarkdown(text) {
  if (!markedReady.value || typeof window.marked === 'undefined') {
    return escHtml(text)
  }
  const rawHtml = window.marked.parse(text || '')
  if (typeof window.DOMPurify !== 'undefined') {
    return window.DOMPurify.sanitize(rawHtml, { USE_PROFILES: { html: true } })
  }
  return rawHtml
}

// ─── 工具卡片 ──────────────────────────────────────────────────

function ToolCard({ msg, paneId }) {
  const [expanded, setExpanded] = useState(false)
  const [outputExpanded, setOutputExpanded] = useState(false)

  const inputJson =
    typeof msg.input === 'string'
      ? msg.input
      : JSON.stringify(msg.input, null, 2)

  const outputStr =
    typeof msg.output === 'string'
      ? msg.output
      : msg.output != null
      ? JSON.stringify(msg.output, null, 2)
      : ''

  const outputLines = (outputStr || '').split('\n').length
  const isCollapsible = outputLines > 20

  return html`
    <div class="message tool-card">
      <div
        class="tool-header"
        onClick=${() => setExpanded(!expanded)}
        style="cursor:pointer;"
      >
        <span class="tool-name">🔧 ${msg.name || 'tool'}</span>
        <span class="tool-toggle">${expanded ? '▼ 折叠' : '▶ 展开'}</span>
      </div>
      ${expanded ? html`
        <div class="tool-body">
          <div class="tool-section">
            <div class="tool-section-label">INPUT</div>
            <div class="tool-input">${inputJson}</div>
          </div>
          ${outputStr ? html`
            <div class="tool-section">
              <div class="tool-section-label">OUTPUT</div>
              <div class=${'tool-output' + (msg.isError ? ' tool-error' : '') + (isCollapsible && !outputExpanded ? ' tool-output-collapsed' : '')}>
                ${outputStr}
              </div>
              ${isCollapsible && !outputExpanded ? html`
                <button class="expand-btn" onClick=${(e) => { e.stopPropagation(); setOutputExpanded(true) }}>
                  ▶ 展开全部
                </button>
              ` : null}
            </div>
          ` : null}
        </div>
      ` : null}
    </div>
  `
}

// ─── 消息气泡 ──────────────────────────────────────────────────

function MessageBubble({ msg, paneId }) {
  const mdReady = markedReady.value

  switch (msg.type) {
    case 'user':
      return html`
        <div class="message msg-user">${msg.text}</div>
      `

    case 'assistant': {
      const content = mdReady
        ? safeMarkdown(msg.text || '')
        : (msg.text || '')

      if (mdReady) {
        return html`
          <div class="message msg-assistant">
            <div
              class="md-content"
              dangerouslySetInnerHTML=${{ __html: content }}
            />
            ${msg.streaming && !msg.isStreamingDone
              ? html`<span class="cursor-blink">｜</span>` : null}
          </div>
        `
      }

      return html`
        <div class="message msg-assistant">
          <div class="md-content">${content}</div>
          ${msg.streaming && !msg.isStreamingDone
            ? html`<span class="cursor-blink">｜</span>` : null}
        </div>
      `
    }

    case 'tool':
      return html`<${ToolCard} msg=${msg} paneId=${paneId} />`

    case 'error':
      return html`<div class="message msg-error">${msg.text}</div>`

    default:
      return html`<div class="message">${JSON.stringify(msg)}</div>`
  }
}

// ─── 消息列表 ──────────────────────────────────────────────────

export function MessageList({ messages, paneId, isRunning, sessionId }) {
  const containerRef = useRef(null)
  const wasAtBottomRef = useRef(true)
  // 订阅 markedReady，确保 marked.js 加载后重新渲染
  useSignalValue(markedReady)

  // 更新前记录是否在底部
  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    wasAtBottomRef.current =
      el.scrollTop + el.clientHeight >= el.scrollHeight - 50
  })

  // 更新后若原本在底部则自动滚动
  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    if (wasAtBottomRef.current) {
      el.scrollTop = el.scrollHeight
    }
  })

  return html`
    <div id=${'messages-' + paneId} class="messages" ref=${containerRef}>
      ${messages.map((msg, i) => html`
        <${MessageBubble} key=${msg.id || i} msg=${msg} paneId=${paneId} />
      `)}

      ${isRunning ? html`
        <div class="message msg-loading">
          <div class="loading-dots">
            <span /><span /><span />
          </div>
          <button class="stop-btn" onClick=${() => {
            sendMessage(sessionId, { type: 'cancel_agent' })
          }}>■ 停止</button>
        </div>
      ` : null}
    </div>
  `
}
