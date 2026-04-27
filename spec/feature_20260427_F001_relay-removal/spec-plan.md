# Relay Server 移除执行计划

**目标:** 完整删除 `rust-relay-server` crate 及 TUI 中所有 Relay 集成代码，将 workspace 从 4 crate 缩减为 3 crate。

**技术栈:** Rust 2021, tokio, ratatui, serde

**设计文档:** spec-design.md

## 改动总览

本计划完整删除 `rust-relay-server` crate 及 TUI 中所有 Relay 集成代码，将 workspace 从 4 crate 缩减为 3 crate。共 7 个 Task：Task 0 环境准备；Task 1 删除 crate 本体和 workspace 配置；Task 2 删除 TUI 中 6 个 Relay 专用文件；Task 3 清理 TUI App 层的 Relay 字段、方法和事件转发；Task 4 清理 TUI UI/Event/Command/Config 层；Task 5 更新全局文档；Task 6 验收。各 Task 按编号顺序执行，Task 1 是后续所有 Task 的基础（Task 3 中移除 `rust-relay-server` 依赖需要 Task 1 先删除目录）。

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证 Rust 工具链可用
  - 运行: `rustc --version && cargo --version`
  - 预期: 输出 Rust 版本和 Cargo 版本
- [x] 验证当前 workspace 可全量构建
  - 运行: `cargo build 2>&1 | tail -5`
  - 预期: 构建成功，无编译错误
- [x] 验证当前 workspace 测试可通过
  - 运行: `cargo test 2>&1 | tail -10`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build 2>&1 | grep -c "error"`
  - 预期: 输出为 0

---

### Task 1: 删除 rust-relay-server crate + workspace 更新

**背景:**
移除已废弃的 `rust-relay-server` crate——该 crate 提供 axum WebSocket 中继服务端和 tokio-tungstenite 客户端库，整体功能不再需要。当前 workspace 根 `Cargo.toml` 的 `members` 数组包含 `"rust-relay-server"`，`rust-agent-tui` 的 `Cargo.toml` 依赖 `rust-relay-server`（client feature）但该依赖将在 Task 3 中清理。本 Task 仅负责删除 crate 目录本身和更新 workspace 级别配置，不处理 TUI 侧的依赖引用。

**涉及文件:**
- 删除: `rust-relay-server/` 整个目录（8 个 Rust 源文件 + web/ 前端目录 + Cargo.toml）
- 修改: `Cargo.toml`（根目录）

**执行步骤:**
- [x] 从 workspace 根 `Cargo.toml` 的 `members` 数组中移除 `"rust-relay-server"` 条目
  - 位置: `/Users/konghayao/code/ai/perihelion/Cargo.toml` ~L6
  - 将 `members = ["rust-create-agent", "rust-agent-middlewares", "rust-agent-tui", "rust-relay-server", "langfuse-client", "perihelion-widgets"]` 中的 `"rust-relay-server", ` 删除
  - 修改后 members 为: `["rust-create-agent", "rust-agent-middlewares", "rust-agent-tui", "langfuse-client", "perihelion-widgets"]`
  - 原因: workspace 不再包含已废弃的 relay-server crate
- [x] 删除 `rust-relay-server/` 整个目录
  - 目录: `/Users/konghayao/code/ai/perihelion/rust-relay-server/`
  - 包含 10 个源文件: `Cargo.toml`, `src/main.rs`, `src/lib.rs`, `src/auth.rs`, `src/client/mod.rs`, `src/protocol.rs`, `src/protocol_types.rs`, `src/relay.rs`, `src/static_files.rs` + `web/` 前端目录（23 个文件）
  - 原因: 整个 crate 功能已废弃，无保留价值
- [x] 更新 `Cargo.lock` 移除 `rust-relay-server` 及其专有依赖
  - 运行: `cargo update -p rust-relay-server 2>&1`（Cargo 会自动从 lock 文件中移除不再被任何 crate 依赖的条目）
  - 注意: 此时 `rust-agent-tui` 仍引用 `rust-relay-server`（client feature），此步骤会报错——这是预期行为。Task 3 清理 TUI 依赖后再次执行 `cargo generate-lockfile` 或 `cargo update` 即可彻底清除
  - 原因: Cargo.lock 需要与 workspace members 保持一致
- [x] 为 workspace 配置变更编写验证测试
  - 测试方式: 执行 `cargo metadata --format-version=1 2>/dev/null | python3 -c "import sys,json; members=[p['name'] for p in json.load(sys.stdin)['workspace_members']]; assert 'rust-relay-server' not in members, f'rust-relay-server still in workspace: {members}'; print('OK: rust-relay-server removed from workspace')"`
  - 注意: 此命令在 Task 3 完成 TUI 依赖清理前会编译失败。本步骤仅验证 workspace members 配置正确——改用 grep 验证:
  - 运行: `grep -c '"rust-relay-server"' /Users/konghayao/code/ai/perihelion/Cargo.toml`
  - 预期: 输出 0（根 Cargo.toml 不再包含该字符串）

**检查步骤:**
- [x] `rust-relay-server/` 目录不存在
  - `test ! -d /Users/konghayao/code/ai/perihelion/rust-relay-server && echo "OK: directory removed"`
  - 预期: 输出 "OK: directory removed"
- [x] 根 `Cargo.toml` 的 members 不包含 `rust-relay-server`
  - `grep '"rust-relay-server"' /Users/konghayao/code/ai/perihelion/Cargo.toml`
  - 预期: 无输出（grep 返回非 0 退出码）
- [x] 根 `Cargo.toml` 仍包含其他 5 个 workspace members
  - `grep -c '"rust-' /Users/konghayao/code/ai/perihelion/Cargo.toml`
  - 预期: 输出 3（rust-create-agent, rust-agent-middlewares, rust-agent-tui）
- [x] workspace 根 `Cargo.toml` 语法有效
  - `cargo metadata --format-version=1 --no-deps 2>&1 | head -1`
  - 预期: 输出 JSON 开头（`{`），无解析错误

---

### Task 2: 删除 TUI Relay 专用文件

**背景:**
移除 TUI 中 6 个仅服务于 Relay 功能的专用文件。这些文件分别定义了 /relay 面板状态（relay_panel.rs）、Relay 连接/断开/事件转发逻辑（relay_ops.rs）、RelayState 子结构体（relay_state.rs）、ExecutorEvent → RelayAgentEvent 适配器（relay_adapter.rs）、/relay 面板 UI 渲染（panels/relay.rs）和 /relay 命令处理（command/relay.rs）。删除后这些模块的声明（mod 声明、use 语句）将在 Task 3（app 层）和 Task 4（UI/Event/Command/Config 层）中清理，本 Task 仅负责删除文件本身。

**涉及文件:**
- 删除: `rust-agent-tui/src/app/relay_panel.rs`（169 行，含 14 个单元测试）
- 删除: `rust-agent-tui/src/app/relay_ops.rs`（221 行）
- 删除: `rust-agent-tui/src/app/relay_state.rs`（25 行）
- 删除: `rust-agent-tui/src/relay_adapter.rs`（75 行）
- 删除: `rust-agent-tui/src/ui/main_ui/panels/relay.rs`（177 行）
- 删除: `rust-agent-tui/src/command/relay.rs`（19 行）

**执行步骤:**
- [x] 删除 `relay_panel.rs` — /relay 面板状态定义
  - 文件: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/relay_panel.rs`
  - 该文件定义了 `RelayPanel` 结构体、`RelayPanelMode` 枚举、`RelayEditField` 枚举及其 `FormField` trait 实现，以及表单编辑/保存/取消方法
  - 文件内含 14 个 `#[test]` 单元测试（test_relay_panel_from_config、test_display_token 等），随文件一起删除
  - 原因: 整个面板功能随 Relay 废弃而移除
- [x] 删除 `relay_ops.rs` — Relay 连接/断开/重连/事件转发
  - 文件: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/relay_ops.rs`
  - 该文件包含 `ws_url_to_http`、`get_or_register_user_id` 两个辅助函数和 `App` 的三个方法实现：`check_relay_reconnect`（重连计时器检查）、`poll_relay`（消费 Relay WebSocket 事件并分发 UserInput/HitlDecision/AskUserResponse/ClearThread 等）
  - 原因: 连接管理和事件转发逻辑随 Relay 废弃而移除
- [x] 删除 `relay_state.rs` — RelayState 子结构体
  - 文件: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/relay_state.rs`
  - 该文件定义了 `RelayState` 结构体（relay_client、relay_event_rx、relay_params、relay_reconnect_at 四个字段）及其 `Default` 实现
  - 原因: RelayState 是 App 结构体的子模块，仅服务于 Relay 连接状态管理
- [x] 删除 `relay_adapter.rs` — ExecutorEvent → RelayAgentEvent 适配器
  - 文件: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/relay_adapter.rs`
  - 该文件包含 `to_relay_event` 函数，将 `ExecutorEvent` 枚举映射为 `RelayAgentEvent` 枚举（AiReasoning、TextChunk、ToolStart、ToolEnd 等），MessageAdded 和 StateSnapshot 返回 None
  - 原因: 该适配器仅被 `agent.rs` 中的 Relay 事件转发路径使用
- [x] 删除 `panels/relay.rs` — /relay 面板 UI 渲染
  - 文件: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/main_ui/panels/relay.rs`
  - 该文件包含 `render_relay_panel`（面板入口，根据模式分发 View/Edit 渲染）、`render_relay_view`（只读模式，展示 URL/Token/Name/Web URL）、`render_relay_edit`（编辑模式，带光标的表单输入）和 `format_input_field`（光标格式化辅助函数）
  - 原因: 面板渲染逻辑随 /relay 命令废弃而移除
- [x] 删除 `command/relay.rs` — /relay 命令处理
  - 文件: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/command/relay.rs`
  - 该文件定义了 `RelayCommand` 结构体，实现 `Command` trait（name="relay"，description="打开远程控制配置面板"，execute 调用 `app.open_relay_panel()`）
  - 原因: /relay 命令随 Relay 功能废弃而移除
- [x] 验证 6 个文件已删除且无其他文件直接引用被删模块的公共 API
  - 测试方式: 通过文件不存在检查和 grep 搜索确认删除完整
  - 注意: `app/mod.rs` 中的 `pub mod relay_panel` / `mod relay_ops` / `mod relay_state` 声明、`lib.rs` 中的 `pub mod relay_adapter` 声明、`panels/mod.rs` 中的 `pub mod relay` 声明、`command/mod.rs` 中的 `pub mod relay` 声明将在 Task 3/4 中清理，本步骤仅验证文件层面删除完成
  - 运行:
    ```bash
    for f in \
      rust-agent-tui/src/app/relay_panel.rs \
      rust-agent-tui/src/app/relay_ops.rs \
      rust-agent-tui/src/app/relay_state.rs \
      rust-agent-tui/src/relay_adapter.rs \
      rust-agent-tui/src/ui/main_ui/panels/relay.rs \
      rust-agent-tui/src/command/relay.rs; do
      test ! -f "/Users/konghayao/code/ai/perihelion/$f" && echo "OK: $f removed" || echo "FAIL: $f still exists"
    done
    ```
  - 预期: 全部输出 "OK: ... removed"

**检查步骤:**
- [x] 6 个 Relay 专用文件不存在
  - `for f in rust-agent-tui/src/app/relay_panel.rs rust-agent-tui/src/app/relay_ops.rs rust-agent-tui/src/app/relay_state.rs rust-agent-tui/src/relay_adapter.rs rust-agent-tui/src/ui/main_ui/panels/relay.rs rust-agent-tui/src/command/relay.rs; do test ! -f "/Users/konghayao/code/ai/perihelion/$f" && echo "OK" || echo "FAIL: $f"; done`
  - 预期: 输出 6 行 "OK"
- [x] `app/` 目录中不再有 `relay_` 前缀文件
  - `ls /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/relay_*.rs 2>&1`
  - 预期: 输出 "No match found" 或 ls 报错（无匹配文件）
- [x] 项目根目录的 `src/` 下不再有 `relay_adapter.rs`
  - `test ! -f /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/relay_adapter.rs && echo "OK"`
  - 预期: 输出 "OK"
- [x] `panels/` 目录中不再有 `relay.rs`
  - `test ! -f /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/main_ui/panels/relay.rs && echo "OK"`
  - 预期: 输出 "OK"
- [x] `command/` 目录中不再有 `relay.rs`
  - `test ! -f /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/command/relay.rs && echo "OK"`
  - 预期: 输出 "OK"

---

### Task 3: 清理 TUI App 层 Relay 集成

**背景:**
Task 1 删除了 `rust-relay-server` crate 目录，Task 2 删除了 6 个 Relay 专用文件，但 TUI App 层（`app/mod.rs`、`agent.rs`、`agent_ops.rs`、`panel_ops.rs`、`hitl_ops.rs`、`ask_user_ops.rs`、`thread_ops.rs`、`lib.rs`、`events.rs`、`Cargo.toml`）仍保留大量 Relay 相关的字段、方法、事件变体和依赖声明。本 Task 彻底清除这些残留，使 TUI App 层不再包含任何 Relay 逻辑。本 Task 依赖 Task 1（删除 crate 目录使 `Cargo.toml` 依赖无法解析）和 Task 2（删除文件使 `mod` 声明无法编译）。本 Task 的输出是 Task 4（清理 UI/Event/Command/Config 层）的基础——Task 4 中 `main.rs` 对 `RelayCli`/`parse_relay_args`/`try_connect_relay` 的调用才会在本 Task 清理完定义后处理。

**涉及文件:**
- 修改: `rust-agent-tui/Cargo.toml`
- 修改: `rust-agent-tui/src/lib.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/app/agent.rs`
- 修改: `rust-agent-tui/src/app/agent_ops.rs`
- 修改: `rust-agent-tui/src/app/panel_ops.rs`
- 修改: `rust-agent-tui/src/app/hitl_ops.rs`
- 修改: `rust-agent-tui/src/app/ask_user_ops.rs`
- 修改: `rust-agent-tui/src/app/thread_ops.rs`
- 修改: `rust-agent-tui/src/app/events.rs`

**执行步骤:**
- [x] 移除 `rust-relay-server` 依赖（Cargo.toml）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/Cargo.toml` ~L36-38
  - 删除以下 3 行:
    ```toml
    rust-relay-server = { path = "../rust-relay-server", default-features = false, features = [
        "client",
    ] }
    ```
  - 原因: crate 已在 Task 1 中删除，保留此依赖会导致编译失败
- [x] 移除 `relay_adapter` 模块声明和 `RelayCli`/`parse_relay_args`（lib.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/lib.rs`
  - 删除第 9 行 `pub mod relay_adapter;`（relay_adapter.rs 已在 Task 2 删除）
  - 删除第 13-19 行 `RelayCli` struct 定义（L13-19）:
    ```rust
    /// CLI 参数解析结果：--remote-control [url] [--relay-token <token>] [--relay-name <name>]
    /// url 为空字符串表示 `--remote-control` 无参数模式（从配置读取）
    pub struct RelayCli {
        pub url: String,
        pub token: Option<String>,
        pub name: Option<String>,
    }
    ```
  - 删除第 21-42 行 `parse_relay_args` 函数（L21-42）:
    ```rust
    pub fn parse_relay_args(args: &[String]) -> Option<RelayCli> {
        // 查找 --remote-control 参数位置
        let remote_idx = args.iter().position(|a| a == "--remote-control")?;
        // 检查是否有值（即 --remote-control <url> 格式）
        // 有值条件：下一个参数存在且不以 -- 开头
        let url = if remote_idx + 1 < args.len() && !args[remote_idx + 1].starts_with("--") {
            args[remote_idx + 1].clone()
        } else {
            String::new()
        };
        let token = args.windows(2)
            .find(|w| w[0] == "--relay-token")
            .map(|w| w[1].clone());
        let name = args.windows(2)
            .find(|w| w[0] == "--relay-name")
            .map(|w| w[1].clone());
        Some(RelayCli { url, token, name })
    }
    ```
  - 原因: RelayCli 和 parse_relay_args 仅服务于 Relay 连接功能，relay_adapter.rs 已删除
- [x] 移除 Relay 模块声明和 re-export（app/mod.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/mod.rs`
  - 删除第 7 行 `pub mod relay_panel;`（relay_panel.rs 已在 Task 2 删除）
  - 删除第 23 行 `mod relay_state;`（relay_state.rs 已在 Task 2 删除）
  - 删除第 24 行 `mod relay_ops;`（relay_ops.rs 已在 Task 2 删除）
  - 删除第 55 行 `pub use relay_panel::RelayPanel;`（RelayPanel 不再存在）
  - 删除第 67 行 `pub use relay_state::RelayState;`（RelayState 不再存在）
  - 原因: 这 5 行声明/re-export 对应的文件已在 Task 2 中删除
- [x] 移除 App struct 的 Relay 字段和初始化（app/mod.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/mod.rs`
  - 删除第 74 行 `pub relay: RelayState,`（RelayState 已删除）
  - 删除第 85 行 `pub relay_panel: Option<RelayPanel>,`（RelayPanel 已删除）
  - 删除 `App::new()` 中第 151 行 `relay: RelayState::default(),`
  - 删除 `App::new()` 中第 161 行 `relay_panel: None,`
  - 原因: App struct 不再持有 Relay 状态和面板数据
- [x] 移除 `try_connect_relay` 方法（app/mod.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/mod.rs` ~L221-371
  - 删除整个 `try_connect_relay` 方法（从 `pub async fn try_connect_relay(&mut self, cli: Option<&crate::RelayCli>)` 开始，到方法结束的 `}`）
  - 该方法约 150 行，包含 CLI 参数解析、user_id 注册、RelayClient::connect 调用、连接成功/失败回调
  - 原因: Relay 连接功能整体移除
- [x] 移除 headless test 中的 Relay 字段引用（app/mod.rs → panel_ops.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/panel_ops.rs` ~L267 和 ~L277
  - 在 `new_headless` 方法的 App 构造体中:
    - 删除 `relay: super::RelayState::default(),`（~L267）
    - 删除 `relay_panel: None,`（~L277）
  - 原因: 构造体字段已从 App struct 中移除
- [x] 移除 `AgentRunConfig.relay_client` 字段和所有 Relay 转发逻辑（agent.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/agent.rs`
  - 删除第 26 行 `pub relay_client: Option<Arc<rust_relay_server::client::RelayClient>>,`
  - 删除 `run_universal_agent` 函数中第 44 行的 `relay_client,` 解构字段
  - 删除事件回调中的 Relay 转发块（~L101-118，`let relay_for_handler = relay_client.clone();` 和 `if let Some(ref relay) = relay_for_handler { ... }` 块）:
    ```rust
    // 删除: L101
    let relay_for_handler = relay_client.clone();
    // 删除: L106-118（在 FnEventHandler 闭包内的 Relay 转发逻辑）
    if let Some(ref relay) = relay_for_handler {
        match &event {
            ExecutorEvent::MessageAdded(msg) => {
                relay.send_message(&serde_json::to_value(msg).unwrap_or_default());
            }
            _ => {
                if let Some(relay_event) = crate::relay_adapter::to_relay_event(&event) {
                    relay.send_agent_event(&relay_event);
                }
            }
        }
    }
    ```
  - 删除 `run_universal_agent` 函数中 agent 执行前的 Relay 通知（~L208-210）:
    ```rust
    if let Some(ref relay) = relay_client {
        relay.send_value(serde_json::json!({ "type": "agent_running" }));
    }
    ```
  - 删除 `run_universal_agent` 函数中 agent 执行后的 Relay 通知（~L240-242）:
    ```rust
    if let Some(ref relay) = relay_client {
        relay.send_value(serde_json::json!({ "type": "agent_done" }));
    }
    ```
  - 原因: relay_client 字段类型引用了已删除的 `rust_relay_server` crate，且所有转发逻辑不再需要
- [x] 修改 `map_executor_event` 中 `MessageAdded` 的映射为 `return None`（agent.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/agent.rs` ~L254
  - 将 `ExecutorEvent::MessageAdded(msg) => AgentEvent::MessageAdded(msg),` 改为与 `StateSnapshot` 相同的处理方式
  - 在 match arm 的"无需转发的内部事件"分组中添加 `ExecutorEvent::MessageAdded(_)`:
    ```rust
    // 修改后的忽略分支:
    ExecutorEvent::ToolEnd { .. }
    | ExecutorEvent::StepDone { .. }
    | ExecutorEvent::StateSnapshot(_)
    | ExecutorEvent::MessageAdded(_)
    | ExecutorEvent::LlmCallStart { .. }
    | ExecutorEvent::LlmCallEnd { .. } => return None,
    ```
  - 同时删除原来的 `ExecutorEvent::MessageAdded(msg) => AgentEvent::MessageAdded(msg),` 行
  - 原因: `ExecutorEvent::MessageAdded` 在 `rust-create-agent` 的 executor 单元测试中仍被使用，保留在 executor 侧；TUI 层不再需要此事件——AgentEvent::MessageAdded 将在后续步骤中从 events.rs 删除
- [x] 移除 `submit_message` 中的 relay_client 提取和传递（agent_ops.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/agent_ops.rs` ~L132 和 ~L147
  - 删除第 132 行 `let relay_client = self.relay.relay_client.clone();`（self.relay 字段已移除）
  - 删除 `AgentRunConfig` 构造中的 `relay_client,` 字段（~L147）
  - 原因: AgentRunConfig 的 relay_client 字段已在上一步移除
- [x] 移除 `handle_agent_event` 中 `AgentEvent::MessageAdded` 分支（agent_ops.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/agent_ops.rs` ~L234-242
  - 删除整个 `AgentEvent::MessageAdded(msg)` match arm:
    ```rust
    AgentEvent::MessageAdded(msg) => {
        // SubAgent 执行期间忽略 MessageAdded（不影响父 Agent 消息历史）
        if self.core.subagent_group_idx.is_some() {
            return (false, false, false);
        }
        // AI 消息文本由紧随其后的 AiReasoning→AssistantChunk 事件处理，此处不处理
        let _ = msg;
        (true, false, false)
    }
    ```
  - 原因: AgentEvent::MessageAdded 变体将从 events.rs 中移除，此 match arm 不再可编译
- [x] 移除 `handle_agent_event` 中 `InteractionRequest::Approval` 分支内的 Relay 转发（agent_ops.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/agent_ops.rs` ~L401-411
  - 删除 `if let Some(ref relay) = self.relay.relay_client { ... }` 块:
    ```rust
    // 转发 HITL 审批请求到 Relay（统一 interaction_request 消息）
    if let Some(ref relay) = self.relay.relay_client {
        let relay_items: Vec<serde_json::Value> = batch_items
            .iter()
            .map(|item| serde_json::json!({ "tool_name": item.tool_name, "input": item.input }))
            .collect();
        relay.send_value(serde_json::json!({
            "type": "interaction_request",
            "ctx_type": "approval",
            "items": relay_items
        }));
    }
    ```
  - 原因: self.relay 字段已从 App struct 移除
- [x] 移除 `handle_agent_event` 中 `InteractionRequest::Questions` 分支内的 Relay 转发（agent_ops.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/agent_ops.rs` ~L444-459
  - 删除 `if let Some(ref relay) = self.relay.relay_client { ... }` 块:
    ```rust
    if let Some(ref relay) = self.relay.relay_client {
        let questions_json: Vec<serde_json::Value> = ask_questions.iter().map(|q| {
            serde_json::json!({
                "tool_call_id": q.tool_call_id,
                "question": q.question,
                "header": q.header,
                "multi_select": q.multi_select,
                "options": q.options.iter().map(|o| serde_json::json!({"label": o.label, "description": o.description})).collect::<Vec<_>>(),
            })
        }).collect();
        relay.send_value(serde_json::json!({
            "type": "interaction_request",
            "ctx_type": "questions",
            "questions": questions_json
        }));
    }
    ```
  - 原因: self.relay 字段已从 App struct 移除
- [x] 移除 `handle_agent_event` 中 `TodoUpdate` 分支内的 Relay 转发（agent_ops.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/agent_ops.rs` ~L470-484
  - 删除 `if let Some(ref relay) = self.relay.relay_client { ... }` 块:
    ```rust
    if let Some(ref relay) = self.relay.relay_client {
        let items: Vec<serde_json::Value> = todos
            .iter()
            .map(|t| {
                serde_json::json!({
                    "content": t.content,
                    "status": format!("{:?}", t.status),
                })
            })
            .collect();
        relay.send_value(serde_json::json!({
            "type": "todo_update",
            "items": items,
        }));
    }
    ```
  - 原因: self.relay 字段已从 App struct 移除
- [x] 移除 `handle_agent_event` 中 `CompactDone` 分支内的 Relay 通知（agent_ops.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/agent_ops.rs` ~L582-589
  - 删除 `if let Some(ref relay) = self.relay.relay_client { ... }` 块:
    ```rust
    // 通知 Relay Web 前端：compact 完成
    if let Some(ref relay) = self.relay.relay_client {
        relay.send_value(serde_json::json!({
            "type": "compact_done",
            "summary": summary,
            "new_thread_id": new_tid,
            "old_thread_id": old_thread_id.unwrap_or_default(),
        }));
    }
    ```
  - 原因: self.relay 字段已从 App struct 移除
- [x] 移除全部 Relay 面板操作方法（panel_ops.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/panel_ops.rs` ~L166-214
  - 删除整个 "Relay 面板操作" section，包含 4 个方法:
    - `open_relay_panel`（~L169-185）
    - `close_relay_panel`（~L188-190）
    - `relay_panel_apply_edit`（~L193-203）
    - `relay_panel_cancel_edit`（~L206-214）
  - 删除对应的 section 注释 `// ─── Relay 面板操作 ──────`
  - 原因: 这些方法操作已删除的 RelayPanel 和 RelayState
- [x] 移除 `send_hitl_resolved` 方法及其所有调用点（hitl_ops.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/hitl_ops.rs`
  - 删除 `send_hitl_resolved` 方法定义（~L19-23）:
    ```rust
    fn send_hitl_resolved(&mut self) {
        if let Some(ref relay) = self.relay.relay_client {
            relay.send_value(serde_json::json!({ "type": "interaction_resolved" }));
        }
    }
    ```
  - 删除 `hitl_approve_all` 中对 `self.send_hitl_resolved();` 的调用（~L32）
  - 删除 `hitl_reject_all` 中对 `self.send_hitl_resolved();` 的调用（~L44）
  - 删除 `hitl_confirm` 中对 `self.send_hitl_resolved();` 的调用（~L55）
  - 原因: self.relay 字段已移除，`send_hitl_resolved` 不再有可用数据
- [x] 移除 `ask_user_confirm` 中的 Relay 通知（ask_user_ops.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/ask_user_ops.rs` ~L64-66
  - 删除 `if let Some(ref relay) = self.relay.relay_client { ... }` 块:
    ```rust
    if let Some(ref relay) = self.relay.relay_client {
        relay.send_value(serde_json::json!({ "type": "interaction_resolved" }));
    }
    ```
  - 原因: self.relay 字段已移除
- [x] 移除 `open_thread` 中的 Relay thread reset（thread_ops.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/thread_ops.rs` ~L109-116
  - 删除 `if let Some(ref relay) = self.relay.relay_client { ... }` 块:
    ```rust
    // 通知 Relay Web 前端：thread 已切换，推送完整历史消息
    if let Some(ref relay) = self.relay.relay_client {
        let msg_vals: Vec<serde_json::Value> = base_msgs
            .iter()
            .filter_map(|m| serde_json::to_value(m).ok())
            .collect();
        relay.send_thread_reset(&msg_vals);
    }
    ```
  - 原因: self.relay 字段已移除
- [x] 移除 `new_thread` 中的 Relay thread reset（thread_ops.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/thread_ops.rs` ~L135-137
  - 删除:
    ```rust
    if let Some(ref relay) = self.relay.relay_client {
        relay.send_thread_reset(&[]);
    }
    ```
  - 原因: self.relay 字段已移除
- [x] 移除 `AgentEvent::MessageAdded` 变体（events.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/events.rs` ~L15-16
  - 删除:
    ```rust
    /// 新消息添加到状态（包括最终 AI 回答）
    MessageAdded(rust_create_agent::messages::BaseMessage),
    ```
  - 原因: TUI 层不再消费此事件，所有 MessageAdded 相关逻辑已在前面步骤中清除
- [x] 为 TUI App 层 Relay 清除编写验证测试
  - 测试方式: 编译验证 + grep 结构检查
  - 由于本 Task 是纯删除操作（不新增业务逻辑），不需要新增单元测试。现有 headless 测试（`panel_ops.rs` 中的 `new_headless`）在移除 relay/relay_panel 字段后仍可正常编译，验证 App 构造完整性。
  - 运行命令: `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 编译成功（注意: 此时 `main.rs` 中对 `RelayCli`/`parse_relay_args`/`try_connect_relay` 的引用尚未清理，编译仍会失败。这是预期行为——这些引用在 Task 4 中清理。本步骤的编译验证应在 Task 4 完成后最终确认）
  - 替代验证: 在本 Task 范围内，通过 grep 确认所有指定删除项已被清除:
    ```bash
    cd /Users/konghayao/code/ai/perihelion
    # 验证 Cargo.toml 不再依赖 rust-relay-server
    grep "rust-relay-server" rust-agent-tui/Cargo.toml
    # 验证 lib.rs 不再包含 RelayCli 和 relay_adapter
    grep -n "RelayCli\|relay_adapter" rust-agent-tui/src/lib.rs
    # 验证 app/mod.rs 不再包含 relay 相关声明
    grep -n "relay_panel\|relay_state\|relay_ops\|RelayPanel\|RelayState\|try_connect_relay" rust-agent-tui/src/app/mod.rs
    # 验证 events.rs 不再包含 MessageAdded
    grep -n "MessageAdded" rust-agent-tui/src/app/events.rs
    # 验证 agent.rs 不再包含 relay_client
    grep -n "relay_client\|relay_for_handler\|relay_adapter" rust-agent-tui/src/app/agent.rs
    # 验证 agent_ops.rs 不再包含 relay
    grep -n "relay_client\|MessageAdded" rust-agent-tui/src/app/agent_ops.rs
    # 验证 panel_ops.rs 不再包含 relay 方法
    grep -n "relay_panel\|open_relay\|close_relay\|RelayPanel" rust-agent-tui/src/app/panel_ops.rs
    # 验证 hitl_ops.rs 不再包含 send_hitl_resolved
    grep -n "send_hitl_resolved\|relay_client" rust-agent-tui/src/app/hitl_ops.rs
    # 验证 ask_user_ops.rs 不再包含 relay
    grep -n "relay_client" rust-agent-tui/src/app/ask_user_ops.rs
    # 验证 thread_ops.rs 不再包含 relay
    grep -n "relay_client\|send_thread_reset" rust-agent-tui/src/app/thread_ops.rs
    ```
  - 预期: 所有 grep 命令均无输出（返回非 0 退出码），确认 10 个文件中的 Relay 残留已全部清除

**检查步骤:**
- [x] `Cargo.toml` 不再包含 `rust-relay-server` 依赖
  - `grep "rust-relay-server" /Users/konghayao/code/ai/perihelion/rust-agent-tui/Cargo.toml`
  - 预期: 无输出
- [x] `lib.rs` 不再包含 `relay_adapter`、`RelayCli`、`parse_relay_args`
  - `grep -c "relay_adapter\|RelayCli\|parse_relay_args" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/lib.rs`
  - 预期: 输出 0
- [x] `app/mod.rs` 不再包含 `relay_panel`、`relay_state`、`relay_ops`、`RelayPanel`、`RelayState`、`try_connect_relay`、`relay:` 字段
  - `grep -c "relay_panel\|relay_state\|relay_ops\|RelayPanel\|RelayState\|try_connect_relay" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/mod.rs`
  - 预期: 输出 0
- [x] `events.rs` 不再包含 `MessageAdded` 变体
  - `grep -c "MessageAdded" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/events.rs`
  - 预期: 输出 0
- [x] `agent.rs` 不再包含 `relay_client`、`relay_adapter` 引用
  - `grep -c "relay_client\|relay_adapter\|relay_for_handler" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/agent.rs`
  - 预期: 输出 0
- [x] `agent_ops.rs` 不再包含 `relay_client` 引用和 `MessageAdded` match arm
  - `grep -c "relay_client\|MessageAdded" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/agent_ops.rs`
  - 预期: 输出 0
- [x] `panel_ops.rs` 不再包含 Relay 面板操作方法和 `relay` 字段引用
  - `grep -c "relay_panel\|open_relay\|close_relay\|RelayPanel\|relay:" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/panel_ops.rs`
  - 预期: 输出 0
- [x] `hitl_ops.rs` 不再包含 `send_hitl_resolved` 和 `relay_client`
  - `grep -c "send_hitl_resolved\|relay_client" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/hitl_ops.rs`
  - 预期: 输出 0
- [x] `ask_user_ops.rs` 不再包含 `relay_client`
  - `grep -c "relay_client" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/ask_user_ops.rs`
  - 预期: 输出 0
- [x] `thread_ops.rs` 不再包含 `relay_client` 和 `send_thread_reset`
  - `grep -c "relay_client\|send_thread_reset" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/thread_ops.rs`
  - 预期: 输出 0

---

### Task 4: 清理 TUI UI/Event/Command/Config 层

**背景:**
Task 1-3 删除了 `rust-relay-server` crate 和 TUI App 层的全部 Relay 集成，但 UI 渲染层（`main_ui.rs`、`panels/mod.rs`）、事件处理层（`event.rs`）、命令注册层（`command/mod.rs`）、配置类型层（`config/types.rs`、`config/mod.rs`）、主入口（`main.rs`）和 headless 测试（`headless.rs`）中仍残留 Relay 相关的渲染分发、按键处理、命令注册、配置结构体和测试代码。本 Task 清除这些残留，使整个 TUI crate 编译通过且无 Relay 引用。本 Task 依赖 Task 2（删除 `panels/relay.rs`、`command/relay.rs` 文件使 mod 声明失效）和 Task 3（移除 `App.relay_panel` 字段、`RelayPanel`/`RelayState` 类型、`RelayCli`/`parse_relay_args`/`AgentEvent::MessageAdded` 定义）。Task 4 完成后 `cargo build -p rust-agent-tui` 应能成功编译。

**涉及文件:**
- 修改: `rust-agent-tui/src/event.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`
- 修改: `rust-agent-tui/src/ui/main_ui/panels/mod.rs`
- 修改: `rust-agent-tui/src/command/mod.rs`
- 修改: `rust-agent-tui/src/config/types.rs`
- 修改: `rust-agent-tui/src/config/mod.rs`
- 修改: `rust-agent-tui/src/main.rs`
- 修改: `rust-agent-tui/src/ui/headless.rs`

**执行步骤:**
- [x] 移除 `/relay` 面板优先处理块（event.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/event.rs` L98-102
  - 删除以下 5 行:
    ```rust
    // /relay 面板优先处理
    if app.relay_panel.is_some() {
        handle_relay_panel(app, input);
        return Ok(Some(Action::Redraw));
    }
    ```
  - 原因: `app.relay_panel` 字段已在 Task 3 中移除，此分支无法编译
- [x] 移除 `Event::Paste` 中 relay_panel 粘贴处理（event.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/event.rs` L382-388
  - 删除以下 7 行:
    ```rust
    // relay_panel 编辑模式下粘贴到面板
    if let Some(panel) = app.relay_panel.as_mut() {
        if panel.mode == crate::app::relay_panel::RelayPanelMode::Edit {
            panel.form.handle_paste(&text);
            return Ok(Some(Action::Redraw));
        }
    }
    ```
  - 原因: `app.relay_panel` 字段和 `RelayPanelMode` 类型已在 Task 3 中移除
- [x] 移除 `handle_relay_panel` 函数（event.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/event.rs` L645-703
  - 删除整个 `handle_relay_panel` 函数（从 `// ─── /relay 面板键盘处理` 注释行到函数结束的 `}`），共约 59 行
  - 该函数引用了 `crate::app::relay_panel::RelayPanelMode`、`app.relay_panel`、`app.close_relay_panel()`、`app.relay_panel_apply_edit()`、`app.relay_panel_cancel_edit()`，这些均已在 Task 2/3 中删除
  - 原因: relay 面板按键处理函数整体移除
- [x] 移除 relay panel 渲染分发（main_ui.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/main_ui.rs` L96-98
  - 删除以下 3 行:
    ```rust
    if app.relay_panel.is_some() {
        panels::relay::render_relay_panel(f, app, panel_area);
    }
    ```
  - 原因: `app.relay_panel` 字段和 `panels::relay` 模块已分别在 Task 3 和 Task 2 中移除
- [x] 移除 `active_panel_height` 函数中 `relay_panel` 高度计算分支（main_ui.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/main_ui.rs` L124-125
  - 删除以下 2 行:
    ```rust
    } else if app.relay_panel.is_some() {
        10
    ```
  - 原因: `app.relay_panel` 字段已移除，此分支无法编译；移除后 agent_panel 分支的 `}` 后直接接 cron 分支
- [x] 移除 `pub mod relay;` 声明（panels/mod.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/main_ui/panels/mod.rs` L4
  - 删除 `pub mod relay;` 这一行
  - 原因: `panels/relay.rs` 已在 Task 2 中删除，此 mod 声明会导致编译错误
- [x] 移除 `pub mod relay;` 声明（command/mod.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/command/mod.rs` L9
  - 删除 `pub mod relay;` 这一行
  - 原因: `command/relay.rs` 已在 Task 2 中删除
- [x] 移除 relay 命令注册（command/mod.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/command/mod.rs` L20
  - 删除 `r.register(Box::new(relay::RelayCommand));` 这一行
  - 原因: `relay::RelayCommand` 类型已随 `command/relay.rs` 删除
- [x] 移除 `RemoteControlConfig` struct（config/types.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/config/types.rs` L80-102
  - 删除 `RemoteControlConfig` struct 定义（L80-95）和 `RemoteControlConfig::is_complete()` impl 块（L97-102），共 23 行:
    ```rust
    /// 远程控制配置
    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct RemoteControlConfig {
        #[serde(default)]
        pub url: String,
        #[serde(default)]
        pub token: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub user_id: Option<String>,
    }

    impl RemoteControlConfig {
        pub fn is_complete(&self) -> bool {
            !self.url.is_empty()
        }
    }
    ```
  - 原因: RemoteControlConfig 仅服务于 Relay 连接配置，随 Relay 功能整体废弃
- [x] 移除 `AppConfig` 中 `remote_control` 字段（config/types.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/config/types.rs` L127-129
  - 删除以下 3 行:
    ```rust
    /// 远程控制配置
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_control: Option<RemoteControlConfig>,
    ```
  - 原因: RemoteControlConfig 类型已在上一步移除
- [x] 移除所有 RemoteControlConfig 相关测试（config/types.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/config/types.rs` L330-400
  - 删除以下 6 个测试函数（约 71 行）:
    - `test_remote_control_config_is_complete`（L332-342）
    - `test_remote_control_config_serde_roundtrip`（L344-357）
    - `test_remote_control_config_skip_name_when_none`（L359-369）
    - `test_app_config_remote_control_optional`（L371-376）
    - `test_app_config_remote_control_roundtrip`（L378-393）
    - `test_app_config_remote_control_skip_when_none`（L395-400）
  - 同时删除 `// ── RemoteControlConfig 测试 ──` section 注释（L330）
  - 原因: 这些测试引用已删除的 RemoteControlConfig 类型，无法编译
- [x] 移除 `RemoteControlConfig` re-export（config/mod.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/config/mod.rs` L5
  - 将 `pub use types::{ModelAliasConfig, ProviderConfig, RemoteControlConfig, ThinkingConfig, ZenConfig};` 中的 `RemoteControlConfig, ` 删除
  - 修改后: `pub use types::{ModelAliasConfig, ProviderConfig, ThinkingConfig, ZenConfig};`
  - 原因: RemoteControlConfig 类型已从 types.rs 中移除
- [x] 移除 Relay 相关 import（main.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/main.rs` L15
  - 将 `use rust_agent_tui::{parse_relay_args, RelayCli};` 删除
  - 原因: `RelayCli` 和 `parse_relay_args` 已在 Task 3 中从 lib.rs 移除
- [x] 移除 `parse_relay_args` 调用（main.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/main.rs` L70
  - 删除 `let relay_cli = parse_relay_args(&args);` 这一行
  - 原因: `parse_relay_args` 函数已移除
- [x] 移除 `run_app` 调用中的 `relay_cli` 参数传递（main.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/main.rs` L89
  - 将 `let result = run_app(&mut terminal, relay_cli).await;` 改为 `let result = run_app(&mut terminal).await;`
  - 原因: `run_app` 函数签名将在下一步移除 `relay_cli` 参数
- [x] 移除 `run_app` 函数签名中 `relay_cli` 参数（main.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/main.rs` L110
  - 将 `async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, relay_cli: Option<RelayCli>) -> Result<()> {` 改为 `async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {`
  - 原因: RelayCli 类型已移除，relay_cli 参数不再使用
- [x] 移除 `run_app` 函数体中 `try_connect_relay` 调用（main.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/main.rs` L123-124
  - 删除以下 2 行:
    ```rust
    // 尝试连接 Relay Server（CLI 参数优先，其次读 settings.json）
    app.try_connect_relay(relay_cli.as_ref()).await;
    ```
  - 原因: `try_connect_relay` 方法已在 Task 3 中从 App 移除
- [x] 移除 `run_app` 事件循环中 `poll_relay` 和 `check_relay_reconnect` 调用（main.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/main.rs` L134-137
  - 删除以下 4 行:
    ```rust
    // 轮询 Relay 事件（Web 端控制消息）
    let relay_updated = app.poll_relay();
    // 检查 Relay 是否需要重连（断线 3s 后自动重试）
    app.check_relay_reconnect().await;
    ```
  - 原因: `poll_relay` 和 `check_relay_reconnect` 方法已在 Task 3 中从 App 移除
- [x] 移除 redraw 条件中 `relay_updated`（main.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/main.rs` L156
  - 将 `if cache_updated || agent_updated || relay_updated || app.core.loading {` 改为 `if cache_updated || agent_updated || app.core.loading {`
  - 原因: `relay_updated` 变量已在上一步移除
- [x] 移除所有 relay 相关测试（main.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/main.rs` L175-243
  - 删除以下 5 个测试函数（约 69 行）:
    - `test_parse_relay_args_no_url`（L176-185）
    - `test_parse_relay_args_with_url`（L187-201）
    - `test_parse_relay_args_with_all_params`（L203-221）
    - `test_parse_relay_args_url_starts_with_dash`（L223-235）
    - `test_parse_relay_args_none`（L237-243）
  - 原因: 这些测试引用已移除的 `parse_relay_args` 函数和 `RelayCli` 类型，无法编译
- [x] 移除 `test_tool_call_message_collapsed_by_default` 测试（headless.rs）
  - 位置: `/Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/headless.rs` L158-185
  - 删除整个 `test_tool_call_message_collapsed_by_default` 测试函数（约 28 行）:
    ```rust
    #[tokio::test]
    async fn test_tool_call_message_collapsed_by_default() {
        // ... 使用了 AgentEvent::MessageAdded(ai_msg)，该变体已在 Task 3 中移除
    }
    ```
  - 原因: 该测试使用 `AgentEvent::MessageAdded`，该变体已在 Task 3 中从 events.rs 移除
- [x] 为 TUI UI/Event/Command/Config 层 Relay 清除编写验证测试
  - 测试方式: 编译验证 + grep 结构检查
  - 由于本 Task 是纯删除操作（不新增业务逻辑），不需要新增单元测试。验证方式为全量编译通过和 grep 确认无残留引用。
  - 运行命令: `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 编译成功，无错误
  - 替代验证: 通过 grep 确认所有指定删除项已被清除:
    ```bash
    cd /Users/konghayao/code/ai/perihelion
    # 验证 event.rs 不再包含 relay_panel 和 handle_relay_panel
    grep -n "relay_panel\|handle_relay_panel\|RelayPanelMode" rust-agent-tui/src/event.rs
    # 验证 main_ui.rs 不再包含 relay_panel 和 panels::relay
    grep -n "relay_panel\|panels::relay" rust-agent-tui/src/ui/main_ui.rs
    # 验证 panels/mod.rs 不再包含 relay
    grep -n "relay" rust-agent-tui/src/ui/main_ui/panels/mod.rs
    # 验证 command/mod.rs 不再包含 relay
    grep -n "relay" rust-agent-tui/src/command/mod.rs
    # 验证 config/types.rs 不再包含 RemoteControlConfig 和 remote_control
    grep -n "RemoteControlConfig\|remote_control" rust-agent-tui/src/config/types.rs
    # 验证 config/mod.rs 不再包含 RemoteControlConfig
    grep -n "RemoteControlConfig" rust-agent-tui/src/config/mod.rs
    # 验证 main.rs 不再包含 relay 相关引用
    grep -n "relay_cli\|parse_relay_args\|RelayCli\|poll_relay\|check_relay_reconnect\|try_connect_relay\|relay_updated" rust-agent-tui/src/main.rs
    # 验证 headless.rs 不再包含 MessageAdded
    grep -n "MessageAdded" rust-agent-tui/src/ui/headless.rs
    ```
  - 预期: 所有 grep 命令均无输出（返回非 0 退出码），确认 8 个文件中的 Relay 残留已全部清除

**检查步骤:**
- [x] `rust-agent-tui` crate 编译成功
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 "Compiling" 和 "Finished"，无 error
- [x] `event.rs` 不再包含 `relay_panel`、`handle_relay_panel`、`RelayPanelMode`
  - `grep -c "relay_panel\|handle_relay_panel\|RelayPanelMode" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/event.rs`
  - 预期: 输出 0
- [x] `main_ui.rs` 不再包含 `relay_panel` 和 `panels::relay`
  - `grep -c "relay_panel\|panels::relay" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 输出 0
- [x] `panels/mod.rs` 不再包含 `relay`
  - `grep -c "relay" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/main_ui/panels/mod.rs`
  - 预期: 输出 0
- [x] `command/mod.rs` 不再包含 `relay`
  - `grep -c "relay" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/command/mod.rs`
  - 预期: 输出 0
- [x] `config/types.rs` 不再包含 `RemoteControlConfig` 和 `remote_control`
  - `grep -c "RemoteControlConfig\|remote_control" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/config/types.rs`
  - 预期: 输出 0
- [x] `config/mod.rs` 不再包含 `RemoteControlConfig`
  - `grep -c "RemoteControlConfig" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/config/mod.rs`
  - 预期: 输出 0
- [x] `main.rs` 不再包含任何 relay 相关引用
  - `grep -c "relay_cli\|parse_relay_args\|RelayCli\|poll_relay\|check_relay_reconnect\|try_connect_relay\|relay_updated" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/main.rs`
  - 预期: 输出 0
- [x] `headless.rs` 不再包含 `MessageAdded`
  - `grep -c "MessageAdded" /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/headless.rs`
  - 预期: 输出 0
- [x] `rust-agent-tui` 测试全部通过
  - `cargo test -p rust-agent-tui 2>&1 | tail -10`
  - 预期: 所有测试通过，无失败

---

### Task 5: 更新全局文档

**背景:**
Task 1-4 删除了 `rust-relay-server` crate 及 TUI 中所有 Relay 集成代码，但 4 个全局文档（`architecture.md`、`constraints.md`、`features.md`、`CLAUDE.md`）仍保留大量 Relay 相关的架构描述、技术栈条目、功能特性和开发注意事项。本 Task 同步更新这些文档，使文档与代码状态一致——workspace 从 4 crate 变为 3 crate，所有 Relay 相关内容移除。本 Task 依赖 Task 1-4 全部完成（代码层 Relay 已彻底移除）。

**涉及文件:**
- 修改: `spec/global/architecture.md`
- 修改: `spec/global/constraints.md`
- 修改: `spec/global/features.md`
- 修改: `CLAUDE.md`

**执行步骤:**
- [x] 更新 `spec/global/architecture.md` — 系统组件表
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/architecture.md` L9-10
  - 删除 `rust-agent-tui` 描述中的 `Relay 集成`:
    - 将 `│ rust-agent-tui │ 可执行文件 │ 基于 ratatui 的交互式 TUI，异步渲染、多会话管理、HITL/AskUser 弹窗、配置面板、Langfuse 追踪、Relay 集成 │` 改为 `│ rust-agent-tui │ 可执行文件 │ 基于 ratatui 的交互式 TUI，异步渲染、多会话管理、HITL/AskUser 弹窗、配置面板、Langfuse 追踪 │`
  - 删除整个 `rust-relay-server` 行:
    - `│ rust-relay-server │ 可执行文件 + 客户端库 │ axum WebSocket 中继服务（server feature），支持远程控制本地 Agent；client feature 供 TUI 集成；多用户隔离（UserNamespace 分层 + /register 匿名账号）；前端为 Preact + Signals + htm（esm.sh CDN，无打包工具） │`
  - 原因: workspace 从 4 crate 减为 3 crate，系统组件表只保留实际存在的 crate
- [x] 更新 `spec/global/architecture.md` — Workspace 依赖关系图
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/architecture.md` L14-27
  - 将整个 Workspace 依赖关系代码块替换为:
    ```
    rust-create-agent           ← 零内部依赖，纯核心框架
        ↑
    rust-agent-middlewares      ← 依赖 rust-create-agent
        ↑
    rust-agent-tui              ← 依赖 rust-agent-middlewares
    ```
  - 删除 L23-27 的 Feature Gates 说明:
    ```
    **Feature Gates（rust-relay-server）：**
    - `server`（默认）：axum + dashmap + rust-embed，编译为独立中继服务
    - `client`：仅 tokio-tungstenite，嵌入 TUI 使用
    ```
  - 原因: workspace 依赖关系图中移除 rust-relay-server 节点及其 Feature Gates
- [x] 更新 `spec/global/architecture.md` — rust-agent-tui 内部模块列表
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/architecture.md` L119-126
  - 删除以下 3 行:
    - L119: `│   ├── relay_panel.rs    — /relay 面板状态（URL/Token/Name 配置）`
    - L125: `│   ├── relay_ops.rs      — Relay 连接/断开/事件转发操作`
  - 删除 L150 `├── config/types.rs          — 配置类型定义（Provider/Model/RemoteControl）` 中的 `RemoteControl`:
    - 改为 `├── config/types.rs          — 配置类型定义（Provider/Model）`
  - 删除 L134: `│   │   │   ├── relay.rs  — /relay 面板 UI`
  - 删除 L160: `│   ├── relay.rs          — /relay 命令处理`
  - 原因: 这些模块文件已在 Task 2-4 中删除
- [x] 删除 `spec/global/architecture.md` — rust-relay-server 内部模块 section
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/architecture.md` L169-211
  - 删除整个 `### rust-relay-server 内部模块` section（从 L169 到 L211 结尾），包括代码块中的所有模块描述和 web/ 前端目录结构、CDN 依赖列表
  - 原因: crate 已删除，其模块划分文档不再有意义
- [x] 更新 `spec/global/architecture.md` — 事件系统表中 MessageAdded 描述
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/architecture.md` L225
  - 将 `│ MessageAdded │ 增量消息 │ 单条 BaseMessage（用于 Relay 传输） │` 改为 `│ MessageAdded │ 增量消息 │ 单条 BaseMessage（用于持久化和遥测） │`
  - 原因: MessageAdded 事件仍存在于核心层用于持久化，但其 Relay 传输用途已移除
- [x] 更新 `spec/global/architecture.md` — TUI 异步通信数据流图中 MessageAdded 分支
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/architecture.md` L284
  - 删除 `│       MessageAdded        → RelayClient 转发给远程 Web 端` 这一行
  - 原因: TUI 层不再消费 MessageAdded 事件用于 Relay 转发
- [x] 删除 `spec/global/architecture.md` — Relay 双向通信 section
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/architecture.md` L302-328
  - 删除整个 `### Relay 双向通信` section（从 L302 到 L328），包括数据流图
  - 原因: Relay 功能已整体移除
- [x] 更新 `spec/global/architecture.md` — 外部集成表
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/architecture.md` L380
  - 删除 Relay Server 行:
    - `│ Relay Server │ WebSocket (ws:/wss:) │ RELAY_TOKEN 查询参数 │ axum 监听端口（默认 8080），静态文件 rust-embed 内嵌 │`
  - 原因: Relay Server 已不存在于 workspace
- [x] 删除 `spec/global/architecture.md` — 远程控制模式部署拓扑
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/architecture.md` L399-407
  - 删除整个 `**远程控制模式（可选）：**` section（L399-407），包括代码块
  - 原因: 远程控制模式随 Relay Server 移除
- [x] 更新 `spec/global/architecture.md` — 文档尾部时间戳
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/architecture.md` L419
  - 将时间戳更新为: `*最后更新: 2026-04-27 — 移除 rust-relay-server crate 及 Relay 集成*`
  - 原因: 反映本次变更
- [x] 更新 `spec/global/constraints.md` — 技术栈 section
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/constraints.md` L14
  - 删除 `Web 框架（Relay Server）` 条目:
    - `- **Web 框架（Relay Server）:** axum 0.8（WebSocket feature）`
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/constraints.md` L21
  - 删除 `Web 前端 CDN` 条目:
    - `- **Web 前端 CDN（relay-server，ES Module，来自 esm.sh）:** preact + preact/hooks + htm + @preact/signals（声明式 UI + 响应式状态）；marked.js 15 + highlight.js 11.9（GitHub Dark 主题）+ DOMPurify（XSS 净化，动态 UMD script 注入）`
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/constraints.md` L22
  - 删除 `Web 前端 Signal 订阅规则` 条目:
    - `- **Web 前端 Signal 订阅规则:** esm.sh 多版本场景下 @preact/signals auto-tracking 不可靠，组件必须通过 \`useSignalValue(signal)\` 显式订阅，禁止在 render 函数中直接读取 \`signal.value\``
  - 原因: axum、Web 前端 CDN、前端 Signal 规则均为 Relay Server 专用，不再适用
- [x] 更新 `spec/global/constraints.md` — 架构决策 section
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/constraints.md` L26
  - 将 `Workspace 多 crate 分层` 条目中的 `→ rust-agent-tui` / `rust-relay-server`（应用层）改为 `→ rust-agent-tui`（应用层）:
    - `- **Workspace 多 crate 分层:** \`rust-create-agent\`（核心 lib）→ \`rust-agent-middlewares\`（中间件 lib）→ \`rust-agent-tui\`（应用层），禁止下层依赖上层`
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/constraints.md` L37
  - 删除 `Relay Server` API 风格条目:
    - `- **Relay Server:** WebSocket 协议，JSON 消息帧，客户端通过 \`tokio-tungstenite\` 连接`
  - 原因: workspace 分层和 API 风格中不再包含 Relay Server
- [x] 更新 `spec/global/constraints.md` — 文档尾部时间戳
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/constraints.md` L64
  - 将时间戳更新为: `*最后更新: 2026-04-27 — 移除 Relay Server 相关技术栈、架构决策和 API 风格条目*`
  - 原因: 反映本次变更
- [x] 更新 `spec/global/features.md` — TUI 界面 section
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/features.md` L50-52
  - 删除 L50（整个条目为 Relay 专用，无保留内容）:
    - L50: `- **Relay 集成:** 可选连接 Relay Server，事件实时转发，支持远程操控；Web 端支持 \`/compact\` 命令触发压缩；Agent thread 状态变更（clear/history/compact）通过 \`ThreadReset\` 消息自动同步到 Web 前端；Web 端支持"停止"按钮（\`CancelAgent\` 消息）中断 Agent 运行`
  - 修改 L51 `/compact Thread 迁移` 条目——保留功能描述但移除 Relay 部分:
    - 改为: `- **/compact Thread 迁移:** /compact 执行后创建新 Thread 保留旧历史，新 Thread 以摘要 System 消息开头`
  - 修改 L52 `App 结构体拆分` 条目——移除 RelayState:
    - 改为: `- **App 结构体拆分:** App 拆分为 AppCore/AgentComm/LangfuseState 三个子结构体，对外 API 通过转发方法保持不变`
  - 原因: Relay 集成功能已移除；/compact 和 App 拆分功能保留但去除 Relay 依赖描述
- [x] 更新 `spec/global/features.md` — 基础设施 section
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/features.md` L60
  - 删除 Relay Server 基础设施条目:
    - `- **Relay Server:** axum + tokio-tungstenite，支持 WebSocket 多 Agent 会话管理、心跳、Tab 状态广播；可选 client feature 仅引入 tungstenite；多用户隔离（UserNamespace + 匿名注册 /register）`
  - 原因: Relay Server 已从 workspace 移除
- [x] 更新 `spec/global/features.md` — 文档尾部时间戳
  - 位置: `/Users/konghayao/code/ai/perihelion/spec/global/features.md` L63
  - 将时间戳更新为: `*最后更新: 2026-04-27 — 移除 Relay 功能特性和基础设施条目*`
  - 原因: 反映本次变更
- [x] 更新 `CLAUDE.md` — 项目概述
  - 位置: `/Users/konghayao/code/ai/perihelion/CLAUDE.md` L7-12
  - 将 `包含 **4 个 Workspace Crate**` 改为 `包含 **3 个 Workspace Crate**`
  - 删除 L12 整行: `- **\`rust-relay-server\`**：远程控制 WebSocket 中继服务（Agent ↔ Web 双向通信）`
  - 原因: workspace crate 数量变更
- [x] 更新 `CLAUDE.md` — 开发命令 section
  - 位置: `/Users/konghayao/code/ai/perihelion/CLAUDE.md` L24
  - 删除 `RELAY_TOKEN=your-token cargo run -p rust-relay-server --features server  # 启动 Relay Server` 这一行
  - 原因: Relay Server 已删除
- [x] 更新 `CLAUDE.md` — Workspace 依赖关系图
  - 位置: `/Users/konghayao/code/ai/perihelion/CLAUDE.md` L29-37
  - 将整个依赖关系代码块替换为:
    ```
    rust-create-agent (核心框架，无内部依赖)
        ↑
    rust-agent-middlewares (中间件实现)
        ↑
    rust-agent-tui (TUI 应用，依赖 middlewares)
    ```
  - 原因: 移除 rust-relay-server 节点，移除 TUI 对 relay-server client 的依赖
- [x] 删除 `CLAUDE.md` — Relay 双向通信 section
  - 位置: `/Users/konghayao/code/ai/perihelion/CLAUDE.md` L90-174
  - 删除整个 `### Relay 双向通信（rust-relay-server）` section（从 L90 到 L174），包括:
    - 服务器路由表
    - 连接限制说明
    - RelayMessage 表
    - WebMessage 表
    - BroadcastMessage 表
    - RelayClient 特性列表
    - 前端描述
    - 前端文件结构代码块
    - Signal 订阅规则
    - Session 清理说明
  - 原因: 整个 Relay 双向通信功能已移除
- [x] 更新 `CLAUDE.md` — TUI 命令表
  - 位置: `/Users/konghayao/code/ai/perihelion/CLAUDE.md` L321
  - 删除 `/relay` 命令行: `│ \`/relay\` │ 打开远程控制配置面板（URL/Token/Name） │`
  - 原因: /relay 命令已在 Task 4 中移除
- [x] 更新 `CLAUDE.md` — 环境变量表
  - 位置: `/Users/konghayao/code/ai/perihelion/CLAUDE.md` L407 之后
  - 此表中没有 `RELAY_TOKEN` 条目（RELAY_TOKEN 仅在开发命令和 CLI 参数中出现），无需修改
- [x] 更新 `CLAUDE.md` — CLI 参数表
  - 位置: `/Users/konghayao/code/ai/perihelion/CLAUDE.md` L417-419
  - 删除以下 3 行:
    - `│ \`--remote-control [url]\` │ 连接 Relay Server │`
    - `│ \`--relay-token <token>\` │ Relay 认证 Token │`
    - `│ \`--relay-name <name>\` │ 客户端名称 │`
  - 原因: 这 3 个 CLI 参数已在 Task 4 中移除
- [x] 删除 `CLAUDE.md` — 远程控制配置示例
  - 位置: `/Users/konghayao/code/ai/perihelion/CLAUDE.md` L421-433
  - 删除整个配置示例代码块:
    ```
    配置示例（`~/.zen-code/settings.json`）：

    ```json
    {
      "config": {
        "remote_control": {
          "url": "ws://localhost:8080",
          "token": "your-token-here",
          "name": "my-laptop"
        }
      }
    }
    ```
    ```
  - 原因: RemoteControlConfig 已在 Task 4 中移除
- [x] 更新 `CLAUDE.md` — 开发注意事项 section
  - 位置: `/Users/konghayao/code/ai/perihelion/CLAUDE.md` L439-442
  - 删除以下 4 个注意事项:
    - L439: `- **relay-server 前端**：\`rust-relay-server/web/\` 下是纯静态文件，修改后需 \`touch rust-relay-server/src/static_files.rs\` 再重新编译 \`relay-server\`（\`include_bytes!\` 打包）。`
    - L440: `- **relay-server 启动**：必须设置 \`RELAY_TOKEN\` 环境变量，否则 panic。示例：\`RELAY_TOKEN=test-token cargo run -p rust-relay-server --features server\`。`
    - L441: `- **前端 Signal 订阅**：组件内读取 Signal 值必须用 \`useSignalValue(signal)\`（来自 \`utils/hooks.js\`），不可直接用 \`signal.value\` 作为响应式依赖，否则在 esm.sh 多版本环境下不会触发重渲染。`
    - L442: `- **前端 CSS**：每个组件的样式文件与 JS 文件同名同目录（如 \`Sidebar.css\` 在 \`components/\`），\`index.html\` 中逐一 \`<link>\` 引入；不使用 Tailwind，不使用任何 CSS-in-JS。`
  - 原因: 这 4 个注意事项均为 Relay Server 前端专用
- [x] 为全局文档 Relay 残留编写 grep 验证测试
  - 测试方式: 对 4 个全局文档执行 grep 搜索，确认不存在 Relay 相关残留
  - 运行命令:
    ```bash
    cd /Users/konghayao/code/ai/perihelion
    echo "=== architecture.md ==="
    grep -in "relay\|relay-server\|relayserver\|remote.control\|RemoteControl\|rust-relay" spec/global/architecture.md || echo "OK: no relay references"
    echo "=== constraints.md ==="
    grep -in "relay\|axum\|esm.sh\|preact\|useSignalValue\|relay-server\|tungstenite" spec/global/constraints.md || echo "OK: no relay references"
    echo "=== features.md ==="
    grep -in "relay\|RelayState\|ThreadReset\|CompactDone\|RelayClient\|Relay Server" spec/global/features.md || echo "OK: no relay references"
    echo "=== CLAUDE.md ==="
    grep -in "rust-relay-server\|relay-server\|/relay\|--remote-control\|--relay-token\|--relay-name\|RemoteControl\|relay_client\|RelayCli\|relay_cli\|RELAY_TOKEN" CLAUDE.md || echo "OK: no relay references"
    ```
  - 预期: 4 个文件均输出 "OK: no relay references"

**检查步骤:**
- [x] `architecture.md` 不再包含 `rust-relay-server`、`Relay`、`remote_control` 引用
  - `grep -ic "rust-relay-server\|relay_server\|RelayState\|relay-server\|Relay 双向\|RelayClient\|remote_control\|RemoteControl" /Users/konghayao/code/ai/perihelion/spec/global/architecture.md`
  - 预期: 输出 0
- [x] `constraints.md` 不再包含 Relay 相关技术栈条目
  - `grep -ic "relay\|axum\|esm.sh\|preact\|useSignalValue\|tungstenite" /Users/konghayao/code/ai/perihelion/spec/global/constraints.md`
  - 预期: 输出 0
- [x] `features.md` 不再包含 Relay 功能特性
  - `grep -ic "relay\|RelayState\|ThreadReset\|CompactDone\|RelayClient" /Users/konghayao/code/ai/perihelion/spec/global/features.md`
  - 预期: 输出 0
- [x] `CLAUDE.md` 不再包含 Relay 相关内容
  - `grep -ic "rust-relay-server\|/relay\|--remote-control\|--relay-token\|--relay-name\|RELAY_TOKEN\|RelayCli\|relay-server" /Users/konghayao/code/ai/perihelion/CLAUDE.md`
  - 预期: 输出 0
- [x] `CLAUDE.md` 项目概述中 workspace crate 数量为 3
  - `grep "Workspace Crate" /Users/konghayao/code/ai/perihelion/CLAUDE.md`
  - 预期: 输出包含 "3 个 Workspace Crate"
- [x] `CLAUDE.md` Workspace 依赖关系图中不包含 `rust-relay-server`
  - `grep "rust-relay-server" /Users/konghayao/code/ai/perihelion/CLAUDE.md`
  - 预期: 无输出
- [x] `CLAUDE.md` TUI 命令表中不包含 `/relay`
  - `grep "/relay" /Users/konghayao/code/ai/perihelion/CLAUDE.md`
  - 预期: 无输出
- [x] `CLAUDE.md` 开发注意事项中不包含 `relay-server`、`前端 Signal`、`前端 CSS`
  - `grep -c "relay-server\|前端 Signal\|前端 CSS" /Users/konghayao/code/ai/perihelion/CLAUDE.md`
  - 预期: 输出 0
- [x] `CLAUDE.md` CLI 参数表中不包含 `--remote-control`、`--relay-token`、`--relay-name`
  - `grep -c "remote-control\|relay-token\|relay-name" /Users/konghayao/code/ai/perihelion/CLAUDE.md`
  - 预期: 输出 0

---

### Task 6: Relay 移除验收

**前置条件:**
- Task 1-5 全部完成
- 无未提交的变更冲突
- 当前工作目录为项目根 `/Users/konghayao/code/ai/perihelion`

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test --workspace 2>&1 | tail -20`
   - 预期: 全部测试通过，无编译错误
   - 失败排查: 检查各 Task 的测试步骤，优先查看编译错误指向的文件

2. 全量构建验证
   - `cargo build --workspace 2>&1 | tail -10`
   - 预期: 构建成功，无警告中的 relay 相关引用
   - 失败排查: 检查编译错误中引用的文件，确认是否为 Relay 残留引用

3. Cargo.lock 中无 rust-relay-server 残留
   - `grep -c "rust-relay-server" /Users/konghayao/code/ai/perihelion/Cargo.lock`
   - 预期: 输出 0
   - 失败排查: 运行 `cargo update` 刷新 Cargo.lock

4. TUI 中无 relay/Relay/RemoteControl 相关代码残留
   - `grep -rn "relay\|Relay\|relay_client\|relay_panel\|RelayState\|RelayCli\|RemoteControl\|remote_control" rust-agent-tui/src/ --include="*.rs" | grep -v "vendor\|target" | head -20`
   - 预期: 无输出（或仅含 `unsubscribe_relay` 等无关匹配）
   - 失败排查: 根据匹配文件定位残留代码，对应 Task 3 或 Task 4 补充清理

5. rust-create-agent 中无 TUI 特有的 relay 引用
   - `grep -rn "relay" rust-create-agent/src/ --include="*.rs" | head -10`
   - 预期: 无输出（MessageAdded 事件保留但其中不含 "relay" 字符串）
   - 失败排查: 检查匹配是否为业务残留

6. rust-relay-server 目录不存在
   - `test ! -d /Users/konghayao/code/ai/perihelion/rust-relay-server && echo "OK"`
   - 预期: 输出 "OK"
   - 失败排查: 检查 Task 1 是否完成

7. 全局文档已更新
   - `grep -c "relay-server\|Relay Server\|远程控制" /Users/konghayao/code/ai/perihelion/spec/global/architecture.md /Users/konghayao/code/ai/perihelion/spec/global/constraints.md /Users/konghayao/code/ai/perihelion/spec/global/features.md /Users/konghayao/code/ai/perihelion/CLAUDE.md`
   - 预期: 每个文件输出 0
   - 失败排查: 检查 Task 5 是否遗漏了对应文档中的 Relay 引用

8. TUI 运行冒烟测试（手动）
   - `cargo run -p rust-agent-tui`
   - 预期: TUI 正常启动，无 panic，输入框可正常输入，`/help` 不显示 `/relay` 命令
   - 失败排查: 查看 panic 信息，定位崩溃位置
