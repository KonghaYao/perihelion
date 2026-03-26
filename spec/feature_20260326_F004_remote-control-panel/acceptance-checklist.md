# Remote Control Panel Acceptance Checklist

**Feature:** Remote Control Panel for rust-agent-tui  
**Spec Plan:** spec-plan.md  
**Created:** 2026-03-26

---

## [A] Automated Verification Steps

### Task 1: 数据模型定义
- [A] Verify `RemoteControlConfig` compiles without errors
  ```bash
  cargo check -p rust-agent-tui 2>&1 | head -20
  ```
  Expected: No compilation errors

- [A] Run unit tests for config types
  ```bash
  cargo test -p rust-agent-tui --lib -- config::types::tests 2>&1 | tail -10
  ```
  Expected: All tests pass (31 passed)

### Task 2: RelayPanel 状态管理
- [A] Verify RelayPanel compiles
  ```bash
  cargo check -p rust-agent-tui 2>&1 | head -20
  ```
  Expected: No compilation errors

- [A] Run RelayPanel tests
  ```bash
  cargo test -p rust-agent-tui --lib -- relay_panel 2>&1 | tail -10
  ```
  Expected: Tests pass (10 passed)

### Task 3: /relay 命令注册
- [A] Verify command compiles
  ```bash
  cargo check -p rust-agent-tui 2>&1 | head -20
  ```
  Expected: No compilation errors

### Task 4: CLI 参数解析增强
- [A] Run argument parsing tests
  ```bash
  cargo test -p rust-agent-tui --lib -- parse_relay_args 2>&1 | tail -10
  ```
  Expected: Tests pass (5 passed)

### Task 5: 配置读取逻辑集成
- [A] Verify compilation
  ```bash
  cargo check -p rust-agent-tui 2>&1 | head -20
  ```
  Expected: No compilation errors

- [A] Run connection logic tests
  ```bash
  cargo test -p rust-agent-tui --lib -- try_connect_relay 2>&1 | tail -10
  ```
  Expected: Tests pass

### Task 6: RelayPanel UI 渲染
- [A] Verify UI compilation
  ```bash
  cargo check -p rust-agent-tui 2>&1 | head -20
  ```
  Expected: No compilation errors

### Task 7: 键盘事件处理
- [A] Verify event handling compilation
  ```bash
  cargo check -p rust-agent-tui 2>&1 | head -20
  ```
  Expected: No compilation errors

### Task 9: 文档更新
- [A] Verify documentation includes /relay command
  ```bash
  grep -A2 "/relay" CLAUDE.md
  ```
  Expected: Output includes /relay command description

---

## [H] Human Verification Steps

### Task 3: /relay 命令注册
- [H] Verify `/help` command includes `/relay`
  - Start TUI: `cargo run -p rust-agent-tui`
  - Type `/help`
  - Expected: List includes `/relay - 打开远程控制配置面板`

### Task 6: RelayPanel UI 渲染
- [H] Verify panel renders correctly
  - Start TUI: `cargo run -p rust-agent-tui`
  - Type `/relay`
  - Expected: Panel displays without panic, shows current config or "无配置"

### Task 7: 键盘事件处理
- [H] Verify keyboard interaction flow
  - Start TUI → `/relay` → Press `e` to edit
  - Type test URL/Token/Name, press `Tab` to switch fields
  - Press `Enter` to save
  - Restart TUI → `/relay`
  - Expected: Configuration persists and displays correctly

---

## [H] End-to-End Scenarios

### Scenario 1: 首次配置流程
1. Start TUI: `cargo run -p rust-agent-tui &`
2. Type `/relay` → Panel should show "无配置" or empty state
3. Press `e` → Enter edit mode
4. Press `Tab` to switch between URL/Token/Name fields
5. Input test values:
   - URL: `ws://localhost:8080`
   - Token: `test-token-123`
   - Name: `test-client`
6. Press `Enter` to save
7. Verify persistence:
   ```bash
   cat ~/.zen-code/settings.json | jq .config.remote_control
   ```
   Expected: Output contains `url`, `token`, `name` fields

**On failure:** Check Task 2 (RelayPanel), Task 7 (键盘事件)

### Scenario 2: 无参数启动自动连接
1. Ensure `remote_control.url` is configured (from Scenario 1)
2. Start with flag only:
   ```bash
   cargo run -p rust-agent-tui -- --remote-control 2>&1 | head -5
   ```
3. Expected: TUI message area shows "Relay connected (session: xxx)" or similar success message

**On failure:** Check Task 4 (CLI 参数), Task 5 (配置读取)

### Scenario 3: CLI 参数覆盖
1. Start with CLI parameters:
   ```bash
   cargo run -p rust-agent-tui -- --remote-control ws://temp:8080 --relay-token temp123 2>&1 | head -5
   ```
2. Expected: Connects to temporary server `ws://temp:8080`
3. Verify config file unchanged:
   ```bash
   cat ~/.zen-code/settings.json | jq .config.remote_control.url
   ```
   Expected: URL remains the original value (not `ws://temp:8080`)

**On failure:** Check Task 5 (优先级逻辑)

### Scenario 4: 配置不完整提示
1. Remove URL from config:
   ```bash
   # Edit ~/.zen-code/settings.json, remove remote_control.url field
   ```
2. Start with flag:
   ```bash
   cargo run -p rust-agent-tui -- --remote-control 2>&1 | head -5
   ```
3. Expected: TUI shows error message "未配置远程控制，请使用 /relay 命令配置" or similar

**On failure:** Check Task 5 (错误处理)

### Scenario 5: 向后兼容旧 extra 字段
1. Remove `remote_control` field, add legacy `extra` fields:
   ```bash
   # Edit ~/.zen-code/settings.json:
   # {
   #   "config": {
   #     "extra": {
   #       "relay_url": "ws://legacy:8080",
   #       "relay_token": "legacy-token"
   #     }
   #   }
   # }
   ```
2. Start with flag:
   ```bash
   cargo run -p rust-agent-tui -- --remote-control 2>&1 | head -5
   ```
3. Expected: Successfully connects using `extra.relay_*` fields

**On failure:** Check Task 5 (fallback 逻辑)

### Scenario 6: /help 命令包含 /relay
1. Start TUI: `cargo run -p rust-agent-tui`
2. Type `/help`
3. Expected: Command list includes `/relay - 打开远程控制配置面板`

**On failure:** Check Task 3 (命令注册)

---

## Cleanup (Optional)

```bash
# Remove test configuration
rm -f ~/.zen-code/settings.json
```

---

**Total Tasks:** 9  
**Automated Checks:** 13  
**Human Checks:** 10
