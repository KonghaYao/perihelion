# relay-command-sync 人工验收清单

**生成时间:** 2026-03-27
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 全量编译无错误: `cargo build --workspace 2>&1 | grep -E "^error" | wc -l`
- [ ] [AUTO/SERVICE] 启动 Relay Server（需设置 RELAY_TOKEN 环境变量）: `RELAY_TOKEN=test cargo run -p rust-relay-server --features server` (port: 8080)
- [ ] [AUTO] 验证 Relay Server 健康: `curl -s http://localhost:8080/health`
- [ ] [MANUAL] 在另一个终端启动 Agent TUI 并连接 Relay（需设置 API Key）：`ANTHROPIC_API_KEY=<your-key> cargo run -p rust-agent-tui -- -y --remote-control ws://localhost:8080 --relay-token test --relay-name test-agent`
- [ ] [MANUAL] 打开浏览器，访问 `http://localhost:8080/web/?token=test`，确认看到 Agent 在线（侧边栏显示绿点）

### 测试数据准备

- [ ] [MANUAL] 在浏览器 Web UI 的输入框中发送 2-3 条普通消息（如「你好」「帮我解释一下变量」），确保消息列表不为空（供后续测试场景使用）

---

## 验收项目

### 场景 1：编译与代码质量验证

> 本场景全部可自动化验证，无需浏览器操作。

#### - [x] 1.1 全量构建无错误

- **来源:** Task 6 端到端验证
- **操作步骤:**
  1. [A] `cargo build --workspace 2>&1 | grep -E "^error" | wc -l` → 期望: 输出 `0`
  2. [A] `cargo build -p rust-relay-server --features server 2>&1 | tail -3` → 期望: 输出包含 `Finished`，无 `error`
- **异常排查:**
  - 如果编译报错: 检查 `rust-relay-server/src/protocol.rs` 中 `CompactThread` 和 `ThreadReset` 变体语法

#### - [x] 1.2 协议序列化单元测试全部通过

- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-relay-server --lib -- protocol::tests 2>&1 | tail -5` → 期望: 输出包含 `test result: ok. 12 passed; 0 failed`
  2. [A] `cargo test -p rust-relay-server --lib -- test_thread_reset 2>&1 | tail -3` → 期望: `test protocol::tests::test_thread_reset_serialization ... ok`
- **异常排查:**
  - 如果测试失败: 运行 `cargo test -p rust-relay-server --lib -- protocol::tests -- --nocapture` 查看详细错误

#### - [x] 1.3 代码实现覆盖度检查

- **来源:** Task 2-5 检查步骤
- **操作步骤:**
  1. [A] `grep -n "send_thread_reset" rust-relay-server/src/client/mod.rs` → 期望: 至少一行包含 `pub fn send_thread_reset`
  2. [A] `grep -A 8 "WebMessage::ClearThread" rust-agent-tui/src/app/relay_ops.rs | grep send_thread_reset` → 期望: 找到 `relay.send_thread_reset(&[])`
  3. [A] `grep -n "CompactThread\|start_compact" rust-agent-tui/src/app/relay_ops.rs` → 期望: 至少两行，含 `CompactThread` 和 `start_compact`
  4. [A] `grep -A 15 "pub fn new_thread" rust-agent-tui/src/app/thread_ops.rs | grep send_thread_reset` → 期望: 找到匹配行
  5. [A] `grep -A 55 "pub fn open_thread" rust-agent-tui/src/app/thread_ops.rs | grep -E "send_thread_reset|clear_history"` → 期望: 两行，分别含 `clear_history` 和 `send_thread_reset`
  6. [A] `grep -n "compact_thread" rust-relay-server/web/js/render.js` → 期望: 包含 `type: 'compact_thread'`
  7. [A] `grep -n "thread_reset" rust-relay-server/web/js/events.js` → 期望: 包含 `case 'thread_reset'`
- **异常排查:**
  - 如果某个 grep 无输出: 检查对应文件中的实现代码

---

### 场景 2：Web 前端 /clear 命令

> 前提：Relay Server 运行中，Agent TUI 已连接，浏览器已打开且消息列表非空。

#### - [x] 2.1 Web 前端输入 /clear 清空消息列表

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -n "clear_thread\|agent\.messages = \[\]" rust-relay-server/web/js/render.js` → 期望: 至少两行，分别含 `clear_thread` 和 `agent.messages = []`
  2. [H] 在浏览器输入框中输入 `/clear`，按 Enter 发送 → 消息列表是否立即清空（原有消息消失）？ 是/否
  3. [H] 查看 Agent TUI 终端，是否出现了新 thread 提示（消息列表被重置，无旧消息）？ 是/否
  4. [H] 在浏览器再发送一条新消息「测试」，确认消息列表正常显示新消息 → 新消息是否正常显示？ 是/否
- **异常排查:**
  - 如果消息列表未清空: 打开浏览器 DevTools Console，检查是否有 WebSocket 连接错误
  - 如果 TUI 未响应: 确认 TUI 连接的 Relay Server 地址和 token 正确

---

### 场景 3：Web 前端 /compact 命令

> 前提：Relay Server 运行中，Agent TUI 已连接并有多条消息历史（LLM API Key 有效），浏览器已打开。

#### - [x] 3.1 Web 前端输入 /compact 触发压缩并同步消息

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -n "compact_thread" rust-relay-server/web/js/render.js` → 期望: 包含 `type: 'compact_thread'`
  2. [H] 确认 Agent TUI 当前对话有至少 3 条消息（若没有，先发送几条消息给 Agent）→ 消息数量是否 ≥3？ 是/否
  3. [H] 在浏览器输入框中输入 `/compact`，按 Enter 发送 → 输入框是否清空（命令已发出）？ 是/否
  4. [H] 等待约 10-30 秒（LLM 压缩需要时间），观察 Agent TUI 是否显示「📦 上下文已压缩」提示 → 是否出现压缩提示？ 是/否
  5. [H] 观察浏览器消息列表是否被替换（原有消息消失，显示压缩摘要「📋 压缩摘要：...」）→ 是否显示压缩后的内容？ 是/否
- **异常排查:**
  - 如果 TUI 无反应: 检查 ANTHROPIC_API_KEY 是否有效，查看 TUI 错误提示
  - 如果浏览器消息列表未更新: 打开 DevTools Network，检查 WebSocket 中是否有 `thread_reset` 消息到达

---

### 场景 4：Agent TUI /clear 同步到 Web 前端

> 前提：Relay Server 运行中，Agent TUI 已连接，浏览器已打开且消息列表非空。

#### - [x] 4.1 Agent TUI 执行 /clear 后 Web 前端自动清空

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -A 15 "pub fn new_thread" rust-agent-tui/src/app/thread_ops.rs | grep send_thread_reset` → 期望: 找到 `relay.send_thread_reset(&[])`
  2. [H] 确认浏览器消息列表当前有消息（非空）→ 消息列表是否非空？ 是/否
  3. [H] 在 Agent TUI 输入框中输入 `/clear` 并按 Enter → TUI 消息列表是否立即清空？ 是/否
  4. [H] 观察浏览器页面，在 1 秒内消息列表是否也自动清空（无需刷新页面）？ 是/否
- **异常排查:**
  - 如果浏览器消息未清空: 打开 DevTools Network，找到 WebSocket 连接，查看 Messages，检查是否有 `{"type":"thread_reset","messages":[]}` 帧
  - 如果 TUI 清空了但浏览器没更新: 检查 relay_client 是否已连接（TUI 启动时是否显示了 Relay 连接成功）

---

### 场景 5：Agent TUI 历史切换 + Compact 完成同步

> 前提：Relay Server 运行中，Agent TUI 已连接，浏览器已打开，且 TUI 中有至少 1 条历史对话。

#### - [x] 5.1 Agent TUI 切换历史后 Web 前端消息列表替换

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -A 55 "pub fn open_thread" rust-agent-tui/src/app/thread_ops.rs | grep -E "send_thread_reset|clear_history"` → 期望: 两行分别匹配
  2. [H] 在 Agent TUI 输入 `/history` 打开历史面板，按 ↑↓ 选择一条有内容的历史，按 Enter 打开 → 历史是否成功加载到 TUI？ 是/否
  3. [H] 观察浏览器消息列表，是否被替换为所选历史的消息内容（与 TUI 显示一致）？ 是/否
- **异常排查:**
  - 如果浏览器未更新: 确认 `open_thread` 中已添加 `relay.send_thread_reset(&base_msgs)` 调用（运行检查步骤 1）
  - 如果历史消息在浏览器中显示错乱: 检查 `events.js` 中 `handleBaseMessage` 对 user/assistant/tool 角色的处理是否正确

#### - [x] 5.2 Compact 完成后 Web 前端显示压缩后内容

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -A 60 "AgentEvent::CompactDone" rust-agent-tui/src/app/agent_ops.rs | grep send_thread_reset` → 期望: 找到匹配行
  2. [H] 在 Agent TUI 输入 `/compact` 并等待完成，观察浏览器消息列表是否替换为压缩摘要（显示「📋 压缩摘要」类内容）→ 是/否
- **异常排查:**
  - 如果浏览器未更新: 检查 `agent_ops.rs` CompactDone 分支中 `send_thread_reset` 调用是否在 `agent_rx = None` 之后

---

### 场景 6：多 Web 客户端 ThreadReset 广播

> 本场景可部分自动化验证（代码机制），浏览器部分需手动。

#### - [x] 6.1 多 Web 客户端同时连接时均收到 ThreadReset

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -n "forward_to_web" rust-relay-server/src/relay.rs` → 期望: 至少一行，确认广播函数存在
  2. [A] `grep -A 10 "fn forward_to_web" rust-relay-server/src/relay.rs | grep "for tx in"` → 期望: 找到遍历所有 web_txs 的循环（确认广播机制）
  3. [A] `grep -n "send_raw\|forward_to_web" rust-relay-server/src/client/mod.rs` → 期望: `send_raw` 存在（ThreadReset 通过此路径发送）
  4. [A] `grep -c "web_txs" rust-relay-server/src/relay.rs` → 期望: ≥3（多处使用 web_txs，确认广播结构）
  5. [A] `grep -rn "send_thread_reset" rust-agent-tui/src/app/ | wc -l` → 期望: ≥4
  6. [A] `cargo test -p rust-relay-server --lib 2>&1 | tail -3` → 期望: `test result: ok`
- **异常排查:**
  - 如果 forward_to_web 循环未找到: 查看 `rust-relay-server/src/relay.rs` 中 `forward_to_web` 实现

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | 全量构建无错误 | 2 | 0 | ✅ | |
| 场景 1 | 1.2 | 协议序列化单元测试 | 2 | 0 | ✅ | |
| 场景 1 | 1.3 | 代码实现覆盖度检查 | 7 | 0 | ✅ | |
| 场景 2 | 2.1 | Web 前端 /clear 命令效果 | 1 | 3 | ✅ | |
| 场景 3 | 3.1 | Web 前端 /compact 命令效果 | 1 | 4 | ✅ | |
| 场景 4 | 4.1 | Agent TUI /clear 同步 Web | 1 | 3 | ✅ | |
| 场景 5 | 5.1 | Agent TUI 历史切换同步 Web | 1 | 2 | ✅ | |
| 场景 5 | 5.2 | Compact 完成后 Web 更新 | 1 | 1 | ✅ | |
| 场景 6 | 6.1 | 多 Web 客户端广播（代码验证） | 6 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
