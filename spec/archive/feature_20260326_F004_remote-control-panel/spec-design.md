# Feature: 20260326_F004 - remote-control-panel

## 需求背景

当前 `rust-agent-tui` 的远程控制功能仅支持通过 CLI 参数传递：

```bash
cargo run -p rust-agent-tui -- --remote-control <url> --relay-token <token> --relay-name <name>
```

这种方式存在以下问题：

1. **重复输入**：每次启动都需要输入完整的 URL 和 Token，用户体验差
2. **密钥暴露风险**：在命令行历史中会记录敏感信息
3. **配置不持久**：无法保存常用的远程服务器配置
4. **不符合 TUI 应用模式**：作为 TUI 应用，应该在界面内完成配置，而非依赖外部参数

参考项目中已有的 `/model` 命令，用户习惯在 TUI 界面内配置各项设置。需要提供类似的远程控制面板功能。

## 目标

- **核心目标 1**：提供 TUI 内的远程控制配置界面，支持配置 URL、Token、Name 并持久化到 settings.json
- **核心目标 2**：支持 `--remote-control` 无参数启动，自动从配置读取
- **核心目标 3**：保持 CLI 参数覆盖能力，支持临时指定不同服务器

## 方案设计

### 架构设计

参考现有的 `ModelPanel` 设计模式，新增 `RelayPanel` 组件，实现独立的远程控制配置面板。

```
rust-agent-tui/src/app/
├── relay_panel.rs         — 新增：远程控制面板状态管理
├── relay_ops.rs           — 修改：现有 relay 操作逻辑扩展
└── mod.rs                 — 修改：集成 RelayPanel，新增 relay_panel 字段

rust-agent-tui/src/config/
└── types.rs               — 修改：新增 RemoteControlConfig 结构体

rust-agent-tui/src/command/
├── mod.rs                 — 修改：新增 /relay 命令注册
└── relay.rs               — 新增：/relay 命令实现

rust-agent-tui/src/main.rs — 修改：CLI 参数解析支持 --remote-control 无参数

rust-agent-tui/src/ui/
└── main_ui.rs             — 修改：新增 render_relay_panel 函数
```

### 数据模型设计

在 `rust-agent-tui/src/config/types.rs` 中新增结构化的远程控制配置：

```rust
/// 远程控制配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RemoteControlConfig {
    /// Relay Server URL（如 ws://localhost:8080 或 wss://relay.example.com）
    #[serde(default)]
    pub url: String,
    /// 认证 Token（可选）
    #[serde(default)]
    pub token: String,
    /// 客户端名称（可选，用于标识连接）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl RemoteControlConfig {
    /// 检查配置是否完整（URL 必填）
    pub fn is_complete(&self) -> bool {
        !self.url.is_empty()
    }
}
```

在 `AppConfig` 中新增字段：

```rust
/// 远程控制配置
#[serde(default, skip_serializing_if = "Option::is_none")]
pub remote_control: Option<RemoteControlConfig>,
```

### RemotePanel 状态管理

新增 `rust-agent-tui/src/app/relay_panel.rs`，参考 `ModelPanel` 的设计：

```rust
pub struct RelayPanel {
    pub mode: RelayPanelMode,
    pub edit_field: RelayEditField,
    pub buf_url: String,
    pub buf_token: String,
    pub buf_name: String,
    pub status_message: Option<String>,
    pub cursor: usize,
}

pub enum RelayPanelMode {
    View,   // 浏览模式：显示当前配置（Token 脱敏）
    Edit,   // 编辑模式：修改配置
}

pub enum RelayEditField {
    Url,
    Token,
    Name,
}

impl RelayPanel {
    /// 从 ZenConfig 加载配置
    pub fn from_config(config: &ZenConfig) -> Self {
        let rc = config.config.remote_control.as_ref();
        Self {
            mode: RelayPanelMode::View,
            edit_field: RelayEditField::Url,
            buf_url: rc.map(|r| r.url.clone()).unwrap_or_default(),
            buf_token: rc.map(|r| r.token.clone()).unwrap_or_default(),
            buf_name: rc.map(|r| r.name.clone().unwrap_or_default()).unwrap_or_default(),
            status_message: None,
            cursor: 0,
        }
    }

    /// View 模式下显示脱敏的 Token（如 "****abc123****"）
    pub fn display_token(&self) -> String {
        if self.buf_token.is_empty() {
            "(未设置)".to_string()
        } else if self.buf_token.len() <= 8 {
            "****".to_string()
        } else {
            format!("****{}****", &self.buf_token[self.buf_token.len()-4..])
        }
    }
}
```

### 用户交互流程

![远程控制面板用户流程](./images/01-user-flow.png.txt)

#### 场景 1：首次配置

1. 用户启动 TUI：`cargo run -p rust-agent-tui`
2. 输入 `/relay` 命令
3. 显示远程控制面板（View 模式），显示 "无配置"
4. 用户按 `e` 进入 Edit 模式
5. 按 `Tab` 切换编辑字段（Url → Token → Name）
6. 输入完成后按 `Enter` 保存
7. 配置写入 `~/.zen-code/settings.json`
8. 显示 "配置已保存" 状态消息

#### 场景 2：使用已保存的配置启动

1. 用户启动 TUI：`cargo run -p rust-agent-tui -- --remote-control`
2. 应用从 `settings.json` 读取 `remote_control` 配置
3. 自动连接到保存的 Relay Server
4. 显示连接状态消息

#### 场景 3：临时覆盖配置

1. 用户启动 TUI：`cargo run -p rust-agent-tui -- --remote-control <temp-url> --relay-token <temp-token>`
2. CLI 参数优先级高于 settings.json
3. 使用临时参数连接，不修改配置文件

### CLI 参数增强

修改 `rust-agent-tui/src/main.rs` 的 `parse_relay_args` 函数，支持 `--remote-control` 无参数模式：

```rust
fn parse_relay_args(args: &[String]) -> Option<RelayCli> {
    // 查找 --remote-control 参数位置
    let remote_idx = args.iter().position(|a| a == "--remote-control")?;

    // 检查是否有值（即 --remote-control <url> 格式）
    // 有值条件：下一个参数存在且不以 -- 开头
    let url = if remote_idx + 1 < args.len() && !args[remote_idx + 1].starts_with("--") {
        args[remote_idx + 1].clone()
    } else {
        // --remote-control 无参数，返回空字符串标记"从配置读取"
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

**关键变更**：原实现使用 `windows(2)` 强制要求 `--remote-control` 后必须跟 URL，新实现支持无参数模式（返回空 URL 字符串）。

修改 `try_connect_relay` 逻辑，支持从 `RemoteControlConfig` 字段读取配置（同时兼容旧的 `extra` 字段）：

```rust
pub async fn try_connect_relay(&mut self, cli: Option<&crate::RelayCli>) {
    let (relay_url, relay_token, relay_name) = if let Some(c) = cli {
        // CLI 参数模式
        if c.url.is_empty() {
            // --remote-control 无参数：从配置读取
            let config = self.zen_config
                .as_ref()
                .and_then(|cfg| cfg.config.remote_control.as_ref())
                .filter(|rc| rc.is_complete());

            match config {
                Some(rc) => (rc.url.clone(), rc.token.clone(), rc.name.clone()),
                None => {
                    // 回退到旧 extra 字段（向后兼容）
                    let extra_config = self.zen_config
                        .as_ref()
                        .and_then(|cfg| cfg.config.extra.get("relay_url"))
                        .and_then(|v| v.as_str());
                    if extra_config.is_none() {
                        let msg = MessageViewModel::from_base_message(
                            &BaseMessage::system("未配置远程控制，请使用 /relay 命令配置".to_string()),
                            &[]
                        );
                        let _ = self.render_tx.send(RenderEvent::AddMessage(msg));
                        return;
                    }
                    let url = extra_config.unwrap().to_string();
                    let token = self.zen_config
                        .as_ref()
                        .and_then(|cfg| cfg.config.extra.get("relay_token"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = self.zen_config
                        .as_ref()
                        .and_then(|cfg| cfg.config.extra.get("relay_name"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    (url, token, name)
                }
            }
        } else {
            // --remote-control <url>：使用 CLI 参数（token 可从配置 fallback）
            let token = c.token.clone().unwrap_or_else(|| {
                // 优先从新字段读取，fallback 到 extra 字段
                self.zen_config
                    .as_ref()
                    .and_then(|cfg| cfg.config.remote_control.as_ref())
                    .map(|rc| rc.token.clone())
                    .unwrap_or_else(|| {
                        self.zen_config
                            .as_ref()
                            .and_then(|cfg| cfg.config.extra.get("relay_token"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string()
                    })
            });
            (c.url.clone(), token, c.name.clone())
        }
    } else {
        // 无 CLI 参数：从配置读取（新字段优先，fallback 到 extra）
        let config = self.zen_config
            .as_ref()
            .and_then(|cfg| cfg.config.remote_control.as_ref())
            .filter(|rc| rc.is_complete());

        match config {
            Some(rc) => (rc.url.clone(), rc.token.clone(), rc.name.clone()),
            None => {
                // 回退到旧 extra 字段
                let url = self.zen_config
                    .as_ref()
                    .and_then(|cfg| cfg.config.extra.get("relay_url"))
                    .and_then(|v| v.as_str());
                match url {
                    Some(u) => {
                        let token = self.zen_config
                            .as_ref()
                            .and_then(|cfg| cfg.config.extra.get("relay_token"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = self.zen_config
                            .as_ref()
                            .and_then(|cfg| cfg.config.extra.get("relay_name"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        (u.to_string(), token, name)
                    }
                    None => return,
                }
            }
        }
    };

    // 连接逻辑保持不变
    match rust_relay_server::client::RelayClient::connect(
        &relay_url,
        &relay_token,
        relay_name.as_deref(),
    ).await {
        Ok((client, event_rx)) => {
            let sid = client.session_id.read().await.clone().unwrap_or_default();
            let status_msg = format!("Relay connected (session: {})", &sid[..8.min(sid.len())]);
            let vm = MessageViewModel::from_base_message(&BaseMessage::system(status_msg), &[]);
            let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
            self.relay_client = Some(Arc::new(client));
            self.relay_event_rx = Some(event_rx);
        }
        Err(e) => {
            let error_msg = format!("Relay connection failed: {}", e);
            let vm = MessageViewModel::from_base_message(&BaseMessage::system(error_msg), &[]);
            let _ = self.render_tx.send(RenderEvent::AddMessage(vm));
        }
    }
}
```

**配置优先级**：`remote_control` 字段 > `extra.relay_*` 字段（向后兼容）

### TUI 命令集成

在 `rust-agent-tui/src/command/mod.rs` 中新增 `/relay` 命令（避免与 `/history` 的 `/r` 前缀冲突）：

```rust
pub fn default_registry() -> CommandRegistry {
    let mut r = CommandRegistry::new();
    r.register(Box::new(agents::AgentsCommand));
    r.register(Box::new(model::ModelCommand));
    r.register(Box::new(clear::ClearCommand));
    r.register(Box::new(compact::CompactCommand));
    r.register(Box::new(help::HelpCommand));
    r.register(Box::new(history::HistoryCommand));
    r.register(Box::new(relay::RelayCommand));  // 新增
    r
}
```

新增 `rust-agent-tui/src/command/relay.rs`：

```rust
use super::{Command, CommandRegistry};
use crate::app::App;

pub struct RelayCommand;

impl Command for RelayCommand {
    fn name(&self) -> &str {
        "relay"
    }

    fn description(&self) -> &str {
        "打开远程控制配置面板"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        app.open_relay_panel();
    }
}
```

**命令快捷方式**：`/re` 可唯一匹配 `/relay`（与 `/remote` 相比更简洁）。

### UI 渲染逻辑

在 `rust-agent-tui/src/main_ui.rs` 中新增 `RemotePanel` 的渲染：

```rust
pub fn render<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let size = f.size();

    // 优先渲染弹窗
    if let Some(ref prompt) = app.hitl_prompt {
        return render_hitl_prompt(f, app, prompt, size);
    }
    if let Some(ref prompt) = app.ask_user_prompt {
        return render_ask_user_prompt(f, app, prompt, size);
    }
    if let Some(ref mut panel) = app.model_panel {
        return render_model_panel(f, app, panel, size);
    }
    if let Some(ref mut panel) = app.agent_panel {
        return render_agent_panel(f, app, panel, size);
    }
    if let Some(ref mut panel) = app.relay_panel {  // 新增
        return render_relay_panel(f, app, panel, size);
    }

    // 主界面渲染...
}
```

**View 模式布局示例**：

```
┌─────────────────────────────────────────────────────────┐
│                    远程控制配置                          │
├─────────────────────────────────────────────────────────┤
│ URL:    wss://relay.example.com                         │
│ Token:  ****3456****                                     │
│ Name:   my-laptop                                        │
│                                                         │
│ [e] 编辑  [Esc] 关闭                                     │
└─────────────────────────────────────────────────────────┘
```

**Edit 模式布局示例**：

```
┌─────────────────────────────────────────────────────────┐
│                    远程控制配置 (编辑)                   │
├─────────────────────────────────────────────────────────┤
│ URL:    [wss://relay.example.com____________]           │
│ Token:  [my-secret-token___________________]           │
│ Name:   [my-laptop_________________________]           │
│                                                         │
│ [Enter] 保存  [Esc] 取消                                │
└─────────────────────────────────────────────────────────┘
```

### 配置读写集成

在 `rust-agent-tui/src/config/store.rs` 中确保 `RemoteControlConfig` 的正确读写：

```rust
// 配置读取时自动解析 remote_control 字段
pub fn load() -> Result<ZenConfig> {
    let path = config_path()?;
    let content = fs::read_to_string(path)?;
    let config: ZenConfig = serde_json::from_str(&content)?;
    Ok(config)
}

// 配置写入时保存 remote_control 字段
pub fn save(config: &ZenConfig) -> Result<()> {
    let path = config_path()?;
    let content = serde_json::to_string_pretty(config)?;
    fs::write(path, content)?;
    Ok(())
}
```

## 实现要点

### 关键技术决策

1. **配置结构化**：使用 `RemoteControlConfig` 结构体替代 `extra` 字段，提供类型安全和编译时检查
2. **向后兼容**：`extra` 字段仍然保留，迁移逻辑从 `extra` 读取旧配置并写入新字段
3. **CLI 优先级**：CLI 参数 > settings.json 配置 > 环境变量
4. **UI 模式复用**：参考 `ModelPanel` 的 View/Edit 模式切换，保持交互一致性

### 依赖模块

- **复用现有功能**：
  - `AppConfig` 配置读写（`rust-agent-tui/src/config/store.rs`）
  - `RelayClient` 连接逻辑（`rust-relay-server` client feature）
  - TUI 渲染框架（`ratatui`）
  
- **新增依赖**：无（纯 Rust 实现，无外部依赖）

### 测试策略

1. **单元测试**：
   - `RemoteControlConfig::is_complete()` 边界测试
   - 配置序列化/反序列化测试
   - CLI 参数解析测试（`parse_relay_args`）

2. **集成测试**（Headless 模式）：
   - `/relay` 命令打开面板
   - 编辑配置并保存
   - 验证 settings.json 写入正确

3. **手动测试**：
   - 无配置启动 `--remote-control`，验证错误提示
   - 有配置启动 `--remote-control`，验证自动连接
   - CLI 参数覆盖测试

### 错误处理

- **配置不完整**：显示友好的 TUI 消息，引导用户使用 `/relay` 配置
- **连接失败**：显示详细错误信息（WebSocket 握手失败、认证失败等）
- **配置文件损坏**：fallback 到默认配置，记录日志

## 约束一致性

### 与 spec/global/constraints.md 一致性

- ✅ **技术栈**：纯 Rust 实现，使用现有的 ratatui/serde/tokio
- ✅ **架构决策**：
  - 遵循 Workspace 分层（新增文件仅在 `rust-agent-tui` crate）
  - 异步优先（`try_connect_relay` 保持 async）
  - 事件驱动 TUI 通信（通过 `render_tx` 发送消息）
- ✅ **编码规范**：
  - 命名约定：`RelayPanel`/`RelayPanelMode`/`RelayEditField`
  - 错误处理：TUI 层用 `anyhow::Result`，配置层用 `serde` 错误
  - 测试：单元测试在 `src/config/types.rs` 内 `#[cfg(test)]`
- ✅ **安全约束**：
  - Token 在 `settings.json` 中明文存储（与现有 API Key 处理方式一致）
  - `.zen-code/` 目录已通过 OS 权限保护

### 与 spec/global/architecture.md 一致性

- ✅ **模块划分**：新增 `relay_panel.rs` 符合 `app/` 模块组织规范
- ✅ **数据流**：
  - CLI 参数 → `try_connect_relay` → `RelayClient` 连接
  - `/relay` 命令 → `RelayPanel` 编辑 → `settings.json` 持久化
- ✅ **外部集成**：Relay Server 连接逻辑不变，仅配置来源扩展

### 新增/变更约束

无新增约束。本功能完全符合现有架构模式，是对现有远程控制功能的增强，不引入新的技术债务或架构偏离。

## 验收标准

- [ ] **数据模型**：
  - [ ] `RemoteControlConfig` 结构体定义完整，支持序列化/反序列化
  - [ ] `AppConfig` 新增 `remote_control` 字段，`skip_serializing_if = "Option::is_none"`
  - [ ] 单元测试覆盖配置解析逻辑

- [ ] **RelayPanel 组件**：
  - [ ] View 模式显示当前配置（URL/Token/Name），Token 脱敏显示（如 `****3456****`）
  - [ ] Edit 模式支持 Tab 切换字段，Enter 保存，Esc 取消
  - [ ] URL 输入验证（非空检查）
  - [ ] 保存成功后显示状态消息，自动关闭面板

- [ ] **CLI 参数增强**：
  - [ ] `--remote-control` 无参数时从配置读取
  - [ ] `--remote-control <url>` 覆盖配置，Token 支持从配置 fallback
  - [ ] 配置不完整时显示友好的 TUI 错误消息
  - [ ] 向后兼容：支持从 `extra.relay_*` 字段读取旧配置

- [ ] **命令集成**：
  - [ ] `/relay` 命令打开配置面板
  - [ ] 支持 `/re` 前缀匹配（避免与 `/history` 的 `/r` 冲突）
  - [ ] `/help` 命令列表包含 `/relay`

- [ ] **配置持久化**：
  - [ ] 保存配置到 `~/.zen-code/settings.json` 的 `remote_control` 字段
  - [ ] TUI 重启后配置正确加载
  - [ ] 向后兼容：优先读取 `remote_control` 字段，fallback 到 `extra.relay_*`

- [ ] **错误处理**：
  - [ ] 配置不完整（URL 为空）时显示提示消息
  - [ ] Relay 连接失败时显示详细错误信息
  - [ ] 配置文件损坏时 fallback 到默认配置，不崩溃

- [ ] **文档更新**：
  - [ ] CLAUDE.md 更新 `/relay` 命令说明
  - [ ] README.md 更新远程控制使用示例
  - [ ] 代码注释完整（`///` 文档注释）

- [ ] **测试覆盖**：
  - [ ] `RemoteControlConfig` 单元测试（序列化/反序列化/`is_complete`）
  - [ ] CLI 参数解析单元测试（无参数模式、有参数模式）
  - [ ] 配置读取优先级测试（新字段 > 旧字段）
  - [ ] Headless 集成测试（`/relay` 命令 + 配置保存）

---
*创建日期: 2026-03-26*
