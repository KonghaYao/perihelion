// components/Sidebar.js — 左侧 Agent 列表侧边栏
import { html } from '../utils/html.js'
import { agents, connectionStatus, activePane } from '../state.js'
import { assignAgentToPane } from '../connection.js'
import { useSignalValue } from '../utils/hooks.js'

export function Sidebar({ mobileSidebarOpen, onCloseMobile }) {
  const agentsMap = useSignalValue(agents)
  const connStatus = useSignalValue(connectionStatus)
  const agentList = [...agentsMap]

  const statusClass = connStatus === 'connected'
    ? 'connected'
    : connStatus === 'reconnecting'
    ? 'reconnecting'
    : 'disconnected'

  const statusText = connStatus === 'connected'
    ? '已连接'
    : connStatus === 'reconnecting'
    ? '重连中...'
    : '断线'

  const sidebarClass = 'sidebar' + (mobileSidebarOpen ? ' mobile-visible' : '')

  return html`
    <aside id="sidebar" class=${sidebarClass}>
      <div class="sidebar-header">
        <div class="text-sm font-bold" style="color: var(--accent)">在线 Agent</div>
      </div>

      <div id="agent-list" class="flex-1 overflow-y-auto">
        ${agentList.map(([sid, agent]) => {
          const dotClass = 'dot ' + (agent.status === 'online' ? 'dot-online' : 'dot-offline')
          const hasNotification = agent.pendingHitl || agent.pendingAskUser

          return html`
            <div
              key=${sid}
              class="agent-item"
              onClick=${() => {
                assignAgentToPane(activePane.value, sid)
                onCloseMobile && onCloseMobile()
              }}
            >
              <span class=${dotClass} />
              <span class="agent-name">${agent.name}</span>
              ${hasNotification ? html`<span class="badge">🔔</span>` : null}
            </div>
          `
        })}
      </div>

      <div id="sidebar-footer">
        <div id="connection-indicator">
          <span class=${'status-dot ' + statusClass} />
          <span id="connection-text">${statusText}</span>
        </div>
      </div>
    </aside>
  `
}
