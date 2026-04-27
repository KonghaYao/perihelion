# Feature: 20260427_F003 - model-config-refactor

## 需求背景

当前 `/model` 命令同时承载了 Provider 管理（CRUD）和模型别名配置两个职责。模型名分散存储在 `model_aliases` 中（每个别名绑定 `provider_id` + `model_id`），而非 Provider 内部，导致同一 Provider 的三个模型名需要在多个别名中重复配置。此外，Provider 管理与模型选择混在同一个面板，交互复杂度高。

**核心问题：**
- 模型名与 Provider 分离，配置不直观
- `/model` 面板承载职责过多（Provider CRUD + 别名映射 + Thinking）
- 新用户首次配置需理解"别名→Provider→模型"三层映射关系

## 目标

- **Provider 自包含**：每个 Provider 内部存储 opus/sonnet/haiku 三个模型名，消除 `ModelAliasMap` 间接映射
- **职责分离**：`/login` 负责 Provider CRUD，`/model` 只负责选择模型级别 + Thinking 开关
- **不兼容旧格式**：新安装使用新格式，旧配置需用户手动重新配置

## 方案设计

### 数据模型变更

**ProviderConfig 新增 `models` 字段：**

```rust
/// Provider 内的三级别模型名映射
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderModels {
    pub opus: String,
    pub sonnet: String,
    pub haiku: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    pub id: String,
    #[serde(rename = "type", default)]
    pub provider_type: String,
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    #[serde(rename = "baseUrl", default)]
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub models: ProviderModels,  // 新增：三级别模型名
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
```

**AppConfig 变更：**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// 当前激活的模型级别（"opus" | "sonnet" | "haiku"）
    #[serde(default = "default_alias")]
    pub active_alias: String,
    /// 当前激活的 provider ID（替代旧 model_aliases 间接映射）
    #[serde(default)]
    pub active_provider_id: String,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    // skills_dir, remote_control, env, extra 不变
}
```

**移除的字段/类型：**
- `AppConfig.model_aliases`（`ModelAliasMap`）
- `AppConfig.provider_id`（旧格式兼容字段）
- `AppConfig.model_id`（旧格式兼容字段）
- `ModelAliasConfig` 类型
- `ModelAliasMap` 类型
- `default_alias()` 函数保留（默认值 "opus"）

**运行时模型解析：**

```
active_provider_id → providers.find(|p| p.id == active_provider_id)
                   → p.models.{opus|sonnet|haiku}（按 active_alias 取值）
                   → 实际模型名（如 "claude-opus-4-7"）
```

**配置文件示例（`~/.zen-code/settings.json`）：**

```json
{
  "config": {
    "active_alias": "opus",
    "active_provider_id": "anthropic",
    "providers": [
      {
        "id": "anthropic",
        "type": "anthropic",
        "apiKey": "sk-ant-xxx",
        "baseUrl": "",
        "models": {
          "opus": "claude-opus-4-7",
          "sonnet": "claude-sonnet-4-6",
          "haiku": "claude-haiku-4-5"
        }
      },
      {
        "id": "openrouter",
        "type": "openai",
        "apiKey": "sk-or-xxx",
        "baseUrl": "https://openrouter.ai/api/v1",
        "models": {
          "opus": "anthropic/claude-opus-4-7",
          "sonnet": "anthropic/claude-sonnet-4-6",
          "haiku": "anthropic/claude-haiku-4-5"
        }
      }
    ],
    "thinking": { "enabled": true, "budget_tokens": 8000 }
  }
}
```

### `/login` 命令：Provider CRUD

**命令注册：**
- 新建 `rust-agent-tui/src/command/login.rs`
- 注册到 `CommandRegistry`（`/login`、前缀匹配 `/l`）
- `/help` 输出中增加 login 命令

**交互流程：**

1. **Provider 列表视图（Browse 模式）**
   - 列出所有 provider，每行格式：`▶ ● name (type)`
   - 光标 `▶` 标记当前选中，`●` 标记当前激活
   - ↑↓ 移动光标
   - 快捷键：`n` 新建 / `e` 编辑 / `d` 删除 / `Esc` 关闭

2. **新建/编辑表单（Edit/New 模式）**
   - 字段顺序（Tab/↑↓ 切换）：
     | 字段 | 输入方式 | 说明 |
     |------|---------|------|
     | Name | 文本输入 | Provider 显示名，id 由 name 派生（小写+下划线） |
     | Type | Space 循环切换 | openai / anthropic |
     | Base URL | 文本输入 | API 基础 URL |
     | API Key | 文本输入 | API 密钥 |
     | Opus Model | 文本输入 | 默认值根据 type 自动填（如 anthropic → `claude-opus-4-7`） |
     | Sonnet Model | 文本输入 | 默认值根据 type 自动填（如 anthropic → `claude-sonnet-4-6`） |
     | Haiku Model | 文本输入 | 默认值根据 type 自动填（如 anthropic → `claude-haiku-4-5`） |
   - Enter 保存，Esc 取消返回列表
   - Type 切换时，三个模型名字段自动更新为对应 provider_type 的默认值（仅当字段为空或仍为旧默认值时）

3. **删除确认（ConfirmDelete 模式）**
   - 显示"确认删除 xxx ？"，`y` 确认，`n`/`Esc` 取消

**实现文件：**

| 文件 | 职责 |
|------|------|
| `rust-agent-tui/src/app/login_panel.rs` | LoginPanel 状态机（Browse/Edit/New/ConfirmDelete） |
| `rust-agent-tui/src/command/login.rs` | LoginCommand 注册 |
| `rust-agent-tui/src/ui/main_ui/panels/login.rs` | Login 面板渲染 |

**LoginPanel 结构：**

```rust
pub struct LoginPanel {
    pub providers: Vec<ProviderConfig>,
    pub mode: LoginPanelMode,
    pub cursor: usize,
    pub edit_field: LoginEditField,
    // 编辑缓冲区
    pub buf_name: String,
    pub buf_type: String,
    pub buf_base_url: String,
    pub buf_api_key: String,
    pub buf_opus_model: String,
    pub buf_sonnet_model: String,
    pub buf_haiku_model: String,
}

pub enum LoginPanelMode { Browse, Edit, New, ConfirmDelete }
pub enum LoginEditField { Name, Type, BaseUrl, ApiKey, OpusModel, SonnetModel, HaikuModel }
```

**默认模型名映射：**

```rust
const DEFAULT_MODELS: &[(&str, &str, &str, &str)] = &[
    // (provider_type, opus, sonnet, haiku)
    ("anthropic", "claude-opus-4-7", "claude-sonnet-4-6", "claude-haiku-4-5"),
    ("openai",     "gpt-4o",          "gpt-4o-mini",       "gpt-3.5-turbo"),
];
```

Type 切换时查找该表，自动填充模型名。

### `/model` 命令：模型选择 + Thinking

**交互流程：**

1. **Provider 选择列表**
   - 每个 provider 显示一行：`▶ ● name (type)  opus=xxx  sonnet=xxx  haiku=xxx`
   - ↑↓ 移动光标，Enter 确认选择
   - 确认后更新 `active_provider_id` 并从该 provider 的 `models` 中按 `active_alias` 取模型名

2. **模型级别快捷切换**
   - 上方显示三个级别按钮：`[★ Opus]  [ Sonnet ]  [ Haiku ]`
   - ←→ 或 1/2/3 切换 `active_alias`
   - 当前激活的级别用 `★` 标记

3. **Thinking 配置区**
   - `Thinking  [ON]  budget: 8000`
   - Tab 在 provider 列表 / 级别选择 / thinking 之间切换焦点
   - Space 切换 enabled，数字键输入 budget_tokens

4. **快捷键**
   - `Enter`：确认选择并关闭面板
   - `Esc`：关闭面板
   - `/model <alias>`：直接切换 active_alias（保留现有快捷切换行为）

**ModelPanel 简化：**

```rust
pub struct ModelPanel {
    pub providers: Vec<ProviderConfig>,
    pub mode: ModelPanelMode,
    pub cursor: usize,
    pub active_tab: AliasTab,       // 保留：当前选中的级别
    pub focus_area: ModelFocusArea, // 新增：焦点区域
    pub buf_thinking_enabled: bool,
    pub buf_thinking_budget: String,
}

pub enum ModelPanelMode { SelectProvider, EditThinking }
pub enum ModelFocusArea { ProviderList, AliasTabs, Thinking }
```

**移除的内容：**
- `ModelPanelMode::AliasConfig` / `Browse` / `Edit` / `New` / `ConfirmDelete`
- `buf_alias_provider` / `buf_alias_model` / `alias_edit_field`
- Provider CRUD 相关方法（`enter_edit`、`enter_new`、`apply_edit`、`confirm_delete` 等）
- `EditField` 枚举（仅保留 ThinkingBudget 相关逻辑）
- `AliasEditField` 枚举

### 影响范围

**需要修改的文件：**

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `rust-agent-tui/src/config/types.rs` | 修改 | 新增 ProviderModels，AppConfig 移除 model_aliases，新增 active_provider_id |
| `rust-agent-tui/src/config/mod.rs` | 修改 | 适配新的 config 结构 |
| `rust-agent-tui/src/app/model_panel.rs` | 重写 | 简化为 Provider 选择 + Thinking |
| `rust-agent-tui/src/app/panel_ops.rs` | 修改 | 适配 model_panel 新接口 |
| `rust-agent-tui/src/app/mod.rs` | 修改 | 新增 login_panel 字段 |
| `rust-agent-tui/src/app/core.rs` | 修改 | 适配新的 provider 解析逻辑 |
| `rust-agent-tui/src/command/model.rs` | 修改 | 简化 execute 逻辑 |
| `rust-agent-tui/src/command/mod.rs` | 修改 | 注册 login 命令 |
| `rust-agent-tui/src/ui/main_ui/panels/model.rs` | 重写 | 简化渲染 |
| `rust-agent-tui/src/ui/main_ui.rs` | 修改 | 增加 login panel 渲染分支 |
| `rust-agent-tui/src/ui/main_ui/panels/mod.rs` | 修改 | 新增 login 模块 |
| `rust-agent-tui/src/event.rs` | 修改 | 新增 login panel 键盘事件处理 |

**需要新建的文件：**

| 文件 | 说明 |
|------|------|
| `rust-agent-tui/src/app/login_panel.rs` | LoginPanel 状态机 |
| `rust-agent-tui/src/command/login.rs` | LoginCommand |
| `rust-agent-tui/src/ui/main_ui/panels/login.rs` | Login 面板渲染 |

**外部引用点（需要适配）：**

| 引用点 | 文件 | 说明 |
|--------|------|------|
| `LlmProvider::from_config` | `rust-agent-tui/src/app/agent.rs` | 适配新的 provider 解析（active_provider_id + models） |
| `/help` 命令输出 | `rust-agent-tui/src/command/help.rs` | 增加 `/login` 描述 |
| `CLAUDE.md` | 项目根目录 | 更新 TUI 命令列表 |

## 实现要点

1. **Type 切换自动填充**：在 LoginPanel 中，当 provider_type 切换时，检测三个模型名字段是否为空或等于旧 provider_type 的默认值；若是则自动填入新 type 的默认值，避免用户手动输入
2. **App 与 LoginPanel/ModelPanel 互斥**：同一时间只能打开一个配置面板（`login_panel` / `model_panel` 互斥），打开新面板时自动关闭另一个
3. **Thinking 配置位置不变**：Thinking 仍为全局配置（不属于单个 provider），放在 `/model` 面板中
4. **旧格式不兼容**：移除 `model_aliases` 相关的反序列化字段，旧配置文件加载后 `model_aliases` 被忽略；`active_provider_id` 默认为空字符串，用户需通过 `/login` 重新配置
5. **provider_type 默认模型名常量表**：集中维护在一个常量中，方便后续扩展新的 provider_type
6. **LoginPanel 粘贴支持**：复用 ModelPanel 现有的 `paste_text` 模式，过滤换行符，支持 Ctrl+V

## 约束一致性

- **与 architecture.md 一致**：修改范围限定在 `rust-agent-tui` crate（应用层），不涉及核心框架和中间件层
- **与 constraints.md 一致**：
  - 使用 ratatui 渲染面板（TUI 框架不变）
  - 配置持久化走 `serde_json` 序列化到 `~/.zen-code/settings.json`（不变）
  - 新命令注册遵循 `Command` trait + `CommandRegistry` 模式（不变）
  - 事件处理通过 `Event` 枚举分发（不变）
- **无新增架构约束**

## 验收标准

- [ ] ProviderConfig 包含 `models: ProviderModels` 字段，序列化/反序列化正确
- [ ] AppConfig 移除 `model_aliases`，新增 `active_provider_id`
- [ ] `/login` 命令注册到 CommandRegistry，`/help` 中可见
- [ ] `/login` 支持新建/编辑/删除 Provider，表单包含 base_url / api_key / 三模型名
- [ ] Type 切换时自动填充模型名默认值
- [ ] `/model` 只显示 Provider 列表选择 + 级别切换 + Thinking 配置
- [ ] `/model <alias>` 快捷切换保留正常工作
- [ ] 旧格式配置文件加载不崩溃（`model_aliases` 被安全忽略）
- [ ] 所有现有测试通过（适配新数据结构后）
- [ ] 新增 LoginPanel 和简化后 ModelPanel 的单元测试
