# relay-loading-state-sync 人工验收清单

**生成时间:** 2026-03-26
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 编译 relay server 和 TUI: `cargo build -p rust-relay-server -p rust-agent-tui 2>&1 | tail -3`
- [ ] [AUTO] 确认 RELAY_TOKEN 环境变量已设置（或在命令行指定）: `echo "将在启动命令中通过 RELAY_TOKEN=test 指定"`
- [ ] [AUTO/SERVICE] 启动 Relay Server（新终端）: `RELAY_TOKEN=test cargo run -p rust-relay-server` (port: 8080)
- [ ] [MANUAL] 启动 TUI 并连接 relay（另一个新终端，需要有效的 API Key 配置）: `cargo run -p rust-agent-tui -- --remote-control ws://localhost:8080 --relay-token test -y`
- [ ] [MANUAL] 在浏览器中打开页面：`http://localhost:8080?token=test`，确认侧边栏出现已连接的 Agent

### 测试数据准备
- 测试用 token: `test`（与启动命令中 RELAY_TOKEN 保持一致）
- 测试消息: 发送 `hello` 或任意简短文字即可触发 loading
- 长时工具调用消息（用于场景4）: 发送 `请用 bash 执行 sleep 5` 可触发约 5 秒的 loading 等待

---

## 验收项目

### 场景 1：静态代码验证

#### - [x] 1.1 后端事件注入代码正确性
- **来源:** Task 1 执行步骤
- **操作步骤:**
  1. [A] `grep -n "agent_running" rust-agent-tui/src/app/agent.rs` → 期望: 输出至少 1 行，包含 `agent_running`
  2. [A] `grep -n "agent_done" rust-agent-tui/src/app/agent.rs` → 期望: 输出至少 1 行，包含 `agent_done`
  3. [A] `grep -c "send_value" rust-agent-tui/src/app/agent.rs` → 期望: 输出数字 `>= 2`（agent_running + agent_done 各一次）
- **异常排查:**
  - 若找不到 agent_running: 检查 `rust-agent-tui/src/app/agent.rs` 第 229-235 行是否已添加 send_value 调用

#### - [x] 1.2 编译与单元测试通过
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | grep -E "^error|Finished"` → 期望: 输出含 `Finished` 且无 `error:` 开头的行
  2. [A] `cargo test -p rust-relay-server --lib -- test_relay 2>&1 | tail -3` → 期望: 输出含 `ok` 且无 `FAILED`
- **异常排查:**
  - 编译失败: 检查 relay_client 变量是否在 match 块之后仍然可用（未被 move）

---

### 场景 2：前端 Loading 状态基础功能

> **前置条件:** 完成验收前准备，浏览器已打开 `http://localhost:8080?token=test`，侧边栏可见已连接的 Agent

#### - [x] 2.1 发送消息后 loading 出现
- **来源:** Task 5.1，spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在浏览器 `http://localhost:8080?token=test` 的消息输入框中输入 `hello` 并按 Enter 发送 → 发送后 **1秒内**，输入框左侧是否出现橙色文字「正在思考…」→ 是/否
- **异常排查:**
  - 若未出现: 打开浏览器 DevTools → Network → WS → 检查 WebSocket 帧中是否收到 `{"type":"agent_running"...}` 消息
  - 若 WS 无该消息: 检查后端 agent.rs 是否正确发送（Task 1）

#### - [x] 2.2 Agent 完成后 loading 消失
- **来源:** Task 5.2，spec-design.md 验收标准
- **操作步骤:**
  1. [H] 等待 Agent 回复完成（消息区域出现 AI 回复文字）→ 输入框左侧的「正在思考…」是否已消失（变为空白）→ 是/否
- **异常排查:**
  - 若未消失: DevTools WS 帧中检查是否收到 `{"type":"agent_done"...}` 消息
  - 若 WS 无该消息: 检查后端 match result 块之后是否正确发送 agent_done

---

### 场景 3：状态文字样式外观

> **前置条件:** 场景 2 已验证，loading 可正常出现和消失

#### - [x] 3.1 状态文字样式符合设计
- **来源:** Task 4，spec-design.md 设计方案
- **操作步骤:**
  1. [A] `grep -A5 "\.agent-status {" rust-relay-server/web/style.css` → 期望: 输出包含 `color: var(--accent)` 和 `transition: opacity`
  2. [H] 再次发送消息（如 `hello`），观察 loading 出现时文字颜色 → 文字是否为橙色（与界面主题强调色一致）→ 是/否
  3. [H] 观察「正在思考…」出现和消失时是否有渐变效果（非突然闪现）→ 是/否
- **异常排查:**
  - 颜色不对: 检查 `style.css` 中 `.agent-status { color: var(--accent); }` 是否存在
  - 无渐变: 检查 `transition: opacity 0.2s` 和 `.agent-status.visible { opacity: 1; }` 是否存在

---

### 场景 4：历史事件缓存与重连恢复

#### - [x] 4.1 agent_running/agent_done 事件含 seq 且被历史缓存
- **来源:** Task 5.3，spec-design.md Sync 重放行为
- **操作步骤:**
  1. [H] 在浏览器 DevTools → Network → WS → 找到 session WebSocket 连接（URL 含 `session=`） → 查看帧列表，找到 `type:"sync_response"` 帧 → 展开 events 数组，是否包含 `{"type":"agent_running","seq":N}` 和 `{"type":"agent_done","seq":M}` 两种事件 → 是/否
- **异常排查:**
  - 若无这两个事件: 确认已发送过至少一条消息并等待 Agent 完成；检查 relay_client.send_value 调用是否在执行路径上

#### - [x] 4.2 刷新页面后 loading 状态正确恢复
- **来源:** Task 5.4，spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在输入框发送 `请用 bash 执行 sleep 5`，确认「正在思考…」出现
  2. [H] 在「正在思考…」显示期间（Agent 未完成时）立即按 F5 刷新浏览器 → 页面重新加载后重连，是否仍然显示「正在思考…」→ 是/否
- **异常排查:**
  - 若刷新后不显示: 检查 events.js 的 sync_response 处理路径，确认重放历史事件时经过 handleSingleEvent → handleLegacyEvent → agent_running case

---

### 场景 5：出错场景 loading 清除

#### - [x] 5.1 Agent 出错时 loading 正确清除
- **来源:** Task 5.5，spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在 TUI 终端中按 Ctrl+C 中断 Agent 进程（或通过 TUI 发送导致错误的请求），然后在浏览器中观察 → 若之前有「正在思考…」显示，中断/出错后是否消失 → 是/否
- **异常排查:**
  - 若未消失: 检查 events.js 中 `case 'error':` 是否有 `agent.isRunning = false;` 或 agent_done 是否在所有错误分支后都被发送

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景1 | 1.1 | 后端事件注入代码正确性 | 3 | 0 | ✅ | |
| 场景1 | 1.2 | 编译与单元测试通过 | 2 | 0 | ✅ | |
| 场景2 | 2.1 | 发送消息后 loading 出现 | 0 | 1 | ✅ | |
| 场景2 | 2.2 | Agent 完成后 loading 消失 | 0 | 1 | ✅ | |
| 场景3 | 3.1 | 状态文字样式符合设计 | 1 | 2 | ✅ | |
| 场景4 | 4.1 | 事件含 seq 且被历史缓存 | 0 | 1 | ✅ | |
| 场景4 | 4.2 | 刷新页面后 loading 状态恢复 | 0 | 2 | ✅ | |
| 场景5 | 5.1 | 出错时 loading 正确清除 | 0 | 1 | ✅ | |

**验收结论:** ✅ 全部通过
