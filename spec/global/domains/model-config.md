# 模型配置 领域

## 领域综述

模型配置领域负责 Provider 和模型的管理，包括 Provider 的 CRUD 操作、三级别模型名（opus/sonnet/haiku）内聚配置、/login 与 /model 面板的职责分离。

核心职责：
- ProviderConfig 自包含 ProviderModels 字段，每个 Provider 独立管理三个模型名
- active_provider_id + active_alias 直接解析，移除 ModelAliasMap 间接映射
- /login 面板负责 Provider CRUD（新建/编辑/删除），/model 面板仅负责选择 + Thinking 配置
- Type 切换时自动填充对应 provider_type 的默认模型名

## 核心流程

### Provider 配置管理

```
/login 面板（Browse 模式）
  → 列出所有 Provider
  → Enter 进入编辑 / Space 激活
  → 编辑模式: 7 个字段（Name/Type/BaseUrl/ApiKey/OpusModel/SonnetModel/HaikuModel）
  → Type 切换 → 自动填充默认模型名
  → Enter 保存 → settings.json 原子写回
```

### 模型选择流程

```
/model 面板
  → 选择 Provider → 选择 Alias（opus/sonnet/haiku）
  → 解析: active_provider_id + active_alias → ProviderConfig.models.{alias}
  → Thinking 配置（budget_tokens）
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 配置存储 | ~/.zen-code/settings.json，AppConfig 统一读写 |
| Provider 数据结构 | ProviderConfig { id, type, baseUrl, apiKey, models: ProviderModels } |
| 别名解析 | active_provider_id + active_alias 直接定位，无间接映射 |
| 默认模型名 | DEFAULT_MODELS 常量表（anthropic → claude-sonnet-4-6, openai → gpt-4o） |
| 面板设计 | LoginPanel（Browse/Edit/New/ConfirmDelete）与 ModelPanel 互斥 |
| 向后兼容 | model_aliases 被 serde 安全忽略 |

## Feature 附录

### feature_20260427_F003_model-config-refactor
**摘要:** Provider 自包含三级别模型名，/login 与 /model 职责分离
**关键决策:**
- ProviderConfig 新增 ProviderModels 字段，opus/sonnet/haiku 模型名内聚到 Provider
- 移除 ModelAliasMap 间接映射，改为 active_provider_id + active_alias 直接解析
- /login 负责 Provider CRUD，/model 仅负责选择 + Thinking
- Type 切换时自动填充对应 provider_type 的默认模型名
- 旧配置格式不兼容，model_aliases 被 serde 安全忽略
- LoginPanel 与 ModelPanel 互斥，同一时间只能打开一个配置面板
**归档:** [链接](../../archive/feature_20260427_F003_model-config-refactor/)
**归档日期:** 2026-04-30

---

## 相关 Feature
