// components/PaneContainer.js — 分屏容器
import { html } from '../utils/html.js'
import { layout, activeMobilePane } from '../state.js'
import { setCols } from '../connection.js'
import { Pane } from './Pane.js'
import { useSignalValue } from '../utils/hooks.js'

export function PaneContainer() {
  const layoutVal = useSignalValue(layout)
  const mobilePane = useSignalValue(activeMobilePane)
  const { cols, panes } = layoutVal
  const isMobile = window.matchMedia('(max-width: 768px)').matches

  // 移动端：只渲染 activeMobilePane 对应的单面板
  if (isMobile) {
    const sessionId = panes[mobilePane] ?? panes[0]
    return html`
      <div style="display:flex;flex-direction:column;flex:1;overflow:hidden;">
        <!-- 移动端 Tab 栏（多面板时显示） -->
        ${renderMobileTabs(cols, panes, mobilePane)}
        <div id="pane-container">
          <div id="pane-0" class="pane">
            <${Pane} paneId=${0} sessionId=${sessionId} />
          </div>
        </div>
      </div>
    `
  }

  // 桌面端：渲染 1~3 列
  const paneEls = []
  for (let i = 0; i < cols; i++) {
    if (i > 0) {
      paneEls.push(html`<div key=${'div-' + i} class="pane-divider" />`)
    }
    paneEls.push(html`
      <div key=${'pane-' + i} id=${'pane-' + i} class="pane">
        <${Pane} paneId=${i} sessionId=${panes[i]} />
      </div>
    `)
  }

  return html`
    <div style="display:flex;flex-direction:column;flex:1;overflow:hidden;">
      <!-- 分屏工具栏 -->
      <div id="layout-toolbar">
        <button
          id="btn-cols-1"
          class=${'layout-btn' + (cols === 1 ? ' active' : '')}
          title="单栏"
          onClick=${() => setCols(1)}
        >1</button>
        <button
          id="btn-cols-2"
          class=${'layout-btn' + (cols === 2 ? ' active' : '')}
          title="双栏"
          onClick=${() => setCols(2)}
        >2</button>
        <button
          id="btn-cols-3"
          class=${'layout-btn' + (cols === 3 ? ' active' : '')}
          title="三栏"
          onClick=${() => setCols(3)}
        >3</button>
      </div>
      <div id="pane-container">
        ${paneEls}
      </div>
    </div>
  `
}

function renderMobileTabs(cols, panes, mobilePane) {
  const boundPanes = panes
    .map((sid, idx) => ({ sid, idx }))
    .filter(({ sid }) => sid)

  if (boundPanes.length <= 1) return null

  return html`
    <div id="mobile-tabs" class="has-tabs">
      ${boundPanes.map(({ sid, idx }) => html`
        <button
          key=${idx}
          class=${'mobile-tab' + (idx === mobilePane ? ' active' : '')}
          onClick=${() => { activeMobilePane.value = idx }}
        >${sid ? sid.slice(0, 8) : '?'}</button>
      `)}
    </div>
  `
}
