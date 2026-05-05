// ─── acpx-g Runs Page (Nexus) ──────────────────────────────────

function renderRunsPage() {
  return `
  <div class="runs-page stagger">
    <div class="config-page-header">
      <h1 class="config-page-title">运行记录</h1>
      <div class="config-actions">
        <button class="btn btn-sm btn-ghost" id="btnRefreshRuns">
          <i data-lucide="refresh-cw" style="width:14px;height:14px"></i> 刷新
        </button>
      </div>
    </div>

    <div class="config-toolbar">
      <div class="config-search">
        <i data-lucide="search" style="width:14px;height:14px;color:var(--text-dim);flex-shrink:0"></i>
        <input id="runsSearchInput" placeholder="搜索工作流...">
      </div>
      <div style="display:flex;gap:4px;" id="statusFilters">
        <button class="config-filter-btn active" data-status="all">全部</button>
        <button class="config-filter-btn" data-status="running">运行中</button>
        <button class="config-filter-btn" data-status="success">成功</button>
        <button class="config-filter-btn" data-status="failed">失败</button>
      </div>
      <div class="view-toggle" id="viewToggle">
        <button class="view-toggle-btn active" data-view="table">
          <i data-lucide="table" style="width:14px;height:14px"></i>
        </button>
        <button class="view-toggle-btn" data-view="cards">
          <i data-lucide="layout-grid" style="width:14px;height:14px"></i>
        </button>
      </div>
    </div>

    <div id="runsContent">
      <div class="skeleton skeleton-row"></div>
      <div class="skeleton skeleton-row"></div>
      <div class="skeleton skeleton-row"></div>
      <div class="skeleton skeleton-row"></div>
      <div class="skeleton skeleton-row"></div>
    </div>

    <div id="runsPagination" class="pagination"></div>
  </div>`;
}

let runsState = {
  runs: [],
  page: 1,
  perPage: 20,
  total: 0,
  view: 'table',
  statusFilter: 'all',
  searchQuery: '',
};

function initRuns() {
  document.getElementById('btnRefreshRuns')?.addEventListener('click', () => loadRuns(1));

  document.getElementById('runsSearchInput')?.addEventListener('input', function() {
    runsState.searchQuery = this.value.toLowerCase();
    filterAndRenderRuns();
  });

  document.querySelectorAll('#statusFilters .config-filter-btn').forEach(btn => {
    btn.addEventListener('click', function() {
      document.querySelectorAll('#statusFilters .config-filter-btn').forEach(b => b.classList.remove('active'));
      this.classList.add('active');
      runsState.statusFilter = this.dataset.status;
      filterAndRenderRuns();
    });
  });

  document.querySelectorAll('#viewToggle .view-toggle-btn').forEach(btn => {
    btn.addEventListener('click', function() {
      document.querySelectorAll('#viewToggle .view-toggle-btn').forEach(b => b.classList.remove('active'));
      this.classList.add('active');
      runsState.view = this.dataset.view;
      renderRunsContent();
    });
  });

  loadRuns(1);
}

async function loadRuns(page) {
  runsState.page = page;
  const content = document.getElementById('runsContent');
  if (!content) return;

  content.innerHTML = Array(5).fill('<div class="skeleton skeleton-row"></div>').join('');

  try {
    const data = await api(`${API_WF}?page=${page}&per_page=${runsState.perPage}`);
    runsState.runs = data.runs || [];
    runsState.total = data.total || 0;
    renderRunsContent();
    renderRunsPagination();
    updateSidebarStatus();
  } catch (e) {
    content.innerHTML = `
      <div class="empty-state">
        <div class="empty-state-icon"><i data-lucide="alert-triangle" style="width:28px;height:28px"></i></div>
        <div class="empty-state-title">加载失败</div>
        <div class="empty-state-desc">${escapeHtml(e.message)}</div>
      </div>`;
    lucide.createIcons({ nodes: [content] });
  }
}

function getFilteredRuns() {
  let runs = runsState.runs;
  if (runsState.statusFilter !== 'all') {
    runs = runs.filter(r => r.status === runsState.statusFilter);
  }
  if (runsState.searchQuery) {
    runs = runs.filter(r => (r.workflow_name || '').toLowerCase().includes(runsState.searchQuery));
  }
  return runs;
}

function filterAndRenderRuns() {
  renderRunsContent();
}

function renderRunsContent() {
  const content = document.getElementById('runsContent');
  if (!content) return;

  const runs = getFilteredRuns();

  if (!runs.length) {
    content.innerHTML = `
      <div class="empty-state">
        <div class="empty-state-icon"><i data-lucide="inbox" style="width:28px;height:28px"></i></div>
        <div class="empty-state-title">暂无运行记录</div>
        <div class="empty-state-desc">提交工作流后，运行记录将显示在这里</div>
      </div>`;
    lucide.createIcons({ nodes: [content] });
    return;
  }

  if (runsState.view === 'table') {
    renderRunsTable(content, runs);
  } else {
    renderRunsCards(content, runs);
  }
}

function renderRunsTable(container, runs) {
  container.innerHTML = `
    <div class="card">
      <table class="data-table">
        <thead>
          <tr>
            <th>工作流</th>
            <th>状态</th>
            <th>节点数</th>
            <th>开始时间</th>
            <th>耗时</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          ${runs.map(r => `
            <tr onclick="location.hash='#run/${escapeHtml(r.id)}'">
              <td>
                <div style="font-weight:500;color:var(--text-bright);font-family:var(--font-display);">${escapeHtml(r.workflow_name)}</div>
                <div style="font-size:11px;color:var(--text-dim);font-family:var(--font-mono);margin-top:2px;">v${escapeHtml(r.workflow_version || '1.0')}</div>
              </td>
              <td>
                <span class="status-indicator">
                  <span class="status-dot ${statusClass(r.status)}"></span>
                  <span class="status-text">${statusText(r.status)}</span>
                </span>
              </td>
              <td style="font-family:var(--font-mono);font-size:12px;">${r.node_count || 0}</td>
              <td style="font-size:12px;color:var(--text-secondary);">${relativeTime(r.created_at)}</td>
              <td style="font-family:var(--font-mono);font-size:12px;color:var(--text-secondary);">${formatDuration(r.started_at, r.finished_at)}</td>
              <td>
                <div style="display:flex;gap:4px;">
                  ${r.status === 'running' ? `<button class="table-action" title="取消" onclick="event.stopPropagation();cancelRun('${escapeHtml(r.id)}')"><i data-lucide="square" style="width:14px;height:14px"></i></button>` : ''}
                  ${r.status === 'success' || r.status === 'failed' || r.status === 'cancelled' ? `<button class="table-action" title="重新运行" onclick="event.stopPropagation();rerunRun('${escapeHtml(r.id)}')"><i data-lucide="rotate-cw" style="width:14px;height:14px"></i></button>` : ''}
                  <button class="table-action" title="删除" onclick="event.stopPropagation();deleteRun('${escapeHtml(r.id)}')"><i data-lucide="trash-2" style="width:14px;height:14px"></i></button>
                </div>
              </td>
            </tr>
          `).join('')}
        </tbody>
      </table>
    </div>`;
  lucide.createIcons({ nodes: [container] });
}

function renderRunsCards(container, runs) {
  container.innerHTML = `
    <div class="run-card-grid">
      ${runs.map(r => `
        <div class="run-card" onclick="location.hash='#run/${escapeHtml(r.id)}'">
          <div class="run-card-top">
            <div class="run-card-icon ${statusClass(r.status)}">
              <i data-lucide="${r.status === 'running' ? 'loader' : r.status === 'success' ? 'check' : r.status === 'failed' ? 'x' : 'clock'}" style="width:18px;height:18px"></i>
            </div>
            <div class="run-card-info">
              <div class="run-card-name">${escapeHtml(r.workflow_name)}</div>
              <div class="run-card-meta">v${escapeHtml(r.workflow_version || '1.0')} · ${r.node_count || 0} 节点</div>
            </div>
            <span class="run-card-status ${statusClass(r.status)}">${statusText(r.status)}</span>
          </div>
          <div class="run-card-stats">
            <div class="run-card-stat">
              <div class="run-card-stat-value">${formatDuration(r.started_at, r.finished_at)}</div>
              <div class="run-card-stat-label">耗时</div>
            </div>
            <div class="run-card-stat">
              <div class="run-card-stat-value">${relativeTime(r.created_at)}</div>
              <div class="run-card-stat-label">创建</div>
            </div>
          </div>
          <div class="run-card-footer">
            <span class="run-card-time">${relativeTime(r.started_at || r.created_at)}</span>
            <div style="display:flex;gap:4px;">
              ${r.status === 'running' ? `<button class="btn btn-sm btn-danger-ghost" onclick="event.stopPropagation();cancelRun('${escapeHtml(r.id)}')">取消</button>` : ''}
              <button class="btn btn-sm btn-ghost" onclick="event.stopPropagation();location.hash='#run/${escapeHtml(r.id)}'">
                详情 <i data-lucide="arrow-right" style="width:12px;height:12px"></i>
              </button>
            </div>
          </div>
        </div>
      `).join('')}
    </div>`;
  lucide.createIcons({ nodes: [container] });
}

function renderRunsPagination() {
  const el = document.getElementById('runsPagination');
  if (!el) return;

  const totalPages = Math.ceil(runsState.total / runsState.perPage);
  if (totalPages <= 1) { el.innerHTML = ''; return; }

  let html = '';
  html += `<button class="pagination-btn" onclick="loadRuns(${runsState.page - 1})" ${runsState.page <= 1 ? 'disabled' : ''}><i data-lucide="chevron-left" style="width:14px;height:14px"></i></button>`;

  const start = Math.max(1, runsState.page - 2);
  const end = Math.min(totalPages, runsState.page + 2);
  for (let i = start; i <= end; i++) {
    html += `<button class="pagination-btn ${i === runsState.page ? 'active' : ''}" onclick="loadRuns(${i})">${i}</button>`;
  }

  html += `<span class="pagination-info">共 ${runsState.total} 条</span>`;
  html += `<button class="pagination-btn" onclick="loadRuns(${runsState.page + 1})" ${runsState.page >= totalPages ? 'disabled' : ''}><i data-lucide="chevron-right" style="width:14px;height:14px"></i></button>`;

  el.innerHTML = html;
  lucide.createIcons({ nodes: [el] });
}

async function cancelRun(id) {
  confirmDialog('取消运行', '确定要取消此工作流运行吗？', id, async () => {
    try {
      await api(`${API_WF}/${id}/cancel`, { method: 'POST' });
      showToast('运行已取消', 'success');
      loadRuns(runsState.page);
    } catch (e) {
      showToast(e.message, 'error');
    }
  });
}

async function rerunRun(id) {
  try {
    const result = await api(`${API_WF}/${id}/rerun`, { method: 'POST' });
    showToast('已重新启动: ' + (result.run_id || ''), 'success');
    location.hash = '#run/' + (result.run_id || id);
  } catch (e) {
    showToast(e.message, 'error');
  }
}

async function deleteRun(id) {
  confirmDialog('删除运行记录', '确定要删除此运行记录吗？此操作不可撤销。', id, async () => {
    try {
      await api(`${API_WF}/${id}`, { method: 'DELETE' });
      showToast('运行记录已删除', 'success');
      loadRuns(runsState.page);
    } catch (e) {
      showToast(e.message, 'error');
    }
  });
}

function updateSidebarStatus() {
  const runningCount = runsState.runs.filter(r => r.status === 'running').length;
  const el = document.getElementById('statusRunningCount');
  if (el) el.textContent = runningCount;
}
