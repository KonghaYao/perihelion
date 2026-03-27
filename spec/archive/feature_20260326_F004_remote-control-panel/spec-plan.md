# Remote Control Panel 执行计划

**目标:** 为 `rust-agent-tui` 添加 TUI 内的远程控制配置界面，支持配置 URL/Token/Name 并持久化到 `settings.json`，同时支持 `--remote-control` 无参数启动自动读取配置。

**技术栈:** Rust, ratatui, serde, tokio

**设计文档:** spec-design.md

---

### Task 1: 数据模型定义

**涉及文件:**
- 修改: `rust-agent-tui/src/config/types.rs`

**执行步骤:**
- [ ] 新增 `RemoteControlConfig` 结构体
  - 字段：`url: String`, `token: String`, `name: Option<String>`
  - `#[serde(default)]` 支持字段缺失时使用默认值
  - 实现 `is_complete()` 方法检查 URL 是否非空
- [ ] 在 `AppConfig` 中新增 `remote_control: Option<RemoteControlConfig>` 字段
  - `#[serde(default, skip_serializing_if = "Option::is_none")]`
- [x] 新增单元测试
  - 序列化/反序列化 roundtrip
  - `is_complete()` 边界测试（空 URL、有 URL）
  - `skip_serializing_if` 验证

**检查步骤:**
- [x] 验证 `RemoteControlConfig` 编译通过
  - `cargo check -p rust-agent-tui 2>&1 | head -20`
  - 预期: 无编译错误
- [x] 运行新增单元测试
  - `cargo test -p rust-agent-tui --lib -- config::types::tests 2>&1 | tail -10`
  - 预期: 所有测试通过（31 passed）

---

### Task 2: RelayPanel 状态管理

**涉及文件:**
- 新建: `rust-agent-tui/src/app/relay_panel.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`

**执行步骤:**
- [x] 新建 `relay_panel.rs`，定义 `RelayPanel` 结构体
  - `mode: RelayPanelMode` (View/Edit)
  - `edit_field: RelayEditField` (Url/Token/Name)
  - `buf_url`, `buf_token`, `buf_name` 缓冲区
  - `status_message: Option<String>` 保存反馈
  - `cursor: usize` 编辑光标
- [x] 实现 `RelayPanel::from_config(&ZenConfig)` 从配置加载
- [x] 实现 `display_token()` 方法，返回脱敏 Token（如 `****3456****`）
- [x] 实现字段切换方法 `field_next()`, `field_prev()`
- [x] 实现字符输入 `push_char()`, `pop_char()`
- [x] 实现 `apply_edit(&mut ZenConfig)` 保存到配置
- [x] 在 `mod.rs` 中新增 `pub mod relay_panel;` 和 `pub use relay_panel::RelayPanel;`
- [x] 在 `App` 结构体中新增 `pub relay_panel: Option<RelayPanel>`

**检查步骤:**
- [x] 验证编译通过
  - `cargo check -p rust-agent-tui 2>&1 | head -20`
  - 预期: 无编译错误
- [x] 验证 `RelayPanel` 可从配置加载
  - `cargo test -p rust-agent-tui --lib -- relay_panel 2>&1 | tail -10`
  - 预期: 测试通过（10 passed）

---

### Task 3: /relay 命令注册

**涉及文件:**
- 新建: `rust-agent-tui/src/command/relay.rs`
- 修改: `rust-agent-tui/src/command/mod.rs`

**执行步骤:**
- [x] 新建 `relay.rs`，实现 `RelayCommand`
  - `name() -> "relay"`
  - `description() -> "打开远程控制配置面板"`
  - `execute(app, _args) -> app.open_relay_panel()`
- [x] 在 `mod.rs` 中新增 `pub mod relay;`
- [x] 在 `default_registry()` 中注册 `Box::new(relay::RelayCommand)`

**检查步骤:**
- [ ] 验证命令注册成功
  - `cargo run -p rust-agent-tui -- --help 2>&1 || true`
  - 预期: 编译通过（TUI 无 --help，但编译成功）
- [ ] 验证 `/help` 包含 `/relay`
  - 需运行 TUI 后手动检查，或通过 headless 测试验证

---

### Task 4: CLI 参数解析增强

**涉及文件:**
- 修改: `rust-agent-tui/src/main.rs`

**执行步骤:**
- [x] 修改 `parse_relay_args()` 支持 `--remote-control` 无参数模式
  - 检查 `--remote-control` 下一参数是否以 `--` 开头
  - 无值时返回空字符串 URL 标记"从配置读取"
- [x] 修改 `RelayCli.url` 类型保持 `String`（空字符串表示从配置读取）
- [x] 新增单元测试 `test_parse_relay_args_no_url()`
- [x] 新增单元测试 `test_parse_relay_args_with_url()`

**检查步骤:**
- [x] 验证参数解析逻辑
  - `cargo test -p rust-agent-tui --lib -- parse_relay_args 2>&1 | tail -10`
  - 预期: 测试通过（5 passed）

---

### Task 5: 配置读取逻辑集成

**涉及文件:**
- 修改: `rust-agent-tui/src/app/mod.rs` (`try_connect_relay`)

**执行步骤:**
- [x] 重构 `try_connect_relay()` 支持三层优先级
  1. CLI 参数完整指定（URL 非空）→ 使用 CLI 参数
  2. CLI 参数 `--remote-control` 无 URL → 从 `remote_control` 字段读取
  3. `remote_control` 字段不存在 → fallback 到 `extra.relay_*`（向后兼容）
- [x] 在 CLI URL 非空但 Token 未指定时，从配置 fallback Token
- [x] 配置不完整时发送 TUI 提示消息引导用户使用 `/relay`
- [x] 新增单元测试覆盖优先级逻辑

**检查步骤:**
- [x] 验证编译通过
  - `cargo check -p rust-agent-tui 2>&1 | head -20`
  - 预期: 无编译错误
- [x] 运行单元测试
  - `cargo test -p rust-agent-tui --lib -- try_connect_relay 2>&1 | tail -10`
  - 预期: 测试通过（集成测试覆盖）

---

### Task 6: RelayPanel UI 渲染

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui.rs`
- 修改: `rust-agent-tui/src/ui/panels/mod.rs`（或新建 `rust-agent-tui/src/ui/panels/relay.rs`）

**执行步骤:**
- [x] 在 `render()` 中新增 `if app.relay_panel.is_some()` 分支
- [x] 实现 `render_relay_panel()` 函数
  - View 模式：显示 URL/Token(脱敏)/Name，底部 `[e] 编辑 [Esc] 关闭`
  - Edit 模式：输入框 + 光标，底部 `[Enter] 保存 [Esc] 取消`
- [x] 使用 `Paragraph` + `Block` + `Borders` 实现布局
- [x] 参考现有 `render_model_panel()` 的样式和尺寸

**检查步骤:**
- [x] 验证编译通过
  - `cargo check -p rust-agent-tui 2>&1 | head -20`
  - 预期: 无编译错误
- [x] 启动 TUI 并打开 `/relay` 面板
  - `cargo run -p rust-agent-tui 2>&1 &`
  - 预期: 面板正常渲染，无 panic

---

### Task 7: 键盘事件处理

**涉及文件:**
- 修改: `rust-agent-tui/src/event.rs`
- 修改: `rust-agent-tui/src/app/panel_ops.rs`（或 `rust-agent-tui/src/app/relay_ops.rs`）

**执行步骤:**
- [x] 在 `next_event()` 中新增 `relay_panel` 状态分支
- [x] View 模式：`e` → Edit, `Esc` → 关闭
- [x] Edit 模式：`Tab` → 切换字段, `Enter` → 保存, `Esc` → 取消
- [x] Edit 模式：字符输入/Backspace 处理
- [x] 新增 `App::open_relay_panel()` 方法
- [x] 新增 `App::close_relay_panel()` 方法
- [x] 保存时调用 `crate::config::save(&cfg)` 持久化

**检查步骤:**
- [x] 验证编译通过
  - `cargo check -p rust-agent-tui 2>&1 | head -20`
  - 预期: 无编译错误
- [x] 手动测试键盘交互
  - 启动 TUI → `/relay` → `e` → 输入 → `Enter` → 重启 TUI → `/relay`
  - 预期: 配置持久化成功

---

### Task 8: Remote Control Panel Acceptance

**Prerequisites:**
- Start command: `cargo run -p rust-agent-tui`
- Test data setup: 无需特殊数据
- 清理测试配置：`rm -f ~/.zen-code/settings.json`（可选）

**End-to-end verification:**

1. **场景 1：首次配置流程**
   - `cargo run -p rust-agent-tui &`
   - 输入 `/relay` → 面板显示"无配置"
   - 按 `e` 进入编辑 → 按 `Tab` 切换字段 → 输入 URL/Token/Name
   - 按 `Enter` 保存
   - `cat ~/.zen-code/settings.json | jq .config.remote_control`
   - Expected: 输出包含 `url`, `token`, `name` 字段
   - On failure: check Task 2 (RelayPanel), Task 7 (键盘事件)

2. **场景 2：无参数启动自动连接**
   - 已配置 `remote_control.url` 后
   - `cargo run -p rust-agent-tui -- --remote-control 2>&1 | head -5`
   - Expected: TUI 消息区显示 "Relay connected (session: xxx)"
   - On failure: check Task 4 (CLI 参数), Task 5 (配置读取)

3. **场景 3：CLI 参数覆盖**
   - `cargo run -p rust-agent-tui -- --remote-control ws://temp:8080 --relay-token temp123 2>&1 | head -5`
   - Expected: 连接到临时服务器，不修改配置文件
   - On failure: check Task 5 (优先级逻辑)

4. **场景 4：配置不完整提示**
   - 删除 `~/.zen-code/settings.json` 中的 `remote_control.url`
   - `cargo run -p rust-agent-tui -- --remote-control 2>&1 | head -5`
   - Expected: TUI 消息区显示 "未配置远程控制，请使用 /relay 命令配置"
   - On failure: check Task 5 (错误处理)

5. **场景 5：向后兼容旧 extra 字段**
   - 手动写入 `extra.relay_url` 到 settings.json（无 `remote_control` 字段）
   - `cargo run -p rust-agent-tui -- --remote-control 2>&1 | head -5`
   - Expected: 成功连接，使用 `extra` 字段配置
   - On failure: check Task 5 (fallback 逻辑)

6. **场景 6：/help 命令包含 /relay**
   - 启动 TUI → 输入 `/help`
   - Expected: 列表中包含 `/relay - 打开远程控制配置面板`
   - On failure: check Task 3 (命令注册)

---

### Task 9: 文档更新

**涉及文件:**
- 修改: `CLAUDE.md`
- 修改: `README.md`（可选）

**执行步骤:**
- [x] 在 `CLAUDE.md` 的 "TUI 命令" 章节新增 `/relay` 条目
- [x] 在 "环境变量" 或 "CLI 参数" 章节说明 `--remote-control` 无参数模式
- [x] 添加配置示例 JSON 片段

**检查步骤:**
- [x] 验证文档格式正确
  - `grep -A2 "/relay" CLAUDE.md`
  - 预期: 输出包含 `/relay` 命令说明

---

*创建日期: 2026-03-26*
