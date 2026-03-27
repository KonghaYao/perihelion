# relay-loading-state-sync 执行计划

**目标:** 前后端协商 loading 状态同步——Agent 执行时前端显示「正在思考…」，完成后自动消失，刷新/重连后状态可恢复

**技术栈:** Rust (tokio, serde_json), JavaScript (ES Module, WebSocket)

**设计文档:** ./spec-design.md

---

### Task 1: 后端发送 loading 事件

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent.rs`

**执行步骤:**
- [x] 在 `executor.execute()` 调用前，通过 `relay_client` 发送 `agent_running` 事件
  - 在 `let agent_input = input;` 之后、`let result = executor.execute(...)` 之前插入：
    ```rust
    if let Some(ref relay) = relay_client {
        relay.send_value(serde_json::json!({ "type": "agent_running" }));
    }
    ```
- [x] 在 `match result { ... }` 块结束后（三个分支之后）统一发送 `agent_done`
  - 在最后一个 `}` 之后插入：
    ```rust
    if let Some(ref relay) = relay_client {
        relay.send_value(serde_json::json!({ "type": "agent_done" }));
    }
    ```
  - 注意：`relay_client` 在解构 cfg 时已经 move 进函数，需在代码中确保其可用；若已被 `relay_for_handler` 的 `clone()` 消耗，从 `relay_client` 原变量直接使用

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 `Finished` 且无 `error`
- [x] 事件类型序列化正确
  - `cargo test -p rust-relay-server --lib -- test_relay 2>&1 | tail -5`
  - 预期: 所有测试通过

---

### Task 2: 前端状态层 — isRunning 字段

**涉及文件:**
- 修改: `rust-relay-server/web/js/state.js`

**执行步骤:**
- [x] 在 `upsertAgent` 新建分支中，初始状态加 `isRunning: false`
  - 在 `maxSeq: data.maxSeq || 0,` 后追加：
    ```js
    isRunning: data.isRunning ?? false,
    ```
- [x] 在 `upsertAgent` 合并分支中，保留 isRunning 的合并（`...data` 已覆盖，无需额外处理）

**检查步骤:**
- [x] 字段存在于新建 agent 对象
  - 在浏览器 console 执行: `[...window.__state?.agents?.values()][0]?.isRunning`
  - 预期: 返回 `false`（非 undefined）

---

### Task 3: 前端事件层 — 处理 agent_running/agent_done

**涉及文件:**
- 修改: `rust-relay-server/web/js/events.js`

**执行步骤:**
- [x] 在 `handleLegacyEvent` switch 中新增两个 case（建议放在 `llm_call_start/end` 附近）：
  ```js
  case 'agent_running':
    agent.isRunning = true;
    break;

  case 'agent_done':
    agent.isRunning = false;
    break;
  ```
- [x] 在 `case 'error':` 处补充 `agent.isRunning = false;`（在现有逻辑前加）：
  ```js
  case 'error':
    agent.isRunning = false;
    agent.messages.push({ type: 'error', text: event['0'] || 'Error' });
    break;
  ```

**检查步骤:**
- [x] 模拟 agent_running 事件后 isRunning 变为 true
  - 在浏览器 console 手动触发: `import('./js/events.js').then(m => { const a = window.__state?.agents?.values()?.next()?.value; if(a) { m.handleSingleEvent([...window.__state.agents.keys()][0], {type:'agent_running'}); console.log(a.isRunning); } })`
  - 预期: 输出 `true`

---

### Task 4: 前端渲染层 — 状态文字显示

**涉及文件:**
- 修改: `rust-relay-server/web/js/render.js`
- 修改: `rust-relay-server/web/style.css`

**执行步骤:**
- [x] 在 `render.js` 的 `renderPane` 函数中，输入栏 `inputBar` HTML 模板里添加状态文字占位元素：
  ```js
  inputBar.innerHTML = `
    <span id="status-${paneId}" class="agent-status"></span>
    <input type="text" id="input-${paneId}" placeholder="输入消息..." autocomplete="off" />
    <button class="send-btn" data-pane="${paneId}">发送</button>
  `;
  ```
- [x] 在 `render.js` 中新增 `renderStatus(paneId, agent)` 函数（在 `renderTodoPanel` 附近）：
  ```js
  export function renderStatus(paneId, agent) {
    const el = document.getElementById(`status-${paneId}`);
    if (!el) return;
    el.textContent = agent.isRunning ? '正在思考…' : '';
    el.classList.toggle('visible', !!agent.isRunning);
  }
  ```
- [x] 在 `events.js` 的 `renderPaneForAllPanes` 中，增加 `renderStatus` 调用：
  ```js
  async function renderPaneForAllPanes() {
    const { renderMessages, renderTodoPanel, renderStatus } = await import('./render.js');
    state.layout.panes.forEach((sessionId, paneIdx) => {
      if (!sessionId) return;
      const agent = getAgent(sessionId);
      if (!agent) return;
      renderMessages(paneIdx, agent);
      renderTodoPanel(paneIdx, agent.todos);
      renderStatus(paneIdx, agent);
    });
  }
  ```
- [x] 在 `style.css` 的 `.pane-input` 区域末尾追加样式：
  ```css
  .agent-status {
    font-size: 12px;
    color: var(--accent);
    white-space: nowrap;
    opacity: 0;
    transition: opacity 0.2s;
    min-width: 80px;
  }
  .agent-status.visible {
    opacity: 1;
  }
  ```

**检查步骤:**
- [x] `#status-0` 元素存在于 DOM
  - `document.getElementById('status-0')` 在浏览器 console
  - 预期: 返回非 null 元素
- [x] 样式编译无冲突（静态检查）
  - `grep -n "agent-status" rust-relay-server/web/style.css`
  - 预期: 找到对应样式定义

---

### Task 5: Loading 状态同步验收

**Prerequisites:**
- 启动 relay server: `cargo run -p rust-relay-server`
- 启动 TUI 并连接 relay: `cargo run -p rust-agent-tui -- --remote-control ws://localhost:8080 --relay-token <token>`
- 在浏览器打开: `http://localhost:8080?token=<token>`

**端到端验证:**

1. **发送消息后 loading 出现**
   - 在 Web 前端输入框发送任意消息，观察输入栏左侧
   - 预期: 发送后立即出现「正在思考…」文字（橙色）
   - 失败时检查: Task 1（后端是否发 agent_running）、Task 3（前端是否处理 agent_running）

2. **Agent 回复完毕后 loading 消失**
   - 等待 Agent 回复完成
   - 预期: 「正在思考…」消失，输入栏恢复空白
   - 失败时检查: Task 1（后端是否发 agent_done）、Task 3（前端是否处理 agent_done）

3. **agent_running/agent_done 事件进入历史缓存**
   - 向 relay server session WS 发送 sync_request: `{"type":"sync_request","since_seq":0}` 后观察 sync_response
   - `curl -s --include --no-buffer -H "Connection: Upgrade" -H "Upgrade: websocket" ...` 或用浏览器 WS 工具
   - 预期: sync_response.events 中包含 `{type: "agent_running", seq: N}` 和 `{type: "agent_done", seq: M}`

4. **刷新页面后 loading 状态正确恢复（Agent 执行中）**
   - 发送一条触发长时工具调用的消息后立即刷新浏览器
   - 预期: 重连并 replay history 后，「正在思考…」仍然显示
   - 失败时检查: Task 3（events.js sync_response 重放路径是否经过 handleSingleEvent）

5. **Agent 出错时 loading 清除**
   - 手动触发 agent 错误场景（如发送导致 LLM 错误的请求）
   - 预期: 出现 error 消息后，「正在思考…」消失
   - 失败时检查: Task 3（error case 是否设置 isRunning = false）
