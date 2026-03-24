# 远程控制访问 人工验收清单

**生成时间:** 2026-03-23 23:00
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 确认 Rust 工具链可用: `rustc --version`
- [ ] [AUTO] 全量编译通过: `cargo build`
- [ ] [AUTO/SERVICE] 启动 Relay Server: `RELAY_TOKEN=test RELAY_PORT=18080 cargo run -p rust-relay-server` (port: 18080)

### 测试数据准备
- [ ] [MANUAL] 准备两个终端窗口，用于分别启动 Agent A 和 Agent B（配置 relay 后启动 TUI）

---

## 验收项目

### 场景 1：服务启动与健康检查

#### - [x] 1.1 Relay Server 启动与健康检查
- **来源:** Task 3 检查步骤 / Task 7 验收 #1
- **操作步骤:**
  1. [A] `curl -s -o /dev/null -w "%{http_code}" http://localhost:18080/health` → 期望: 输出 200
  2. [A] `curl -s "http://localhost:18080/agents?token=test"` → 期望: 返回 `[]`（空 JSON 数组）
- **异常排查:**
  - 如果返回连接拒绝: 检查 Relay Server 是否已启动，端口 18080 是否被占用（`lsof -i :18080`）
  - 如果 /agents 返回 401: 检查 RELAY_TOKEN 环境变量是否设置为 `test`

#### - [x] 1.2 Token 验证拒绝非法请求
- **来源:** Task 3 检查步骤 / Task 7 验收 #6
- **操作步骤:**
  1. [A] `curl -s -o /dev/null -w "%{http_code}" "http://localhost:18080/agents?token=invalid"` → 期望: 输出 401
  2. [A] `curl -s -o /dev/null -w "%{http_code}" "http://localhost:18080/agents"` → 期望: 输出 401（无 token 也被拒绝）
- **异常排查:**
  - 如果返回 200: 检查 `rust-relay-server/src/auth.rs` 的 `validate_token` 逻辑

### 场景 2：Feature 隔离与编译质量

#### - [x] 2.1 client feature 不引入 server 依赖
- **来源:** Task 1/4 检查步骤 / Task 7 验收 #8
- **操作步骤:**
  1. [A] `cargo tree -p rust-relay-server --no-default-features --features client 2>/dev/null | grep -c axum` → 期望: 输出 0
  2. [A] `cargo build -p rust-relay-server --no-default-features --features client 2>&1 | tail -1` → 期望: 输出包含 "Finished"
- **异常排查:**
  - 如果 axum 出现: 检查 `rust-relay-server/Cargo.toml` 中 axum 是否正确标记为 `optional = true` 且仅在 `server` feature 中引入

#### - [x] 2.2 全量编译无 warning
- **来源:** Task 7 验收 #11
- **操作步骤:**
  1. [A] `cargo build 2>&1 | grep -c "warning\[" || echo 0` → 期望: 输出 0
- **异常排查:**
  - 如果有 warning: 根据 warning 消息定位并修复

#### - [x] 2.3 rust-create-agent 现有测试无回归
- **来源:** Task 2 检查步骤 / Task 7 验收 #12
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent 2>&1 | grep "test result"` → 期望: 输出包含 "ok" 且 "0 failed"
- **异常排查:**
  - 如果测试失败: 检查 `rust-create-agent/src/agent/events.rs` 的 `Serialize`/`Deserialize` 派生是否破坏了现有 AgentEvent 结构

### 场景 3：Web 前端页面加载

#### - [x] 3.1 静态资源可访问
- **来源:** Task 5 检查步骤 / Task 7 验收 #4 #5
- **操作步骤:**
  1. [A] `curl -s -o /dev/null -w "%{http_code}" "http://localhost:18080/web/?token=test"` → 期望: 输出 200
  2. [A] `curl -s -o /dev/null -w "%{http_code}" "http://localhost:18080/web/app.js"` → 期望: 输出 200
  3. [A] `curl -s -o /dev/null -w "%{http_code}" "http://localhost:18080/web/style.css"` → 期望: 输出 200
- **异常排查:**
  - 如果返回 404: 检查 `rust-relay-server/src/static_files.rs` 中 `rust-embed` 的 `folder` 路径配置和路由注册

#### - [x] 3.2 页面布局与样式验证
- **来源:** spec-design.md Web 前端设计
- **操作步骤:**
  1. [H] 在浏览器打开 `http://localhost:18080/web/?token=test`，页面是否正常加载（无白屏、无 JS 报错） → 是/否
  2. [H] 页面顶部是否有 Tab 栏区域（即使没有 Agent 连接也应有栏位），右侧是否显示连接状态文字 → 是/否
  3. [H] 页面底部是否有输入框和"发送"按钮 → 是/否
  4. [H] 页面背景是否为深色（暗色主题），文字是否为浅色 → 是/否
- **异常排查:**
  - 如果白屏: 打开浏览器 DevTools Console 查看 JS 错误
  - 如果样式异常: 检查 Network 面板中 style.css 是否成功加载

### 场景 4：Web 前端交互功能

#### - [x] 4.1 输入框发送消息
- **来源:** spec-design.md 输入区域
- **操作步骤:**
  1. [H] 在 Web 前端输入框输入任意文字，按 Enter 键，消息区域是否出现一条用户消息（蓝色背景、靠右对齐） → 是/否
  2. [H] 在输入框输入 `/clear`，按 Enter，消息区域是否被清空 → 是/否
- **异常排查:**
  - 如果无反应: 打开 DevTools Console 检查 WebSocket 连接状态，确认管理 WS 已建立
  - 需要先有 Agent 在线并建立 session WS 连接后才能发送消息

#### - [x] 4.2 Tab 多 Agent 切换
- **来源:** spec-design.md Tab 管理 / 验收标准
- **操作步骤:**
  1. [H] 启动一个配置了 relay 的 TUI Agent（`relay_url: "ws://localhost:18080"`, `relay_token: "test"`, `relay_name: "Agent-A"`），Web 页面顶部 Tab 栏是否自动出现 "Agent-A" Tab 且绿点状态 → 是/否
  2. [H] 再启动第二个 Agent（`relay_name: "Agent-B"`），Tab 栏是否自动出现第二个 Tab "Agent-B" 且无需刷新页面 → 是/否
  3. [H] 点击不同 Tab 切换，消息区域是否切换到对应 Agent 的内容（各 Agent 消息隔离） → 是/否
- **异常排查:**
  - 如果 Tab 不出现: 检查 TUI 日志中是否有 "Relay 已连接" 消息，检查 `~/.zen-code/settings.json` 中 relay 配置是否正确
  - 配置示例: `{"config": {"relay_url": "ws://localhost:18080", "relay_token": "test", "relay_name": "Agent-A", ...}}`

#### - [x] 4.3 Agent 在线/断线状态显示
- **来源:** spec-design.md Tab 状态规范 / Task 7 验收 #10
- **操作步骤:**
  1. [A] `curl -s "http://localhost:18080/agents?token=test" | python3 -c "import sys,json; data=json.load(sys.stdin); print(len(data))"` → 期望: 输出当前连接的 Agent 数量（>= 1）
  2. [H] 关闭其中一个 Agent TUI（Ctrl+C），对应 Tab 的状态点是否从绿色变为灰色 → 是/否
  3. [H] 重新启动该 Agent，对应 Tab 是否恢复绿色状态（新 Tab 出现或已有 Tab 状态恢复） → 是/否
- **异常排查:**
  - 如果断线后 Tab 状态未变: 检查 Relay Server 是否正确广播 `agent_offline` 事件，检查 Web 端 `handleBroadcast` 逻辑

#### - [x] 4.4 HITL 审批弹窗
- **来源:** spec-design.md HITL 弹窗 / 验收标准
- **操作步骤:**
  1. [H] 在 Web 前端向一个 Agent 发送需要执行写操作的指令（如 "创建一个文件 /tmp/test-relay.txt"），当 HITL 审批弹窗出现时，弹窗是否显示工具名称和参数 → 是/否
  2. [H] 点击"全部批准"按钮，弹窗是否关闭，Agent 是否继续执行（消息区域出现新的工具调用结果） → 是/否
- **异常排查:**
  - 如果弹窗不出现: 确认 Agent 未使用 YOLO 模式（`-y` 参数会跳过 HITL）
  - 如果 Tab 上显示角标但弹窗不出现: 切换到该 Agent 的 Tab 后应自动显示

#### - [x] 4.5 TODO 面板显示
- **来源:** spec-design.md TODO 面板 / 验收标准
- **操作步骤:**
  1. [H] 向 Agent 发送一个会产生 TODO 列表的指令（如 "列出3个待办事项并使用 todo_write 工具"），TODO 面板是否在消息区域上方出现，且颜色分类正确（待办白色、进行中黄色、完成暗灰） → 是/否
- **异常排查:**
  - 如果 TODO 面板不出现: 检查 `app.js` 中 `handleAgentEvent` 是否处理了 `todo_update` 事件

### 场景 5：TUI 集成与兼容性

#### - [x] 5.1 TUI 无 relay 配置时行为无回归
- **来源:** Task 6 检查步骤 / Task 7 验收 #9 / 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | grep "test result"` → 期望: 输出包含 "ok" 且 "0 failed"
  2. [A] `cargo test -p rust-relay-server --lib 2>&1 | grep "test result"` → 期望: 输出包含 "ok" 且 "0 failed"
- **异常排查:**
  - 如果测试失败: 检查 `App` 新增的 relay 字段是否正确初始化为 `None`，以及 `poll_relay` 在无 rx 时是否立即返回

#### - [x] 5.2 TUI 配置 relay 后事件转发正常
- **来源:** Task 6 / spec-design.md
- **操作步骤:**
  1. [A] `grep -c "relay_client" rust-agent-tui/src/app/mod.rs` → 期望: 输出 >= 5（多处引用 relay_client 字段）
  2. [A] `grep -c "send_agent_event" rust-agent-tui/src/app/agent.rs` → 期望: 输出 >= 1（事件转发调用存在）
  3. [H] 配置 TUI 的 `~/.zen-code/settings.json` 添加 `relay_url`/`relay_token`/`relay_name`，启动 TUI，终端日志中是否出现 "Relay 已连接" 或 "session" 相关日志 → 是/否
- **异常排查:**
  - 如果无连接日志: 确认 settings.json 中 relay 配置在 `config` 对象内（`{"config": {"relay_url": "...", ...}}`）
  - 如果连接失败: 确认 Relay Server 正在运行且端口可达

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 服务启动与健康检查 | 1.1 | Server 启动与健康检查 | 2 | 0 | ⬜ | |
| 服务启动与健康检查 | 1.2 | Token 验证拒绝非法请求 | 2 | 0 | ⬜ | |
| Feature 隔离与编译 | 2.1 | client feature 隔离 | 2 | 0 | ⬜ | |
| Feature 隔离与编译 | 2.2 | 全量编译无 warning | 1 | 0 | ⬜ | |
| Feature 隔离与编译 | 2.3 | rust-create-agent 测试无回归 | 1 | 0 | ⬜ | |
| Web 前端页面加载 | 3.1 | 静态资源可访问 | 3 | 0 | ⬜ | |
| Web 前端页面加载 | 3.2 | 页面布局与样式 | 0 | 4 | ⬜ | |
| Web 前端交互 | 4.1 | 输入框发送消息 | 0 | 2 | ⬜ | |
| Web 前端交互 | 4.2 | Tab 多 Agent 切换 | 0 | 3 | ⬜ | |
| Web 前端交互 | 4.3 | Agent 在线/断线状态 | 1 | 2 | ⬜ | |
| Web 前端交互 | 4.4 | HITL 审批弹窗 | 0 | 2 | ⬜ | |
| Web 前端交互 | 4.5 | TODO 面板显示 | 0 | 1 | ⬜ | |
| TUI 集成 | 5.1 | TUI 无 relay 配置无回归 | 2 | 0 | ⬜ | |
| TUI 集成 | 5.2 | TUI 配置 relay 后事件转发 | 2 | 1 | ⬜ | |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
