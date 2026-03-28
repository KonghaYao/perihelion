// components/App.js — 根布局组件
import { h } from 'https://esm.sh/preact'
import { useEffect, useState } from 'https://esm.sh/preact/hooks'
import { html } from '../utils/html.js'
import { layout, activeMobilePane } from '../state.js'
import { connectManagement, getUserId } from '../connection.js'
import { Sidebar } from './Sidebar.js'
import { PaneContainer } from './PaneContainer.js'
import { HitlDialog } from './HitlDialog.js'
import { AskUserDialog } from './AskUserDialog.js'

export function App() {
  const token = new URLSearchParams(location.search).get('token') || ''
  const userId = getUserId()
  const [mobileSidebarOpen, setMobileSidebarOpen] = useState(false)

  useEffect(() => {
    if (!token || !userId) return
    connectManagement()
  }, [])

  if (!token) {
    return html`
      <div style="display:flex;align-items:center;justify-content:center;height:100vh;color:var(--text-muted);font-size:14px;">
        请在 URL 中提供 token 参数，如 ?token=your-token
      </div>
    `
  }

  if (!userId) {
    return html`
      <div style="display:flex;align-items:center;justify-content:center;height:100vh;color:var(--text-muted);font-size:14px;text-align:center;padding:20px;">
        请从 TUI 的 /relay 面板复制完整的接入 URL（包含 #user_id=...）
      </div>
    `
  }

  const isMobile = window.matchMedia('(max-width: 768px)').matches

  return html`
    <div id="app-root" style="display:flex;height:100dvh;width:100vw;overflow:hidden;">

      <!-- 移动端遮罩层 -->
      ${mobileSidebarOpen ? html`
        <div
          id="mobile-overlay"
          class="visible"
          onClick=${() => setMobileSidebarOpen(false)}
        />
      ` : html`<div id="mobile-overlay" />`}

      <!-- 侧边栏 -->
      <${Sidebar}
        mobileSidebarOpen=${mobileSidebarOpen}
        onCloseMobile=${() => setMobileSidebarOpen(false)}
      />

      <!-- 右侧主内容区 -->
      <main id="main-content">
        <!-- 移动端顶部导航栏 -->
        ${isMobile ? html`
          <div id="mobile-topbar">
            <button
              id="hamburger-btn"
              aria-label="打开 Agent 列表"
              onClick=${() => setMobileSidebarOpen(true)}
            >☰</button>
            <span id="mobile-title">Agent Remote Control</span>
          </div>
        ` : null}

        <!-- 分屏容器 -->
        <${PaneContainer} />
      </main>

      <!-- 全局弹窗（挂载在根层，不随面板重建） -->
      <${HitlDialog} />
      <${AskUserDialog} />

    </div>
  `
}
