# Feature: 20260329_F002 - subagent-model-switch

## 需求背景

当前 `agent.md` 的 `model` 字段（如 `model: haiku`）已被 `ClaudeAgentFrontmatter` 解析，但在 `SubAgentTool::invoke()` 中完全未被消费——所有子 Agent 始终通过 `llm_factory()` 创建与父 Agent 相同的 LLM。这导致无法为不同子 Agent 配置不同模型（如主 Agent 用 Opus、code-reviewer 用 Haiku），浪费了已定义的 model 字段。

同时，`SkillFrontmatter` 目前只有 `name` 和 `description`，没有 `model` 字段。虽然 Skill 不直接运行 Agent，但增加 `model` 字段可以作为文档提示，让用户和 AI 了解该 Skill 设计时的目标模型。

## 目标

- 让 `agent.md` 中的 `model: haiku/sonnet/opus/inherit` 字段真正生效，子 Agent 使用对应别名解析出的 provider
- `SKILL.md` 增加 `model` 字段（可选），仅作文档提示展示在 Skills 摘要中，不参与模型切换
- 复用现有的三级别名表（`ModelAliasMap`），alias 解析逻辑保持在 TUI 层

## 方案设计

### 核心思路：LLM Factory 签名升级

将 `SubAgentTool` 的 `llm_factory` 从 `Fn() -> Box<dyn ReactLLM>` 升级为 `Fn(Option<&str>) -> Box<dyn ReactLLM>`，接受可选的 model alias 参数。TUI 层的 factory 闭包内部持有 `ZenConfig`，根据 alias 查 `model_aliases` 表构造对应的 `LlmProvider`。

> 在适合的章节中插入设计配图：`![子 Agent 模型切换数据流](./images/01-data-flow.png)`

### LLM Factory 签名变更

**当前**（`subagent/tool.rs:30`）：
```rust
llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>
```

**改为**：
```rust
llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>
```

- 参数 `Option<&str>` 为 model alias（如 `"haiku"`、`"sonnet"`）
- 传入 `None` 时使用父 Agent 的模型（等价于 `inherit`）
- 传入 `Some("haiku")` 时由 factory 内部解析为对应的 `LlmProvider`

### SubAgentTool::invoke() 消费 model 字段

在 `tool.rs:191` 处，将：
```rust
let llm = (self.llm_factory)();
```
改为：
```rust
let model_alias = agent_def.frontmatter.model.as_deref()
    .filter(|m| !m.is_empty() && m != &"inherit");
let llm = (self.llm_factory)(model_alias);
```

`"inherit"` 和 `None`、空字符串均视为继承父 Agent 模型。

### TUI 层 Factory 构造（`agent.rs:149`）

将 `run_universal_agent()` 中的 `llm_factory` 升级为持有 `ZenConfig` 的闭包：

```rust
let config_for_factory = config.clone(); // 新增：传入 ZenConfig
let llm_factory = Arc::new(move |model_alias: Option<&str>| -> Box<dyn ReactLLM + Send + Sync> {
    match model_alias {
        Some(alias) => {
            if let Some(provider) = LlmProvider::from_config_for_alias(&config_for_factory, alias) {
                return Box::new(BaseModelReactLLM::new(provider.into_model()));
            }
            tracing::warn!("未知 model alias '{}', fallback 到父 agent 模型", alias);
        }
        None => {}
    }
    Box::new(BaseModelReactLLM::new(provider_clone.clone().into_model()))
});
```

需要在 `LlmProvider` 上新增 `from_config_for_alias(cfg, alias)` 方法，直接按指定 alias 查别名表，忽略 `active_alias`。

### SkillFrontmatter 增加 model 字段

`skills/loader.rs` 的 `SkillFrontmatter` 增加可选 `model` 字段：
```rust
struct SkillFrontmatter {
    name: String,
    description: String,
    model: Option<String>,  // 新增
}
```

`SkillMetadata` 也增加 `model: Option<String>`，并在 `SkillsMiddleware::build_summary()` 的摘要中展示：
```
- **code-review**: /path/to/SKILL.md Expert code review... (model: haiku)
```

### AgentRunConfig 扩展

当前 `run_universal_agent()` 接收 `AgentRunConfig`，不持有 `ZenConfig`。需要新增 `config: Arc<ZenConfig>` 字段，仅在构造 `llm_factory` 时使用。

### 改动文件清单

| 文件 | 改动类型 | 说明 |
|------|---------|------|
| `rust-agent-middlewares/src/subagent/tool.rs` | 修改 | `llm_factory` 签名改为 `Fn(Option<&str>)`，`invoke()` 传入 model alias |
| `rust-agent-middlewares/src/subagent/mod.rs` | 修改 | `SubAgentMiddleware` 构造函数的 `llm_factory` 签名同步更新 |
| `rust-agent-middlewares/src/skills/loader.rs` | 修改 | `SkillFrontmatter` 增加 `model` 字段，`SkillMetadata` 增加 `model` 字段 |
| `rust-agent-middlewares/src/skills/mod.rs` | 修改 | `build_summary()` 中展示 skill 的 model 信息 |
| `rust-agent-tui/src/app/agent.rs` | 修改 | `AgentRunConfig` 增加 `config` 字段，`llm_factory` 升级为 alias-aware |
| `rust-agent-tui/src/app/provider.rs` | 修改 | 新增 `LlmProvider::from_config_for_alias()` 方法 |
| `rust-agent-tui/src/app/agent_ops.rs` | 修改 | 构造 `AgentRunConfig` 时传入 `config` |

> 在适合的章节中插入设计配图：`![改动文件与依赖关系](./images/02-file-changes.png)`

### 数据流

```
agent.md 解析:
  parse_agent_file() → ClaudeAgent { frontmatter.model: Some("haiku"), ... }
                                  ↓
SubAgentTool::invoke():
  model_alias = frontmatter.model.filter(not "inherit")
                                  ↓
  llm = llm_factory(model_alias)  // Some("haiku")
                                  ↓
  TUI factory 闭包:
    LlmProvider::from_config_for_alias(&config, "haiku")
      → config.model_aliases.haiku → ModelAliasConfig { provider_id, model_id }
      → providers.find(id) → LlmProvider::Anthropic/OpenAi { model }
      → provider.into_model() → Box<dyn BaseModel>
                                  ↓
  ReActAgent::new(llm)  // 子 Agent 使用 haiku 对应的模型
```

## 实现要点

- **架构分层**：alias 解析逻辑完全在 TUI 层（`rust-agent-tui`），中间件层（`rust-agent-middlewares`）仅传递 `Option<&str>` 参数，不引入配置类型依赖
- **错误处理**：未知 alias 时 `warn!` 日志 + fallback 到父模型；alias 对应 provider 未配置时同理
- **Skill model**：仅解析存储、展示在摘要中，不影响模型选择逻辑
- **兼容性**：现有 `llm_factory` 的调用点（`SubAgentTool` 测试中的 `EchoLLM` 等）需同步更新签名为 `|_| Box::new(EchoLLM)`

## 约束一致性

- **架构分层**：alias 解析在 TUI 层，中间件层不引入配置依赖。符合 `spec/global/constraints.md` 中"禁止下层依赖上层"的约束
- **消息不可变历史**：model 切换发生在 Agent 构建阶段，不影响消息历史
- **异步优先**：factory 闭包为同步函数（创建 LLM 实例），不违反 async 约束
- **防递归**：`launch_agent` 仍被排除，model 切换不影响防递归逻辑

## 验收标准

- [ ] `agent.md` 中设置 `model: haiku` 时，子 Agent 使用 haiku 别名对应的 provider
- [ ] `agent.md` 中设置 `model: inherit` 或省略 model 时，子 Agent 继承父 Agent 模型
- [ ] `agent.md` 中设置未知 alias（如 `model: ultra`）时，warn 日志 + fallback 到父模型
- [ ] `SKILL.md` 可声明 `model` 字段，出现在 Skills 摘要中但不影响模型选择
- [ ] 所有现有测试通过
- [ ] 新增 `LlmProvider::from_config_for_alias()` 单元测试
- [ ] 新增 `SubAgentTool` model alias 传递的测试
