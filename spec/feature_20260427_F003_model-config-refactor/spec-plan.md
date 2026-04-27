# Model Config 重构执行计划

**目标:** Provider 自包含模型名，`/login` 管理 Provider CRUD，`/model` 只负责选择模型级别 + Thinking 开关

**技术栈:** Rust 2021, serde/serde_json, ratatui TUI 框架

**设计文档:** spec/feature_20260427_F003_model-config-refactor/spec-design.md

## 改动总览

- 本次改动涉及 `rust-agent-tui` crate 内 4 个模块共 17 个文件（含 CLAUDE.md）：数据模型层（types.rs, mod.rs, store.rs）、Provider 解析层（provider.rs）、Login 面板层（login_panel.rs, command/login.rs, panels/login.rs）、Model 面板层（model_panel.rs, panels/model.rs）、App 集成层（event.rs, panel_ops.rs, app/mod.rs, app/core.rs, main_ui.rs, panels/mod.rs）、外部引用适配（setup_wizard.rs, status_bar.rs, headless.rs, CLAUDE.md）
- 依赖链：Task 1（数据模型）→ Task 2（LoginPanel）+ Task 3（ModelPanel 简化）→ Task 4（UI 渲染与事件集成）→ Task 5（外部引用适配），必须严格按顺序执行
- 关键设计决策：Provider 自包含三级别模型名（ProviderModels），AppConfig 直接存储 `active_provider_id` 替代间接映射 `model_aliases`，Login/Model 面板互斥，旧配置不兼容（`model_aliases` 被 `extra: Map` 静默吸收不崩溃）

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，避免后续 Task 因环境问题阻塞。

**执行步骤:**
- [x] 验证构建工具可用
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
- [x] 验证测试工具可用
  - `cargo test -p rust-agent-tui --lib 2>&1 | tail -5`

**检查步骤:**
- [x] 构建命令执行成功
  - `cargo build -p rust-agent-tui 2>&1 | tail -3`
  - 预期: 编译成功（可能有 warning，无 error）
- [x] 测试命令可用
  - `cargo test -p rust-agent-tui --lib 2>&1 | tail -5`
  - 预期: 测试框架正常运行

---

### Task 1: 数据模型与 LlmProvider 重构

**背景:**
当前模型名通过 `ModelAliasMap`（含三个 `ModelAliasConfig`）分散存储在 `AppConfig.model_aliases` 中，每个别名绑定 `provider_id` + `model_id`，导致同一 Provider 的三个模型名在多个别名中重复配置。本 Task 将模型名内聚到 `ProviderConfig.models`（`ProviderModels`），并新增 `AppConfig.active_provider_id` 字段直接指向激活的 Provider，消除间接映射。Task 2（LoginPanel）、Task 3（ModelPanel 简化）、Task 4（UI 层适配）均依赖本 Task 产出的新数据结构。

**涉及文件:**
- 修改: `rust-agent-tui/src/config/types.rs`
- 修改: `rust-agent-tui/src/config/mod.rs`
- 修改: `rust-agent-tui/src/config/store.rs`
- 修改: `rust-agent-tui/src/app/provider.rs`

**执行步骤:**

- [x] 在 `types.rs` 中新增 `ProviderModels` 结构体 — 为 `ProviderConfig` 提供三级别模型名容器
  - 位置: `rust-agent-tui/src/config/types.rs`，在 `ModelAliasMap` struct 定义之前（~L25 之前），删除 `ModelAliasConfig` struct（L13-19）和 `ModelAliasMap` struct 及其 impl（L26-46），在原位置插入新类型
  - 关键逻辑:
    ```rust
    /// Provider 内的三级别模型名映射
    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct ProviderModels {
        pub opus: String,
        pub sonnet: String,
        pub haiku: String,
    }

    impl ProviderModels {
        /// 按 alias 名（大小写不敏感）获取对应模型名
        pub fn get_model(&self, alias: &str) -> Option<&str> {
            match alias.to_lowercase().as_str() {
                "opus" => Some(&self.opus),
                "sonnet" => Some(&self.sonnet),
                "haiku" => Some(&self.haiku),
                _ => None,
            }
        }
    }
    ```
  - 原因: `ProviderModels` 替代 `ModelAliasConfig` + `ModelAliasMap`，模型名从"别名→Provider→模型"的三层映射简化为"Provider→模型名"的直接映射

- [x] 修改 `ProviderConfig` struct — 新增 `models` 字段
  - 位置: `rust-agent-tui/src/config/types.rs`，`ProviderConfig` struct（~L139-154），在 `name` 字段之后、`extra` 字段之前插入新字段
  - 关键逻辑:
    ```rust
    // 在 pub name: Option<String> 之后新增：
    #[serde(default)]
    pub models: ProviderModels,
    ```
  - 原因: 每个 Provider 自包含三级别模型名，消除对外部 `ModelAliasMap` 的依赖

- [x] 修改 `AppConfig` struct — 移除旧字段，新增 `active_provider_id`
  - 位置: `rust-agent-tui/src/config/types.rs`，`AppConfig` struct（~L105-136）
  - 关键逻辑:
    - 删除 `provider_id` 字段（L108-109）
    - 删除 `model_id` 字段（L111-112）
    - 删除 `model_aliases` 字段（L117-118）
    - 在 `active_alias` 字段之后新增：
      ```rust
      /// 当前激活的 provider ID（直接指向 providers 列表中的某个 Provider）
      #[serde(default)]
      pub active_provider_id: String,
      ```
  - 原因: 旧格式迁移到 `model_aliases` 的逻辑不再需要，`active_provider_id` 直接指向 Provider；旧字段 `provider_id`/`model_id`/`model_aliases` 在 serde 反序列化时会被 `extra: Map<String, Value>` 的 `#[serde(flatten)]` 静默吸收，不会崩溃

- [x] 移除 `ModelAliasConfig` 和 `ModelAliasMap` 的所有 import 和 `pub use` — 清理模块导出
  - 位置: `rust-agent-tui/src/config/mod.rs`（L5）
  - 关键逻辑:
    - 将 `pub use types::{ModelAliasConfig, ProviderConfig, RemoteControlConfig, ThinkingConfig, ZenConfig};`
    - 改为 `pub use types::{ProviderConfig, ProviderModels, RemoteControlConfig, ThinkingConfig, ZenConfig};`
  - 原因: `ModelAliasConfig` 已删除，新增 `ProviderModels` 需要导出

- [x] 简化 `config/store.rs` — 移除迁移逻辑
  - 位置: `rust-agent-tui/src/config/store.rs`
  - 关键逻辑:
    - 删除 `use super::types::ModelAliasConfig;` 导入（L3），改为 `use super::types::ZenConfig;`（保留，因为已有）
    - 删除整个 `migrate_if_needed` 函数（L13-36）
    - 修改 `load()` 函数（L39-50）：移除 `mut`（`let mut cfg` → `let cfg`），移除 `if migrate_if_needed(&mut cfg) { let _ = save(&cfg); }` 两行
    - 最终 `load()` 函数体：
      ```rust
      pub fn load() -> Result<ZenConfig> {
          let path = config_path();
          if !path.exists() {
              return Ok(ZenConfig::default());
          }
          let content = std::fs::read_to_string(&path)?;
          let cfg: ZenConfig = serde_json::from_str(&content)?;
          Ok(cfg)
      }
      ```
  - 原因: 旧格式迁移（`provider_id/model_id` → `model_aliases`）已不再需要，新设计不兼容旧配置

- [x] 重写 `LlmProvider::from_config` — 按 `active_provider_id` + `ProviderModels` 解析模型
  - 位置: `rust-agent-tui/src/app/provider.rs`，`from_config` 方法（~L62-107）
  - 关键逻辑:
    ```rust
    /// 从 ZenConfig 构造 LlmProvider（按 active_provider_id 查找 Provider，再按 active_alias 取模型名）
    pub fn from_config(cfg: &ZenConfig) -> Option<Self> {
        let app = &cfg.config;
        let provider = app.providers.iter().find(|p| p.id == app.active_provider_id)?;

        if provider.api_key.is_empty() {
            return None;
        }

        let alias = app.active_alias.as_str();
        let model = provider.models.get_model(alias)
            .filter(|m| !m.is_empty())
            .map(|m| m.to_string())
            .unwrap_or_else(|| {
                // fallback：按 provider_type 返回默认模型名
                match provider.provider_type.as_str() {
                    "anthropic" => "claude-sonnet-4-6".to_string(),
                    _ => "gpt-4o".to_string(),
                }
            });

        let thinking = app.thinking.clone().filter(|t| t.enabled);

        match provider.provider_type.as_str() {
            "anthropic" => Some(Self::Anthropic {
                api_key: provider.api_key.clone(),
                model,
                base_url: if provider.base_url.is_empty() { None } else { Some(provider.base_url.clone()) },
                thinking,
            }),
            _ => Some(Self::OpenAi {
                api_key: provider.api_key.clone(),
                base_url: if provider.base_url.is_empty() {
                    "https://api.openai.com/v1".to_string()
                } else {
                    provider.base_url.clone()
                },
                model,
                thinking,
            }),
        }
    }
    ```
  - 原因: 模型解析路径从 `alias → model_aliases.{alias}.provider_id → providers.find → model_aliases.{alias}.model_id` 简化为 `active_provider_id → providers.find → models.{alias}`

- [x] 重写 `LlmProvider::from_config_for_alias` — 按 `active_provider_id` + 指定 alias 解析
  - 位置: `rust-agent-tui/src/app/provider.rs`，`from_config_for_alias` 方法（~L111-150）
  - 关键逻辑:
    ```rust
    /// 从 ZenConfig 按指定 alias（如 "haiku"/"sonnet"/"opus"）构造 LlmProvider
    /// 大小写不敏感；未知 alias 返回 None
    pub fn from_config_for_alias(cfg: &ZenConfig, alias: &str) -> Option<Self> {
        let app = &cfg.config;
        let provider = app.providers.iter().find(|p| p.id == app.active_provider_id)?;

        if provider.api_key.is_empty() {
            return None;
        }

        let model = provider.models.get_model(alias)
            .filter(|m| !m.is_empty())
            .map(|m| m.to_string())
            .unwrap_or_else(|| {
                match provider.provider_type.as_str() {
                    "anthropic" => "claude-sonnet-4-6".to_string(),
                    _ => "gpt-4o".to_string(),
                }
            });

        let thinking = app.thinking.clone().filter(|t| t.enabled);

        match provider.provider_type.as_str() {
            "anthropic" => Some(Self::Anthropic {
                api_key: provider.api_key.clone(),
                model,
                base_url: if provider.base_url.is_empty() { None } else { Some(provider.base_url.clone()) },
                thinking,
            }),
            _ => Some(Self::OpenAi {
                api_key: provider.api_key.clone(),
                base_url: if provider.base_url.is_empty() {
                    "https://api.openai.com/v1".to_string()
                } else {
                    provider.base_url.clone()
                },
                model,
                thinking,
            }),
        }
    }
    ```
  - 原因: 与 `from_config` 保持一致的解析逻辑，`/model <alias>` 快捷切换走此路径

- [x] 更新 `provider.rs` 测试中的 import 和 helper — 适配新数据结构
  - 位置: `rust-agent-tui/src/app/provider.rs`，`#[cfg(test)] mod tests`（~L189-305）
  - 关键逻辑:
    - 将 `use crate::config::{ModelAliasConfig, ProviderConfig, ZenConfig};` 改为 `use crate::config::{ProviderConfig, ProviderModels, ZenConfig};`
    - 重写 `make_config_with_alias` helper，用新数据结构构造测试配置：
      ```rust
      fn make_config(alias: &str, provider_id: &str, model_id: &str, provider_type: &str) -> ZenConfig {
          let mut cfg = ZenConfig::default();
          cfg.config.active_alias = alias.to_string();
          cfg.config.active_provider_id = provider_id.to_string();
          cfg.config.providers.push(ProviderConfig {
              id: provider_id.to_string(),
              provider_type: provider_type.to_string(),
              api_key: "test-key".to_string(),
              models: ProviderModels {
                  opus: if alias == "opus" { model_id.to_string() } else { String::new() },
                  sonnet: if alias == "sonnet" { model_id.to_string() } else { String::new() },
                  haiku: if alias == "haiku" { model_id.to_string() } else { String::new() },
              },
              ..Default::default()
          });
          cfg
      }
      ```
    - 更新所有测试用例，将 `make_config_with_alias` 调用改为 `make_config`

- [x] 更新 `types.rs` 测试 — 移除引用 `model_aliases` 的测试用例
  - 位置: `rust-agent-tui/src/config/types.rs`，`#[cfg(test)] mod tests`（~L162-439）
  - 关键逻辑:
    - `test_app_config_thinking_optional`（~L212-215）：将输入 JSON 中的 `"model_aliases": {}` 替换为 `"active_provider_id": ""`
    - `test_app_config_thinking_roundtrip`（~L219-234）：保持不变（不引用 `model_aliases`）
    - `test_model_panel_*` 系列 4 个测试（~L247-328）：这些测试引用了 `ModelPanel` 和 `model_aliases`，全部注释掉或删除（ModelPanel 重构在后续 Task，这些测试在 Task 3 中重写）
    - `test_model_panel_from_config_loads_thinking`、`test_model_panel_from_config_defaults_when_no_thinking`、`test_model_panel_toggle_thinking`、`test_model_panel_thinking_budget_input_only_digits`、`test_model_panel_apply_edit_saves_thinking` — 这 5 个测试全部删除（引用 `model_aliases.opus.provider_id` 和 `EditField::ThinkingBudget`，ModelPanel 后续 Task 重写）
  - 原因: 移除旧类型后，引用 `model_aliases` 的编译会失败；ModelPanel 测试在 Task 3 重写

- [x] 移除 `store.rs` 中的旧迁移测试
  - 位置: `rust-agent-tui/src/config/store.rs`，`#[cfg(test)] mod tests`（~L52-96）
  - 关键逻辑:
    - 删除 `test_migration_from_old_format` 测试（L58-74）— 引用已删除的 `provider_id`/`model_id`/`model_aliases` 字段
    - 删除 `test_no_migration_when_new_format` 测试（L77-84）— 引用 `model_aliases`
    - 删除 `test_migration_active_alias_is_opus` 测试（L87-95）— 引用 `provider_id`/`model_id`
    - 清空后整个 `#[cfg(test)] mod tests` 块可删除或保留空壳
  - 原因: `migrate_if_needed` 函数已删除，相关测试不再适用

- [x] 为新数据模型编写单元测试
  - 测试文件: `rust-agent-tui/src/config/types.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_provider_models_get_model_known_aliases`: `ProviderModels { opus: "o", sonnet: "s", haiku: "h" }` → `get_model("opus")` 返回 `"o"`，`get_model("sonnet")` 返回 `"s"`，`get_model("haiku")` 返回 `"h"`
    - `test_provider_models_get_model_case_insensitive`: `get_model("Opus")` / `get_model("SONNET")` / `get_model("Haiku")` 均返回对应模型名
    - `test_provider_models_get_model_unknown_returns_none`: `get_model("turbo")` 返回 `None`
    - `test_provider_models_default`: `ProviderModels::default()` 三个字段均为空字符串
    - `test_provider_config_models_serde_roundtrip`: 构造含 `models` 字段的 `ProviderConfig`，序列化后反序列化，验证 `models.opus/sonnet/haiku` 一致
    - `test_app_config_active_provider_id_serde`: 构造含 `active_provider_id` 的 JSON 反序列化为 `AppConfig`，验证字段值正确
    - `test_app_config_old_fields_ignored`: 含 `"provider_id": "old", "model_id": "old-model", "model_aliases": {...}` 的旧 JSON 反序列化不崩溃，`active_provider_id` 为空（旧字段被 `extra` 吸收）
  - 运行命令: `cargo test -p rust-agent-tui --lib -- config::types::tests`
  - 预期: 所有测试通过

- [x] 为 `LlmProvider` 新逻辑编写单元测试
  - 测试文件: `rust-agent-tui/src/app/provider.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_from_config_opus_alias`: `active_alias="opus"`, `active_provider_id="anthropic"`, `provider.models.opus="claude-opus-4-7"` → `model_name()` 返回 `"claude-opus-4-7"`
    - `test_from_config_sonnet_alias`: `active_alias="sonnet"`, `active_provider_id="openrouter"`, `provider.models.sonnet="gpt-5.4"` → `model_name()` 返回 `"gpt-5.4"`
    - `test_from_config_empty_model_fallback_anthropic`: `models.opus=""` → fallback 到 `"claude-sonnet-4-6"`
    - `test_from_config_empty_model_fallback_openai`: `models.haiku=""` → fallback 到 `"gpt-4o"`
    - `test_from_config_unknown_alias_fallback`: `active_alias="ultra"` → `get_model("ultra")` 返回 `None`，fallback 到 `"claude-sonnet-4-6"`（anthropic provider）
    - `test_from_config_empty_api_key_returns_none`: `api_key=""` → `from_config` 返回 `None`
    - `test_from_config_provider_not_found_returns_none`: `active_provider_id="nonexistent"` → `from_config` 返回 `None`
    - `test_from_config_for_alias_known`: 分别传入 `"opus"/"sonnet"/"haiku"` 验证返回正确模型名
    - `test_from_config_for_alias_unknown_returns_none`: 传入 `"turbo"` → `get_model("turbo")` 返回 `None`，fallback 到默认模型（因为 `from_config_for_alias` 与 `from_config` 逻辑一致，均使用 `unwrap_or_else` fallback）
    - `test_from_config_for_alias_case_insensitive`: 传入 `"Opus"` / `"HAIKU"` 正确匹配
  - 运行命令: `cargo test -p rust-agent-tui --lib -- app::provider::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 `ProviderModels` 类型已导出
  - `grep -n 'pub use types::.*ProviderModels' rust-agent-tui/src/config/mod.rs`
  - 预期: 输出包含 `ProviderModels`
- [x] 验证旧类型已移除
  - `grep -rn 'ModelAliasConfig\|ModelAliasMap' rust-agent-tui/src/config/types.rs`
  - 预期: 无输出（已完全移除）
- [x] 验证 `store.rs` 无迁移逻辑
  - `grep -n 'migrate_if_needed\|ModelAliasConfig' rust-agent-tui/src/config/store.rs`
  - 预期: 无输出
- [x] 验证 `provider.rs` 不引用 `model_aliases`
  - `grep -n 'model_aliases' rust-agent-tui/src/app/provider.rs`
  - 预期: 无输出
- [x] 运行 `rust-agent-tui` 单元测试（types + provider + store）
  - `cargo test -p rust-agent-tui --lib -- config::types::tests config::store::tests app::provider::tests`
  - 预期: 全部测试通过，无编译错误
- [x] 验证整个 crate 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 编译可能报错（其他文件如 `model_panel.rs`、`setup_wizard.rs`、`status_bar.rs` 仍引用旧类型），这些文件在后续 Task 中修改；本 Task 仅确保 `types.rs`、`store.rs`、`provider.rs`、`mod.rs` 四个文件的内部一致性

---

### Task 2: LoginPanel 状态机与命令注册

**背景:**
Provider 的 CRUD 操作（新建/编辑/删除）从 `/model` 面板迁移到独立的 `/login` 面板，实现职责分离。当前 `ModelPanel` 同时承载别名配置和 Provider 管理，交互复杂度高。本 Task 新建 `LoginPanel` 状态机（Browse/Edit/New/ConfirmDelete 四种模式）和 `LoginCommand`，并将 `/login` 注册到命令系统。本 Task 依赖 Task 1 产出的 `ProviderModels` 和 `active_provider_id` 数据结构；Task 3（ModelPanel 简化）、Task 4（UI 渲染）依赖本 Task 的 `LoginPanel` 状态机。

**涉及文件:**
- 新建: `rust-agent-tui/src/app/login_panel.rs`
- 新建: `rust-agent-tui/src/command/login.rs`
- 修改: `rust-agent-tui/src/command/mod.rs`

**执行步骤:**

- [x] 新建 `login_panel.rs` — 定义 `LoginPanelMode`、`LoginEditField` 枚举和 `LoginPanel` 结构体
  - 位置: `rust-agent-tui/src/app/login_panel.rs`（新文件），文件顶部
  - 关键逻辑:
    ```rust
    use crate::config::{ProviderConfig, ProviderModels, ZenConfig};

    // ─── 默认模型名常量表 ─────────────────────────────────────────────────────────

    /// (provider_type, opus, sonnet, haiku)
    const DEFAULT_MODELS: &[(&str, &str, &str, &str)] = &[
        ("anthropic", "claude-opus-4-7", "claude-sonnet-4-6", "claude-haiku-4-5"),
        ("openai",     "gpt-4o",          "gpt-4o-mini",       "gpt-3.5-turbo"),
    ];

    /// provider_type 循环切换列表
    const PROVIDER_TYPES: &[&str] = &["openai", "anthropic"];

    // ─── 枚举 ─────────────────────────────────────────────────────────────────────

    #[derive(Debug, Clone, PartialEq)]
    pub enum LoginPanelMode {
        Browse,
        Edit,
        New,
        ConfirmDelete,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum LoginEditField {
        Name,
        Type,
        BaseUrl,
        ApiKey,
        OpusModel,
        SonnetModel,
        HaikuModel,
    }

    impl LoginEditField {
        pub fn next(&self) -> Self {
            match self {
                Self::Name       => Self::Type,
                Self::Type       => Self::BaseUrl,
                Self::BaseUrl    => Self::ApiKey,
                Self::ApiKey     => Self::OpusModel,
                Self::OpusModel  => Self::SonnetModel,
                Self::SonnetModel => Self::HaikuModel,
                Self::HaikuModel => Self::Name,
            }
        }

        pub fn prev(&self) -> Self {
            match self {
                Self::Name       => Self::HaikuModel,
                Self::Type       => Self::Name,
                Self::BaseUrl    => Self::Type,
                Self::ApiKey     => Self::BaseUrl,
                Self::OpusModel  => Self::ApiKey,
                Self::SonnetModel => Self::OpusModel,
                Self::HaikuModel => Self::SonnetModel,
            }
        }

        pub fn label(&self) -> &str {
            match self {
                Self::Name        => "Name        ",
                Self::Type        => "Type        ",
                Self::BaseUrl     => "Base URL    ",
                Self::ApiKey      => "API Key     ",
                Self::OpusModel   => "Opus Model  ",
                Self::SonnetModel => "Sonnet Model",
                Self::HaikuModel  => "Haiku Model ",
            }
        }
    }
    ```
  - 原因: `LoginEditField` 包含 7 个字段（Name/Type/BaseUrl/ApiKey/OpusModel/SonnetModel/HaikuModel），与 `ModelPanel` 的 `EditField` 不同，不包含 ThinkingBudget（Thinking 在 `/model` 面板）

- [x] 新建 `login_panel.rs` — 定义 `LoginPanel` struct 和 `from_config` 构造方法
  - 位置: `rust-agent-tui/src/app/login_panel.rs`，在枚举定义之后
  - 关键逻辑:
    ```rust
    pub struct LoginPanel {
        /// provider 列表快照（从 ZenConfig 获取）
        pub providers: Vec<ProviderConfig>,
        /// 当前模式
        pub mode: LoginPanelMode,
        /// 光标位置（Browse 模式下标记选中行）
        pub cursor: usize,
        /// 正在编辑的字段（Edit/New 模式下）
        pub edit_field: LoginEditField,
        /// 编辑缓冲区
        pub buf_name: String,
        pub buf_type: String,
        pub buf_base_url: String,
        pub buf_api_key: String,
        pub buf_opus_model: String,
        pub buf_sonnet_model: String,
        pub buf_haiku_model: String,
        /// 内容滚动偏移
        pub scroll_offset: u16,
    }

    impl LoginPanel {
        /// 从 ZenConfig 初始化面板（Browse 模式，光标定位到 active_provider_id 对应的 Provider）
        pub fn from_config(cfg: &ZenConfig) -> Self {
            let providers = cfg.config.providers.clone();
            let cursor = providers
                .iter()
                .position(|p| p.id == cfg.config.active_provider_id)
                .unwrap_or(0);
            Self {
                providers,
                mode: LoginPanelMode::Browse,
                cursor,
                edit_field: LoginEditField::Name,
                buf_name: String::new(),
                buf_type: String::new(),
                buf_base_url: String::new(),
                buf_api_key: String::new(),
                buf_opus_model: String::new(),
                buf_sonnet_model: String::new(),
                buf_haiku_model: String::new(),
                scroll_offset: 0,
            }
        }
    }
    ```
  - 原因: `from_config` 在 Browse 模式下打开面板，光标定位到当前激活的 Provider，参照 `ModelPanel::from_config` 的初始化模式

- [x] 新建 `login_panel.rs` — 实现 Browse 模式操作方法
  - 位置: `rust-agent-tui/src/app/login_panel.rs`，`impl LoginPanel` 块内，`from_config` 之后
  - 关键逻辑:
    ```rust
    // ── Browse 模式操作 ──────────────────────────────────────────────────────

    /// 列表上下移动光标（循环）
    pub fn move_cursor(&mut self, delta: isize) {
        if self.providers.is_empty() { return; }
        let len = self.providers.len();
        self.cursor = ((self.cursor as isize + delta).rem_euclid(len as isize)) as usize;
    }

    /// 进入编辑模式（编辑光标处的 provider）
    pub fn enter_edit(&mut self) {
        if let Some(p) = self.providers.get(self.cursor) {
            self.buf_name = p.display_name().to_string();
            self.buf_type = p.provider_type.clone();
            self.buf_base_url = p.base_url.clone();
            self.buf_api_key = p.api_key.clone();
            self.buf_opus_model = p.models.opus.clone();
            self.buf_sonnet_model = p.models.sonnet.clone();
            self.buf_haiku_model = p.models.haiku.clone();
            self.edit_field = LoginEditField::Name;
            self.mode = LoginPanelMode::Edit;
        }
    }

    /// 进入新建模式（清空所有缓冲，type 默认 "openai"，模型名按 type 自动填充）
    pub fn enter_new(&mut self) {
        self.buf_name = String::new();
        self.buf_type = "openai".to_string();
        self.buf_base_url = String::new();
        self.buf_api_key = String::new();
        self.buf_opus_model = String::new();
        self.buf_sonnet_model = String::new();
        self.buf_haiku_model = String::new();
        self.auto_fill_models_for_type();
        self.edit_field = LoginEditField::Name;
        self.mode = LoginPanelMode::New;
    }

    /// 进入删除确认模式
    pub fn request_delete(&mut self) {
        if !self.providers.is_empty() {
            self.mode = LoginPanelMode::ConfirmDelete;
        }
    }

    /// 取消删除确认，回到浏览模式
    pub fn cancel_delete(&mut self) {
        self.mode = LoginPanelMode::Browse;
    }
    ```
  - 原因: Browse 模式下的操作与 `ModelPanel` 的 Browse 模式操作一致，`enter_edit` 需要将 Provider 的 `models` 字段值填充到三个模型名缓冲区

- [x] 新建 `login_panel.rs` — 实现 Edit/New 模式操作方法（字段导航、文本输入、粘贴）
  - 位置: `rust-agent-tui/src/app/login_panel.rs`，`impl LoginPanel` 块内，Browse 操作之后
  - 关键逻辑:
    ```rust
    // ── Edit/New 模式操作 ────────────────────────────────────────────────────

    /// 字段导航：下一个字段
    pub fn field_next(&mut self) {
        self.edit_field = self.edit_field.next();
    }

    /// 字段导航：上一个字段
    pub fn field_prev(&mut self) {
        self.edit_field = self.edit_field.prev();
    }

    /// 循环切换 provider_type（Space 键，仅在 edit_field == Type 时生效）
    /// 切换后自动调用 auto_fill_models_for_type 更新模型名默认值
    pub fn cycle_type(&mut self) {
        if self.edit_field == LoginEditField::Type {
            let cur = PROVIDER_TYPES.iter().position(|&t| t == self.buf_type).unwrap_or(0);
            self.buf_type = PROVIDER_TYPES[(cur + 1) % PROVIDER_TYPES.len()].to_string();
            self.auto_fill_models_for_type();
        }
    }

    /// 输入字符到当前活动字段（Type 字段不可直接输入，只能 cycle）
    pub fn push_char(&mut self, c: char) {
        match self.edit_field {
            LoginEditField::Name        => self.buf_name.push(c),
            LoginEditField::Type        => {} // 只能 cycle
            LoginEditField::BaseUrl     => self.buf_base_url.push(c),
            LoginEditField::ApiKey      => self.buf_api_key.push(c),
            LoginEditField::OpusModel   => self.buf_opus_model.push(c),
            LoginEditField::SonnetModel => self.buf_sonnet_model.push(c),
            LoginEditField::HaikuModel  => self.buf_haiku_model.push(c),
        }
    }

    /// 删除当前活动字段末字符（Backspace）
    pub fn pop_char(&mut self) {
        match self.edit_field {
            LoginEditField::Name        => { self.buf_name.pop(); }
            LoginEditField::Type        => {}
            LoginEditField::BaseUrl     => { self.buf_base_url.pop(); }
            LoginEditField::ApiKey      => { self.buf_api_key.pop(); }
            LoginEditField::OpusModel   => { self.buf_opus_model.pop(); }
            LoginEditField::SonnetModel => { self.buf_sonnet_model.pop(); }
            LoginEditField::HaikuModel  => { self.buf_haiku_model.pop(); }
        }
    }

    /// 粘贴文本到当前活动字段（过滤换行符，Type 字段忽略粘贴）
    pub fn paste_text(&mut self, text: &str) {
        let text: String = text.chars().filter(|&c| c != '\n' && c != '\r').collect();
        match self.edit_field {
            LoginEditField::Name        => self.buf_name.push_str(&text),
            LoginEditField::Type        => {}
            LoginEditField::BaseUrl     => self.buf_base_url.push_str(&text),
            LoginEditField::ApiKey      => self.buf_api_key.push_str(&text),
            LoginEditField::OpusModel   => self.buf_opus_model.push_str(&text),
            LoginEditField::SonnetModel => self.buf_sonnet_model.push_str(&text),
            LoginEditField::HaikuModel  => self.buf_haiku_model.push_str(&text),
        }
    }
    ```
  - 原因: 输入/粘贴/导航逻辑参照 `ModelPanel` 的对应方法，但字段映射到 7 个缓冲区（无 ThinkingBudget，新增三个模型名字段）

- [x] 新建 `login_panel.rs` — 实现 Type 切换自动填充逻辑 `auto_fill_models_for_type`
  - 位置: `rust-agent-tui/src/app/login_panel.rs`，`impl LoginPanel` 块内，Edit/New 操作之后
  - 关键逻辑:
    ```rust
    /// Type 切换时自动填充模型名默认值
    /// 规则：检测三个模型名字段是否为空或等于旧 provider_type 的默认值；若是则填入新 type 的默认值
    pub fn auto_fill_models_for_type(&mut self) {
        let new_defaults = DEFAULT_MODELS.iter().find(|(t, _, _, _)| *t == self.buf_type);
        let (opus_default, sonnet_default, haiku_default) = match new_defaults {
            Some((_, o, s, h)) => (o.to_string(), s.to_string(), h.to_string()),
            None => return, // 未知 provider_type，不自动填充
        };

        // 收集所有 provider_type 的默认值作为"旧默认值"候选
        let all_defaults: Vec<(String, String, String)> = DEFAULT_MODELS
            .iter()
            .map(|(_, o, s, h)| (o.to_string(), s.to_string(), h.to_string()))
            .collect();

        let is_default_or_empty = |val: &str| -> bool {
            if val.is_empty() { return true; }
            all_defaults.iter().any(|(o, s, h)| val == o || val == s || val == h)
        };

        if is_default_or_empty(&self.buf_opus_model) {
            self.buf_opus_model = opus_default;
        }
        if is_default_or_empty(&self.buf_sonnet_model) {
            self.buf_sonnet_model = sonnet_default;
        }
        if is_default_or_empty(&self.buf_haiku_model) {
            self.buf_haiku_model = haiku_default;
        }
    }
    ```
  - 原因: 自动填充避免用户手动输入模型名。仅当字段为空或仍为旧 provider_type 默认值时覆盖，用户自定义的模型名不被覆盖

- [x] 新建 `login_panel.rs` — 实现 `apply_edit` 和 `confirm_delete` 保存/删除方法
  - 位置: `rust-agent-tui/src/app/login_panel.rs`，`impl LoginPanel` 块内，`auto_fill_models_for_type` 之后
  - 关键逻辑:
    ```rust
    // ── 保存/删除操作 ──────────────────────────────────────────────────────────

    /// 将编辑/新建的内容保存到 ZenConfig，并更新内部 providers 快照
    /// 返回 true 表示成功
    /// 新建 Provider 后，active_provider_id 为空时自动设置为新建的 Provider ID
    pub fn apply_edit(&mut self, cfg: &mut ZenConfig) -> bool {
        let is_new = self.mode == LoginPanelMode::New;
        let id = if is_new {
            if self.buf_name.trim().is_empty() {
                return false;
            }
            self.buf_name.trim().to_lowercase().replace(' ', "_")
        } else {
            self.providers.get(self.cursor).map(|p| p.id.clone()).unwrap_or_default()
        };

        if id.is_empty() { return false; }

        let mut p = ProviderConfig {
            id: id.clone(),
            provider_type: self.buf_type.clone(),
            api_key: self.buf_api_key.clone(),
            base_url: self.buf_base_url.clone(),
            name: if self.buf_name.trim().is_empty() { None } else { Some(self.buf_name.trim().to_string()) },
            models: ProviderModels {
                opus: self.buf_opus_model.clone(),
                sonnet: self.buf_sonnet_model.clone(),
                haiku: self.buf_haiku_model.clone(),
            },
            extra: Default::default(),
        };

        // 编辑模式：保留原有的 extra 字段
        if self.mode == LoginPanelMode::Edit {
            if let Some(orig) = self.providers.get(self.cursor) {
                p.extra = orig.extra.clone();
            }
        }

        if is_new {
            cfg.config.providers.push(p);
            self.cursor = cfg.config.providers.len() - 1;
            // active_provider_id 为空时自动设置
            if cfg.config.active_provider_id.is_empty() {
                cfg.config.active_provider_id = id;
            }
        } else if let Some(existing) = cfg.config.providers.iter_mut().find(|x| x.id == id) {
            *existing = p;
        }

        self.providers = cfg.config.providers.clone();
        self.mode = LoginPanelMode::Browse;
        true
    }

    /// 确认删除光标处的 provider，写入 cfg
    pub fn confirm_delete(&mut self, cfg: &mut ZenConfig) {
        if let Some(p) = self.providers.get(self.cursor) {
            let id = p.id.clone();
            cfg.config.providers.retain(|x| x.id != id);
            self.providers = cfg.config.providers.clone();
            if self.cursor >= self.providers.len() && !self.providers.is_empty() {
                self.cursor = self.providers.len() - 1;
            }
            // 如果删除的是当前激活的 provider，清空 active_provider_id
            if cfg.config.active_provider_id == id {
                cfg.config.active_provider_id.clear();
            }
        }
        self.mode = LoginPanelMode::Browse;
    }
    ```
  - 原因: `apply_edit` 在保存时构造 `ProviderModels { opus, sonnet, haiku }` 写入 `ProviderConfig.models`，替代旧方案中 `ModelAliasMap` 的间接映射。`confirm_delete` 在删除 Provider 时检查并清空 `active_provider_id`

- [x] 新建 `login_panel.rs` — 编写单元测试
  - 测试文件: `rust-agent-tui/src/app/login_panel.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_login_panel_from_config_cursor_at_active_provider`: 构造 2 个 Provider，`active_provider_id="openrouter"`，验证 `cursor == 1`
    - `test_login_panel_from_config_empty_providers_cursor_zero`: 无 Provider 时 `cursor == 0`
    - `test_login_panel_move_cursor_cycle`: 2 个 Provider，`move_cursor(1)` 从 0→1→0 循环；`move_cursor(-1)` 从 0→1 循环
    - `test_login_panel_enter_edit_fills_buffers`: 构造含 `models.opus="claude-opus-4-7"` 的 Provider，调用 `enter_edit()`，验证 `buf_opus_model == "claude-opus-4-7"`
    - `test_login_panel_enter_new_auto_fills_openai`: `enter_new()` 后，`buf_type == "openai"`，`buf_opus_model == "gpt-4o"`，`buf_sonnet_model == "gpt-4o-mini"`，`buf_haiku_model == "gpt-3.5-turbo"`
    - `test_login_panel_cycle_type_auto_fills_anthropic`: 初始 `buf_type = "openai"`（默认模型已填），调用 `cycle_type()` 切换为 `"anthropic"`，验证三个模型名自动更新为 `"claude-opus-4-7"/"claude-sonnet-4-6"/"claude-haiku-4-5"`
    - `test_login_panel_cycle_type_preserves_custom_model`: 设置 `buf_opus_model = "my-custom-model"`，`cycle_type()` 后验证 `buf_opus_model` 仍为 `"my-custom-model"`
    - `test_login_panel_field_navigation`: 验证 `field_next()` 按 Name→Type→BaseUrl→ApiKey→OpusModel→SonnetModel→HaikuModel→Name 循环；`field_prev()` 反向循环
    - `test_login_panel_push_pop_char`: 设置 `edit_field = OpusModel`，`push_char('x')` 两次，验证 `buf_opus_model == "xx"`；`pop_char()` 一次，验证 `buf_opus_model == "x"`
    - `test_login_panel_push_char_ignored_for_type`: 设置 `edit_field = Type`，`push_char('a')`，验证 `buf_type` 不变
    - `test_login_panel_paste_text_filters_newlines`: 设置 `edit_field = ApiKey`，`paste_text("key\nval\r\nend")`，验证 `buf_api_key == "keyvalend"`
    - `test_login_panel_paste_text_ignored_for_type`: 设置 `edit_field = Type`，`paste_text("anthropic")`，验证 `buf_type` 不变
    - `test_login_panel_apply_edit_new_provider`: New 模式下填入 name/api_key/models，`apply_edit()` 返回 `true`，验证 `cfg.config.providers` 新增了一个 Provider，其 `models.opus/sonnet/haiku` 正确
    - `test_login_panel_apply_edit_new_provider_sets_active_id_when_empty`: `active_provider_id` 为空时，新建 Provider 后验证 `cfg.config.active_provider_id` 被设置为新 Provider 的 id
    - `test_login_panel_apply_edit_existing_provider`: Edit 模式下修改 `buf_api_key`，`apply_edit()` 后验证 `cfg.config.providers` 中对应 Provider 的 `api_key` 已更新
    - `test_login_panel_apply_edit_empty_name_returns_false`: New 模式下 `buf_name` 为空，`apply_edit()` 返回 `false`
    - `test_login_panel_confirm_delete_removes_provider`: 删除第二个 Provider，验证 providers 长度减 1，cursor 调整
    - `test_login_panel_confirm_delete_clears_active_provider_id`: 删除 `active_provider_id` 指向的 Provider，验证 `cfg.config.active_provider_id` 被清空
    - `test_login_panel_request_delete_no_providers_noop`: providers 为空时 `request_delete()` 不改变 mode
  - 运行命令: `cargo test -p rust-agent-tui --lib -- app::login_panel::tests`
  - 预期: 所有测试通过

- [x] 新建 `command/login.rs` — 实现 `LoginCommand`
  - 位置: `rust-agent-tui/src/command/login.rs`（新文件）
  - 关键逻辑:
    ```rust
    use crate::app::App;
    use super::Command;

    pub struct LoginCommand;

    impl Command for LoginCommand {
        fn name(&self) -> &str {
            "login"
        }

        fn description(&self) -> &str {
            "管理 Provider 配置（新建/编辑/删除）"
        }

        fn execute(&self, app: &mut App, _args: &str) {
            app.open_login_panel();
        }
    }
    ```
  - 原因: `/login` 命令无参数子命令，直接打开 LoginPanel；与 `ModelCommand` 无参数时 `app.open_model_panel()` 的模式一致

- [x] 修改 `command/mod.rs` — 注册 `login` 子模块和 `LoginCommand`
  - 位置: `rust-agent-tui/src/command/mod.rs`
  - 关键逻辑:
    - 在文件顶部模块声明区域（L1-9）新增一行：`pub mod login;`，插入在 `pub mod loop_cmd;` 之前（按字母序）
    - 在 `default_registry()` 函数体（L12-23）中，在 `r.register(Box::new(model::ModelCommand));` 之后新增一行：
      ```rust
      r.register(Box::new(login::LoginCommand));
      ```
  - 原因: 注册 `/login` 命令到 `CommandRegistry`，使 `/login` 可被 `dispatch` 精确匹配。注意：由于已有 `/loop` 命令也以 `l` 开头，`/l` 前缀将产生歧义无法匹配，用户需输入 `/lo` 或 `/login`

**检查步骤:**
- [x] 验证 `login_panel.rs` 文件存在且导出 `LoginPanel`、`LoginPanelMode`、`LoginEditField`
  - `grep -n 'pub struct LoginPanel\|pub enum LoginPanelMode\|pub enum LoginEditField' rust-agent-tui/src/app/login_panel.rs`
  - 预期: 输出 3 行，分别包含三个类型定义
- [x] 验证 `command/login.rs` 文件存在且实现 `Command` trait
  - `grep -n 'impl Command for LoginCommand\|fn name\|fn description\|fn execute' rust-agent-tui/src/command/login.rs`
  - 预期: 输出 4 行
- [x] 验证 `command/mod.rs` 已注册 login 模块和命令
  - `grep -n 'pub mod login\|login::LoginCommand' rust-agent-tui/src/command/mod.rs`
  - 预期: 输出 2 行
- [x] 验证 `DEFAULT_MODELS` 常量包含 anthropic 和 openai 两组默认值
  - `grep -A3 'const DEFAULT_MODELS' rust-agent-tui/src/app/login_panel.rs`
  - 预期: 输出包含 `"anthropic"` 和 `"openai"` 两行
- [x] 验证 `auto_fill_models_for_type` 方法存在
  - `grep -n 'fn auto_fill_models_for_type' rust-agent-tui/src/app/login_panel.rs`
  - 预期: 输出 1 行
- [x] 验证 `LoginPanel` 的 `apply_edit` 设置 `ProviderModels`
  - `grep -n 'ProviderModels' rust-agent-tui/src/app/login_panel.rs`
  - 预期: 输出包含 `use` 导入行和 `apply_edit` 中的 `ProviderModels { ... }` 构造
- [x] 运行 `login_panel` 单元测试
  - `cargo test -p rust-agent-tui --lib -- app::login_panel::tests`
  - 预期: 全部测试通过
- [x] 验证 `/login` 命令在 registry 中可见（`/help` 输出包含 login）
  - `grep -n 'login' rust-agent-tui/src/command/mod.rs`
  - 预期: 输出包含 `pub mod login` 和 `login::LoginCommand`

---

### Task 3: ModelPanel 简化

**背景:**
当前 `ModelPanel`（`model_panel.rs` ~675 行）同时承载别名配置（`AliasConfig` 模式）和 Provider CRUD（`Browse/Edit/New/ConfirmDelete` 模式），交互复杂度高。Task 1 已将 `model_aliases` 迁移到 `ProviderConfig.models`，Task 2 已将 Provider CRUD 迁移到 `LoginPanel`。本 Task 重写 `ModelPanel`，将其职责简化为 Provider 列表选择 + 模型级别快捷切换 + Thinking 配置，消除所有 `model_aliases` 引用。本 Task 依赖 Task 1 的 `active_provider_id` + `ProviderModels` 数据结构和 Task 2 的 `LoginPanel`；Task 4（UI 渲染）和 Task 5（事件处理适配）依赖本 Task 产出的新 `ModelPanel` 接口。

**涉及文件:**
- 重写: `rust-agent-tui/src/app/model_panel.rs`
- 修改: `rust-agent-tui/src/app/panel_ops.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/command/model.rs`
- 修改: `rust-agent-tui/src/config/types.rs`（移除引用旧 ModelPanel 的测试）

**执行步骤:**

- [x] 重写 `model_panel.rs` — 删除全部内容，写入新的枚举定义和 `ModelPanel` struct
  - 位置: `rust-agent-tui/src/app/model_panel.rs`，整个文件重写
  - 关键逻辑:
    ```rust
    use crate::config::{ProviderConfig, ThinkingConfig, ZenConfig};

    // ─── AliasTab 枚举（完全保留原有逻辑）───────────────────────────────────────

    #[derive(Debug, Clone, PartialEq)]
    pub enum AliasTab {
        Opus,
        Sonnet,
        Haiku,
    }

    impl AliasTab {
        pub fn next(&self) -> Self {
            match self {
                Self::Opus   => Self::Sonnet,
                Self::Sonnet => Self::Haiku,
                Self::Haiku  => Self::Opus,
            }
        }
        pub fn prev(&self) -> Self {
            match self {
                Self::Opus   => Self::Haiku,
                Self::Sonnet => Self::Opus,
                Self::Haiku  => Self::Sonnet,
            }
        }
        pub fn label(&self) -> &str {
            match self {
                Self::Opus   => "Opus",
                Self::Sonnet => "Sonnet",
                Self::Haiku  => "Haiku",
            }
        }
        pub fn to_key(&self) -> &str {
            match self {
                Self::Opus   => "opus",
                Self::Sonnet => "sonnet",
                Self::Haiku  => "haiku",
            }
        }
        pub fn index(&self) -> usize {
            match self {
                Self::Opus   => 0,
                Self::Sonnet => 1,
                Self::Haiku  => 2,
            }
        }
        pub fn from_key(key: &str) -> Self {
            match key {
                "sonnet" => Self::Sonnet,
                "haiku"  => Self::Haiku,
                _        => Self::Opus,
            }
        }
    }

    // ─── 新枚举 ─────────────────────────────────────────────────────────────────

    #[derive(Debug, Clone, PartialEq)]
    pub enum ModelPanelMode {
        SelectProvider,
        EditThinking,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub enum ModelFocusArea {
        ProviderList,
        AliasTabs,
        Thinking,
    }

    impl ModelFocusArea {
        pub fn next(&self) -> Self {
            match self {
                Self::ProviderList => Self::AliasTabs,
                Self::AliasTabs    => Self::Thinking,
                Self::Thinking     => Self::ProviderList,
            }
        }
        pub fn prev(&self) -> Self {
            match self {
                Self::ProviderList => Self::Thinking,
                Self::AliasTabs    => Self::ProviderList,
                Self::Thinking     => Self::AliasTabs,
            }
        }
    }

    // ─── ModelPanel ───────────────────────────────────────────────────────────────

    pub struct ModelPanel {
        /// provider 列表快照（从 ZenConfig 获取）
        pub providers: Vec<ProviderConfig>,
        /// 当前模式（SelectProvider: 选择 Provider 列表；EditThinking: 编辑 Thinking 预算）
        pub mode: ModelPanelMode,
        /// 光标位置（Provider 列表中）
        pub cursor: usize,
        /// 当前选中的模型级别 Tab（Opus/Sonnet/Haiku）
        pub active_tab: AliasTab,
        /// 当前焦点区域（Provider 列表 / 级别 Tab / Thinking 配置）
        pub focus_area: ModelFocusArea,
        /// Thinking 配置缓冲
        pub buf_thinking_enabled: bool,
        pub buf_thinking_budget: String,
    }
    ```
  - 原因: 移除 `AliasEditField`、`EditField`、`PROVIDER_TYPES`、旧 `ModelPanelMode` 的 5 个变体（`AliasConfig/Browse/Edit/New/ConfirmDelete`），替换为 2 个模式的 `ModelPanelMode` 和 3 区域的 `ModelFocusArea`；移除 `buf_alias_provider`/`buf_alias_model`/`alias_edit_field`/`active_id`/`edit_field`/`buf_name`/`buf_type`/`buf_model`/`buf_api_key`/`buf_base_url`/`scroll_offset` 等旧字段

- [x] 重写 `model_panel.rs` — 实现 `from_config` 构造方法
  - 位置: `rust-agent-tui/src/app/model_panel.rs`，`impl ModelPanel` 块内
  - 关键逻辑:
    ```rust
    impl ModelPanel {
        /// 从 ZenConfig 初始化面板（SelectProvider 模式，光标定位到 active_provider_id 对应的 Provider）
        pub fn from_config(cfg: &ZenConfig) -> Self {
            let providers = cfg.config.providers.clone();
            let active_tab = AliasTab::from_key(&cfg.config.active_alias);
            let cursor = providers
                .iter()
                .position(|p| p.id == cfg.config.active_provider_id)
                .unwrap_or(0);
            let (thinking_enabled, thinking_budget) = match &cfg.config.thinking {
                Some(t) => (t.enabled, t.budget_tokens.to_string()),
                None => (false, "8000".to_string()),
            };
            Self {
                providers,
                mode: ModelPanelMode::SelectProvider,
                cursor,
                active_tab,
                focus_area: ModelFocusArea::ProviderList,
                buf_thinking_enabled: thinking_enabled,
                buf_thinking_budget: thinking_budget,
            }
        }
    ```
  - 原因: 从 `active_provider_id` 直接查找 Provider 设置 cursor，不再需要从 `model_aliases` 推断 `active_id`

- [x] 重写 `model_panel.rs` — 实现 Provider 列表操作和焦点导航方法
  - 位置: `rust-agent-tui/src/app/model_panel.rs`，`impl ModelPanel` 块内，`from_config` 之后
  - 关键逻辑:
    ```rust
        // ── Provider 列表操作 ───────────────────────────────────────────────────

        /// 列表上下移动光标（循环）
        pub fn move_cursor(&mut self, delta: isize) {
            if self.providers.is_empty() { return; }
            let len = self.providers.len();
            self.cursor = ((self.cursor as isize + delta).rem_euclid(len as isize)) as usize;
        }

        /// 确认选择当前 cursor 处的 Provider，更新 cfg 的 active_provider_id
        pub fn confirm_provider(&self, cfg: &mut ZenConfig) {
            if let Some(p) = self.providers.get(self.cursor) {
                cfg.config.active_provider_id = p.id.clone();
            }
        }

        // ── Tab / 焦点导航 ──────────────────────────────────────────────────────

        pub fn tab_next(&mut self) {
            self.active_tab = self.active_tab.next();
        }

        pub fn tab_prev(&mut self) {
            self.active_tab = self.active_tab.prev();
        }

        /// 焦点区域切换：下一个
        pub fn focus_next(&mut self) {
            self.focus_area = self.focus_area.next();
        }

        /// 焦点区域切换：上一个
        pub fn focus_prev(&mut self) {
            self.focus_area = self.focus_area.prev();
        }
    ```
  - 原因: `confirm_provider` 替代旧 `confirm_select`，直接写入 `active_provider_id`；`focus_next`/`focus_prev` 替代旧 `field_next`/`field_prev`，在三个区域间循环

- [x] 重写 `model_panel.rs` — 实现 Thinking 配置操作和文本输入/粘贴方法
  - 位置: `rust-agent-tui/src/app/model_panel.rs`，`impl ModelPanel` 块内，焦点导航之后
  - 关键逻辑:
    ```rust
        // ── Thinking 配置 ──────────────────────────────────────────────────────

        /// 切换 thinking enabled（Space 键，当 focus_area == Thinking 时）
        pub fn toggle_thinking(&mut self) {
            if self.focus_area == ModelFocusArea::Thinking {
                self.buf_thinking_enabled = !self.buf_thinking_enabled;
            }
        }

        /// 输入数字到 thinking budget 缓冲（当 focus_area == Thinking 时）
        pub fn push_char(&mut self, c: char) {
            if self.focus_area == ModelFocusArea::Thinking {
                if c.is_ascii_digit() {
                    self.buf_thinking_budget.push(c);
                }
            }
        }

        /// 删除 thinking budget 缓冲末字符（Backspace，当 focus_area == Thinking 时）
        pub fn pop_char(&mut self) {
            if self.focus_area == ModelFocusArea::Thinking {
                self.buf_thinking_budget.pop();
            }
        }

        /// 粘贴文本到 thinking budget 字段（过滤非数字字符和换行符）
        pub fn paste_text(&mut self, text: &str) {
            if self.focus_area == ModelFocusArea::Thinking {
                let digits: String = text
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect();
                self.buf_thinking_budget.push_str(&digits);
            }
        }

        // ── 保存操作 ──────────────────────────────────────────────────────────────

        /// 将当前面板状态写入 ZenConfig 并保存
        /// 写入项：active_provider_id、active_alias、thinking
        pub fn apply_to_config(&self, cfg: &mut ZenConfig) {
            // 1. 写入 active_provider_id
            if let Some(p) = self.providers.get(self.cursor) {
                cfg.config.active_provider_id = p.id.clone();
            }
            // 2. 写入 active_alias
            cfg.config.active_alias = self.active_tab.to_key().to_string();
            // 3. 写入 thinking
            let budget_tokens = self.buf_thinking_budget.trim().parse::<u32>().unwrap_or(8000);
            cfg.config.thinking = Some(ThinkingConfig {
                enabled: self.buf_thinking_enabled,
                budget_tokens,
            });
        }
    ```
  - 原因: `push_char`/`pop_char`/`paste_text` 仅在 `focus_area == Thinking` 时生效，仅处理数字输入；`apply_to_config` 替代旧 `apply_edit` + `apply_alias_edit` + `activate_current_tab` 三个方法，一次性写入所有配置项

- [x] 重写 `model_panel.rs` — 编写单元测试
  - 测试文件: `rust-agent-tui/src/app/model_panel.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_model_panel_from_config_cursor_at_active_provider`: 构造 2 个 Provider，`active_provider_id="openrouter"`，验证 `cursor == 1`
    - `test_model_panel_from_config_default_mode_is_select_provider`: 验证 `mode == ModelPanelMode::SelectProvider`，`focus_area == ModelFocusArea::ProviderList`
    - `test_model_panel_from_config_loads_thinking`: `thinking = Some { enabled: true, budget_tokens: 4000 }`，验证 `buf_thinking_enabled == true`，`buf_thinking_budget == "4000"`
    - `test_model_panel_from_config_defaults_when_no_thinking`: 无 thinking 配置时 `buf_thinking_enabled == false`，`buf_thinking_budget == "8000"`
    - `test_model_panel_tab_switching`: `tab_next()` 从 Opus→Sonnet→Haiku→Opus 循环；`tab_prev()` 从 Opus→Haiku 循环
    - `test_model_panel_move_cursor_cycle`: 2 个 Provider，`move_cursor(1)` 从 0→1→0 循环；`move_cursor(-1)` 从 0→1 循环
    - `test_model_panel_confirm_provider`: 2 个 Provider，cursor=1，`confirm_provider(&mut cfg)` 后 `cfg.config.active_provider_id == "openrouter"`
    - `test_model_panel_focus_area_navigation`: `focus_next()` 按 ProviderList→AliasTabs→Thinking→ProviderList 循环；`focus_prev()` 反向循环
    - `test_model_panel_toggle_thinking`: `focus_area = Thinking`，`toggle_thinking()` 切换 enabled 状态；`focus_area = ProviderList` 时 `toggle_thinking()` 不生效
    - `test_model_panel_push_char_only_digits_when_thinking_focus`: `focus_area = Thinking`，`push_char('1')`/`push_char('a')`/`push_char('2')`，验证 `buf_thinking_budget == "12"`
    - `test_model_panel_push_char_ignored_when_provider_list_focus`: `focus_area = ProviderList`，`push_char('1')`，验证 `buf_thinking_budget` 不变
    - `test_model_panel_pop_char`: `focus_area = Thinking`，`buf_thinking_budget = "120"`，`pop_char()` 后为 `"12"`
    - `test_model_panel_paste_text_filters_non_digits`: `focus_area = Thinking`，`paste_text("abc123\nxyz")`，验证 `buf_thinking_budget == "123"`
    - `test_model_panel_paste_text_ignored_when_provider_list_focus`: `focus_area = ProviderList`，`paste_text("123")`，验证 `buf_thinking_budget` 不变
    - `test_model_panel_apply_to_config_writes_all`: 设置 `cursor=1`（openrouter）、`active_tab=Sonnet`、`buf_thinking_enabled=true`、`buf_thinking_budget="5000"`，调用 `apply_to_config(&mut cfg)`，验证 `cfg.config.active_provider_id == "openrouter"`、`cfg.config.active_alias == "sonnet"`、`cfg.config.thinking` 为 `Some { enabled: true, budget_tokens: 5000 }`
  - 运行命令: `cargo test -p rust-agent-tui --lib -- app::model_panel::tests`
  - 预期: 所有测试通过

- [x] 重写 `panel_ops.rs` 中的 ModelPanel 相关方法 — 简化为统一的 `model_panel_apply` 方法
  - 位置: `rust-agent-tui/src/app/panel_ops.rs`，替换整个"Model 面板操作"区域（L4-165）
  - 关键逻辑:
    - 删除旧方法：`open_model_panel`/`close_model_panel`/`model_panel_confirm_select`/`model_panel_apply_edit`/`model_panel_confirm_delete`/`model_panel_activate_tab`/`model_panel_save_alias`
    - 新增方法：
      ```rust
      // ─── Model 面板操作 ───────────────────────────────────────────────────────

      /// 打开 /model 面板
      pub fn open_model_panel(&mut self) {
          let cfg = self.zen_config.get_or_insert_with(ZenConfig::default);
          self.core.model_panel = Some(ModelPanel::from_config(cfg));
      }

      /// 关闭 /model 面板（不保存）
      pub fn close_model_panel(&mut self) {
          self.core.model_panel = None;
      }

      /// 确认选择并保存（Enter 键）：写入 active_provider_id + active_alias + thinking，更新状态栏
      pub fn model_panel_confirm(&mut self) {
          let Some(panel) = self.core.model_panel.as_ref() else {
              return;
          };
          let Some(cfg) = self.zen_config.as_mut() else {
              return;
          };
          panel.apply_to_config(cfg);
          let _ = crate::config::save(cfg);
          if let Some(p) = agent::LlmProvider::from_config(cfg) {
              self.provider_name = p.display_name().to_string();
              self.model_name = p.model_name().to_string();
          }
          self.core.model_panel = None;
      }
      ```
    - 保留 `open_model_panel`/`close_model_panel` 不变（签名和逻辑不变）；新增 `model_panel_confirm` 替代旧的 `model_panel_confirm_select` + `model_panel_apply_edit` + `model_panel_activate_tab` + `model_panel_save_alias` 四个方法
  - 原因: 简化后的 ModelPanel 只有一个确认动作（Enter），将 Provider 选择 + alias 切换 + thinking 配置一次性写入并关闭面板

- [x] 修改 `app/mod.rs` — 更新 import（无变化确认）
  - 位置: `rust-agent-tui/src/app/mod.rs`，L54
  - 关键逻辑: `pub use model_panel::ModelPanel;` 保持不变，`ModelPanel` 仍从 `model_panel` 模块导出
  - 原因: `ModelPanel` struct 名称未变，仅内部字段和关联类型变化，外部导出无需修改

- [x] 修改 `command/model.rs` — 更新描述文案
  - 位置: `rust-agent-tui/src/command/model.rs`，`ModelCommand::description` 方法（L12-14）
  - 关键逻辑:
    ```rust
    fn description(&self) -> &str {
        "打开模型选择面板（Provider + 级别 + Thinking）；带参数时直接切换别名（opus/sonnet/haiku）"
    }
    ```
  - 原因: 描述从"Provider / Model 配置面板"改为"模型选择面板"，反映职责简化

- [x] 确认 `config/types.rs` 中引用旧 ModelPanel 接口的 5 个测试已在 Task 1 中删除
  - 位置: `rust-agent-tui/src/config/types.rs`，`#[cfg(test)] mod tests` 块
  - 关键逻辑: Task 1 已删除 `test_model_panel_from_config_loads_thinking`、`test_model_panel_from_config_defaults_when_no_thinking`、`test_model_panel_toggle_thinking`、`test_model_panel_thinking_budget_input_only_digits`、`test_model_panel_apply_edit_saves_thinking` 这 5 个测试。本步骤验证这些测试已不在文件中；经代码确认，Task 1 的步骤已覆盖此删除操作。
  - 原因: 避免与 Task 1 的删除操作重复；这些测试引用旧 `ModelPanel` 的 `EditField`、`ModelPanelMode::Edit`、`model_aliases` 等已删除的类型/字段；新测试已在 `model_panel.rs` 的 `#[cfg(test)]` 块中编写

- [x] 验证所有文件编译通过 — 修复因接口变化导致的编译错误
  - 位置: 项目根目录
  - 关键逻辑: 运行 `cargo build -p rust-agent-tui 2>&1`，逐个修复编译错误
  - 已知需修改的文件（因引用旧 `ModelPanel` 类型/方法）:
    - `rust-agent-tui/src/event.rs` L7: `use crate::app::model_panel::ModelPanelMode` — import 路径不变（`ModelPanelMode` 名称保留），但 `handle_model_panel` 函数体需完全重写（此为 Task 5 范畴，本步骤仅确保 import 无误）
    - `rust-agent-tui/src/ui/main_ui/panels/model.rs` L12: `use crate::app::model_panel::{AliasEditField, AliasTab, EditField, ModelPanelMode, PROVIDER_TYPES}` — 移除 `AliasEditField`、`EditField`、`PROVIDER_TYPES` import（已删除），保留 `AliasTab`、`ModelPanelMode`
  - 原因: 简化后 ModelPanel 删除了 `AliasEditField`、`EditField`、`PROVIDER_TYPES`，所有引用这些类型的代码需同步清理；事件处理和 UI 渲染的完整重写在 Task 4/5 中进行，本步骤仅清理 import 确保编译通过

**检查步骤:**
- [x] 验证新枚举类型已定义
  - `grep -n 'pub enum ModelPanelMode\|pub enum ModelFocusArea\|pub enum AliasTab' rust-agent-tui/src/app/model_panel.rs`
  - 预期: 输出 3 行，分别包含 `ModelPanelMode { SelectProvider, EditThinking }`、`ModelFocusArea { ProviderList, AliasTabs, Thinking }`、`AliasTab`
- [x] 验证旧枚举已移除
  - `grep -n 'AliasEditField\|pub enum EditField\|PROVIDER_TYPES' rust-agent-tui/src/app/model_panel.rs`
  - 预期: 无输出
- [x] 验证旧字段已移除
  - `grep -n 'buf_alias_provider\|buf_alias_model\|alias_edit_field\|active_id\|buf_name\|buf_type\|buf_model\|buf_api_key\|buf_base_url\|edit_field\|scroll_offset' rust-agent-tui/src/app/model_panel.rs`
  - 预期: 无输出
- [x] 验证新方法已实现
  - `grep -n 'fn from_config\|fn confirm_provider\|fn focus_next\|fn focus_prev\|fn toggle_thinking\|fn push_char\|fn pop_char\|fn paste_text\|fn apply_to_config' rust-agent-tui/src/app/model_panel.rs`
  - 预期: 输出 9 行
- [x] 验证旧方法已移除
  - `grep -n 'fn apply_alias_edit\|fn cycle_alias_provider\|fn push_alias_char\|fn pop_alias_char\|fn enter_edit\|fn enter_new\|fn request_delete\|fn cancel_delete\|fn confirm_select\|fn field_next\|fn field_prev\|fn cycle_type\|fn apply_edit\|fn confirm_delete\|fn alias_field_next\|fn alias_field_prev' rust-agent-tui/src/app/model_panel.rs`
  - 预期: 无输出
- [x] 验证 `panel_ops.rs` 中的 Model 面板方法已简化
  - `grep -n 'fn model_panel_confirm\b\|fn open_model_panel\|fn close_model_panel' rust-agent-tui/src/app/panel_ops.rs`
  - 预期: 输出 3 行（`open_model_panel`、`close_model_panel`、`model_panel_confirm`）
  - `grep -n 'fn model_panel_confirm_select\|fn model_panel_apply_edit\|fn model_panel_confirm_delete\|fn model_panel_activate_tab\|fn model_panel_save_alias' rust-agent-tui/src/app/panel_ops.rs`
  - 预期: 无输出
- [x] 验证 `types.rs` 旧 ModelPanel 测试已删除
  - `grep -n 'test_model_panel' rust-agent-tui/src/config/types.rs`
  - 预期: 无输出
- [x] 运行 `model_panel` 单元测试
  - `cargo test -p rust-agent-tui --lib -- app::model_panel::tests`
  - 预期: 全部测试通过
- [x] 运行整个 crate 单元测试
  - `cargo test -p rust-agent-tui --lib 2>&1 | tail -10`
  - 预期: 全部测试通过（或仅剩 `event.rs` 和 `panels/model.rs` 中的编译错误，将在 Task 4/5 修复）

---

### Task 4: UI 渲染与事件集成

**背景:**
Task 1 重构了数据模型（`ProviderModels` + `active_provider_id`），Task 2 创建了 `LoginPanel` 状态机和 `/login` 命令注册，Task 3 简化了 `ModelPanel`（仅 `SelectProvider`/`EditThinking` 两种模式 + 三区域焦点）。本 Task 负责将这两个面板接入 UI 渲染层（ratatui 面板绘制）和事件处理层（键盘/粘贴事件分发），使 `/login` 和 `/model` 命令可通过 TUI 完整交互。本 Task 的输出是用户可感知的最终功能，无后续 Task 依赖。

**涉及文件:**
- 新建: `rust-agent-tui/src/ui/main_ui/panels/login.rs`
- 重写: `rust-agent-tui/src/ui/main_ui/panels/model.rs`
- 修改: `rust-agent-tui/src/ui/main_ui/panels/mod.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`
- 修改: `rust-agent-tui/src/event.rs`
- 修改: `rust-agent-tui/src/app/panel_ops.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/app/core.rs`

**执行步骤:**

- [x] 修改 `app/core.rs` — 在 `AppCore` struct 中新增 `login_panel` 字段
  - 位置: `rust-agent-tui/src/app/core.rs`，`AppCore` struct（L18-40），在 `pub model_panel: Option<ModelPanel>,`（L37）之后插入新字段
  - 关键逻辑:
    - 在文件顶部（L12-15）新增 import：`use super::login_panel::LoginPanel;`
    - 在 struct 中新增字段：
      ```rust
      pub login_panel: Option<LoginPanel>,
      ```
    - 在 `AppCore::new()` 方法（L54-77）的 `Self { ... }` 初始化块中新增：
      ```rust
      login_panel: None,
      ```
      插入在 `model_panel: None,`（L73）之后
  - 原因: `LoginPanel` 作为 `AppCore` 的可选项字段，与 `ModelPanel` 平级；面板互斥逻辑通过 `panel_ops.rs` 方法保证

- [x] 修改 `app/mod.rs` — 新增 `login_panel` 模块声明并导出 `LoginPanel` 类型
  - 位置: `rust-agent-tui/src/app/mod.rs`
  - 关键逻辑:
    - 在文件顶部模块声明区域（L1-9）新增：`pub mod login_panel;`（插入在 `pub mod model_panel;` 之前）
    - 在 `pub use model_panel::ModelPanel;`（L54）之后新增：
      ```rust
      pub use login_panel::LoginPanel;
      ```
  - 原因: `LoginPanel` 的字段存储在 `AppCore` 中（上一步骤已新增），`App` struct 无需重复声明。本步骤仅需注册模块和导出类型。`App::new()` 和 `new_headless()` 无需修改，因为 `AppCore::new()` 已初始化 `login_panel: None`

- [x] 修改 `app/panel_ops.rs` — 新增 Login 面板操作方法，简化 Model 面板操作方法
  - 位置: `rust-agent-tui/src/app/panel_ops.rs`，在 "Model 面板操作" 区域（L4-164）替换
  - 关键逻辑:
    - 删除旧方法（Task 3 已定义简化版本的 `open_model_panel`/`close_model_panel`/`model_panel_confirm`，此处确认与之一致）
    - 确认以下三个方法存在（Task 3 已创建）：
      ```rust
      pub fn open_model_panel(&mut self) { ... }
      pub fn close_model_panel(&mut self) { ... }
      pub fn model_panel_confirm(&mut self) { ... }
      ```
    - 在 "Model 面板操作" 和 "Agent 面板操作" 之间新增 Login 面板操作区域：
      ```rust
      // ─── Login 面板操作 ───────────────────────────────────────────────────────

      /// 打开 /login 面板（同时关闭 model 面板，实现互斥）
      pub fn open_login_panel(&mut self) {
          let cfg = self.zen_config.get_or_insert_with(ZenConfig::default);
          self.core.login_panel = Some(LoginPanel::from_config(cfg));
          // 互斥：关闭 model 面板
          self.core.model_panel = None;
      }

      /// 关闭 /login 面板（不保存）
      pub fn close_login_panel(&mut self) {
          self.core.login_panel = None;
      }

      /// 保存 Login 面板的编辑/新建内容到 ZenConfig
      pub fn login_panel_apply_edit(&mut self) {
          let Some(panel) = self.core.login_panel.as_mut() else {
              return;
          };
          let Some(cfg) = self.zen_config.as_mut() else {
              return;
          };
          panel.apply_edit(cfg);
          let _ = crate::config::save(cfg);
          if let Some(p) = agent::LlmProvider::from_config(cfg) {
              self.provider_name = p.display_name().to_string();
              self.model_name = p.model_name().to_string();
          }
      }

      /// 确认删除光标处的 Provider
      pub fn login_panel_confirm_delete(&mut self) {
          let Some(panel) = self.core.login_panel.as_mut() else {
              return;
          };
          let Some(cfg) = self.zen_config.as_mut() else {
              return;
          };
          panel.confirm_delete(cfg);
          let _ = crate::config::save(cfg);
          if let Some(p) = agent::LlmProvider::from_config(cfg) {
              self.provider_name = p.display_name().to_string();
              self.model_name = p.model_name().to_string();
          }
      }
      ```
    - 修改 `open_model_panel` 方法，在打开时关闭 login 面板：
      ```rust
      pub fn open_model_panel(&mut self) {
          let cfg = self.zen_config.get_or_insert_with(ZenConfig::default);
          self.core.model_panel = Some(ModelPanel::from_config(cfg));
          // 互斥：关闭 login 面板
          self.core.login_panel = None;
      }
      ```
  - 原因: Login 和 Model 面板互斥，打开一个时关闭另一个；`login_panel_apply_edit`/`login_panel_confirm_delete` 封装了保存配置 + 刷新状态栏的逻辑，与 `model_panel_confirm` 保持一致

- [x] 修改 `panels/mod.rs` — 新增 `login` 子模块
  - 位置: `rust-agent-tui/src/ui/main_ui/panels/mod.rs`（L1-5）
  - 关键逻辑:
    - 在现有模块声明中新增一行（按字母序插入在 `model` 之前）：
      ```rust
      pub mod login;
      ```
  - 原因: 注册 `panels/login.rs` 渲染模块，使 `main_ui.rs` 可以调用 `panels::login::render_login_panel`

- [x] 新建 `panels/login.rs` — 实现 Login 面板渲染
  - 位置: `rust-agent-tui/src/ui/main_ui/panels/login.rs`（新文件）
  - 关键逻辑:
    ```rust
    use ratatui::{
        layout::Rect,
        style::{Color, Modifier, Style},
        text::{Line, Span, Text},
        widgets::Paragraph,
        Frame,
    };

    use perihelion_widgets::BorderedPanel;

    use crate::app::login_panel::{LoginEditField, LoginPanelMode};
    use crate::app::App;
    use crate::ui::theme;

    /// /login 面板渲染（底部展开区）
    pub(crate) fn render_login_panel(f: &mut Frame, app: &App, area: Rect) {
        let Some(panel) = &app.core.login_panel else { return };

        let (border_color, title) = match panel.mode {
            LoginPanelMode::Browse        => (theme::MUTED,    " /login — Provider 管理 "),
            LoginPanelMode::Edit          => (theme::WARNING, " /login — 编辑 Provider "),
            LoginPanelMode::New           => (theme::SAGE,    " /login — 新建 Provider "),
            LoginPanelMode::ConfirmDelete => (theme::ERROR,   " /login — 确认删除 "),
        };

        let inner = BorderedPanel::new(
            Span::styled(title, Style::default().fg(border_color).add_modifier(Modifier::BOLD))
        )
            .border_style(Style::default().fg(border_color))
            .render(f, area);

        let active_provider_id = app.zen_config.as_ref()
            .map(|c| c.config.active_provider_id.as_str())
            .unwrap_or("");

        match panel.mode {
            // ── Browse 模式：Provider 列表 ────────────────────────────────────────
            LoginPanelMode::Browse => {
                let mut lines: Vec<Line> = Vec::new();
                for (i, p) in panel.providers.iter().enumerate() {
                    let is_cursor = i == panel.cursor;
                    let is_active = p.id == active_provider_id;
                    let bullet = if is_active { "●" } else { "○" };
                    let cursor_char = if is_cursor { "▶" } else { " " };
                    let name = p.display_name().to_string();
                    let type_tag = format!("({})", p.provider_type);
                    let row_style = if is_cursor {
                        Style::default().fg(Color::White).bg(theme::ACCENT)
                    } else if is_active {
                        Style::default().fg(theme::ACCENT)
                    } else {
                        Style::default().fg(theme::TEXT)
                    };
                    lines.push(Line::from(vec![
                        Span::styled(format!("{} {} ", cursor_char, bullet), row_style),
                        Span::styled(format!("{} ", name), row_style.add_modifier(Modifier::BOLD)),
                        Span::styled(type_tag, row_style.fg(if is_cursor { Color::White } else { theme::MUTED })),
                    ]));
                }
                if panel.providers.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "  （无 provider，按 n 新建）",
                        Style::default().fg(theme::MUTED),
                    )));
                }
                // 快捷键提示
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled(" e", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                    Span::styled(":编辑  ", Style::default().fg(theme::MUTED)),
                    Span::styled("n", Style::default().fg(theme::SAGE).add_modifier(Modifier::BOLD)),
                    Span::styled(":新建  ", Style::default().fg(theme::MUTED)),
                    Span::styled("d", Style::default().fg(theme::ERROR).add_modifier(Modifier::BOLD)),
                    Span::styled(":删除  ", Style::default().fg(theme::MUTED)),
                    Span::styled("Esc", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                    Span::styled(":关闭", Style::default().fg(theme::MUTED)),
                ]));
                lines.truncate(inner.height as usize);
                f.render_widget(Paragraph::new(Text::from(lines)), inner);
            }

            // ── Edit/New 模式：7 字段表单 ────────────────────────────────────────
            LoginPanelMode::Edit | LoginPanelMode::New => {
                let fields: &[(LoginEditField, &str, &str)] = &[
                    (LoginEditField::Name,        "Name        ", &panel.buf_name),
                    (LoginEditField::Type,        "Type        ", &panel.buf_type),
                    (LoginEditField::BaseUrl,     "Base URL    ", &panel.buf_base_url),
                    (LoginEditField::ApiKey,      "API Key     ", &panel.buf_api_key),
                    (LoginEditField::OpusModel,   "Opus Model  ", &panel.buf_opus_model),
                    (LoginEditField::SonnetModel, "Sonnet Model", &panel.buf_sonnet_model),
                    (LoginEditField::HaikuModel,  "Haiku Model ", &panel.buf_haiku_model),
                ];

                let mut lines: Vec<Line> = Vec::new();
                for (field, label, value) in fields {
                    let is_active = *field == panel.edit_field;
                    let value_display = if *field == LoginEditField::Type {
                        // Type 字段：显示循环切换选项
                        let types = ["openai", "anthropic"];
                        types.iter()
                            .map(|t| if *t == value { format!("[{}]", t) } else { t.to_string() })
                            .collect::<Vec<_>>()
                            .join("  ")
                    } else if *field == LoginEditField::ApiKey && !is_active {
                        // API Key 非编辑时遮盖
                        mask_api_key(value)
                    } else if is_active {
                        format!("{}█", value)
                    } else {
                        value.to_string()
                    };

                    let (label_style, value_style) = if is_active {
                        (
                            Style::default().fg(Color::White).bg(theme::ACCENT).add_modifier(Modifier::BOLD),
                            Style::default().fg(Color::White).bg(theme::ACCENT),
                        )
                    } else {
                        (Style::default().fg(theme::MUTED), Style::default().fg(theme::TEXT))
                    };

                    lines.push(Line::from(vec![
                        Span::styled(format!("  {} ", label), label_style),
                        Span::styled(format!(" {}", value_display), value_style),
                    ]));
                }

                // 快捷键提示
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled(" Tab", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                    Span::styled(":切换字段  ", Style::default().fg(theme::MUTED)),
                    Span::styled("Space", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                    Span::styled(":切换Type  ", Style::default().fg(theme::MUTED)),
                    Span::styled("Enter", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                    Span::styled(":保存  ", Style::default().fg(theme::MUTED)),
                    Span::styled("Esc", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
                    Span::styled(":取消", Style::default().fg(theme::MUTED)),
                ]));
                lines.truncate(inner.height as usize);
                f.render_widget(Paragraph::new(Text::from(lines)), inner);
            }

            // ── ConfirmDelete 模式 ──────────────────────────────────────────────
            LoginPanelMode::ConfirmDelete => {
                // 上半：provider 列表（复用 Browse 的渲染逻辑）
                let mut list_lines: Vec<Line> = Vec::new();
                for (i, p) in panel.providers.iter().enumerate() {
                    let is_cursor = i == panel.cursor;
                    let is_active = p.id == active_provider_id;
                    let bullet = if is_active { "●" } else { "○" };
                    let cursor_char = if is_cursor { "▶" } else { " " };
                    let row_style = if is_cursor {
                        Style::default().fg(Color::White).bg(theme::ACCENT)
                    } else if is_active {
                        Style::default().fg(theme::ACCENT)
                    } else {
                        Style::default().fg(theme::TEXT)
                    };
                    list_lines.push(Line::from(vec![
                        Span::styled(format!("{} {} ", cursor_char, bullet), row_style),
                        Span::styled(p.display_name().to_string(), row_style.add_modifier(Modifier::BOLD)),
                    ]));
                }
                list_lines.truncate(inner.height.saturating_sub(5) as usize);
                f.render_widget(Paragraph::new(Text::from(list_lines)), inner);

                // 下半：确认提示
                let confirm_y = inner.y + inner.height.saturating_sub(4);
                let confirm_area = Rect { y: confirm_y, height: 4, ..inner };
                if let Some(p) = panel.providers.get(panel.cursor) {
                    let confirm_lines = vec![
                        Line::from(""),
                        Line::from(vec![
                            Span::styled("  确认删除 ", Style::default().fg(theme::TEXT)),
                            Span::styled(p.display_name().to_string(), Style::default().fg(theme::ERROR).add_modifier(Modifier::BOLD)),
                            Span::styled(" ？", Style::default().fg(theme::TEXT)),
                        ]),
                        Line::from(vec![
                            Span::styled(" y", Style::default().fg(theme::ERROR).add_modifier(Modifier::BOLD)),
                            Span::styled(":确认删除  ", Style::default().fg(theme::MUTED)),
                            Span::styled("n/Esc", Style::default().fg(theme::SAGE).add_modifier(Modifier::BOLD)),
                            Span::styled(":取消", Style::default().fg(theme::MUTED)),
                        ]),
                    ];
                    f.render_widget(Paragraph::new(Text::from(confirm_lines)), confirm_area);
                }
            }
        }
    }

    /// 遮盖 API Key 中间部分
    fn mask_api_key(key: &str) -> String {
        let chars: Vec<char> = key.chars().collect();
        let len = chars.len();
        if len <= 8 {
            return "*".repeat(len);
        }
        let prefix: String = chars[..4].iter().collect();
        let suffix: String = chars[len - 4..].iter().collect();
        format!("{}****{}", prefix, suffix)
    }
    ```
  - 原因: Login 面板渲染逻辑与当前 `panels/model.rs` 的 Provider 管理子面板结构一致，但分离为独立文件；`mask_api_key` 复制一份避免跨模块依赖

- [x] 重写 `panels/model.rs` — 简化为三区域渲染（Provider 列表 + 级别切换 + Thinking）
  - 位置: `rust-agent-tui/src/ui/main_ui/panels/model.rs`，整个文件重写
  - 关键逻辑:
    ```rust
    use ratatui::{
        layout::Rect,
        style::{Color, Modifier, Style},
        text::{Line, Span, Text},
        widgets::Paragraph,
        Frame,
    };

    use perihelion_widgets::BorderedPanel;

    use crate::app::model_panel::{AliasTab, ModelFocusArea, ModelPanelMode};
    use crate::app::App;
    use crate::ui::theme;

    /// /model 面板渲染（底部展开区）—— 简化为 Provider 列表 + 级别切换 + Thinking 配置
    pub(crate) fn render_model_panel(f: &mut Frame, app: &App, area: Rect) {
        let Some(panel) = &app.core.model_panel else { return };

        let inner = BorderedPanel::new(
            Span::styled(" /model — 模型选择 ", Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD))
        )
            .border_style(Style::default().fg(theme::ACCENT))
            .render(f, area);

        let active_provider_id = app.zen_config.as_ref()
            .map(|c| c.config.active_provider_id.as_str())
            .unwrap_or("");

        // ── 区域 1: Provider 列表 ──────────────────────────────────────────────
        let mut lines: Vec<Line> = Vec::new();
        for (i, p) in panel.providers.iter().enumerate() {
            let is_cursor = i == panel.cursor;
            let is_active = p.id == active_provider_id;
            let bullet = if is_active { "●" } else { "○" };
            let cursor_char = if is_cursor { "▶" } else { " " };
            let name = p.display_name().to_string();
            let type_tag = format!("({})", p.provider_type);
            let models_summary = format!(
                "opus={}  sonnet={}  haiku={}",
                if p.models.opus.is_empty() { "-" } else { &p.models.opus },
                if p.models.sonnet.is_empty() { "-" } else { &p.models.sonnet },
                if p.models.haiku.is_empty() { "-" } else { &p.models.haiku },
            );
            let is_focused = panel.focus_area == ModelFocusArea::ProviderList;
            let row_style = if is_cursor && is_focused {
                Style::default().fg(Color::White).bg(theme::ACCENT)
            } else if is_cursor {
                Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD)
            } else if is_active {
                Style::default().fg(theme::ACCENT)
            } else {
                Style::default().fg(theme::TEXT)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{} {} ", cursor_char, bullet), row_style),
                Span::styled(format!("{} ", name), row_style.add_modifier(Modifier::BOLD)),
                Span::styled(format!("{} ", type_tag), row_style.fg(if is_cursor { Color::White } else { theme::MUTED })),
                Span::styled(models_summary, Style::default().fg(theme::MUTED)),
            ]));
        }
        if panel.providers.is_empty() {
            lines.push(Line::from(Span::styled(
                "  （无 provider，使用 /login 添加）",
                Style::default().fg(theme::MUTED),
            )));
        }
        lines.push(Line::from(""));

        // ── 区域 2: 级别切换栏 ────────────────────────────────────────────────
        let active_alias = app.zen_config.as_ref()
            .map(|c| c.config.active_alias.as_str())
            .unwrap_or("opus");
        let tabs = [AliasTab::Opus, AliasTab::Sonnet, AliasTab::Haiku];
        let mut tab_spans: Vec<Span> = Vec::new();
        tab_spans.push(Span::styled(" ", Style::default()));
        for tab in &tabs {
            let is_current = *tab == panel.active_tab;
            let is_active_alias = tab.to_key() == active_alias;
            let is_focused = panel.focus_area == ModelFocusArea::AliasTabs;
            let label = if is_active_alias {
                format!("★ {}", tab.label())
            } else {
                format!("  {}  ", tab.label())
            };
            let style = if is_current && is_focused {
                Style::default().fg(Color::White).bg(theme::ACCENT).add_modifier(Modifier::BOLD)
            } else if is_current {
                Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD)
            } else if is_active_alias {
                Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::MUTED)
            };
            tab_spans.push(Span::styled(format!("[{}]", label), style));
            tab_spans.push(Span::styled("  ", Style::default()));
        }
        lines.push(Line::from(tab_spans));
        lines.push(Line::from(""));

        // ── 区域 3: Thinking 配置 ──────────────────────────────────────────────
        {
            let is_focused = panel.focus_area == ModelFocusArea::Thinking;
            let enabled_tag = if panel.buf_thinking_enabled { "[ON] " } else { "[OFF]" };
            let budget_display = if panel.mode == ModelPanelMode::EditThinking && is_focused {
                format!("{}█", panel.buf_thinking_budget)
            } else {
                panel.buf_thinking_budget.clone()
            };
            let enabled_color = if panel.buf_thinking_enabled { theme::THINKING } else { theme::MUTED };
            let (label_style, enabled_style, budget_style) = if is_focused {
                (
                    Style::default().fg(Color::White).bg(theme::ACCENT).add_modifier(Modifier::BOLD),
                    Style::default().fg(if panel.buf_thinking_enabled { theme::THINKING } else { theme::MUTED }).bg(theme::ACCENT),
                    Style::default().fg(Color::White).bg(theme::ACCENT),
                )
            } else {
                (Style::default().fg(theme::MUTED), Style::default().fg(enabled_color), Style::default().fg(theme::TEXT))
            };
            lines.push(Line::from(vec![
                Span::styled("  Thinking ", label_style),
                Span::styled(format!(" {} ", enabled_tag), enabled_style),
                Span::styled(format!("budget: {}", budget_display), budget_style),
            ]));
        }

        // ── 快捷键提示 ────────────────────────────────────────────────────────
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(" Tab", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
            Span::styled(":切换焦点  ", Style::default().fg(theme::MUTED)),
            Span::styled("Space", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
            Span::styled(":切换/开关  ", Style::default().fg(theme::MUTED)),
            Span::styled("Enter", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
            Span::styled(":确认  ", Style::default().fg(theme::MUTED)),
            Span::styled("Esc", Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD)),
            Span::styled(":关闭", Style::default().fg(theme::MUTED)),
        ]));
        lines.truncate(inner.height as usize);
        f.render_widget(Paragraph::new(Text::from(lines)), inner);
    }
    ```
  - 原因: 简化后的 ModelPanel 渲染将 Provider 列表、级别切换栏、Thinking 配置合并在一个视图中，Tab 切换焦点区域（`ProviderList`/`AliasTabs`/`Thinking`），而非旧版的嵌套模式切换

- [x] 修改 `main_ui.rs` — 新增 login panel 渲染分支和高度计算
  - 位置: `rust-agent-tui/src/ui/main_ui.rs`
  - 关键逻辑:
    - 在 `active_panel_height()` 函数（L116-144）中，在 `thread_browser` 分支之后、`model_panel` 分支之前新增 login 面板高度计算：
      ```rust
      } else if app.core.login_panel.is_some() {
          14
      ```
      插入位置：将现有的 `else if app.core.model_panel.is_some() {`（L120）替换为在其前面插入 login 分支：
      ```rust
      let raw = if let Some(panel) = &app.core.thread_browser {
          (panel.total() as u16 + 4).max(6)
      } else if app.core.login_panel.is_some() {
          14
      } else if app.core.model_panel.is_some() {
          14
      ```
    - 在渲染分支（L79-105）中，在 `model_panel` 渲染之前新增 `login_panel` 渲染：
      ```rust
      if app.core.login_panel.is_some() {
          panels::login::render_login_panel(f, app, panel_area);
      }
      ```
      插入在 `None => {}` 之后、`if app.core.model_panel.is_some()` 之前（L89-91 之间）
  - 原因: login panel 与 model panel 互斥（`open_login_panel` 中已设置 `model_panel = None`），但渲染和高度计算均需处理 login_panel 分支；优先级上 login 在 model 之前渲染

- [x] 重写 `event.rs` 中的 `handle_model_panel` — 适配简化后的 ModelPanel
  - 位置: `rust-agent-tui/src/event.rs`，`handle_model_panel` 函数（L498-643）
  - 关键逻辑:
    - 修改文件顶部 import（L7），将 `use crate::app::model_panel::ModelPanelMode;` 改为：
      ```rust
      use crate::app::model_panel::{ModelPanelMode, ModelFocusArea};
      ```
    - 重写 `handle_model_panel` 函数整体：
      ```rust
      fn handle_model_panel(app: &mut App, input: Input) {
          let mode = match app.core.model_panel.as_ref() {
              Some(p) => p.mode.clone(),
              None => return,
          };

          match mode {
              ModelPanelMode::SelectProvider => match input {
                  Input { key: Key::Esc, .. } => {
                      app.close_model_panel();
                  }
                  Input { key: Key::Char('v'), ctrl: true, .. } => {
                      if let Ok(mut clipboard) = arboard::Clipboard::new() {
                          if let Ok(text) = clipboard.get_text() {
                              app.core.model_panel.as_mut().unwrap().paste_text(&text);
                          }
                      }
                  }
                  // Tab / Shift+Tab：切换焦点区域
                  Input { key: Key::Tab, shift: false, .. } => {
                      app.core.model_panel.as_mut().unwrap().focus_next();
                  }
                  Input { key: Key::Tab, shift: true, .. } => {
                      app.core.model_panel.as_mut().unwrap().focus_prev();
                  }
                  // ↑↓：Provider 列表上下移动（当焦点在 ProviderList 时）或级别切换（当焦点在 AliasTabs 时）
                  Input { key: Key::Up, .. } | Input { key: Key::Char('k'), .. } => {
                      let focus = app.core.model_panel.as_ref().unwrap().focus_area.clone();
                      match focus {
                          ModelFocusArea::ProviderList => {
                              app.core.model_panel.as_mut().unwrap().move_cursor(-1);
                          }
                          ModelFocusArea::AliasTabs => {
                              app.core.model_panel.as_mut().unwrap().tab_prev();
                          }
                          ModelFocusArea::Thinking => {}
                      }
                  }
                  Input { key: Key::Down, .. } | Input { key: Key::Char('j'), .. } => {
                      let focus = app.core.model_panel.as_ref().unwrap().focus_area.clone();
                      match focus {
                          ModelFocusArea::ProviderList => {
                              app.core.model_panel.as_mut().unwrap().move_cursor(1);
                          }
                          ModelFocusArea::AliasTabs => {
                              app.core.model_panel.as_mut().unwrap().tab_next();
                          }
                          ModelFocusArea::Thinking => {}
                      }
                  }
                  // ←→：级别切换（当焦点在 AliasTabs 时）
                  Input { key: Key::Left, .. } => {
                      let focus = app.core.model_panel.as_ref().unwrap().focus_area.clone();
                      if focus == ModelFocusArea::AliasTabs {
                          app.core.model_panel.as_mut().unwrap().tab_prev();
                      }
                  }
                  Input { key: Key::Right, .. } => {
                      let focus = app.core.model_panel.as_ref().unwrap().focus_area.clone();
                      if focus == ModelFocusArea::AliasTabs {
                          app.core.model_panel.as_mut().unwrap().tab_next();
                      }
                  }
                  // Space：切换 thinking enabled（焦点在 Thinking 时）或无操作
                  Input { key: Key::Char(' '), .. } => {
                      app.core.model_panel.as_mut().unwrap().toggle_thinking();
                  }
                  // 1/2/3：快捷切换级别
                  Input { key: Key::Char('1'), ctrl: false, alt: false, .. } => {
                      app.core.model_panel.as_mut().unwrap().active_tab =
                          crate::app::model_panel::AliasTab::Opus;
                  }
                  Input { key: Key::Char('2'), ctrl: false, alt: false, .. } => {
                      app.core.model_panel.as_mut().unwrap().active_tab =
                          crate::app::model_panel::AliasTab::Sonnet;
                  }
                  Input { key: Key::Char('3'), ctrl: false, alt: false, .. } => {
                      app.core.model_panel.as_mut().unwrap().active_tab =
                          crate::app::model_panel::AliasTab::Haiku;
                  }
                  // Enter：确认选择并关闭
                  Input { key: Key::Enter, .. } => {
                      app.model_panel_confirm();
                  }
                  // Backspace：删除 thinking budget 末字符（焦点在 Thinking 时）
                  Input { key: Key::Backspace, .. } => {
                      app.core.model_panel.as_mut().unwrap().pop_char();
                  }
                  // 数字字符输入到 thinking budget（焦点在 Thinking 时）
                  Input { key: Key::Char(c), ctrl: false, alt: false, .. } => {
                      app.core.model_panel.as_mut().unwrap().push_char(c);
                  }
                  _ => {}
              },
              ModelPanelMode::EditThinking => {
                  // EditThinking 模式与 SelectProvider 的 Thinking 焦点分支一致
                  // 实际上简化后的 ModelPanel 中 EditThinking 仅作为 mode 标记，
                  // 所有输入仍走 SelectProvider 分支（因为 focus_area 决定行为）
                  // 此分支留空 fallback 到 SelectProvider 逻辑
              }
          }
      }
      ```
  - 原因: 简化后的 ModelPanel 只有两种模式，`SelectProvider` 处理所有键盘事件（焦点区域决定行为），消除旧版的 5 模式嵌套分发；`EditThinking` 模式仅用于渲染标记（光标显示在 budget 字段），所有输入统一由 `focus_area` 判断

- [x] 在 `event.rs` 中新增 `handle_login_panel` 函数 — 处理 Login 面板四种模式的键盘事件
  - 位置: `rust-agent-tui/src/event.rs`，在 `handle_model_panel` 函数之前（~L496）
  - 关键逻辑:
    ```rust
    // ─── /login 面板键盘处理 ──────────────────────────────────────────────────────

    fn handle_login_panel(app: &mut App, input: Input) {
        use crate::app::login_panel::LoginPanelMode;

        let mode = match app.core.login_panel.as_ref() {
            Some(p) => p.mode.clone(),
            None => return,
        };

        match mode {
            // ── Browse 模式 ──────────────────────────────────────────────────
            LoginPanelMode::Browse => match input {
                Input { key: Key::Esc, .. } => {
                    app.close_login_panel();
                }
                Input { key: Key::Up, .. } | Input { key: Key::Char('k'), .. } => {
                    app.core.login_panel.as_mut().unwrap().move_cursor(-1);
                }
                Input { key: Key::Down, .. } | Input { key: Key::Char('j'), .. } => {
                    app.core.login_panel.as_mut().unwrap().move_cursor(1);
                }
                Input { key: Key::Char('e'), ctrl: false, alt: false, .. } => {
                    app.core.login_panel.as_mut().unwrap().enter_edit();
                }
                Input { key: Key::Char('n'), ctrl: false, alt: false, .. } => {
                    app.core.login_panel.as_mut().unwrap().enter_new();
                }
                Input { key: Key::Char('d'), ctrl: false, alt: false, .. } => {
                    app.core.login_panel.as_mut().unwrap().request_delete();
                }
                _ => {}
            },
            // ── Edit/New 模式 ────────────────────────────────────────────────
            LoginPanelMode::Edit | LoginPanelMode::New => match input {
                Input { key: Key::Esc, .. } => {
                    app.core.login_panel.as_mut().unwrap().mode = LoginPanelMode::Browse;
                }
                Input { key: Key::Char('v'), ctrl: true, .. } => {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        if let Ok(text) = clipboard.get_text() {
                            app.core.login_panel.as_mut().unwrap().paste_text(&text);
                        }
                    }
                }
                Input { key: Key::Tab, shift: false, .. } => {
                    app.core.login_panel.as_mut().unwrap().field_next();
                }
                Input { key: Key::Tab, shift: true, .. } => {
                    app.core.login_panel.as_mut().unwrap().field_prev();
                }
                // Space：Type 字段循环切换 / 其他字段输入空格
                Input { key: Key::Char(' '), .. } => {
                    let field = app.core.login_panel.as_ref().unwrap().edit_field.clone();
                    if field == crate::app::login_panel::LoginEditField::Type {
                        app.core.login_panel.as_mut().unwrap().cycle_type();
                    } else {
                        app.core.login_panel.as_mut().unwrap().push_char(' ');
                    }
                }
                Input { key: Key::Enter, .. } => {
                    app.login_panel_apply_edit();
                }
                Input { key: Key::Backspace, .. } => {
                    app.core.login_panel.as_mut().unwrap().pop_char();
                }
                Input { key: Key::Char(c), ctrl: false, alt: false, .. } => {
                    app.core.login_panel.as_mut().unwrap().push_char(c);
                }
                _ => {}
            },
            // ── ConfirmDelete 模式 ────────────────────────────────────────────
            LoginPanelMode::ConfirmDelete => match input {
                Input { key: Key::Char('y'), .. } => {
                    app.login_panel_confirm_delete();
                }
                Input { key: Key::Char('n'), .. } | Input { key: Key::Esc, .. } => {
                    app.core.login_panel.as_mut().unwrap().cancel_delete();
                }
                _ => {}
            },
        }
    }
    ```
  - 原因: Login 面板的键盘处理与旧 `handle_model_panel` 的 Browse/Edit/New/ConfirmDelete 逻辑一致，但操作目标从 `app.core.model_panel` 切换到 `app.core.login_panel`，且 Edit 模式使用 `LoginEditField` 的 7 字段导航

- [x] 修改 `event.rs` 中的事件分发顺序 — 在 model 面板之前插入 login 面板处理
  - 位置: `rust-agent-tui/src/event.rs`，`next_event` 函数的 Key 事件分发区域（L104-108）
  - 关键逻辑:
    - 在 `// /model 面板优先处理`（L104）之前新增 login 面板分支：
      ```rust
      // /login 面板优先处理
      if app.core.login_panel.is_some() {
          handle_login_panel(app, input);
          return Ok(Some(Action::Redraw));
      }
      ```
  - 原因: 事件优先级为 `setup_wizard > thread_browser > cron > agent > relay > login > model > askuser > hitl > 正常输入`，login 在 model 之前

- [x] 修改 `event.rs` 中的 Paste 事件 — 新增 login panel 粘贴处理分支
  - 位置: `rust-agent-tui/src/event.rs`，`Event::Paste` 分支（L364-392）
  - 关键逻辑:
    - 在 `// model_panel 打开时粘贴到面板当前字段`（L376-380）之前新增 login_panel 粘贴分支：
      ```rust
      // login_panel 打开时粘贴到面板当前字段
      if app.core.login_panel.is_some() {
          app.core.login_panel.as_mut().unwrap().paste_text(&text);
          return Ok(Some(Action::Redraw));
      }
      ```
  - 原因: Login 面板需要处理 `Event::Paste`（独立于 Key 事件链），与 model_panel 的 Paste 处理方式一致；login_panel 在 model_panel 之前检查

- [x] 为 `panel_ops` 中的 Login 面板新方法编写单元测试
  - 测试文件: `rust-agent-tui/src/app/panel_ops.rs` 的 `#[cfg(test)] mod tests` 块（如不存在则新建）
  - 测试场景:
    - `test_open_login_panel_closes_model_panel`: 先调用 `open_model_panel()`，再调用 `open_login_panel()`，验证 `core.model_panel == None` 且 `core.login_panel.is_some()`
    - `test_open_model_panel_closes_login_panel`: 先调用 `open_login_panel()`，再调用 `open_model_panel()`，验证 `core.login_panel == None` 且 `core.model_panel.is_some()`
    - `test_close_login_panel`: 调用 `open_login_panel()` 再 `close_login_panel()`，验证 `core.login_panel == None`
    - `test_login_panel_apply_edit_saves_config`: 构造 `LoginPanel`（New 模式，填入 `buf_name`/`buf_api_key`/`buf_opus_model` 等），调用 `login_panel_apply_edit()`，验证 `zen_config` 中新增了 Provider 且 `provider_name`/`model_name` 已更新
    - `test_login_panel_confirm_delete_removes_provider`: 构造有 2 个 Provider 的配置，打开 login 面板，cursor 定位到第二个，调用 `login_panel_confirm_delete()`，验证 `zen_config` 中只剩 1 个 Provider
  - 运行命令: `cargo test -p rust-agent-tui --lib -- app::panel_ops::tests`
  - 预期: 所有测试通过

**检查步骤:**
- [x] 验证 `panels/login.rs` 文件存在且导出 `render_login_panel`
  - `grep -n 'pub(crate) fn render_login_panel' rust-agent-tui/src/ui/main_ui/panels/login.rs`
  - 预期: 输出 1 行
- [x] 验证 `panels/mod.rs` 已注册 login 模块
  - `grep -n 'pub mod login' rust-agent-tui/src/ui/main_ui/panels/mod.rs`
  - 预期: 输出 1 行
- [x] 验证 `panels/model.rs` 使用新枚举（`ModelFocusArea`/`ModelPanelMode`），不引用旧枚举
  - `grep -n 'AliasEditField\|EditField\|PROVIDER_TYPES' rust-agent-tui/src/ui/main_ui/panels/model.rs`
  - 预期: 无输出
  - `grep -n 'ModelFocusArea\|ModelPanelMode' rust-agent-tui/src/ui/main_ui/panels/model.rs`
  - 预期: 输出包含这两个类型
- [x] 验证 `main_ui.rs` 中新增了 login_panel 渲染分支和高度计算
  - `grep -n 'login_panel' rust-agent-tui/src/ui/main_ui.rs`
  - 预期: 输出包含 `login_panel.is_some()` 的渲染分支和高度计算分支
- [x] 验证 `event.rs` 中新增了 `handle_login_panel` 函数和分发分支
  - `grep -n 'fn handle_login_panel\|handle_login_panel(app' rust-agent-tui/src/event.rs`
  - 预期: 输出 2 行（函数定义和调用）
- [x] 验证 `event.rs` 中 Paste 事件处理新增了 login_panel 分支
  - `grep -n 'login_panel.*paste_text\|login_panel.*Paste' rust-agent-tui/src/event.rs`
  - 预期: 输出包含 login_panel 粘贴处理
- [x] 验证 `event.rs` 中旧 `handle_model_panel` 不再引用 `AliasEditField`/`EditField`
  - `grep -n 'AliasEditField\|EditField' rust-agent-tui/src/event.rs`
  - 预期: 无输出
- [x] 验证 `app/core.rs` 和 `app/mod.rs` 中新增了 `login_panel` 字段
  - `grep -n 'login_panel' rust-agent-tui/src/app/core.rs rust-agent-tui/src/app/mod.rs`
  - 预期: `core.rs` 中包含 `login_panel` struct 字段声明和初始化，`mod.rs` 中包含 `pub mod login_panel` 和 `pub use login_panel::LoginPanel`
- [x] 验证 `panel_ops.rs` 中新增了 Login 面板操作方法
  - `grep -n 'fn open_login_panel\|fn close_login_panel\|fn login_panel_apply_edit\|fn login_panel_confirm_delete' rust-agent-tui/src/app/panel_ops.rs`
  - 预期: 输出 4 行
- [x] 验证 `open_model_panel` 中互斥关闭 login_panel
  - `grep -A5 'fn open_model_panel' rust-agent-tui/src/app/panel_ops.rs | grep 'login_panel'`
  - 预期: 输出包含 `self.core.login_panel = None`
- [x] 验证 `open_login_panel` 中互斥关闭 model_panel
  - `grep -A5 'fn open_login_panel' rust-agent-tui/src/app/panel_ops.rs | grep 'model_panel'`
  - 预期: 输出包含 `self.core.model_panel = None`
- [x] 运行 `panel_ops` 单元测试
  - `cargo test -p rust-agent-tui --lib -- app::panel_ops::tests`
  - 预期: 全部测试通过
- [x] 编译整个 crate
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 编译成功，无错误

---

### Task 5: 外部引用适配

**背景:**
Task 1 将 `model_aliases`（`ModelAliasMap`/`ModelAliasConfig`）替换为 `ProviderModels` + `active_provider_id`，Task 2-4 完成了 Login/Model 面板重构。本 Task 负责将 `setup_wizard.rs`、`status_bar.rs`、`headless.rs` 中仍引用旧 `model_aliases` 数据结构的代码适配为新数据模型，确保整个 crate 编译通过且所有测试通过。本 Task 不被其他 Task 依赖，是最终集成的收尾工作。

**涉及文件:**
- 修改: `rust-agent-tui/src/app/setup_wizard.rs`
- 修改: `rust-agent-tui/src/ui/main_ui/status_bar.rs`
- 修改: `rust-agent-tui/src/ui/headless.rs`
- 修改: `CLAUDE.md`（项目根目录，更新 TUI 命令列表）
- 确认: `rust-agent-tui/src/command/help.rs`（无需修改）

**执行步骤:**

- [x] 确认 `help.rs` 无需修改 — `/help` 输出已通过命令注册自动包含 `/login`
  - 位置: `rust-agent-tui/src/command/help.rs`（整个文件）
  - 关键逻辑: `HelpCommand::execute` 通过 `app.core.command_help_list` 显示所有命令，该列表在 `AppCore::new()` 时从 `command_registry.list()` 预计算。Task 2 已在 `default_registry()` 中注册 `LoginCommand`，因此 `/help` 输出自动包含 `/login`。无需修改此文件。
  - 原因: `help.rs` 不引用 `model_aliases`，仅依赖 `CommandRegistry` 的运行时列表

- [x] 修改 `CLAUDE.md` — 更新 TUI 命令列表
  - 位置: `CLAUDE.md`（项目根目录），TUI 命令表（L317-326）
  - 关键逻辑:
    - 在 `/model` 行之前新增一行：
      ```
      | `/login` | 管理 Provider 配置（新建/编辑/删除），表单包含 API Key/Base URL/三级别模型名 |
      ```
    - 将 `/model` 行的说明从"打开 Provider/Model 配置面板（AliasConfig/Browse/Edit/New/Delete）"改为"打开模型选择面板（Provider 选择 + 级别切换 + Thinking 配置）"
  - 原因: `CLAUDE.md` 是项目的 AI 辅助开发指引，命令列表需与实际注册的命令一致

- [x] 修改 `setup_wizard.rs` 的 `save_setup_to` 函数 — 使用 `ProviderModels` 和 `active_provider_id` 替代 `model_aliases`
  - 位置: `rust-agent-tui/src/app/setup_wizard.rs`，`save_setup_to` 函数（L408-440）
  - 关键逻辑: 将 L417-432 替换为：
    ```rust
    let provider = crate::config::types::ProviderConfig {
        id: wizard.provider_id.clone(),
        provider_type: provider_type_str.to_string(),
        api_key: wizard.api_key.clone(),
        base_url: wizard.base_url.clone(),
        models: crate::config::types::ProviderModels {
            opus: wizard.aliases[0].model_id.clone(),
            sonnet: wizard.aliases[1].model_id.clone(),
            haiku: wizard.aliases[2].model_id.clone(),
        },
        ..Default::default()
    };
    cfg.config.providers.push(provider);
    cfg.config.active_alias = "opus".to_string();
    cfg.config.active_provider_id = wizard.provider_id.clone();
    ```
    - 删除 L426-432 中所有 `cfg.config.model_aliases.*` 赋值（7 行）
    - 在 `ProviderConfig` 构造中新增 `models: ProviderModels { opus, sonnet, haiku }` 字段
    - 新增 `cfg.config.active_provider_id = wizard.provider_id.clone();`
  - 原因: `ProviderModels` 内聚到 `ProviderConfig` 中，`active_provider_id` 直接指向 Provider，消除 `model_aliases` 的间接映射

- [x] 修改 `setup_wizard.rs` 的 `save_setup` 函数 — 移除 `model_aliases` 合并逻辑
  - 位置: `rust-agent-tui/src/app/setup_wizard.rs`，`save_setup` 函数（L443-462）
  - 关键逻辑: 将 L455-456 的 `merged.config.model_aliases = cfg.config.model_aliases;` 替换为：
    ```rust
    merged.config.active_provider_id = cfg.config.active_provider_id;
    ```
  - 原因: 合并逻辑需同步使用 `active_provider_id` 替代 `model_aliases`；`active_alias` 的合并（L455）保留不变

- [x] 修改 `setup_wizard.rs` 中引用 `model_aliases` 的测试断言 — 更新 `test_save_setup_creates_valid_config`
  - 位置: `rust-agent-tui/src/app/setup_wizard.rs`，`test_save_setup_creates_valid_config`（L720-736）
  - 关键逻辑: 将 L732-733 的：
    ```rust
    assert_eq!(cfg.config.model_aliases.opus.provider_id, "anthropic");
    assert!(cfg.config.model_aliases.opus.model_id.contains("claude-opus"));
    ```
    替换为：
    ```rust
    assert_eq!(cfg.config.active_provider_id, "anthropic");
    assert!(cfg.config.providers[0].models.opus.contains("claude-opus"));
    ```
  - 原因: 测试断言需验证新数据结构（`active_provider_id` + `ProviderModels`）而非旧 `model_aliases`

- [x] 修改 `status_bar.rs` 的模型信息显示 — 从 `active_provider_id` + `ProviderModels` 解析
  - 位置: `rust-agent-tui/src/ui/main_ui/status_bar.rs`，模型信息渲染块（L43-61）
  - 关键逻辑: 将 L44-56 的整个 `let alias_display = ...` 块替换为：
    ```rust
    let alias_display = app.zen_config.as_ref().map(|c| {
        let alias = &c.config.active_alias;
        let alias_cap = alias.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()
            + &alias[alias.char_indices().nth(1).map(|(i,_)|i).unwrap_or(alias.len())..];
        let provider = c.config.providers.iter().find(|p| p.id == c.config.active_provider_id);
        let model_name = provider
            .and_then(|p| p.models.get_model(alias).map(|m| m.to_string()))
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| app.model_name.clone());
        let provider_display = provider
            .map(|p| p.display_name().to_string())
            .unwrap_or_else(|| app.provider_name.clone());
        format!("★{} → {}/{}", alias_cap, provider_display, model_name)
    }).unwrap_or_else(|| format!(" {} {}", app.provider_name, app.model_name));
    ```
  - 原因: 模型解析路径从 `c.config.model_aliases.{alias}.provider_id/model_id` 改为 `c.config.active_provider_id → providers.find → models.get_model(alias)`；显示格式从 `★Alias → provider_id/model_id` 改为 `★Alias → display_name/model_name`

- [x] 修改 `headless.rs` 中引用 `model_aliases` 的测试断言 — 更新 `setup_wizard_e2e` 模块的测试
  - 位置: `rust-agent-tui/src/ui/headless.rs`，`setup_wizard_e2e` 模块
  - 关键逻辑:
    - `test_setup_wizard_full_flow_anthropic`（L1025-1026）：将：
      ```rust
      assert_eq!(cfg.config.model_aliases.opus.provider_id, "anthropic");
      assert!(cfg.config.model_aliases.opus.model_id.contains("claude-opus"));
      ```
      替换为：
      ```rust
      assert_eq!(cfg.config.active_provider_id, "anthropic");
      assert!(cfg.config.providers[0].models.opus.contains("claude-opus"));
      ```
    - `test_setup_wizard_full_flow_openai`（L1080）：将：
      ```rust
      assert_eq!(cfg.config.model_aliases.opus.model_id, "o3");
      ```
      替换为：
      ```rust
      assert_eq!(cfg.config.providers[0].models.opus, "o3");
      ```
  - 原因: headless 集成测试通过 `save_setup_to` 间接验证配置生成，断言需验证新数据结构

- [x] 为 `save_setup_to` 新逻辑编写单元测试
  - 测试文件: `rust-agent-tui/src/app/setup_wizard.rs` 的 `#[cfg(test)] mod tests` 块
  - 测试场景:
    - `test_save_setup_creates_valid_config`: 更新已有测试（L720-736），验证 `save_setup_to` 生成的配置包含 `active_provider_id` 和 `ProviderModels`（已在前面步骤更新）
    - `test_save_setup_to_sets_active_provider_id`: 构造 wizard，调用 `save_setup_to`，验证 `cfg.config.active_provider_id == wizard.provider_id`
    - `test_save_setup_to_sets_provider_models`: 构造 wizard（Anthropic），调用 `save_setup_to`，验证 `cfg.config.providers[0].models.opus/sonnet/haiku` 与 `wizard.aliases[0/1/2].model_id` 一致
    - `test_save_setup_to_openai_models`: 构造 wizard（OpenAiCompatible），调用 `save_setup_to`，验证 `cfg.config.providers[0].models.opus == "o3"`
  - 运行命令: `cargo test -p rust-agent-tui --lib -- app::setup_wizard::tests`
  - 预期: 所有测试通过

- [x] 运行全量编译和测试 — 确认整个 crate 编译通过且所有测试通过
  - 位置: 项目根目录
  - 关键逻辑:
    - 运行 `cargo build -p rust-agent-tui 2>&1`，确认无编译错误
    - 运行 `cargo test -p rust-agent-tui --lib 2>&1`，确认所有测试通过
  - 原因: Task 5 是最终集成的收尾工作，需确认所有旧 `model_aliases` 引用已完全清除

**检查步骤:**
- [x] 验证 `setup_wizard.rs` 不再引用 `model_aliases`
  - `grep -n 'model_aliases' rust-agent-tui/src/app/setup_wizard.rs`
  - 预期: 无输出
- [x] 验证 `setup_wizard.rs` 引用 `ProviderModels` 和 `active_provider_id`
  - `grep -n 'ProviderModels\|active_provider_id' rust-agent-tui/src/app/setup_wizard.rs`
  - 预期: `save_setup_to` 中包含 `ProviderModels` 构造和 `active_provider_id` 赋值，`save_setup` 中包含 `active_provider_id` 合并
- [x] 验证 `status_bar.rs` 不再引用 `model_aliases`
  - `grep -n 'model_aliases' rust-agent-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 无输出
- [x] 验证 `status_bar.rs` 使用 `active_provider_id` + `get_model` 解析模型
  - `grep -n 'active_provider_id\|get_model' rust-agent-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 输出包含 `active_provider_id` 查找和 `get_model` 调用
- [x] 验证 `headless.rs` 不再引用 `model_aliases`
  - `grep -n 'model_aliases' rust-agent-tui/src/ui/headless.rs`
  - 预期: 无输出
- [x] 验证 `help.rs` 不引用 `model_aliases`（无需修改确认）
  - `grep -n 'model_aliases' rust-agent-tui/src/command/help.rs`
  - 预期: 无输出
- [x] 验证 `CLAUDE.md` 包含 `/login` 命令且 `/model` 描述已更新
  - `grep -n '/login\|/model' CLAUDE.md`
  - 预期: 输出包含 `/login` 行和 `/model` 行（描述为"模型选择面板"而非"Provider/Model 配置面板"）
- [x] 全局验证：整个 crate 无 `model_aliases` 引用（排除 spec/ 目录）
  - `grep -rn 'model_aliases' rust-agent-tui/src/`
  - 预期: 无输出
- [x] 运行 setup_wizard 单元测试
  - `cargo test -p rust-agent-tui --lib -- app::setup_wizard::tests`
  - 预期: 全部测试通过
- [x] 运行 headless 集成测试
  - `cargo test -p rust-agent-tui --lib -- ui::headless::tests`
  - 预期: 全部测试通过
- [x] 运行全量单元测试
  - `cargo test -p rust-agent-tui --lib 2>&1 | tail -10`
  - 预期: 全部测试通过
- [x] 编译整个 crate
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 编译成功，无错误

---

### Task 6: Model Config 重构 验收

**前置条件:**
- 启动命令: `cargo run -p rust-agent-tui`
- 所有 Task 1-5 已执行完毕
- 配置文件 `~/.zen-code/settings.json` 使用新格式

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test -p rust-agent-tui --lib 2>&1 | tail -20`
   - 预期: 全部测试通过
   - 失败排查: 检查各 Task 的测试步骤，逐个 Task 执行 `cargo test -p rust-agent-tui --lib -- 模块名::tests`

2. 验证 ProviderConfig 包含 models 字段且序列化/反序列化正确
   - `cargo test -p rust-agent-tui --lib -- config::types::tests 2>&1 | tail -10`
   - 预期: 所有 config::types 测试通过（包含 ProviderModels 相关测试）
   - 失败排查: 检查 Task 1 中 ProviderModels 定义和 serde 属性

3. 验证 AppConfig 移除 model_aliases 后旧格式不崩溃
   - `cargo test -p rust-agent-tui --lib -- config::store::tests 2>&1 | tail -5`
   - 预期: 配置加载测试通过，旧字段 `model_aliases` 被 `extra` 静默吸收
   - 失败排查: 检查 Task 1 中 AppConfig 的 `#[serde(flatten)] extra` 是否正确保留

4. 验证 LlmProvider::from_config 按 active_provider_id + ProviderModels 解析
   - `cargo test -p rust-agent-tui --lib -- app::provider::tests 2>&1 | tail -10`
   - 预期: 所有 provider 解析测试通过
   - 失败排查: 检查 Task 1 中 from_config/from_config_for_alias 的重写

5. 验证 /login 命令注册且 /help 中可见
   - 启动 TUI，输入 `/help`
   - 预期: 输出中包含 `/login` 命令行
   - 失败排查: 检查 Task 2 中 command/mod.rs 的注册

6. 验证 /login 面板能新建/编辑/删除 Provider
   - 启动 TUI，输入 `/login`，按 `n` 新建 Provider，填写 Name/Type/ApiKey/BaseUrl/三模型名，按 Enter 保存
   - 预期: Provider 列表中出现新建的 Provider，配置文件更新
   - 失败排查: 检查 Task 2（LoginPanel 状态机）和 Task 4（UI 渲染 + 事件处理）

7. 验证 Type 切换时自动填充模型名默认值
   - 在 /login 编辑模式，光标在 Type 字段按 Space 切换 provider_type
   - 预期: 三个模型名字段自动更新为对应 provider_type 的默认值（anthropic → claude-xxx，openai → gpt-xxx）
   - 失败排查: 检查 Task 2 中 LoginPanel 的 auto_fill_models_for_type 方法和 DEFAULT_MODELS 常量

8. 验证 /model 面板简化为 Provider 选择 + 级别切换 + Thinking
   - 输入 `/model`，验证显示：Provider 列表、级别切换栏 `[★ Opus] [ Sonnet ] [ Haiku ]`、Thinking 配置区
   - 按 ←→ 切换级别，按 Enter 选择 Provider，按 Tab 切换焦点区域
   - 预期: 面板仅包含三个区域，无 Provider CRUD 操作
   - 失败排查: 检查 Task 3（ModelPanel 简化）和 Task 4（UI 渲染）

9. 验证 /model <alias> 快捷切换保留正常工作
   - 输入 `/model sonnet`
   - 预期: active_alias 切换为 "sonnet"，状态栏显示对应模型名
   - 失败排查: 检查 command/model.rs 的 execute 方法是否正确更新 active_alias

10. 验证 Login/Model 面板互斥
    - 打开 `/model` 面板，然后输入 `/login`
    - 预期: model 面板关闭，login 面板打开
    - 失败排查: 检查 Task 4 中 panel_ops.rs 的面板互斥逻辑

11. 验证 setup wizard 输出新格式配置
    - `cargo test -p rust-agent-tui --lib -- app::setup_wizard::tests 2>&1 | tail -10`
    - 预期: 所有 setup_wizard 测试通过，配置包含 `active_provider_id` + `ProviderModels`
    - 失败排查: 检查 Task 5 中 setup_wizard.rs 的 save_setup_to 适配

12. 验证状态栏模型信息显示正确
    - 启动 TUI，配置好 Provider 后观察状态栏
    - 预期: 显示 `★Opus → ProviderName/claude-opus-4-7` 格式
    - 失败排查: 检查 Task 5 中 status_bar.rs 的模型信息解析
