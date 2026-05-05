// ── API Docs Modal (Nexus) ────────────────────────────────────

let _apiEndpoints = null;

function curlBlock(code) {
  return `<div class="api-curl"><button class="code-copy-btn" onclick="copyToClipboard(this.parentElement.querySelector('code').textContent)">复制</button><code>${escapeHtml(code)}</code></div>`;
}

async function fetchApiEndpoints() {
  if (_apiEndpoints) return _apiEndpoints;
  const data = await api('/api/v1/docs');
  _apiEndpoints = data.endpoints || [];
  return _apiEndpoints;
}

function renderEndpoint(ep) {
  const methodClass = ep.method === 'GET' ? 'method-get' : 'method-post';
  let paramsHtml = '';
  if (ep.params?.length) {
    paramsHtml = `<table class="api-param-table">
      <tr><th>参数</th><th>类型</th><th>说明</th></tr>
      ${ep.params.map(p => `<tr><td><code>${escapeHtml(p.name)}</code></td><td>${escapeHtml(p.type)}</td><td>${escapeHtml(p.description)}</td></tr>`).join('')}
    </table>`;
  }

  let responseHtml = '';
  if (ep.response) {
    responseHtml = `<div class="api-response"><span class="api-response-label">响应</span><pre>${escapeHtml(ep.response)}</pre></div>`;
  }

  return `<div class="api-section">
    <div class="api-endpoint-header">
      <span class="method ${methodClass}">${escapeHtml(ep.method)}</span>
      <code class="api-path">${escapeHtml(ep.path)}</code>
    </div>
    <p style="font-size:12px;color:var(--text-secondary);margin:6px 0 10px;">${escapeHtml(ep.description)}</p>
    ${paramsHtml}
    ${curlBlock(ep.curl)}
    ${responseHtml}
  </div>`;
}

async function showApiDocs() {
  const overlay = document.getElementById('apiDocsOverlay');
  const body = document.getElementById('apiDocsBody');
  if (!overlay || !body) return;

  overlay.classList.add('open');
  body.innerHTML = '<div style="text-align:center;padding:24px;"><div class="spinner-lg"></div></div>';

  let endpoints;
  try {
    endpoints = await fetchApiEndpoints();
  } catch (e) {
    body.innerHTML = '<div style="color:var(--status-error);padding:16px;font-size:13px;">加载 API 文档失败</div>';
    return;
  }

  body.innerHTML = endpoints.map(ep => renderEndpoint(ep)).join('');
  lucide.createIcons({ nodes: [body] });
}

function showTemplateApi(templateName) {
  const overlay = document.getElementById('apiDocsOverlay');
  const body = document.getElementById('apiDocsBody');
  if (!overlay || !body) return;

  overlay.classList.add('open');
  body.innerHTML = '<div style="text-align:center;padding:24px;"><div class="spinner-lg"></div></div>';

  fetchApiEndpoints().then(endpoints => {
    const filtered = endpoints.filter(ep => ep.category === 'templates');
    const html = filtered.map(ep => {
      const concrete = { ...ep };
      concrete.path = ep.path.replace('{name}', templateName);
      concrete.curl = ep.curl.replace('{name}', templateName);
      return renderEndpoint(concrete);
    }).join('');
    body.innerHTML = html;
    lucide.createIcons({ nodes: [body] });
  }).catch(() => {
    body.innerHTML = '<div style="color:var(--status-error);padding:16px;font-size:13px;">加载失败</div>';
  });
}
