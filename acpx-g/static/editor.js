// ─── acpx-g Workflow Editor (Nexus) ──────────────────────────────
// Vanilla JS + Drawflow + js-yaml + Dagre

'use strict';

// ── State ──────────────────────────────────────────────────────────
let dfEditor = null;
let selectedNodeId = null;
let nodeIdCounter = 0;
let historyStack = [];
let redoStack = [];
const MAX_HISTORY = 50;
let debounceTimer = null;

const dfIdToBizId = new Map();
const bizIdToDfId = new Map();
const nodeStore = new Map();
let wfBaseDir = null;

let wfMeta = {
  name: 'new-workflow',
  version: '1.0',
  description: '',
  timeout: null,
  defaults: { retry: 0, timeout: 300, shell: 'bash -c' },
  inputs: {},
  env: {},
  references: {},
};

// ── Page Render ─────────────────────────────────────────────────────
function renderEditorPage() {
  return `
  <div class="editor-layout">
    <!-- Left: Template List -->
    <aside class="editor-sidebar" id="editorSidebar">
      <div class="editor-sidebar-header">
        <span class="editor-sidebar-title">模板</span>
        <div style="display:flex;gap:4px;">
          <button class="btn-icon btn-sm" id="btnNew" title="新建"><i data-lucide="file-plus" style="width:14px;height:14px"></i></button>
          <button class="btn-icon btn-sm" id="btnImport" title="导入"><i data-lucide="upload" style="width:14px;height:14px"></i></button>
        </div>
      </div>
      <div class="editor-sidebar-search">
        <input class="input" id="templateSearchInput" placeholder="搜索模板..." style="font-size:12px;padding:6px 10px;">
      </div>
      <div class="editor-template-list" id="editor-template-list">
        <div class="skeleton skeleton-card"></div>
        <div class="skeleton skeleton-card" style="height:60px"></div>
        <div class="skeleton skeleton-card"></div>
      </div>
    </aside>

    <!-- Center: Canvas + Toolbar -->
    <main class="editor-main">
      <!-- Toolbar -->
      <div class="editor-toolbar">
        <div class="editor-toolbar-left">
          <input class="input" id="wf-name" value="${escapeHtml(wfMeta.name)}" style="width:160px;font-size:12px;padding:4px 8px;" placeholder="工作流名称">
          <input class="input" id="wf-version" value="${escapeHtml(wfMeta.version)}" style="width:70px;font-size:12px;padding:4px 8px;" placeholder="版本">
        </div>
        <div class="editor-toolbar-center">
          <button class="btn btn-sm btn-ghost" id="btnValidate" title="验证"><i data-lucide="check-circle" style="width:14px;height:14px"></i> 验证</button>
          <button class="btn btn-sm btn-ghost" id="btnAutoLayout" title="自动布局"><i data-lucide="layout-grid" style="width:14px;height:14px"></i> 布局</button>
          <button class="btn btn-sm btn-ghost" id="btnSettings" title="设置"><i data-lucide="settings" style="width:14px;height:14px"></i></button>
        </div>
        <div class="editor-toolbar-right">
          <div id="validation-status" class="validation-ok">无错误</div>
          <button class="btn btn-sm btn-secondary" id="btnSave"><i data-lucide="save" style="width:14px;height:14px"></i> 保存</button>
          <button class="btn btn-sm btn-primary" id="btnRun"><i data-lucide="play" style="width:14px;height:14px"></i> 运行</button>
        </div>
      </div>

      <!-- Node Palette + Canvas -->
      <div class="editor-canvas-area">
        <!-- Palette -->
        <div class="node-palette">
          <div class="palette-node" draggable="true" data-type="shell">
            <div class="palette-icon shell-icon"><i data-lucide="terminal" style="width:16px;height:16px"></i></div>
            <span>Shell</span>
          </div>
          <div class="palette-node" draggable="true" data-type="agent">
            <div class="palette-icon agent-icon"><i data-lucide="bot" style="width:16px;height:16px"></i></div>
            <span>代理</span>
          </div>
          <div class="palette-node" draggable="true" data-type="reference">
            <div class="palette-icon ref-icon"><i data-lucide="git-branch" style="width:16px;height:16px"></i></div>
            <span>引用</span>
          </div>
          <div class="palette-divider"></div>
          <button class="btn btn-sm btn-ghost" id="btnToggleYaml" style="width:100%;justify-content:center;">
            <i data-lucide="file-code" style="width:14px;height:14px"></i> YAML
          </button>
        </div>

        <!-- Drawflow Canvas -->
        <div class="editor-canvas" id="drawflow" ondrop="editorDrop(event)" ondragover="event.preventDefault()"></div>

        <!-- YAML Panel (hidden by default) -->
        <div class="yaml-panel" id="yaml-panel">
          <div class="yaml-panel-header">
            <span>YAML 编辑器</span>
            <div style="display:flex;gap:4px;">
              <button class="btn-icon btn-sm" id="btnCopyYaml" title="复制"><i data-lucide="copy" style="width:13px;height:13px"></i></button>
              <button class="btn-icon btn-sm" id="btnDownloadYaml" title="下载"><i data-lucide="download" style="width:13px;height:13px"></i></button>
              <button class="btn-icon btn-sm" id="btnApplyYaml" title="应用到画布"><i data-lucide="check" style="width:13px;height:13px"></i></button>
              <button class="btn-icon btn-sm" id="btnCloseYaml" title="关闭"><i data-lucide="x" style="width:13px;height:13px"></i></button>
            </div>
          </div>
          <div class="yaml-editor-wrap" id="yaml-editor-wrap" style="display:none;">
            <textarea id="yaml-code-editor" class="yaml-textarea" spellcheck="false"></textarea>
          </div>
        </div>
      </div>
    </main>

    <!-- Right: Property Panel -->
    <aside class="property-panel" id="propertyPanel">
      <div class="prop-header">
        <span class="prop-title">属性</span>
      </div>
      <div class="prop-body">
        <div id="prop-empty" class="prop-empty">
          <i data-lucide="mouse-pointer-click" style="width:24px;height:24px;color:var(--text-dim);margin-bottom:8px;"></i>
          <span>选择节点以编辑属性</span>
        </div>
        <div id="prop-content" style="display:none;"></div>
      </div>
    </aside>
  </div>

  <!-- Import Modal (inside page) -->
  <div class="modal-overlay" id="importModal">
    <div class="modal">
      <div class="modal-header">
        <span class="modal-title">导入工作流</span>
        <button class="modal-close" id="closeImportBtn"><i data-lucide="x" style="width:16px;height:16px"></i></button>
      </div>
      <div class="modal-body">
        <div class="input-group">
          <label class="input-label">粘贴 YAML 内容</label>
          <textarea id="import-yaml-input" class="input" rows="12" placeholder="粘贴工作流 YAML..."></textarea>
        </div>
      </div>
      <div class="modal-footer">
        <button class="btn btn-secondary" id="closeImportBtn2">取消</button>
        <button class="btn btn-primary" id="doImportBtn">导入</button>
      </div>
    </div>
  </div>`;
}

// ── Editor Init ─────────────────────────────────────────────────────
function destroyEditor() {
  if (dfEditor) {
    try { dfEditor.destroy(); } catch (_) {}
    dfEditor = null;
  }
  document.removeEventListener('keydown', editorKeyHandler);
  selectedNodeId = null;
  dfIdToBizId.clear();
  bizIdToDfId.clear();
  nodeStore.clear();
  historyStack = [];
  redoStack = [];
}

function initEditor() {
  if (typeof Drawflow === 'undefined') {
    showToast('编辑器库加载中，请稍候重试', 'warning');
    return;
  }

  // Clean up previous instance
  destroyEditor();

  const el = document.getElementById('drawflow');
  if (!el) return;

  dfEditor = new Drawflow(el);
  dfEditor.reroute = true;
  dfEditor.reroute_fix_curvature = true;
  dfEditor.force_first_input = false;
  dfEditor.start();

  // Drawflow events
  dfEditor.on('nodeCreated', () => { updateYamlFromCanvas(); pushHistory(); });
  dfEditor.on('nodeRemoved', (id) => {
    const bizId = dfIdToBizId.get(String(id));
    if (bizId) { nodeStore.delete(bizId); dfIdToBizId.delete(String(id)); bizIdToDfId.delete(bizId); }
    if (selectedNodeId === id) hidePropertyPanel();
    updateYamlFromCanvas(); pushHistory();
  });
  dfEditor.on('connectionCreated', (conn) => {
    const fromBizId = dfIdToBizId.get(String(conn.output_id));
    const toBizId = dfIdToBizId.get(String(conn.input_id));
    if (fromBizId && toBizId) {
      const nd = nodeStore.get(toBizId);
      if (nd && !nd.depends.includes(fromBizId)) nd.depends.push(fromBizId);
    }
    updateYamlFromCanvas(); pushHistory();
  });
  dfEditor.on('connectionRemoved', (conn) => {
    const fromBizId = dfIdToBizId.get(String(conn.output_id));
    const toBizId = dfIdToBizId.get(String(conn.input_id));
    if (fromBizId && toBizId) {
      const nd = nodeStore.get(toBizId);
      if (nd) nd.depends = nd.depends.filter(d => d !== fromBizId);
    }
    updateYamlFromCanvas(); pushHistory();
  });
  dfEditor.on('nodeSelected', (id) => { selectedNodeId = id; showPropertyPanel(id); });
  dfEditor.on('nodeUnselected', () => { selectedNodeId = null; hidePropertyPanel(); });
  dfEditor.on('nodeMoved', () => { clearTimeout(debounceTimer); debounceTimer = setTimeout(() => updateYamlFromCanvas(), 300); });

  // Keyboard shortcuts
  document.addEventListener('keydown', editorKeyHandler);

  // Palette drag
  document.querySelectorAll('.palette-node').forEach(el => {
    el.addEventListener('dragstart', (e) => e.dataTransfer.setData('node-type', el.dataset.type));
  });

  // Toolbar buttons
  document.getElementById('btnValidate')?.addEventListener('click', editorValidate);
  document.getElementById('btnAutoLayout')?.addEventListener('click', editorAutoLayout);
  document.getElementById('btnSettings')?.addEventListener('click', showWorkflowSettings);
  document.getElementById('btnSave')?.addEventListener('click', editorSave);
  document.getElementById('btnRun')?.addEventListener('click', editorRun);
  document.getElementById('btnNew')?.addEventListener('click', editorNew);
  document.getElementById('btnImport')?.addEventListener('click', openImportModal);
  document.getElementById('btnToggleYaml')?.addEventListener('click', toggleYamlPanel);
  document.getElementById('btnCopyYaml')?.addEventListener('click', copyYaml);
  document.getElementById('btnDownloadYaml')?.addEventListener('click', downloadYaml);
  document.getElementById('btnApplyYaml')?.addEventListener('click', applyYamlChanges);
  document.getElementById('btnCloseYaml')?.addEventListener('click', toggleYamlPanel);

  // Import modal
  document.getElementById('closeImportBtn')?.addEventListener('click', closeImportModal);
  document.getElementById('closeImportBtn2')?.addEventListener('click', closeImportModal);
  document.getElementById('doImportBtn')?.addEventListener('click', doImportYaml);
  document.getElementById('importModal')?.addEventListener('click', (e) => {
    if (e.target === e.currentTarget) closeImportModal();
  });

  // Metadata inputs
  document.getElementById('wf-name')?.addEventListener('change', function() {
    wfMeta.name = this.value || 'untitled';
    highlightEditorTemplate(); updateYamlFromCanvas(); saveDraft();
  });
  document.getElementById('wf-version')?.addEventListener('change', function() {
    wfMeta.version = this.value || '1.0';
    updateYamlFromCanvas(); saveDraft();
  });

  // Template search
  document.getElementById('templateSearchInput')?.addEventListener('input', function() {
    const q = this.value.toLowerCase();
    document.querySelectorAll('#editor-template-list .tpl-card').forEach(card => {
      const name = card.dataset.name?.toLowerCase() || '';
      card.style.display = name.includes(q) ? '' : 'none';
    });
  });

  loadDraft();
  loadEditorTemplates();
}

function editorKeyHandler(e) {
  if (AppState.currentPage !== 'editor') return;
  if (document.activeElement?.tagName === 'INPUT' || document.activeElement?.tagName === 'TEXTAREA') return;
  if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !e.shiftKey) { e.preventDefault(); editorUndo(); }
  else if ((e.ctrlKey || e.metaKey) && (e.key === 'y' || (e.key === 'z' && e.shiftKey))) { e.preventDefault(); editorRedo(); }
  else if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); editorSave(); }
  else if (e.key === 'Delete' || e.key === 'Backspace') { editorDeleteSelected(); }
}

// ── Drop Handler ─────────────────────────────────────────────────────
function editorDrop(e) {
  e.preventDefault();
  const type = e.dataTransfer.getData('node-type');
  if (!type) return;

  const bizId = generateBizId(type);
  const data = defaultNodeData(type, bizId);
  nodeStore.set(bizId, data);

  const html = buildNodeHtml(bizId, type, data);
  let posX, posY;
  if (dfEditor.precanvas) {
    const canvas = document.getElementById('drawflow');
    const rect = canvas.getBoundingClientRect();
    posX = (e.clientX - rect.left) / (dfEditor.zoom || 1);
    posY = (e.clientY - rect.top) / (dfEditor.zoom || 1);
  } else {
    posX = e.clientX; posY = e.clientY;
  }
  const dfId = dfEditor.addNode(bizId, 1, 1, posX, posY, type, { bizId, type }, html);
  dfIdToBizId.set(String(dfId), bizId);
  bizIdToDfId.set(bizId, dfId);
  updateYamlFromCanvas(); pushHistory(); saveDraft();
}

// ── Node Helpers ─────────────────────────────────────────────────────
function generateBizId(type) {
  nodeIdCounter++;
  const prefix = { shell: 'step', agent: 'agent', reference: 'ref' }[type] || 'node';
  return `${prefix}-${nodeIdCounter}`;
}

function sanitizeBizId(s) { return s.replace(/[^a-zA-Z0-9_\-./]/g, '-'); }

function defaultNodeData(type, bizId) {
  const base = { type, depends: [], env: {}, outputs: {}, continue_on_error: false, timeout: null, retry: null, shell: null, if_condition: null };
  switch (type) {
    case 'shell': return { ...base, run: 'echo hello' };
    case 'agent': return { ...base, prompt: 'Review the code', agent: 'peri', model: null, cwd: null };
    case 'reference': return { ...base, ref: '', with: {} };
    default: return base;
  }
}

function buildNodeHtml(bizId, type, data) {
  const icons = { shell: '&#9654;', agent: '&#10023;', reference: '&#8635;' };
  const labels = { shell: 'Shell', agent: '代理', reference: '引用' };
  const preview = getNodePreview(type, data);
  const depsHtml = (data.depends && data.depends.length)
    ? '<div class="enode-deps">' + data.depends.map(d => `<span class="enode-dep-tag">${escapeHtml(d)}</span>`).join('') + '</div>'
    : '';
  return `<div class="enode ${type}-node">
    <div class="enode-header">
      <span class="enode-icon">${icons[type]}</span>
      <span class="enode-type">${labels[type]}</span>
      <span class="enode-id">${escapeHtml(bizId)}</span>
    </div>
    <div class="enode-body">
      <div class="enode-preview">${escapeHtml(preview)}</div>
      ${depsHtml}
    </div>
  </div>`;
}

function getNodePreview(type, data) {
  switch (type) {
    case 'shell': return (data.run || '').split('\n')[0].substring(0, 60);
    case 'agent': return (data.prompt || '').split('\n')[0].substring(0, 60);
    case 'reference': return data.ref ? `ref: ${data.ref}` : '(未设置引用)';
    default: return '';
  }
}

function refreshNodeHtml(dfId) {
  const bizId = dfIdToBizId.get(String(dfId));
  if (!bizId) return;
  const data = nodeStore.get(bizId);
  if (!data) return;
  const html = buildNodeHtml(bizId, data.type, data);
  const nodeEl = document.querySelector(`#drawflow .drawflow-node[data-id="${dfId}"] .drawflow_content_node`);
  if (nodeEl) nodeEl.innerHTML = html;
}

// ── Property Panel ───────────────────────────────────────────────────
function showPropertyPanel(dfId) {
  const bizId = dfIdToBizId.get(String(dfId));
  if (!bizId) return;
  const data = nodeStore.get(bizId);
  if (!data) return;

  document.getElementById('prop-empty').style.display = 'none';
  const content = document.getElementById('prop-content');
  content.style.display = 'block';

  let html = '';

  html += `<div class="prop-section">
    <div class="prop-section-title">标识</div>
    <div class="prop-field"><label>节点 ID</label><input type="text" value="${escapeHtml(bizId)}" data-df-id="${dfId}" data-action="rename"></div>
    <div class="prop-field"><label>类型</label><select disabled><option>${escapeHtml(data.type)}</option></select></div>
  </div>`;

  if (data.type === 'shell') {
    html += propSection('命令', propFieldTextarea('运行脚本', 'run', data.run || '', dfId));
  } else if (data.type === 'agent') {
    html += propSection('代理配置',
      propFieldTextarea('提示词', 'prompt', data.prompt || '', dfId) +
      propFieldInput('CLI 名称', 'agent', data.agent || 'peri', dfId, 'text') +
      propFieldSelect('模型', 'model', data.model || '', ['', 'opus', 'sonnet', 'haiku'], dfId) +
      propFieldInput('工作目录', 'cwd', data.cwd || '', dfId, 'text')
    );
  } else if (data.type === 'reference') {
    const refKeys = Object.keys(wfMeta.references);
    html += propSection('引用配置',
      (refKeys.length
        ? propFieldSelect('引用', 'ref', data.ref || '', refKeys, dfId)
        : propFieldInput('引用名称', 'ref', data.ref || '', dfId, 'text')) +
      buildKVEditor('参数 (with)', 'with', data.with || {}, dfId)
    );
  }

  html += propSection('执行配置',
    propFieldInput('超时 (秒)', 'timeout', data.timeout || '', dfId, 'number') +
    propFieldInput('重试次数', 'retry', data.retry != null ? data.retry : '', dfId, 'number') +
    propFieldInput('Shell', 'shell', data.shell || '', dfId, 'text') +
    propFieldInput('条件 (if)', 'if_condition', data.if_condition || '', dfId, 'text') +
    `<div class="prop-field"><label><input type="checkbox" ${data.continue_on_error ? 'checked' : ''} data-df-id="${dfId}" data-key="continue_on_error" data-action="checkbox"> <span>出错时继续</span></label></div>`
  );

  html += propSection('依赖关系', `
    <div class="prop-field"><label>依赖（拖拽连线设置）</label>
    <div style="font-size:12px;padding:4px 0">
      ${data.depends.length ? data.depends.map(d => `<span class="enode-dep-tag">${escapeHtml(d)}</span>`).join(' ') : '<span style="color:var(--text-dim)">无依赖</span>'}
    </div></div>
  `);

  html += propSection('环境变量', buildKVEditor('变量', 'env', data.env || {}, dfId));
  html += propSection('输出', buildKVEditor('输出映射', 'outputs', data.outputs || {}, dfId));

  html += `<div class="prop-section" style="text-align:right">
    <button class="btn btn-sm btn-danger" data-df-id="${dfId}" data-action="delete-node">删除节点</button>
  </div>`;

  content.innerHTML = html;
  bindPropertyEvents(content);
}

function hidePropertyPanel() {
  const empty = document.getElementById('prop-empty');
  const content = document.getElementById('prop-content');
  if (empty) empty.style.display = 'block';
  if (content) content.style.display = 'none';
}

function propSection(title, body) { return `<div class="prop-section"><div class="prop-section-title">${title}</div>${body}</div>`; }
function propFieldInput(label, key, val, dfId, type) { return `<div class="prop-field"><label>${label}</label><input type="${type || 'text'}" value="${escapeHtml(String(val ?? ''))}" data-df-id="${dfId}" data-key="${key}"></div>`; }
function propFieldSelect(label, key, val, options, dfId) { return `<div class="prop-field"><label>${label}</label><select data-df-id="${dfId}" data-key="${key}">${options.map(o => `<option value="${escapeHtml(o)}" ${o === val ? 'selected' : ''}>${escapeHtml(o || '(默认)')}</option>`).join('')}</select></div>`; }
function propFieldTextarea(label, key, val, dfId) { return `<div class="prop-field"><label>${label}</label><textarea data-df-id="${dfId}" data-key="${key}">${escapeHtml(val || '')}</textarea></div>`; }

function buildKVEditor(label, field, obj, dfId) {
  const entries = Object.entries(obj || {});
  let html = `<div class="prop-field"><label>${label}</label><div class="kv-list" data-field="${field}" data-df-id="${dfId}">`;
  entries.forEach(([k, v]) => {
    html += `<div class="kv-row"><input value="${escapeHtml(k)}" placeholder="键"><input value="${escapeHtml(v)}" placeholder="值"><button class="kv-remove" data-df-id="${dfId}" data-field="${field}">&times;</button></div>`;
  });
  html += `</div><button class="kv-add" data-df-id="${dfId}" data-field="${field}">+ 添加</button></div>`;
  return html;
}

function bindPropertyEvents(container) {
  container.querySelectorAll('input[data-key][data-df-id], select[data-key][data-df-id], textarea[data-key][data-df-id]').forEach(el => {
    el.addEventListener('change', function() {
      const dfId = Number(this.dataset.dfId);
      const key = this.dataset.key;
      let val = this.value;
      if (this.type === 'number') val = val ? Number(val) : null;
      updateNodeData(dfId, key, val);
    });
  });
  container.querySelectorAll('input[data-action="rename"]').forEach(el => {
    el.addEventListener('change', function() { renameNode(Number(this.dataset.dfId), this.value); });
  });
  container.querySelectorAll('input[data-action="checkbox"]').forEach(el => {
    el.addEventListener('change', function() { updateNodeData(Number(this.dataset.dfId), this.dataset.key, this.checked); });
  });
  container.querySelectorAll('button[data-action="delete-node"]').forEach(el => {
    el.addEventListener('click', function() { dfEditor.removeNodeId('node-' + this.dataset.dfId); });
  });
  container.querySelectorAll('.kv-add').forEach(btn => {
    btn.addEventListener('click', function() { addKV(this, Number(this.dataset.dfId), this.dataset.field); });
  });
  container.querySelectorAll('.kv-remove').forEach(btn => {
    btn.addEventListener('click', function() { removeKV(this, Number(this.dataset.dfId), this.dataset.field); });
  });
  container.querySelectorAll('.kv-row input').forEach(inp => {
    inp.addEventListener('change', function() { updateKVFromList(this.closest('.kv-list')); });
  });
}

// ── Data Update ──────────────────────────────────────────────────────
function updateNodeData(dfId, key, value) {
  const bizId = dfIdToBizId.get(String(dfId));
  if (!bizId) return;
  const data = nodeStore.get(bizId);
  if (!data) return;
  data[key] = value;
  refreshNodeHtml(dfId); updateYamlFromCanvas(); pushHistory(); saveDraft();
}

function renameNode(dfId, newId) {
  const newBizId = sanitizeBizId(newId);
  if (!newBizId) return;
  const oldBizId = dfIdToBizId.get(String(dfId));
  if (!oldBizId || oldBizId === newBizId) return;
  if (nodeStore.has(newBizId)) { showToast('节点 ID 已存在', 'error'); return; }

  const data = nodeStore.get(oldBizId);
  nodeStore.delete(oldBizId); nodeStore.set(newBizId, data);
  dfIdToBizId.set(String(dfId), newBizId); bizIdToDfId.delete(oldBizId); bizIdToDfId.set(newBizId, dfId);
  nodeStore.forEach((nd) => { nd.depends = nd.depends.map(d => d === oldBizId ? newBizId : d); });
  refreshNodeHtml(dfId); updateYamlFromCanvas(); pushHistory(); saveDraft(); showPropertyPanel(dfId);
}

function addKV(btn, dfId, field) {
  const bizId = dfIdToBizId.get(String(dfId));
  if (!bizId) return;
  const data = nodeStore.get(bizId);
  if (!data) return;
  const obj = data[field] || {};
  obj['key_' + Object.keys(obj).length] = '';
  data[field] = obj;
  showPropertyPanel(dfId); updateYamlFromCanvas(); pushHistory(); saveDraft();
}

function removeKV(btn, dfId, field) {
  const row = btn.closest('.kv-row'); row.remove();
  updateKVFromList(btn.closest('.kv-list'));
  pushHistory(); saveDraft();
}

function updateKVFromList(listEl) {
  if (!listEl) return;
  const dfId = listEl.dataset.dfId;
  const field = listEl.dataset.field;
  const bizId = dfIdToBizId.get(String(dfId));
  if (!bizId) return;
  const data = nodeStore.get(bizId);
  if (!data) return;
  const obj = {};
  listEl.querySelectorAll('.kv-row').forEach(r => {
    const inputs = r.querySelectorAll('input');
    if (inputs[0] && inputs[0].value) obj[inputs[0].value] = inputs[1] ? inputs[1].value : '';
  });
  data[field] = obj; updateYamlFromCanvas(); saveDraft();
}

// ── YAML Sync ────────────────────────────────────────────────────────
function updateYamlFromCanvas() {
  const yaml = exportToYaml();
  const ta = document.getElementById('yaml-code-editor');
  if (ta && document.getElementById('yaml-editor-wrap')?.style.display !== 'none') {
    const pos = ta.selectionStart;
    ta.value = yaml;
    ta.setSelectionRange(pos, pos);
  }
}

function exportToYaml() {
  const nodes = [];
  nodeStore.forEach((data, bizId) => {
    const node = { id: bizId, type: data.type };
    if (data.depends?.length) node.depends = [...data.depends];
    if (data.type === 'shell') node.run = data.run || 'echo hello';
    else if (data.type === 'agent') {
      node.prompt = data.prompt || '';
      if (data.agent && data.agent !== 'peri') node.agent = data.agent;
      if (data.model) node.model = data.model;
      if (data.cwd) node.cwd = data.cwd;
    } else if (data.type === 'reference') {
      node.ref = data.ref || '';
      if (data.with && Object.keys(data.with).length) node.with = { ...data.with };
    }
    if (data.timeout != null) node.timeout = data.timeout;
    if (data.retry != null) node.retry = data.retry;
    if (data.shell) node.shell = data.shell;
    if (data.if_condition) node.if = data.if_condition;
    if (data.continue_on_error) node.continue_on_error = true;
    if (data.env && Object.keys(data.env).length) node.env = { ...data.env };
    if (data.outputs && Object.keys(data.outputs).length) node.outputs = { ...data.outputs };
    nodes.push(node);
  });

  const wf = { name: wfMeta.name, version: wfMeta.version };
  if (wfMeta.description) wf.description = wfMeta.description;
  if (wfMeta.timeout) wf.timeout = wfMeta.timeout;
  const defs = wfMeta.defaults;
  if (defs.retry !== 0 || defs.timeout !== 300 || defs.shell !== 'bash -c') {
    wf.defaults = {};
    if (defs.retry !== 0) wf.defaults.retry = defs.retry;
    if (defs.timeout !== 300) wf.defaults.timeout = defs.timeout;
    if (defs.shell !== 'bash -c') wf.defaults.shell = defs.shell;
  }
  if (Object.keys(wfMeta.inputs).length) wf.inputs = wfMeta.inputs;
  if (Object.keys(wfMeta.env).length) wf.env = { ...wfMeta.env };
  if (Object.keys(wfMeta.references).length) wf.references = { ...wfMeta.references };
  wf.nodes = nodes;

  return jsyaml.dump(wf, { indent: 2, lineWidth: 120, noRefs: true, sortKeys: false });
}

function importFromYaml(yamlStr) {
  try {
    const parsed = jsyaml.load(yamlStr);
    if (!parsed || typeof parsed !== 'object') throw new Error('无效的 YAML 结构');

    dfEditor.clear(); nodeStore.clear(); dfIdToBizId.clear(); bizIdToDfId.clear();
    selectedNodeId = null; hidePropertyPanel();

    wfMeta.name = parsed.name || 'untitled';
    wfMeta.version = parsed.version || '1.0';
    wfMeta.description = parsed.description || '';
    wfMeta.timeout = parsed.timeout || null;
    wfMeta.defaults = { retry: parsed.defaults?.retry ?? 0, timeout: parsed.defaults?.timeout ?? 300, shell: parsed.defaults?.shell ?? 'bash -c' };
    wfMeta.inputs = parsed.inputs || {};
    wfMeta.env = parsed.env || {};
    wfMeta.references = parsed.references || {};

    const nameEl = document.getElementById('wf-name');
    const verEl = document.getElementById('wf-version');
    if (nameEl) nameEl.value = wfMeta.name;
    if (verEl) verEl.value = wfMeta.version;

    const nodes = parsed.nodes || [];
    const idMap = {};
    nodes.forEach(node => {
      const bizId = node.id;
      const type = node.type || 'shell';
      const data = defaultNodeData(type, bizId);
      if (type === 'shell') data.run = typeof node.run === 'string' ? node.run : 'echo hello';
      else if (type === 'agent') { data.prompt = node.prompt || ''; data.agent = node.agent || 'peri'; data.model = node.model || null; data.cwd = node.cwd || null; }
      else if (type === 'reference') { data.ref = node.ref || ''; data.with = node.with || {}; }
      data.depends = node.depends || [];
      data.timeout = node.timeout ?? null;
      data.retry = node.retry ?? null;
      data.shell = node.shell || null;
      data.if_condition = node.if || null;
      data.continue_on_error = node.continue_on_error || false;
      data.env = node.env || {};
      data.outputs = node.outputs || {};

      nodeStore.set(bizId, data);
      const html = buildNodeHtml(bizId, type, data);
      const dfId = dfEditor.addNode(bizId, 1, 1, 100 + Math.random() * 300, 100 + Math.random() * 300, type, { bizId, type }, html);
      dfIdToBizId.set(String(dfId), bizId); bizIdToDfId.set(bizId, dfId); idMap[bizId] = dfId;
    });

    nodes.forEach(node => {
      const dfId = idMap[node.id];
      if (!dfId) return;
      (node.depends || []).forEach(depBizId => {
        const depDfId = idMap[depBizId];
        if (depDfId != null) dfEditor.addConnection(depDfId, dfId, 'output_1', 'input_1');
      });
    });

    setTimeout(() => editorAutoLayout(), 100);
    updateYamlFromCanvas(); clearHistory(); pushHistory(); saveDraft();
    showToast('工作流导入成功', 'success');
  } catch (e) {
    showToast('导入失败: ' + e.message, 'error');
  }
}

function applyYamlChanges() {
  const ta = document.getElementById('yaml-code-editor');
  if (!ta) return;
  importFromYaml(ta.value);
}

// ── Auto Layout ──────────────────────────────────────────────────────
function editorAutoLayout() {
  if (typeof dagre === 'undefined') return;
  const exportData = dfEditor.export();
  const moduleData = exportData.drawflow?.Home?.data;
  if (!moduleData) return;

  const nodeIds = Object.keys(moduleData);
  if (!nodeIds.length) return;

  const NODE_W = 200, NODE_H = 80;
  const g = new dagre.graphlib.Graph();
  g.setGraph({ rankdir: 'TB', nodesep: 60, ranksep: 100, marginx: 60, marginy: 60 });
  g.setDefaultEdgeLabel(() => ({}));

  nodeIds.forEach(id => g.setNode(id, { width: NODE_W, height: NODE_H }));
  nodeStore.forEach((data, bizId) => {
    const dfId = bizIdToDfId.get(bizId);
    if (!dfId) return;
    (data.depends || []).forEach(depBizId => {
      const depDfId = bizIdToDfId.get(depBizId);
      if (depDfId != null) g.setEdge(String(depDfId), String(dfId));
    });
  });

  dagre.layout(g);

  g.nodes().forEach(id => {
    const pos = g.node(id);
    if (!pos) return;
    const x = pos.x - NODE_W / 2;
    const y = pos.y - NODE_H / 2;
    if (moduleData[id]) { moduleData[id].pos_x = x; moduleData[id].pos_y = y; }
    const nodeEl = dfEditor.precanvas.querySelector('#node-' + id);
    if (nodeEl) { nodeEl.style.left = x + 'px'; nodeEl.style.top = y + 'px'; }
  });

  nodeIds.forEach(id => dfEditor.updateConnectionNodes('node-' + id));
}

// ── Validation ───────────────────────────────────────────────────────
function validateWorkflow() {
  const errors = [];
  let parsed;
  try { parsed = jsyaml.load(exportToYaml()); }
  catch (e) { errors.push('YAML 解析错误: ' + e.message); displayValidation(errors); return errors; }

  if (!parsed) { errors.push('工作流为空'); displayValidation(errors); return errors; }
  if (!parsed.name?.trim()) errors.push('缺少工作流名称');
  if (!parsed.version?.trim()) errors.push('缺少版本号');
  if (!parsed.nodes?.length) errors.push('至少需要一个节点');

  const ids = new Set();
  (parsed.nodes || []).forEach(node => {
    const id = node.id;
    if (!id) { errors.push('节点缺少 ID'); return; }
    if (ids.has(id)) errors.push(`重复的节点 ID: ${id}`);
    ids.add(id);
    if (/[^a-zA-Z0-9_\-./]/.test(id)) errors.push(`节点 ID 包含非法字符: ${id}`);
    if (node.depends) {
      node.depends.forEach(d => {
        if (d === id) errors.push(`节点 '${id}' 自依赖`);
        else if (!ids.has(d)) errors.push(`节点 '${id}' 依赖不存在的 '${d}'`);
      });
    }
  });

  const adj = {};
  (parsed.nodes || []).forEach(n => { adj[n.id] = n.depends || []; });
  const visited = new Set(), stack = new Set();
  function dfs(id) {
    if (stack.has(id)) return true;
    if (visited.has(id)) return false;
    visited.add(id); stack.add(id);
    for (const dep of (adj[id] || [])) { if (dfs(dep)) return true; }
    stack.delete(id); return false;
  }
  for (const id of ids) { if (dfs(id)) { errors.push('依赖图中存在循环'); break; } }

  displayValidation(errors);
  return errors;
}

function displayValidation(errors) {
  const statusEl = document.getElementById('validation-status');
  if (!statusEl) return;
  if (!errors.length) {
    statusEl.className = 'validation-ok';
    statusEl.textContent = '无错误';
  } else {
    statusEl.className = 'validation-err';
    statusEl.textContent = `${errors.length} 个错误`;
  }
}

function editorValidate() {
  const errors = validateWorkflow();
  if (errors.length) {
    showToast(`${errors.length} 个验证错误`, 'error');
  } else {
    const yaml = exportToYaml();
    fetch('/api/v1/workflows/validate', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ yaml }),
    }).then(r => r.json()).then(result => {
      if (result.valid) showToast('工作流验证通过', 'success');
      else showToast('服务端验证: ' + (result.errors || []).map(e => e.message).join('; '), 'error');
    }).catch(() => showToast('工作流结构验证通过', 'success'));
  }
}

// ── Undo / Redo ──────────────────────────────────────────────────────
function currentSnapshot() {
  return {
    meta: JSON.parse(JSON.stringify(wfMeta)),
    nodes: JSON.parse(JSON.stringify(Object.fromEntries(nodeStore))),
    dfData: dfEditor.export(),
    dfIdMap: { toBiz: Object.fromEntries(dfIdToBizId), toDf: Object.fromEntries(bizIdToDfId) },
  };
}

function pushHistory() {
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    historyStack.push(currentSnapshot());
    if (historyStack.length > MAX_HISTORY) historyStack.shift();
    redoStack = [];
  }, 300);
}

function clearHistory() { historyStack = []; redoStack = []; }

function editorUndo() {
  if (historyStack.length <= 1) return;
  redoStack.push(historyStack.pop());
  restoreSnapshot(historyStack[historyStack.length - 1]);
}

function editorRedo() {
  if (!redoStack.length) return;
  const snap = redoStack.pop();
  historyStack.push(snap);
  restoreSnapshot(snap);
}

function restoreSnapshot(snap) {
  wfMeta = JSON.parse(JSON.stringify(snap.meta));
  const nameEl = document.getElementById('wf-name');
  const verEl = document.getElementById('wf-version');
  if (nameEl) nameEl.value = wfMeta.name;
  if (verEl) verEl.value = wfMeta.version;

  nodeStore.clear(); dfIdToBizId.clear(); bizIdToDfId.clear();
  Object.entries(snap.nodes).forEach(([bizId, data]) => nodeStore.set(bizId, data));
  dfEditor.import(snap.dfData);
  Object.entries(snap.dfIdMap.toBiz).forEach(([k, v]) => dfIdToBizId.set(k, v));
  Object.entries(snap.dfIdMap.toDf).forEach(([k, v]) => bizIdToDfId.set(k, v));
  hidePropertyPanel(); updateYamlFromCanvas(); saveDraft();
}

// ── Toolbar Actions ───────────────────────────────────────────────────
function editorNew() {
  if (nodeStore.size > 0) {
    confirmDialog('新建工作流', '确定要放弃当前工作流并创建新的吗？', null, () => {
      doEditorNew();
    });
  } else {
    doEditorNew();
  }
}

function doEditorNew() {
  dfEditor.clear(); nodeStore.clear(); dfIdToBizId.clear(); bizIdToDfId.clear();
  nodeIdCounter = 0;
  wfMeta = { name: 'new-workflow', version: '1.0', description: '', timeout: null, defaults: { retry: 0, timeout: 300, shell: 'bash -c' }, inputs: {}, env: {}, references: {} };
  wfBaseDir = null;
  const nameEl = document.getElementById('wf-name');
  const verEl = document.getElementById('wf-version');
  if (nameEl) nameEl.value = wfMeta.name;
  if (verEl) verEl.value = wfMeta.version;
  hidePropertyPanel(); clearHistory(); pushHistory(); updateYamlFromCanvas(); displayValidation([]);
  localStorage.removeItem('acpx-editor-draft');
  highlightEditorTemplate();
}

function openImportModal() { document.getElementById('importModal')?.classList.add('open'); }
function closeImportModal() { document.getElementById('importModal')?.classList.remove('open'); }

function doImportYaml() {
  const yaml = document.getElementById('import-yaml-input')?.value;
  if (!yaml?.trim()) { showToast('请粘贴 YAML 内容', 'error'); return; }
  closeImportModal();
  importFromYaml(yaml);
}

async function editorSave() {
  const errors = validateWorkflow();
  if (errors.length) { showToast('保存前请修复验证错误', 'error'); return; }
  const yaml = exportToYaml();
  const name = wfMeta.name || 'untitled';
  try {
    const result = await api('/api/v1/templates/save', {
      method: 'POST',
      body: JSON.stringify({ name, yaml }),
    });
    if (result.success) {
      showToast(result.message || '保存成功', 'success');
      loadEditorTemplates();
    } else {
      showToast(result.message || '保存失败', 'error');
    }
  } catch (e) {
    showToast('网络错误: ' + e.message, 'error');
  }
}

async function editorRun() {
  const errors = validateWorkflow();
  if (errors.length) { showToast('运行前请修复验证错误', 'error'); return; }
  const yaml = exportToYaml();
  const payload = { yaml };
  if (wfBaseDir) payload.base_dir = wfBaseDir;
  try {
    const result = await api('/api/v1/workflows', { method: 'POST', body: JSON.stringify(payload) });
    showToast('工作流已启动: ' + result.run_id, 'success');
    location.hash = '#run/' + result.run_id;
  } catch (e) {
    showToast(e.message, 'error');
  }
}

function editorDeleteSelected() {
  if (selectedNodeId) dfEditor.removeNodeId('node-' + selectedNodeId);
}

// ── YAML Panel ───────────────────────────────────────────────────────
function toggleYamlPanel() {
  const panel = document.getElementById('yaml-panel');
  const wrap = document.getElementById('yaml-editor-wrap');
  const isOpen = panel?.classList.toggle('open');
  if (isOpen && wrap) {
    wrap.style.display = 'flex';
    const ta = document.getElementById('yaml-code-editor');
    if (ta) ta.value = exportToYaml();
  } else if (wrap) {
    wrap.style.display = 'none';
  }
}

function getYamlText() {
  const ta = document.getElementById('yaml-code-editor');
  return ta ? ta.value : exportToYaml();
}

function copyYaml() {
  copyToClipboard(getYamlText());
}

function downloadYaml() {
  const yaml = getYamlText();
  const name = (wfMeta.name || 'workflow') + '.yaml';
  const blob = new Blob([yaml], { type: 'text/yaml' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url; a.download = name;
  document.body.appendChild(a); a.click();
  document.body.removeChild(a); URL.revokeObjectURL(url);
}

// ── Workflow Settings ─────────────────────────────────────────────────
function showWorkflowSettings() {
  const inputRows = Object.entries(wfMeta.inputs || {}).map(([k, def]) => {
    return `<div class="kv-row"><input value="${escapeHtml(k)}" placeholder="名称" style="width:80px"><select style="width:70px"><option ${def.type === 'string' ? 'selected' : ''}>string</option><option ${def.type === 'number' ? 'selected' : ''}>number</option><option ${def.type === 'boolean' ? 'selected' : ''}>boolean</option></select><input value="${escapeHtml(def.default || '')}" placeholder="默认值"><label style="font-size:10px"><input type="checkbox" ${def.required ? 'checked' : ''}> 必填</label><button class="kv-remove">&times;</button></div>`;
  }).join('');

  const envRows = Object.entries(wfMeta.env || {}).map(([k, v]) => {
    return `<div class="kv-row"><input value="${escapeHtml(k)}" placeholder="键"><input value="${escapeHtml(v)}" placeholder="值"><button class="kv-remove">&times;</button></div>`;
  }).join('');

  const refRows = Object.entries(wfMeta.references || {}).map(([k, v]) => {
    return `<div class="kv-row"><input value="${escapeHtml(k)}" placeholder="别名"><input value="${escapeHtml(v)}" placeholder="路径/URL"><button class="kv-remove">&times;</button></div>`;
  }).join('');

  openModal(`
    <div class="modal-header">
      <span class="modal-title">工作流设置</span>
      <button class="modal-close" onclick="closeModal()"><i data-lucide="x" style="width:16px;height:16px"></i></button>
    </div>
    <div class="modal-body" style="max-height:60vh;">
      <div class="prop-section">
        <div class="prop-section-title">常规</div>
        <div class="prop-field"><label>名称</label><input id="ws-name" value="${escapeHtml(wfMeta.name)}"></div>
        <div class="prop-field"><label>版本</label><input id="ws-version" value="${escapeHtml(wfMeta.version)}"></div>
        <div class="prop-field"><label>描述</label><textarea id="ws-desc" rows="2">${escapeHtml(wfMeta.description || '')}</textarea></div>
        <div class="prop-field"><label>超时 (秒)</label><input id="ws-timeout" type="number" value="${wfMeta.timeout || ''}"></div>
      </div>
      <div class="prop-section">
        <div class="prop-section-title">默认值</div>
        <div class="prop-field"><label>重试</label><input id="ws-retry" type="number" value="${wfMeta.defaults.retry}"></div>
        <div class="prop-field"><label>超时 (秒)</label><input id="ws-def-timeout" type="number" value="${wfMeta.defaults.timeout}"></div>
        <div class="prop-field"><label>Shell</label><input id="ws-shell" value="${escapeHtml(wfMeta.defaults.shell)}"></div>
      </div>
      <div class="prop-section">
        <div class="prop-section-title">输入参数</div>
        <div class="kv-list" id="ws-inputs">${inputRows}</div>
        <button class="kv-add" onclick="addSettingsInputRow('ws-inputs')">+ 添加参数</button>
      </div>
      <div class="prop-section">
        <div class="prop-section-title">环境变量</div>
        <div class="kv-list" id="ws-env">${envRows}</div>
        <button class="kv-add" onclick="addSettingsKvRow('ws-env')">+ 添加变量</button>
      </div>
      <div class="prop-section">
        <div class="prop-section-title">引用</div>
        <div class="kv-list" id="ws-refs">${refRows}</div>
        <button class="kv-add" onclick="addSettingsKvRow('ws-refs')">+ 添加引用</button>
      </div>
    </div>
    <div class="modal-footer">
      <button class="btn btn-secondary" onclick="closeModal()">取消</button>
      <button class="btn btn-primary" onclick="saveWorkflowSettings()">保存设置</button>
    </div>
  `);

  // Bind remove buttons in settings modal
  setTimeout(() => {
    document.querySelectorAll('#modalContent .kv-remove').forEach(btn => {
      btn.addEventListener('click', () => btn.closest('.kv-row')?.remove());
    });
  }, 0);
}

function addSettingsInputRow(containerId) {
  const container = document.getElementById(containerId);
  if (!container) return;
  const row = document.createElement('div');
  row.className = 'kv-row';
  row.innerHTML = '<input placeholder="名称" style="width:80px"><select style="width:70px"><option>string</option><option>number</option><option>boolean</option></select><input placeholder="默认值"><label style="font-size:10px"><input type="checkbox"> 必填</label><button class="kv-remove">&times;</button>';
  container.appendChild(row);
  row.querySelector('.kv-remove').addEventListener('click', () => row.remove());
}

function addSettingsKvRow(containerId) {
  const container = document.getElementById(containerId);
  if (!container) return;
  const row = document.createElement('div');
  row.className = 'kv-row';
  row.innerHTML = '<input placeholder="键"><input placeholder="值"><button class="kv-remove">&times;</button>';
  container.appendChild(row);
  row.querySelector('.kv-remove').addEventListener('click', () => row.remove());
}

function saveWorkflowSettings() {
  wfMeta.name = document.getElementById('ws-name')?.value || wfMeta.name;
  wfMeta.version = document.getElementById('ws-version')?.value || wfMeta.version;
  wfMeta.description = document.getElementById('ws-desc')?.value || '';
  wfMeta.timeout = document.getElementById('ws-timeout')?.value ? Number(document.getElementById('ws-timeout').value) : null;
  wfMeta.defaults.retry = Number(document.getElementById('ws-retry')?.value || 0);
  wfMeta.defaults.timeout = Number(document.getElementById('ws-def-timeout')?.value || 300);
  wfMeta.defaults.shell = document.getElementById('ws-shell')?.value || 'bash -c';

  const nameEl = document.getElementById('wf-name');
  const verEl = document.getElementById('wf-version');
  if (nameEl) nameEl.value = wfMeta.name;
  if (verEl) verEl.value = wfMeta.version;

  wfMeta.inputs = {};
  document.querySelectorAll('#ws-inputs .kv-row').forEach(row => {
    const inputs = row.querySelectorAll('input');
    const select = row.querySelector('select');
    const name = inputs[0]?.value;
    if (!name) return;
    wfMeta.inputs[name] = { type: select?.value || 'string', default: inputs[1]?.value || null, required: inputs[2]?.checked || false };
  });

  wfMeta.env = {};
  document.querySelectorAll('#ws-env .kv-row').forEach(row => {
    const inputs = row.querySelectorAll('input');
    if (inputs[0]?.value) wfMeta.env[inputs[0].value] = inputs[1]?.value || '';
  });

  wfMeta.references = {};
  document.querySelectorAll('#ws-refs .kv-row').forEach(row => {
    const inputs = row.querySelectorAll('input');
    if (inputs[0]?.value) wfMeta.references[inputs[0].value] = inputs[1]?.value || '';
  });

  closeModal(); updateYamlFromCanvas(); pushHistory(); saveDraft();
  showToast('设置已保存', 'success');
}

// ── Template Loading ──────────────────────────────────────────────────
async function loadEditorTemplates() {
  const listEl = document.getElementById('editor-template-list');
  if (!listEl) return;
  try {
    const data = await api(API_TPL);
    const tpls = data.templates || data || [];
    renderEditorTemplateList(tpls);
  } catch (e) {
    listEl.innerHTML = '<div class="empty-state" style="padding:20px;"><span style="color:var(--text-dim);font-size:12px;">加载模板失败</span></div>';
  }
}

function renderEditorTemplateList(tpls) {
  const el = document.getElementById('editor-template-list');
  if (!el) return;
  if (!tpls?.length) {
    el.innerHTML = '<div class="tpl-empty"><i data-lucide="file-x" style="width:20px;height:20px;color:var(--text-dim);margin-bottom:8px;"></i><span>暂无模板</span></div>';
    lucide.createIcons({ nodes: [el] });
    return;
  }

  el.innerHTML = tpls.map(t => `
    <div class="tpl-card${wfMeta.name === t.name ? ' active' : ''}" data-name="${escapeHtml(t.name)}" data-path="${escapeHtml(t.file_path || '')}">
      <div class="tpl-card-name">${escapeHtml(t.name)}</div>
      <div class="tpl-card-desc">${escapeHtml(t.description || '无描述')}</div>
      <div class="tpl-card-meta">
        <span class="tpl-tag">v${escapeHtml(t.version)}</span>
        <span class="tpl-tag">${t.node_count} 节点</span>
      </div>
    </div>
  `).join('');

  el.querySelectorAll('.tpl-card').forEach(card => {
    card.addEventListener('click', () => {
      const path = card.dataset.path;
      if (path) {
        const idx = path.lastIndexOf('/');
        wfBaseDir = idx >= 0 ? path.substring(0, idx) : '.';
      }
      loadTemplateToEditor(card.dataset.name);
    });
  });
}

function highlightEditorTemplate() {
  document.querySelectorAll('#editor-template-list .tpl-card').forEach(card => {
    card.classList.toggle('active', card.dataset.name === wfMeta.name);
  });
}

async function loadTemplateToEditor(name) {
  if (!name) return;
  try {
    const result = await api(`/api/v1/templates/${encodeURIComponent(name)}/yaml`);
    if (result.yaml) {
      importFromYaml(result.yaml);
      highlightEditorTemplate();
    } else {
      showToast(result.error || '加载模板失败', 'error');
    }
  } catch (e) {
    showToast('网络错误: ' + e.message, 'error');
  }
}

// ── Draft ─────────────────────────────────────────────────────────────
function saveDraft() {
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    try {
      localStorage.setItem('acpx-editor-draft', JSON.stringify({
        meta: wfMeta,
        nodes: Object.fromEntries(nodeStore),
        canvas: dfEditor?.export(),
        idMap: { toBiz: Object.fromEntries(dfIdToBizId), toDf: Object.fromEntries(bizIdToDfId) },
        counter: nodeIdCounter,
        ts: Date.now(),
      }));
    } catch (e) { /* ignore */ }
  }, 1000);
}

function loadDraft() {
  try {
    const raw = localStorage.getItem('acpx-editor-draft');
    if (!raw) return;
    const draft = JSON.parse(raw);
    if (!draft.nodes || !draft.canvas) return;

    wfMeta = draft.meta || wfMeta;
    const nameEl = document.getElementById('wf-name');
    const verEl = document.getElementById('wf-version');
    if (nameEl) nameEl.value = wfMeta.name;
    if (verEl) verEl.value = wfMeta.version;

    nodeStore.clear();
    Object.entries(draft.nodes).forEach(([k, v]) => nodeStore.set(k, v));
    if (dfEditor && draft.canvas) dfEditor.import(draft.canvas);

    dfIdToBizId.clear(); bizIdToDfId.clear();
    if (draft.idMap) {
      Object.entries(draft.idMap.toBiz || {}).forEach(([k, v]) => dfIdToBizId.set(k, v));
      Object.entries(draft.idMap.toDf || {}).forEach(([k, v]) => bizIdToDfId.set(k, v));
    }
    nodeIdCounter = draft.counter || nodeStore.size;
    updateYamlFromCanvas(); displayValidation([]); pushHistory();
  } catch (e) { /* corrupt draft */ }
}
