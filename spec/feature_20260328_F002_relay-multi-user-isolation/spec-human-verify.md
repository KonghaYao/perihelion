# relay-multi-user-isolation 人工验收清单

**生成时间:** 2026-03-28 15:47
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求
- [x] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [x] [AUTO] 编译 relay-server: `cargo build -p rust-relay-server --features server 2>&1 | grep -E "^error" | head -5`
- [x] [AUTO] 编译 TUI: `cargo build -p rust-agent-tui 2>&1 | grep "^error" | head -5`
- [x] [AUTO] 安装 node ws 模块（用于 WS 自动化测试）: `cd /tmp && npm install ws --silent && echo "ok"`
- [x] [AUTO/SERVICE] 启动 Relay Server: `RELAY_TOKEN=test-token cargo run -p rust-relay-server --features server` (port: 8080)

### 测试数据准备
- [x] [AUTO] 注册 user_id_A: `export UID_A=$(curl -s -X POST "http://localhost:8080/register?token=test-token" | python3 -c "import sys,json; print(json.load(sys.stdin)['user_id'])") && echo "UID_A=$UID_A"`
- [x] [AUTO] 注册 user_id_B: `export UID_B=$(curl -s -X POST "http://localhost:8080/register?token=test-token" | python3 -c "import sys,json; print(json.load(sys.stdin)['user_id'])") && echo "UID_B=$UID_B"`

---

## 验收项目

### 场景 1：匿名注册

#### - [x] 1.1 /register 返回合法 UUID v4
- **来源:** Task 2 检查步骤 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `curl -s -X POST "http://localhost:8080/register?token=test-token" | python3 -c "import sys,json,re; d=json.load(sys.stdin); uid=d.get('user_id',''); print('PASS' if re.match(r'^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$', uid) else 'FAIL: '+uid)"` → 期望: 输出 `PASS`
  2. [A] `curl -s -X POST "http://localhost:8080/register?token=test-token" | python3 -c "import sys,json; d=json.load(sys.stdin); print(list(d.keys()))"` → 期望: 输出 `['user_id']`，只含 user_id 字段
- **异常排查:**
  - 如果输出 FAIL 或格式不对: 检查 `rust-relay-server/src/main.rs` 中 `register_handler` 的 uuid 生成逻辑

#### - [x] 1.2 错误 token 时 /register 返回 401
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `curl -s -o /dev/null -w "%{http_code}" -X POST "http://localhost:8080/register?token=wrong-token"` → 期望: 输出 `401`
- **异常排查:**
  - 如果返回 200: 检查 `main.rs` 中 `auth::validate_token` 调用是否在 register_handler 中生效

---

### 场景 2：Agent 连接与命名空间隔离

#### - [x] 2.1 同一用户的多个 agent 均可见
- **来源:** spec-design.md 验收标准（"Agent A 和 Agent B 同属 user_id=X，Web 客户端连接 user_id=X 可看到两个 agent"）
- **操作步骤:**
  1. [A] 运行以下脚本连接两个 agent 并查询列表:
     ```bash
     node -e "
     const WebSocket = require('/tmp/node_modules/ws');
     const UID = process.env.UID_A;
     const ws1 = new WebSocket('ws://localhost:8080/agent/ws?token=test-token&user_id=' + UID + '&name=agent-1');
     const ws2 = new WebSocket('ws://localhost:8080/agent/ws?token=test-token&user_id=' + UID + '&name=agent-2');
     ws1.on('open', () => ws2.on('open', () => {
       setTimeout(() => {
         const http = require('http');
         http.get('http://localhost:8080/agents?token=test-token&user_id=' + UID, (res) => {
           let d = ''; res.on('data', c => d += c); res.on('end', () => {
             const agents = JSON.parse(d);
             console.log('agents count:', agents.length);
             ws1.close(); ws2.close(); process.exit(agents.length >= 2 ? 0 : 1);
           });
         });
       }, 500);
     }));
     setTimeout(() => process.exit(1), 8000);
     "
     ```
     → 期望: 输出 `agents count: 2`，退出码 0
  2. [A] `echo "Exit code: $?"` → 期望: `Exit code: 0`
  3. [A] `curl -s "http://localhost:8080/agents?token=test-token&user_id=$UID_A" | python3 -c "import sys,json; agents=json.load(sys.stdin); print('names:', [a.get('name','?') for a in agents])"` → 期望: 输出包含 `agent-1` 和 `agent-2`（需在 agent WS 仍连接时执行）
- **异常排查:**
  - 如果 count 为 0: 检查 agent WS 连接是否成功，排查 `handle_agent_ws` 中 namespace 创建逻辑
  - 如果 count 为 1: 检查是否两个 WS 都成功 open 后才查询

#### - [x] 2.2 跨用户的 agent 不可见（/agents 隔离）
- **来源:** Task 7 验收项 2 / spec-design.md
- **操作步骤:**
  1. [A] 连接 User A 的 agent 并验证 User B 看不到:
     ```bash
     node -e "
     const WebSocket = require('/tmp/node_modules/ws');
     const UID_A = process.env.UID_A;
     const UID_B = process.env.UID_B;
     const ws = new WebSocket('ws://localhost:8080/agent/ws?token=test-token&user_id=' + UID_A + '&name=isolated-agent');
     ws.on('open', () => setTimeout(() => {
       const http = require('http');
       http.get('http://localhost:8080/agents?token=test-token&user_id=' + UID_B, (res) => {
         let d = ''; res.on('data', c => d += c); res.on('end', () => {
           const agents = JSON.parse(d);
           console.log('B sees A agents count:', agents.length);
           ws.close(); process.exit(agents.length === 0 ? 0 : 1);
         });
       });
     }, 500));
     setTimeout(() => process.exit(1), 8000);
     "
     ```
     → 期望: 输出 `B sees A agents count: 0`，退出码 0
  2. [A] `curl -s "http://localhost:8080/agents?token=test-token&user_id=$UID_A" | python3 -c "import sys,json; print('A count:', len(json.load(sys.stdin)))"` → 期望: `A count: 1`（User A 能看到自己的 agent）
  3. [A] `curl -s "http://localhost:8080/agents?token=test-token&user_id=$UID_B" | python3 -c "import sys,json; print('B count:', len(json.load(sys.stdin)))"` → 期望: `B count: 0`（User B 不能看到 User A 的 agent）
- **异常排查:**
  - 如果 B count 不为 0: 检查 `relay.rs` 中 `agents_list` 是否按 user_id 过滤，排查 `get_or_create_namespace` 隔离逻辑

---

### 场景 3：广播隔离

#### - [x] 3.1 User B 看不到 User A 的 AgentOnline 广播
- **来源:** Task 7 验收项 3 / spec-design.md 隔离边界
- **操作步骤:**
  1. [A] 运行广播隔离测试:
     ```bash
     node -e "
     const WebSocket = require('/tmp/node_modules/ws');
     const UID_A = process.env.UID_A;
     const UID_B = process.env.UID_B;
     let bMessages = [];
     const wsB = new WebSocket('ws://localhost:8080/web/ws?token=test-token&user_id=' + UID_B);
     wsB.on('open', () => {
       setTimeout(() => {
         const wsAgent = new WebSocket('ws://localhost:8080/agent/ws?token=test-token&user_id=' + UID_A + '&name=broadcast-test');
         wsAgent.on('open', () => setTimeout(() => {
           const hasAgentOnline = bMessages.some(m => { try { return JSON.parse(m).type === 'agent_online'; } catch { return false; } });
           console.log('B received agent_online:', hasAgentOnline);
           wsAgent.close(); wsB.close();
           process.exit(hasAgentOnline ? 1 : 0);
         }, 1000));
       }, 300);
     });
     wsB.on('message', d => bMessages.push(d.toString()));
     setTimeout(() => process.exit(1), 10000);
     "
     ```
     → 期望: 输出 `B received agent_online: false`，退出码 0
  2. [A] `echo "Broadcast isolation exit: $?"` → 期望: `Broadcast isolation exit: 0`
- **异常排查:**
  - 如果 B 收到了 agent_online: 检查 `relay.rs` 中 `broadcast` 方法是否按 user_id 隔离广播目标

#### - [x] 3.2 同一用户的管理端收到 AgentOnline 广播（正向验证）
- **来源:** spec-design.md（同一用户内正常广播）
- **操作步骤:**
  1. [A] 运行同用户广播正向测试:
     ```bash
     node -e "
     const WebSocket = require('/tmp/node_modules/ws');
     const UID_A = process.env.UID_A;
     let aMessages = [];
     const wsMgmt = new WebSocket('ws://localhost:8080/web/ws?token=test-token&user_id=' + UID_A);
     wsMgmt.on('open', () => {
       setTimeout(() => {
         const wsAgent = new WebSocket('ws://localhost:8080/agent/ws?token=test-token&user_id=' + UID_A + '&name=same-user-agent');
         wsAgent.on('open', () => setTimeout(() => {
           const types = aMessages.map(m => { try { return JSON.parse(m).type; } catch { return '?'; } });
           console.log('A mgmt received types:', JSON.stringify(types));
           const hasAgentOnline = types.includes('agent_online') || types.includes('agents_list');
           console.log('A mgmt received broadcast:', hasAgentOnline);
           wsAgent.close(); wsMgmt.close();
           process.exit(hasAgentOnline ? 0 : 1);
         }, 1000));
       }, 300);
     });
     wsMgmt.on('message', d => aMessages.push(d.toString()));
     setTimeout(() => process.exit(1), 10000);
     "
     ```
     → 期望: 输出 `A mgmt received broadcast: true`，退出码 0
  2. [A] `echo "Same-user broadcast exit: $?"` → 期望: `Same-user broadcast exit: 0`
- **异常排查:**
  - 如果 A 收不到广播: 检查 `handle_web_management_ws` 中 broadcast_tx 注册到正确 namespace

---

### 场景 4：Session 访问控制

#### - [x] 4.1 跨用户 session 连接返回 session_not_found
- **来源:** Task 7 验收项 4 / spec-design.md
- **操作步骤:**
  1. [A] 运行跨用户 session 访问测试:
     ```bash
     node -e "
     const WebSocket = require('/tmp/node_modules/ws');
     const http = require('http');
     const UID_A = process.env.UID_A;
     const UID_B = process.env.UID_B;
     const wsAgent = new WebSocket('ws://localhost:8080/agent/ws?token=test-token&user_id=' + UID_A + '&name=session-test');
     wsAgent.on('open', () => setTimeout(() => {
       http.get('http://localhost:8080/agents?token=test-token&user_id=' + UID_A, (res) => {
         let d = ''; res.on('data', c => d += c); res.on('end', () => {
           const agents = JSON.parse(d);
           const sessionId = agents[0] && agents[0].session_id;
           if (!sessionId) { console.log('No session found'); process.exit(1); }
           const wsB = new WebSocket('ws://localhost:8080/web/ws?token=test-token&user_id=' + UID_B + '&session=' + sessionId);
           let receivedError = false;
           wsB.on('message', m => {
             const msg = JSON.parse(m.toString());
             if (msg.type === 'error' && msg.code === 'session_not_found') receivedError = true;
           });
           wsB.on('close', () => {
             console.log('Got session_not_found:', receivedError);
             wsAgent.close();
             process.exit(receivedError ? 0 : 1);
           });
         });
       });
     }, 500));
     setTimeout(() => process.exit(1), 10000);
     "
     ```
     → 期望: 输出 `Got session_not_found: true`，退出码 0
  2. [A] `echo "Session isolation exit: $?"` → 期望: `Session isolation exit: 0`
  3. [A] `# 验证错误消息格式包含正确字段` 检查日志（可选）
- **异常排查:**
  - 如果 receivedError 为 false: 检查 `relay.rs` 中 `handle_web_session_ws` 的双重匹配逻辑（user_id + session_id）

#### - [x] 4.2 正确用户可访问自己的 session
- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [A] 运行同用户 session 连接测试:
     ```bash
     node -e "
     const WebSocket = require('/tmp/node_modules/ws');
     const http = require('http');
     const UID_A = process.env.UID_A;
     const wsAgent = new WebSocket('ws://localhost:8080/agent/ws?token=test-token&user_id=' + UID_A + '&name=own-session-test');
     wsAgent.on('open', () => setTimeout(() => {
       http.get('http://localhost:8080/agents?token=test-token&user_id=' + UID_A, (res) => {
         let d = ''; res.on('data', c => d += c); res.on('end', () => {
           const agents = JSON.parse(d);
           const sessionId = agents[0] && agents[0].session_id;
           if (!sessionId) { console.log('No session found'); process.exit(1); }
           const wsWeb = new WebSocket('ws://localhost:8080/web/ws?token=test-token&user_id=' + UID_A + '&session=' + sessionId);
           wsWeb.on('open', () => {
             console.log('Own session connected: true');
             wsWeb.close(); wsAgent.close(); process.exit(0);
           });
           wsWeb.on('error', e => { console.log('Own session error:', e.message); process.exit(1); });
         });
       });
     }, 500));
     setTimeout(() => process.exit(1), 10000);
     "
     ```
     → 期望: 输出 `Own session connected: true`，退出码 0
  2. [A] `echo "Own session exit: $?"` → 期望: `Own session exit: 0`
- **异常排查:**
  - 如果无法连接: 检查 WS 升级握手及 `handle_web_session_ws` 中 namespace 查找逻辑

---

### 场景 5：参数校验

#### - [x] 5.1 /agents 缺少 user_id 返回 400
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `curl -s -o /dev/null -w "%{http_code}" "http://localhost:8080/agents?token=test-token"` → 期望: `400`
- **异常排查:**
  - 如果返回其他状态码: 检查 `main.rs` 中 `agents_handler` 的 user_id 缺失校验

#### - [x] 5.2 /agent/ws 缺少 user_id 返回 400
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `curl -s -o /dev/null -w "%{http_code}" "http://localhost:8080/agent/ws?token=test-token"` → 期望: `400`
- **异常排查:**
  - 如果返回其他状态码: 检查 `agent_ws_handler` 中 user_id 缺失校验逻辑

#### - [x] 5.3 /web/ws 缺少 user_id 返回 400
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `curl -s -o /dev/null -w "%{http_code}" "http://localhost:8080/web/ws?token=test-token"` → 期望: `400`
- **异常排查:**
  - 如果返回其他状态码: 检查 `web_ws_handler` 中 user_id 缺失校验逻辑

---

### 场景 6：TUI 客户端集成

#### - [x] 6.1 TUI 首次连接自动注册并持久化 user_id
- **来源:** Task 5 检查步骤 / Task 7 验收项 6 / spec-design.md
- **操作步骤:**
  1. [A] 确保 settings.json 中 user_id 为空（备份现有设置后操作）:
     `python3 -c "import json,os; p=os.path.expanduser('~/.zen-code/settings.json'); d=json.load(open(p)) if os.path.exists(p) else {}; rc=d.get('config',{}).get('remote_control',{}); print('Before user_id:', rc.get('user_id','<not set>'))"`
     → 期望: 如已有 user_id 则记录，测试前可手动置空
  2. [H] 确保 Relay Server 正在运行（已在准备步骤启动），然后运行 TUI 并通过 `/relay` 命令连接到 Relay Server（URL: `ws://localhost:8080`，token: `test-token`）。等待出现 "Relay connected" 消息后，按 `Ctrl+C` 退出 → 是/否（TUI 显示连接成功）
  3. [H] 退出 TUI 后，在终端检查: `cat ~/.zen-code/settings.json | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('config',{}).get('remote_control',{}).get('user_id','null'))"` → 输出是否为有效 UUID（非 null 非空字符串）→ 是/否
- **异常排查:**
  - 如果 user_id 仍为 null: 检查 `relay_ops.rs` 中 `get_or_register_user_id` 函数以及 `mod.rs` 中保存逻辑
  - 检查 Relay Server 日志是否有 `/register` 请求记录

#### - [x] 6.2 重启 TUI 后复用同一 user_id（不重新注册）
- **来源:** Task 5 检查步骤
- **操作步骤:**
  1. [A] 记录第一次 user_id: `FIRST_UID=$(cat ~/.zen-code/settings.json | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('config',{}).get('remote_control',{}).get('user_id',''))") && echo "First UID: $FIRST_UID"` → 期望: 输出非空 UUID
  2. [H] 再次运行 TUI 并连接到同一 Relay Server，等待出现 "Relay connected" 消息后退出 → 是/否（TUI 再次连接成功）
  3. [H] 检查 settings.json 中的 user_id 是否与第一次相同: `SECOND_UID=$(cat ~/.zen-code/settings.json | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('config',{}).get('remote_control',{}).get('user_id',''))") && echo "Second UID: $SECOND_UID" && [ "$FIRST_UID" = "$SECOND_UID" ] && echo "MATCH: same user_id reused" || echo "MISMATCH: user_id changed!"` → 是/否（输出 MATCH）
- **异常排查:**
  - 如果 user_id 变化: 检查 `get_or_register_user_id` 中 `existing.is_some()` 直接返回逻辑是否正确

#### - [x] 6.3 /relay 面板显示含 user_id hash 的完整 Web 接入 URL
- **来源:** spec-design.md（"/relay 面板显示含 user_id hash 的完整 Web 接入 URL"）
- **操作步骤:**
  1. [H] 运行 TUI 并通过 `/relay` 命令连接成功后，打开 `/relay` 面板（再次输入 `/relay` 命令查看状态），检查面板内是否显示格式为 `http://localhost:8080/web/#user_id=XXXXXXXX-...` 的 Web 接入 URL → 是/否
- **异常排查:**
  - 如果不显示 URL: 检查 `mod.rs` 中连接成功后 `relay_panel.set_web_access_url()` 是否被调用，检查 `relay_panel.rs` 中 `web_access_url` 字段
  - 检查 TUI 的 relay 面板 UI 渲染是否使用了该字段

---

### 场景 7：前端 user_id 支持

#### - [x] 7.1 无 user_id 时前端显示提示页面
- **来源:** Task 6 / spec-design.md（"如果 hash 中没有 user_id，显示提示页面"）
- **操作步骤:**
  1. [A] `curl -s "http://localhost:8080/web/" | grep -c "token"` → 期望: 输出 > 0（前端页面可访问）
  2. [H] 在浏览器打开 `http://localhost:8080/web/?token=test-token`（不含 user_id hash），检查页面是否显示提示文字"请从 TUI 的 /relay 面板复制完整的接入 URL（包含 #user_id=...）"而非正常 Agent 列表界面 → 是/否
- **异常排查:**
  - 如果没有显示提示: 检查 `App.js` 中对 `userId = getUserId()` 的 null 判断逻辑
  - 检查 `connection.js` 中 `getUserId()` 函数是否正确解析 hash

#### - [x] 7.2 含 user_id hash 的 URL 可正常建立 WS 连接
- **来源:** Task 6 检查步骤
- **操作步骤:**
  1. [A] 先确保有 agent 连接: `node -e "const WebSocket = require('/tmp/node_modules/ws'); const ws = new WebSocket('ws://localhost:8080/agent/ws?token=test-token&user_id=' + process.env.UID_A + '&name=frontend-test'); ws.on('open', () => { console.log('agent connected'); setTimeout(() => ws.close(), 30000); }); setTimeout(() => {}, 100000);" &`
  2. [A] 验证 WS 连接在服务端层面成功（不需要 WS Upgrade header）: `curl -s -o /dev/null -w "%{http_code}" "http://localhost:8080/web/ws?token=test-token&user_id=$UID_A"` → 期望: `400`（含 user_id 但无 WS Upgrade 头，axum 正确处理，非 user_id 引起的 400）
  3. [A] 对比：验证不含 user_id 时服务端直接返回 400（user_id 缺失校验）: `curl -sv "http://localhost:8080/web/ws?token=test-token" 2>&1 | grep "User-agent\|400\|Bad Request" | head -5` → 期望: 包含 `400 Bad Request`
  4. [H] 在浏览器打开 `http://localhost:8080/web/?token=test-token#user_id=YOUR_UID_A`（将 YOUR_UID_A 替换为 $UID_A 的实际值），等待 3 秒，检查页面是否正常显示 Agent 列表（侧边栏连接指示灯为绿色"已连接"，并显示 "frontend-test" agent） → 是/否
  5. [H] 在浏览器开发者工具的 Network 面板中，找到 WebSocket 连接，确认请求 URL 中包含 `user_id=...` 参数 → 是/否
- **异常排查:**
  - 如果浏览器页面无法连接: 检查 `connection.js` 中 `connectManagement()` 的 user_id 拼接逻辑
  - 如果显示"断线": 查看浏览器 Console 是否有 WebSocket 错误

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 匿名注册 | 1.1 | /register 返回合法 UUID v4 | 2 | 0 | ✅ | |
| 匿名注册 | 1.2 | 错误 token 返回 401 | 1 | 0 | ✅ | |
| Agent 隔离 | 2.1 | 同一用户多 agent 均可见 | 3 | 0 | ✅ | |
| Agent 隔离 | 2.2 | 跨用户 agent 不可见 | 3 | 0 | ✅ | |
| 广播隔离 | 3.1 | B 看不到 A 的 AgentOnline 广播 | 2 | 0 | ✅ | |
| 广播隔离 | 3.2 | 同用户内正常广播（正向验证） | 2 | 0 | ✅ | |
| Session 访问控制 | 4.1 | 跨用户 session 返回 session_not_found | 2 | 0 | ✅ | |
| Session 访问控制 | 4.2 | 正确用户可访问自己的 session | 2 | 0 | ✅ | |
| 参数校验 | 5.1 | /agents 缺少 user_id 返回 400 | 1 | 0 | ✅ | |
| 参数校验 | 5.2 | /agent/ws 缺少 user_id 返回 400 | 1 | 0 | ✅ | |
| 参数校验 | 5.3 | /web/ws 缺少 user_id 返回 400 | 1 | 0 | ✅ | |
| TUI 集成 | 6.1 | 首次连接自动注册并持久化 user_id | 1 | 2 | ✅ | |
| TUI 集成 | 6.2 | 重启后复用同一 user_id | 1 | 2 | ✅ | |
| TUI 集成 | 6.3 | /relay 面板显示 Web 接入 URL | 0 | 1 | ✅ | |
| 前端行为 | 7.1 | 无 user_id 时显示提示页面 | 1 | 1 | ✅ | |
| 前端行为 | 7.2 | 含 user_id hash 的 URL 正常工作 | 3 | 2 | ✅ | |

**验收结论:** ✅ 全部通过（含 1 项修复后通过：6.3 /relay 面板 Web URL 显示）
