# 前端消息 ID 去重 执行计划

**目标:** 在前端 agent.messages 中引入基于消息 ID 的 upsert 语义，消除断线重连和实时推送导致的消息重复

**技术栈:** 纯 ES Module JavaScript（无构建工具）

**设计文档:** ./spec-design.md

---

### Task 1: state.js 新增 upsertMessage

**涉及文件:**
- 修改: `rust-relay-server/web/js/state.js`

**执行步骤:**
- [x] 在 state.js 末尾新增并导出 `upsertMessage(agent, msg)` 函数
  - 若 `msg.id` 存在，用 `findIndex` 在 `agent.messages` 中查找同 id 的条目
  - 找到则 merge（`{ ...old, ...new }`），保留旧字段（如 `output`、`streaming`）
  - 未找到或无 id 则 `agent.messages.push(msg)`

**检查步骤:**
- [x] 确认 `upsertMessage` 已导出
  - `grep -n "export function upsertMessage" rust-relay-server/web/js/state.js`
  - 预期: 输出含 `export function upsertMessage` 的行
- [x] 确认函数体含 findIndex 去重逻辑
  - `grep -n "findIndex" rust-relay-server/web/js/state.js`
  - 预期: 输出含 `findIndex` 的行
- [x] 确认 merge 语义（spread）
  - `grep -n "\.\.\.agent\.messages\[idx\]" rust-relay-server/web/js/state.js`
  - 预期: 输出含 spread merge 的行

---

### Task 2: events.js 调用 upsertMessage

**涉及文件:**
- 修改: `rust-relay-server/web/js/events.js`

**执行步骤:**
- [x] 在 events.js 顶部 import 中添加 `upsertMessage`
  - 将 `import { state, upsertAgent, getAgent, setPaneAgent } from './state.js';` 改为同时导入 `upsertMessage`
- [x] 修改 `handleBaseMessage` 的 `user` 分支
  - 将 `agent.messages.push({ type: 'user', text, seq: event.seq })` 替换为
    `upsertMessage(agent, { type: 'user', text, id: event.id, seq: event.seq })`
  - user 消息现在也携带 `id` 字段，支持去重
- [x] 修改 `handleBaseMessage` 的 `assistant` 分支（无 tool_calls 时的文本消息）
  - 将 `agent.messages.push({ type: 'assistant', text, streaming: false, id: event.id })` 替换为
    `upsertMessage(agent, { type: 'assistant', text, streaming: false, id: event.id })`
- [x] 保持 tool_calls 创建的 tool slot 不变（继续使用 push，无 id 去重需求）
- [x] 保持 tool role 分支不变（按 tool_call_id 匹配）

**检查步骤:**
- [x] 确认 import 包含 upsertMessage
  - `grep -n "upsertMessage" rust-relay-server/web/js/events.js | head -5`
  - 预期: 第一行为 import 声明，后续有 2 处调用
- [x] 确认 user 分支使用 upsertMessage 且携带 id
  - `grep -A1 "case 'user'" rust-relay-server/web/js/events.js`
  - 预期: 下一行含 `upsertMessage` 而非 `push`
- [x] 确认 assistant 分支（text push 行）使用 upsertMessage
  - `grep -n "type: 'assistant'.*streaming: false" rust-relay-server/web/js/events.js`
  - 预期: 该行以 `upsertMessage` 开头
- [x] 确认 handleBaseMessage 中不再有裸 push user/assistant 消息
  - `grep -n "agent\.messages\.push.*type: 'user'" rust-relay-server/web/js/events.js`
  - 预期: 无输出（user 消息已全部走 upsertMessage）

---

### Task 3: 消息去重 Acceptance

**前置条件:**
- 启动命令: `cargo run -p rust-relay-server --features server`（可选，用于集成验证）
- 静态文件位于 `rust-relay-server/web/js/`，无需构建，可直接 grep 验证逻辑

**端到端验证:**

1. ✅ **upsertMessage 导出正确**
   - `node -e "import('./rust-relay-server/web/js/state.js').then(m => { console.log(typeof m.upsertMessage); })"`
   - Expected: 输出 `function`
   - On failure: 检查 Task 1 state.js 导出

2. ✅ **同 ID 消息不重复插入**
   - 通过 node 脚本验证逻辑：
   ```
   node -e "
   import('./rust-relay-server/web/js/state.js').then(({ upsertMessage }) => {
     const agent = { messages: [] };
     upsertMessage(agent, { id: 'abc', type: 'user', text: 'hello' });
     upsertMessage(agent, { id: 'abc', type: 'user', text: 'hello' });
     console.log(agent.messages.length);
   });"
   ```
   - Expected: 输出 `1`（第二次 upsert 不追加新条目）
   - On failure: 检查 Task 1 findIndex 逻辑

3. ✅ **同 ID 消息触发 merge 更新**
   - ```
     node -e "
     import('./rust-relay-server/web/js/state.js').then(({ upsertMessage }) => {
       const agent = { messages: [] };
       upsertMessage(agent, { id: 'abc', type: 'assistant', text: 'old', streaming: true });
       upsertMessage(agent, { id: 'abc', type: 'assistant', text: 'new', streaming: false });
       console.log(agent.messages[0].text, agent.messages[0].streaming);
     });"
     ```
   - Expected: 输出 `new false`
   - On failure: 检查 Task 1 spread merge 语义

4. ✅ **无 ID 消息仍按追加逻辑处理（legacy 兼容）**
   - ```
     node -e "
     import('./rust-relay-server/web/js/state.js').then(({ upsertMessage }) => {
       const agent = { messages: [] };
       upsertMessage(agent, { type: 'assistant', text: 'a' });
       upsertMessage(agent, { type: 'assistant', text: 'b' });
       console.log(agent.messages.length);
     });"
     ```
   - Expected: 输出 `2`（无 id 时每次追加）
   - On failure: 检查 Task 1 中 `if (msg.id)` 条件分支

5. ✅ **events.js 文件语法正确（无 parse 错误）**
   - `node --input-type=module < /dev/null 2>&1; node --check rust-relay-server/web/js/events.js 2>&1`
   - Expected: 无输出（无语法错误）
   - On failure: 检查 Task 2 修改是否引入语法错误
