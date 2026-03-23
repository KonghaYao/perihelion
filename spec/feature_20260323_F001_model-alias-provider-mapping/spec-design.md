# Feature: 20260323_F001 - model-alias-provider-mapping

## 需求背景

当前系统的模型选择方案是「选定一个 Provider → 在该 Provider 下填写 model_id 字符串」。这导致：

1. 用户切换不同 Provider 的同等级模型时（如从 Anthropic 的 claude-opus-4-6 切换到 OpenRouter 的 gpt-5.4），需要手动记忆各家命名规范
2. 系统缺乏统一的模型能力层级抽象，Agent 定义中无法用「我需要一个高智能模型」来表达意图
3. `AppConfig` 中的 `provider_id + model_id` 仅能指向单一 Provider 下的单一模型，无法同时维护多个层级的模型配置

## 目标

- 引入 **Opus / Sonnet / Haiku** 三个固定模型层级别名，对应「最强 / 均衡 / 快速」三档能力定位
- 每个别名**独立绑定** `(provider_id, model_id)`，可以指向任意不同的 Provider
- Agent 侧只感知别名（如 "opus"），TUI 负责将别名解析为具体的 Provider + Model 发起调用
- `/model` 面板重构为三栏式，分别配置三个别名的映射关系

## 方案设计

### 数据模型

新增 `ModelAliasConfig` 和 `ModelAliasMap` 两个结构：

```rust
/// 单个别名的目标绑定
pub struct ModelAliasConfig {
    pub provider_id: String,  // 对应 providers 列表中的 ProviderConfig.id
    pub model_id: String,     // 具体 model 名称，如 "gpt-5.4" 或 "claude-opus-4-6"
}

/// 三级别名映射表
pub struct ModelAliasMap {
    pub opus: ModelAliasConfig,
    pub sonnet: ModelAliasConfig,
    pub haiku: ModelAliasConfig,
}
```

`AppConfig` 变更：

| 旧字段 | 新字段 | 说明 |
|--------|--------|------|
| `provider_id: String` | 删除 | 移入 alias 配置 |
| `model_id: String` | 删除 | 移入 alias 配置 |
| _(新增)_ | `active_alias: String` | 当前激活的别名（"opus" \| "sonnet" \| "haiku"） |
| _(新增)_ | `model_aliases: ModelAliasMap` | 三个别名的完整绑定配置 |
| `providers: Vec<ProviderConfig>` | 保持不变 | 所有 provider 均可用 |

### settings.json 数据格式

```json
{
  "config": {
    "active_alias": "opus",
    "model_aliases": {
      "opus":   { "provider_id": "openrouter", "model_id": "gpt-5.4" },
      "sonnet": { "provider_id": "anthropic",  "model_id": "claude-sonnet-4-6" },
      "haiku":  { "provider_id": "anthropic",  "model_id": "claude-haiku-4-5-20251001" }
    },
    "providers": [
      { "id": "openrouter", "type": "openai", "apiKey": "...", "baseUrl": "https://openrouter.ai/api/v1" },
      { "id": "anthropic",  "type": "anthropic", "apiKey": "..." }
    ]
  }
}
```

### 向后兼容迁移

加载配置时，若检测到旧版字段（存在 `provider_id` 但不存在 `model_aliases`），自动迁移：
- 将旧 `provider_id + model_id` 填入 `opus` 别名
- `sonnet / haiku` 填入相同的 provider_id，model_id 留空（使用 Provider 的默认值）
- `active_alias` 设为 "opus"

### TUI 面板交互设计

`/model` 面板重构为两层结构：

**第一层（上方横向 Tab 栏）：**
- 显示三个别名 Tab：`[Opus]  [Sonnet]  [Haiku]`
- 当前选中 Tab 高亮显示（无星号标记）
- `Tab` / `Shift+Tab` 切换 Tab（向右/向左）
- `Enter` 将当前 Tab 设为激活别名（写入 `active_alias`）

**第二层（下方编辑区）：**
- 显示当前 Tab 别名的 `Provider` 和 `Model ID` 两个字段
- `Provider`：循环选择（`Space` 切换）——列表来自 `providers` 配置
- `Model ID`：手动输入字符串（所有字符均写入缓冲，不被快捷键拦截）
- `↑` / `↓` 在两个字段间切换
- `s` 或 `Enter` 保存当前 Tab 的配置

**另保留原有的 Provider 管理入口**（新建/编辑/删除 Provider），通过 `p` 键进入。

![TUI /model 面板线框图](./images/01-wireframe.png)

### LlmProvider 解析变更

`LlmProvider::from_config` 改为按 `active_alias` 查 `model_aliases` 表：

```rust
pub fn from_config(cfg: &ZenConfig) -> Option<Self> {
    let alias = &cfg.config.active_alias;  // "opus" | "sonnet" | "haiku"
    let mapping = cfg.config.model_aliases.get(alias)?;  // ModelAliasConfig
    let provider = cfg.config.providers.iter()
        .find(|p| p.id == mapping.provider_id)?;
    // ... 按 provider.provider_type 组装 LlmProvider
}
```

### 数据流

![模型别名解析数据流](./images/02-flow.png)

## 实现要点

1. **数据迁移**：在 `ZenConfig` 反序列化后的 post-process 中自动检测并迁移旧格式，避免用户配置文件失效
2. **ModelPanel 重构**：现有 `ModelPanel` 的状态机需要整体重写，增加 `active_tab: AliasTab`（Opus/Sonnet/Haiku）枚举，以及针对每个 tab 独立的 `buf_provider` / `buf_model`
3. **Provider 列表同步**：切换 Tab 时，Provider 选择项来自 `providers` 列表，动态渲染
4. **TUI 状态栏**：在状态栏显示当前激活别名及其绑定信息，如 `[Opus → openrouter/gpt-5.4]`
5. **空 model_id 处理**：若 alias 的 `model_id` 为空，按 provider 类型使用默认 model（Anthropic → claude-sonnet-4-6，OpenAI → gpt-4o），不 panic

## 约束一致性

（`spec/global/` 目录不存在，省略此章节）

## 验收标准

- [x] `settings.json` 中正确存储 `active_alias` 和 `model_aliases` 三组配置
- [x] `/model` 面板显示 Opus/Sonnet/Haiku 三个 Tab，每个 Tab 可独立选择 Provider 并输入 Model ID
- [x] `Enter` 可将某 Tab 设为当前激活别名（状态栏随之更新）
- [x] 旧格式配置（只有 `provider_id + model_id`）启动时自动迁移为新格式，不丢数据
- [x] 未配置 model_id 的别名回退到 Provider 默认 model，不 panic
- [x] Agent 调用时使用当前激活别名对应的 Provider + Model
- [x] Provider 管理功能（增删改）仍可从 `/model` 面板访问
- [x] `/model <alias>` 命令行直接切换激活别名（无需打开面板）
