// dialog.js — 弹窗管理
import { sendMessage } from './connection.js';

// ─── 工具 ─────────────────────────────────────────────────

function escHtml(str) {
  if (str === null || str === undefined) return '';
  const div = document.createElement('div');
  div.textContent = String(str);
  return div.innerHTML;
}

// ─── 关闭弹窗 ─────────────────────────────────────────────

export function closeDialog(type) {
  const modal = document.getElementById(`${type}-modal`);
  if (modal) modal.classList.add('hidden');
}

// ─── HITL 弹窗 ─────────────────────────────────────────────

export function showHitlDialog(agent, sessionId) {
  if (!agent || !agent.pendingHitl) return;
  const modal = document.getElementById('hitl-modal');
  const itemsEl = document.getElementById('hitl-items');
  if (!modal || !itemsEl) return;

  const requests = agent.pendingHitl.requests || [];
  itemsEl.innerHTML = '';

  (Array.isArray(requests) ? requests : [requests]).forEach(req => {
    const div = document.createElement('div');
    div.className = 'hitl-item';
    const inputStr =
      typeof req.input === 'string'
        ? req.input
        : JSON.stringify(req.input, null, 2);
    div.innerHTML = `
      <div class="tool-info">
        <span class="tool-name" style="color: var(--accent); font-weight: bold;">${escHtml(req.name || 'tool')}</span>
      </div>
      <div class="tool-input">${escHtml(inputStr)}</div>
    `;
    itemsEl.appendChild(div);
  });

  modal.classList.remove('hidden');

  // 绑定 approve/reject 事件
  const approveBtn = document.getElementById('hitl-approve-all');
  const rejectBtn = document.getElementById('hitl-reject-all');
  const closeBtn = document.getElementById('hitl-close');

  const cleanup = () => {
    approveBtn && approveBtn.removeEventListener('click', onApprove);
    rejectBtn && rejectBtn.removeEventListener('click', onReject);
    closeBtn && closeBtn.removeEventListener('click', onClose);
  };

  const onApprove = () => {
    const reqs = agent.pendingHitl.requests || [];
    const decisions = (Array.isArray(reqs) ? reqs : [reqs]).map(r => ({
      tool_call_id: r.tool_call_id || r.id || '',
      decision: 'Approve',
    }));
    sendMessage(sessionId, { type: 'hitl_decision', decisions });
    agent.pendingHitl = null;
    closeDialog('hitl');
    cleanup();
    import('./render.js').then(({ renderPane }) => renderPane(state.activePane, sessionId));
  };

  const onReject = () => {
    const reqs = agent.pendingHitl.requests || [];
    const decisions = (Array.isArray(reqs) ? reqs : [reqs]).map(r => ({
      tool_call_id: r.tool_call_id || r.id || '',
      decision: 'Reject',
    }));
    sendMessage(sessionId, { type: 'hitl_decision', decisions });
    agent.pendingHitl = null;
    closeDialog('hitl');
    cleanup();
    import('./render.js').then(({ renderPane }) => renderPane(state.activePane, sessionId));
  };

  const onClose = () => {
    closeDialog('hitl');
    cleanup();
  };

  approveBtn && approveBtn.addEventListener('click', onApprove);
  rejectBtn && rejectBtn.addEventListener('click', onReject);
  closeBtn && closeBtn.addEventListener('click', onClose);

  // overlay 点击关闭
  const overlay = modal.querySelector('.modal-overlay');
  if (overlay) {
    overlay.onclick = onClose;
  }
}

// ─── AskUser 弹窗 ───────────────────────────────────────────

export function showAskUserDialog(agent, sessionId) {
  if (!agent || !agent.pendingAskUser) return;
  const modal = document.getElementById('askuser-modal');
  const itemsEl = document.getElementById('askuser-items');
  if (!modal || !itemsEl) return;

  const questions = agent.pendingAskUser.questions || [];
  itemsEl.innerHTML = '';

  (Array.isArray(questions) ? questions : [questions]).forEach((q, i) => {
    const div = document.createElement('div');
    div.style.marginBottom = '14px';

    const label = document.createElement('label');
    label.textContent = q.question || q.text || `问题 ${i + 1}`;
    label.style.display = 'block';
    label.style.marginBottom = '6px';
    label.style.color = 'var(--text-primary)';
    label.style.fontSize = '13px';
    div.appendChild(label);

    if (q.options && q.options.length > 0) {
      q.options.forEach(opt => {
        const optDiv = document.createElement('div');
        optDiv.style.marginBottom = '4px';

        const radio = document.createElement('input');
        radio.type = q.multi_select ? 'checkbox' : 'radio';
        radio.name = `askuser_${i}`;
        radio.value = opt.label || opt;
        radio.style.marginRight = '6px';
        radio.style.accentColor = 'var(--accent)';

        const optLabel = document.createElement('span');
        optLabel.textContent = ` ${opt.label || opt}`;
        if (opt.description) optLabel.textContent += ` — ${opt.description}`;
        optLabel.style.color = 'var(--text-muted)';
        optLabel.style.fontSize = '13px';

        optDiv.appendChild(radio);
        optDiv.appendChild(optLabel);
        div.appendChild(optDiv);
      });
    } else {
      const input = document.createElement('input');
      input.type = 'text';
      input.name = `askuser_${i}`;
      input.style.cssText =
        'width:100%; padding:8px; background:var(--bg-surface); border:1px solid var(--border); ' +
        'color:var(--text-primary); border-radius:6px; font-size:13px; font-family:inherit; outline:none;';
      input.addEventListener('focus', () => { input.style.borderColor = 'var(--accent)'; });
      input.addEventListener('blur', () => { input.style.borderColor = 'var(--border)'; });
      div.appendChild(input);
    }

    itemsEl.appendChild(div);
  });

  modal.classList.remove('hidden');

  const submitBtn = document.getElementById('askuser-submit');
  const closeBtn = document.getElementById('askuser-close');

  const cleanup = () => {
    submitBtn && submitBtn.removeEventListener('click', onSubmit);
    closeBtn && closeBtn.removeEventListener('click', onClose);
  };

  const onSubmit = () => {
    const qs = agent.pendingAskUser.questions || [];
    const answers = {};

    (Array.isArray(qs) ? qs : [qs]).forEach((q, i) => {
      const qText = q.question || q.text || `q${i}`;
      const inputs = itemsEl.querySelectorAll(`[name="askuser_${i}"]`);
      if (inputs.length === 1 && inputs[0].type === 'text') {
        answers[qText] = inputs[0].value;
      } else {
        const selected = Array.from(inputs)
          .filter(el => el.checked)
          .map(el => el.value);
        answers[qText] = selected.join(', ');
      }
    });

    sendMessage(sessionId, { type: 'ask_user_response', answers });
    agent.pendingAskUser = null;
    closeDialog('askuser');
    cleanup();
    import('./render.js').then(({ renderPane }) => renderPane(state.activePane, sessionId));
  };

  const onClose = () => {
    closeDialog('askuser');
    cleanup();
  };

  submitBtn && submitBtn.addEventListener('click', onSubmit);
  closeBtn && closeBtn.addEventListener('click', onClose);

  const overlay = modal.querySelector('.modal-overlay');
  if (overlay) {
    overlay.onclick = onClose;
  }
}
