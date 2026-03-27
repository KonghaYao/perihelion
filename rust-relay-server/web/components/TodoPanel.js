// components/TodoPanel.js — TODO 状态面板
import { html } from '../utils/html.js'
import { useState } from 'https://esm.sh/preact/hooks'

export function TodoPanel({ todos }) {
  const [collapsed, setCollapsed] = useState(false)

  if (!todos || todos.length === 0) return null

  return html`
    <div class="pane-todo">
      <div
        class="todo-header"
        onClick=${() => setCollapsed(!collapsed)}
        style="cursor:pointer;"
      >
        <span>📋 TODO</span>
        <span class="todo-toggle-icon">${collapsed ? '▶' : '▼'}</span>
      </div>
      ${!collapsed ? html`
        <ul class="todo-list">
          ${todos.map((item, i) => {
            const status = item.status || 'pending'
            const title = item.title || item.content || ''
            if (status === 'in_progress') {
              return html`<li key=${i} class="todo-in-progress">→ ${title}</li>`
            } else if (status === 'done' || status === 'completed') {
              return html`<li key=${i} class="todo-done">✓ ${title}</li>`
            } else {
              return html`<li key=${i} class="todo-pending">○ ${title}</li>`
            }
          })}
        </ul>
      ` : null}
    </div>
  `
}
