# Feature: 20260330_F005 - tui-setup-wizard

## 需求背景

用户首次安装 Perihelion 后，需要手动编辑 `~/.zen-code/settings.json` 或设置环境变量来配置 Provider、API Key 和模型别名，门槛较高。缺少引导流程导致新用户启动 TUI 后无法立即使用。

## 目标

- 首次启动 TUI 时自动检测配置完整性，未配置则弹出全屏向导引导用户完成 Provider / API Key / 模型别名配置
- 配置完成后写入 `~/.zen-code/settings.json`，提示确认后进入正常对话界面
- 与现有面板模式一致（独立 SetupWizardPanel），不侵入日常使用的 `/model` 面板

## 方案设计

### 触发机制

App 初始化时（`App::new()` 或 `main.rs` terminal setup 之后），检查 `~/.zen-code/settings.json` 的配置完整性：

```rust
fn needs_setup(config: &AppConfig) -> bool {
    // 条件 1：无任何 Provider
    if config.providers.is_empty() {
        return true;
    }
    // 条件 2：有 Provider 但 API Key 缺失（key 为空且对应环境变量也未设置）
    for provider in &config.providers {
        let key_env = match provider.provider_id.as_str() {
            "anthropic" => "ANTHROPIC_API_KEY",
            _ => "OPENAI_API_KEY",
        };
        if provider.api_key.as_deref().unwrap_or("").is_empty()
            && std::env::var(key_env).unwrap_or_default().is_empty()
        {
            return true;
        }
    }
    false
}
```

满足任一条件 → `app.setup_wizard = Some(SetupWizardPanel::new())`，主循环优先渲染 setup 弹窗而非正常对话界面。

### 向导步骤

![向导流程](./images/01-flow.png)

```
Step 1: Provider 选择
  ↓ Enter
Step 2: API Key 输入
  ↓ Enter
Step 3: 模型别名配置（opus / sonnet / haiku）
  ↓ Enter
完成 → 写入 settings.json → 显示确认页 → Enter 进入对话
```

#### Step 1：Provider 选择

![Step 1 线框图](./images/02-wireframe-step1.png)

**UI 元素**：
- 标题：`── Perihelion Setup ── Step 1/3: Provider`
- Provider 类型下拉：`Anthropic` / `OpenAI Compatible`（↑↓ 选择，默认 Anthropic）
- Provider ID 输入框（默认 `anthropic` / `openai`，可自定义）
- Base URL 输入框：
  - Anthropic 模式：只读显示 `https://api.anthropic.com`
  - OpenAI Compatible 模式：可编辑，默认 `https://api.openai.com/v1`
- 底部提示：`Enter 下一步 · Esc 跳过 setup · Tab 切换字段`

**按键处理**：
- Tab：在 Provider 类型 / Provider ID / Base URL 字段间切换
- ↑↓：Provider 类型下拉选择
- Enter：校验非空后进入 Step 2
- Esc：弹窗确认「跳过 setup 将无法使用 AI 功能」，确认则 `setup_wizard = None`

#### Step 2：API Key 输入

![Step 2 线框图](./images/03-wireframe-step2.png)

**UI 元素**：
- 标题：`── Step 2/3: API Key`
- 显示当前 Provider 名称
- API Key 输入框（密码模式，输入显示 `••••••••`）
- 底部提示：`Enter 下一步 · Esc 返回上一步`

**按键处理**：
- Enter：校验非空后进入 Step 3
- Esc：返回 Step 1（保留已填内容）

#### Step 3：模型别名配置

![Step 3 线框图](./images/04-wireframe-step3.png)

**UI 元素**：
- 标题：`── Step 3/3: Model Aliases`
- 三行配置，每行含两个输入框：
  - `Opus  > Provider: [下拉]  Model: [输入框]`
  - `Sonnet > Provider: [下拉]  Model: [输入框]`
  - `Haiku  > Provider: [下拉]  Model: [输入框]`
- Provider 下拉：从已配置的 provider 列表选择（当前只有 Step 1 配置的那一个）
- Model ID 默认值：
  - Anthropic：`claude-opus-4-0-20250514` / `claude-sonnet-4-6-20250514` / `claude-haiku-3-5-20241022`
  - OpenAI Compatible：`o3` / `gpt-4o` / `gpt-4o-mini`
- 底部提示：`Enter 完成配置 · Esc 返回上一步 · Tab 切换字段`

**按键处理**：
- Tab：在 6 个字段（3×provider + 3×model_id）间按行切换
- ↑↓：Provider 下拉选择
- Enter：校验三个 model_id 非空 → 执行保存

#### 完成页

**UI 元素**：
- 标题：`── Setup Complete ✓`
- 摘要：列出已配置的 Provider 和三个模型别名
- 底部提示：`按 Enter 开始使用`

### 数据模型

```rust
// app/setup_wizard.rs

#[derive(Clone, Copy, PartialEq)]
enum SetupStep {
    Provider,
    ApiKey,
    ModelAlias,
    Done,
}

#[derive(Clone, Copy, PartialEq)]
enum ProviderType {
    Anthropic,
    OpenAiCompatible,
}

#[derive(Clone)]
struct AliasConfig {
    provider_index: usize,  // 当前 wizard 中 provider 列表索引
    model_id: String,
}

struct SetupWizardPanel {
    step: SetupStep,
    // Step 1
    provider_type: ProviderType,
    provider_id: String,
    base_url: String,
    // Step 2
    api_key: String,
    // Step 3
    aliases: [AliasConfig; 3],  // opus, sonnet, haiku
    // 导航
    focus_index: usize,
    // textarea（复用 ratatui-textarea）
    textareas: Vec<Textarea<'static>>,
}
```

### 持久化

Setup 完成后，统一写入 `~/.zen-code/settings.json`：

```rust
fn save_setup(config: &SetupWizardPanel) -> anyhow::Result<()> {
    let mut app_config = AppConfig::load().unwrap_or_default();
    
    // 新增或更新 provider
    let provider = ProviderConfig {
        provider_id: config.provider_id.clone(),
        base_url: Some(config.base_url.clone()),
        api_key: Some(config.api_key.clone()),
    };
    app_config.providers.push(provider);
    
    // 设置模型别名
    app_config.model_aliases = ModelAliasMap {
        opus: ModelAliasConfig {
            provider_id: config.provider_id.clone(),
            model_id: config.aliases[0].model_id.clone(),
        },
        sonnet: ModelAliasConfig {
            provider_id: config.provider_id.clone(),
            model_id: config.aliases[1].model_id.clone(),
        },
        haiku: ModelAliasConfig {
            provider_id: config.provider_id.clone(),
            model_id: config.aliases[2].model_id.clone(),
        },
    };
    
    app_config.save()?;
    Ok(())
}
```

写入后刷新内存中的 Provider 状态（`app.provider_manager.refresh()`），使后续 Agent 执行能立即使用新配置。

### UI 渲染集成

在 `main_ui::render()` 入口，优先检查 setup_wizard：

```rust
pub fn render(f: &mut Frame, app: &mut App) {
    if app.setup_wizard.is_some() {
        render_setup_wizard(f, app);
        return;  // 全屏覆盖，不渲染正常界面
    }
    // ... 原有渲染逻辑
}
```

渲染函数在 `ui/main_ui/popups/setup_wizard.rs`，根据 `step` 分发到对应的渲染子函数。

### 事件处理集成

在事件处理循环中（`handle_key_event`），优先检查 setup_wizard：

```rust
fn handle_key_event(app: &mut App, key: KeyEvent) -> Option<Action> {
    if let Some(ref mut wizard) = app.setup_wizard {
        return handle_setup_wizard_key(wizard, key, &mut app.provider_manager);
    }
    // ... 原有事件处理
}
```

## 实现要点

- **Textarea 复用**：输入框复用 `ratatui-textarea` crate（已存在于依赖），每个输入字段对应一个 Textarea 实例，Tab/Shift+Tab 切换焦点
- **密码输入**：Step 2 的 API Key 输入使用 textarea 的 `mask` 功能（如不支持则手动在渲染时替换为 `•` 字符）
- **Provider 类型默认值**：选择 Anthropic 时自动填充 provider_id 和 base_url，切回 OpenAI Compatible 时清空 base_url 让用户填写
- **首次启动检测**：在 `App::new()` 之后、主循环之前调用 `needs_setup()`，避免在构造函数中做 IO
- **跳过 setup 的二次确认**：Esc 跳过时需在 setup 弹窗内显示确认提示，避免误操作
- **配置即时生效**：保存后调用 `provider_manager.refresh()` 刷新内存状态，无需重启

## 约束一致性

- **面板模式一致**：SetupWizardPanel 与 ModelPanel / RelayPanel 模式一致，独立状态 + 渲染函数，不侵入现有逻辑
- **配置持久化**：统一写入 `~/.zen-code/settings.json`（通过 `AppConfig`），与现有配置管理一致
- **事件驱动**：按键处理在主循环事件分发中优先拦截，与 HITL/AskUser 弹窗的拦截模式一致
- **不引入新依赖**：复用已有的 ratatui-textarea、AppConfig、ProviderConfig 等现有组件
- **全屏覆盖**：setup 期间完全替换主界面渲染，与弹窗面板的模式一致但覆盖全屏

## 验收标准

- [ ] 首次启动（无 settings.json 或 providers 为空）自动弹出 setup 向导
- [ ] Provider 有配置但 API Key 缺失时也触发 setup 向导
- [ ] 三步流程：Provider 选择 → API Key → 模型别名，Tab/Enter/Esc 导航正常
- [ ] Anthropic 选项自动填充 base_url，OpenAI Compatible 需手动填写
- [ ] API Key 输入以掩码显示
- [ ] 三个模型别名必填，有对应 Provider 的合理默认值
- [ ] 完成后配置写入 settings.json，内存状态即时刷新
- [ ] 跳过 setup 时二次确认
- [ ] 已完成 setup 后再次启动不再触发（检测到配置完整）
- [ ] Headless 测试模式下 setup 向导可通过代码驱动完成
