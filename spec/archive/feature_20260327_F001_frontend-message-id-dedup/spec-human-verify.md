# 前端消息 ID 去重 人工验收清单

**生成时间:** 2026-03-27
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

> ℹ️ 所有验收项均可自动化验证，无需人类参与。

---

## 验收前准备

### 环境要求
- [x] [AUTO] 检查 Node.js >= 18: `node -v`
- [x] [AUTO] 确认 state.js 文件存在: `test -f rust-relay-server/web/js/state.js && echo OK`
- [x] [AUTO] 确认 events.js 文件存在: `test -f rust-relay-server/web/js/events.js && echo OK`

---

## 验收项目

### 场景 1：upsertMessage 函数实现

#### - [x] 1.1 函数导出正确
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -n "export function upsertMessage" rust-relay-server/web/js/state.js` → 期望: 输出含 `export function upsertMessage` 的行
- **异常排查:**
  - 如果无输出: 检查 state.js 末尾是否已添加 upsertMessage 函数，确认 `export` 关键字存在

#### - [x] 1.2 findIndex 去重逻辑存在
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -n "findIndex" rust-relay-server/web/js/state.js` → 期望: 输出含 `findIndex(m => m.id === msg.id)` 的行
- **异常排查:**
  - 如果无输出: upsertMessage 函数体缺少 findIndex 查重逻辑

#### - [x] 1.3 spread merge 更新语义正确
- **来源:** Task 1 检查步骤 / spec-design.md 实现要点
- **操作步骤:**
  1. [A] `grep -n "\.\.\.agent\.messages\[idx\]" rust-relay-server/web/js/state.js` → 期望: 输出含 `{ ...agent.messages[idx], ...msg }` 的行
- **异常排查:**
  - 如果无输出: 确认 merge 行使用 spread 语法，旧字段在前、新字段在后

---

### 场景 2：消息去重行为

#### - [x] 2.1 同 ID 消息不重复插入
- **来源:** Task 3 端到端验证 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `node -e "import('./rust-relay-server/web/js/state.js').then(m => { console.log(typeof m.upsertMessage); })"` → 期望: 输出 `function`
  2. [A] `node -e "import('./rust-relay-server/web/js/state.js').then(({ upsertMessage }) => { const agent = { messages: [] }; upsertMessage(agent, { id: 'abc', type: 'user', text: 'hello' }); upsertMessage(agent, { id: 'abc', type: 'user', text: 'hello' }); console.log(agent.messages.length); });"` → 期望: 输出 `1`（第二次 upsert 不追加新条目）
- **异常排查:**
  - 如果输出 `2`: findIndex 逻辑未正确判断 id，或 id 字段名拼写错误

#### - [x] 2.2 同 ID 消息触发 merge 更新
- **来源:** Task 3 端到端验证 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `node -e "import('./rust-relay-server/web/js/state.js').then(({ upsertMessage }) => { const agent = { messages: [] }; upsertMessage(agent, { id: 'abc', type: 'assistant', text: 'old', streaming: true }); upsertMessage(agent, { id: 'abc', type: 'assistant', text: 'new', streaming: false }); console.log(agent.messages[0].text, agent.messages[0].streaming); });"` → 期望: 输出 `new false`
  2. [A] `node -e "import('./rust-relay-server/web/js/state.js').then(({ upsertMessage }) => { const agent = { messages: [] }; upsertMessage(agent, { id: 'xyz', output: 'result', streaming: true }); upsertMessage(agent, { id: 'xyz', streaming: false }); console.log(agent.messages[0].output); });"` → 期望: 输出 `result`（旧字段 output 通过 spread 保留）
- **异常排查:**
  - 如果 text 为 old 或 streaming 仍为 true: spread 顺序错误（应为 `{ ...old, ...new }`）
  - 如果 output 丢失: spread 顺序错误（应旧字段在前）

#### - [x] 2.3 无 ID 消息按追加处理（legacy 兼容）
- **来源:** Task 3 端到端验证 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `node -e "import('./rust-relay-server/web/js/state.js').then(({ upsertMessage }) => { const agent = { messages: [] }; upsertMessage(agent, { type: 'assistant', text: 'a' }); upsertMessage(agent, { type: 'assistant', text: 'b' }); console.log(agent.messages.length); });"` → 期望: 输出 `2`（无 id 时每次追加，不去重）
- **异常排查:**
  - 如果输出 `1`: `if (msg.id)` 分支判断有误，对 undefined id 也走了 findIndex 路径

---

### 场景 3：events.js 集成

#### - [x] 3.1 import 包含 upsertMessage
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -n "upsertMessage" rust-relay-server/web/js/events.js | head -5` → 期望: 第一行为 import 声明（含 `from './state.js'`），后续有 2 处函数调用
- **异常排查:**
  - 如果无 import 行: 确认顶部 import 语句已添加 upsertMessage

#### - [x] 3.2 user 分支使用 upsertMessage 且携带 id
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -A1 "case 'user'" rust-relay-server/web/js/events.js` → 期望: case 'user' 下一行含 `upsertMessage` 而非 `agent.messages.push`
- **异常排查:**
  - 如果下一行仍是 push: handleBaseMessage 的 user 分支未修改

#### - [x] 3.3 assistant 分支使用 upsertMessage
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -n "type: 'assistant'.*streaming: false" rust-relay-server/web/js/events.js` → 期望: 输出行以 `upsertMessage` 开头，而非 `agent.messages.push`
- **异常排查:**
  - 如果行以 push 开头: handleBaseMessage 的 assistant 文本消息分支未修改

#### - [x] 3.4 handleBaseMessage 无裸 push user/assistant 消息
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -n "agent\.messages\.push.*type: 'user'" rust-relay-server/web/js/events.js` → 期望: 仅有 handleLegacyEvent 中的 `user_message` case（context 中可见 `handleLegacyEvent` 函数名），handleBaseMessage 中无此模式
- **异常排查:**
  - 如果输出行的上下文来自 handleBaseMessage: 该函数的 user 分支未正确替换

---

### 场景 4：不影响范围验证

#### - [x] 4.1 render.js 无任何改动
- **来源:** spec-design.md 约束（改动集中在 state.js 和 events.js）
- **操作步骤:**
  1. [A] `git diff rust-relay-server/web/js/render.js` → 期望: 无输出（render.js 未被修改）
- **异常排查:**
  - 如果有 diff 输出: render.js 被意外修改，需检查并还原

#### - [x] 4.2 events.js 语法正确（无 parse 错误）
- **来源:** Task 3 端到端验证
- **操作步骤:**
  1. [A] `node --check rust-relay-server/web/js/events.js 2>&1; echo "exit:$?"` → 期望: 输出 `exit:0`，无错误信息
- **异常排查:**
  - 如果有语法错误: 检查 Task 2 中 import 行修改是否遗漏逗号，或 upsertMessage 调用括号是否匹配

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1: upsertMessage 函数实现 | 1.1 | 函数导出正确 | 1 | 0 | ✅ | |
| 场景 1 | 1.2 | findIndex 去重逻辑存在 | 1 | 0 | ✅ | |
| 场景 1 | 1.3 | spread merge 更新语义正确 | 1 | 0 | ✅ | |
| 场景 2: 消息去重行为 | 2.1 | 同ID消息不重复插入 | 2 | 0 | ✅ | |
| 场景 2 | 2.2 | 同ID消息触发 merge 更新 | 2 | 0 | ✅ | |
| 场景 2 | 2.3 | 无ID消息按追加处理 | 1 | 0 | ✅ | |
| 场景 3: events.js 集成 | 3.1 | import 包含 upsertMessage | 1 | 0 | ✅ | |
| 场景 3 | 3.2 | user 分支使用 upsertMessage | 1 | 0 | ✅ | |
| 场景 3 | 3.3 | assistant 分支使用 upsertMessage | 1 | 0 | ✅ | |
| 场景 3 | 3.4 | handleBaseMessage 无裸 push user | 1 | 0 | ✅ | |
| 场景 4: 不影响范围验证 | 4.1 | render.js 无改动 | 1 | 0 | ✅ | |
| 场景 4 | 4.2 | events.js 语法正确 | 1 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
