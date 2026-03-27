# Feature: 20260326_F005 - subagent-skill-preload

## 需求背景

当前 `SkillsMiddleware` 在子 agent 的 `before_agent` 钩子中只注入所有可用 skill 的**摘要**（名称 + 描述），LLM 需要"主动感知"并在消息中提及对应 skill 名称才能获取全文。在子 agent 场景下，父 agent 往往已经明确知道子 agent 需要哪些 skill，但缺少在 agent 定义层面声明并自动注入的机制，导致子 agent 无法在第一轮推理就利用完整的 skill 知识。

## 目标

- 在 `.claude/agents/{id}.md` frontmatter 中支持 `skills` 字段，声明该子 agent 需要预加载的 skill 名称列表
- 子 agent 启动时，自动将声明的 skill SKILL.md 全文以 **fake `read_file` 工具调用 + 结果消息对** 的形式注入到 state 中
- LLM 从第一轮推理开始就能看到完整 skill 内容，无需额外提示

## 方案设计

### Agent 定义文件格式扩展

在 `.claude/agents/{id}.md` 的 YAML frontmatter 中新增可选 `skills` 字段：

```yaml
---
name: api-developer
description: Implement API endpoints following team conventions
skills:
  - api-conventions
  - error-handling-patterns
---
```

对应 `AgentFrontmatter` 结构体（`claude_agent_parser.rs`）：
```rust
pub struct AgentFrontmatter {
    // ...已有字段
    #[serde(default)]
    pub skills: Vec<String>,
}
```

### SkillPreloadMiddleware

新建文件 `rust-agent-middlewares/src/subagent/skill_preload.rs`，实现 `Middleware<S>` trait。

**`before_agent` 注入逻辑：**

1. 遍历 `skill_names` 列表，在 skills 目录（`~/.claude/skills/` → globalConfig → `./.claude/skills/`）中查找对应 `SKILL.md`
2. 读取所有找到的 skill 全文
3. 构造消息序列并通过 `prepend_message` 批量注入：

```
prepend: Human  "（系统：预加载 skill 文件）"
prepend: Ai     [ToolUse { id: "preload_0", name: "read_file", input: {path} }, ...]
prepend: Tool   ToolResult { tool_use_id: "preload_0", content: skill全文 }
prepend: Tool   ToolResult { tool_use_id: "preload_1", content: skill全文 }
...
```

> 使用 `prepend_message` 逐条插入（逆序），保证最终消息顺序为 Human → Ai → Tool…

### 消息注入后 state 结构

最终到 LLM 时的 state 消息顺序（before_agent 全部 prepend 完成后）：

![消息注入顺序示意图](./images/01-flow.png)

```
[0] System   - PrependSystemMiddleware（系统 prompt，最后 prepend 所以排最前）
[1] System   - AgentsMdMiddleware（CLAUDE.md/AGENTS.md 内容）
[2] System   - SkillsMiddleware（所有 skill 摘要）
[3] Human    - "（系统：预加载 skill 文件）"
[4] Ai       - [read_file ToolUse × N]
[5..N+4] Tool - skill 全文 ToolResult × N
[N+5] Human  - 实际任务（execute() 在 before_agent 前 add_message）
```

**Anthropic 适配器兼容性**：`AnthropicAdapter` 会将紧接着 Ai 消息的多个 Tool 消息合并进下一个 Human turn（包含 skill 内容的 ToolResult + 实际任务文本），形成合法的 user turn。OpenAI 适配器则将 tool messages 作为独立角色发送，同样合法。

### SubAgentTool 集成

在 `SubAgentTool::invoke` 解析 agent def 后、组装 `agent_builder` 时追加中间件：

```rust
if !agent_def.frontmatter.skills.is_empty() {
    agent_builder = agent_builder.add_middleware(Box::new(
        SkillPreloadMiddleware::new(
            agent_def.frontmatter.skills.clone(),
            &cwd,
        )
    ));
}
```

**中间件注册顺序**（执行 `before_agent` 时按注册顺序，`prepend` 后倒序展现）：

| 注册顺序 | 中间件 | prepend 后位置 |
|---------|--------|--------------|
| 1 | AgentsMdMiddleware | 靠前（晚 prepend 排前） |
| 2 | SkillsMiddleware（摘要） | 中间 |
| 3 | SkillPreloadMiddleware（全文） ← 新增 | 中间（在摘要后） |
| 4 | TodoMiddleware | 不 prepend，顺序无关 |
| 5 | PrependSystemMiddleware（系统提示） | 最前（最后 prepend） |

### skill 路径查找

复用 `skills::loader::list_skills(dirs)` 逻辑，`SkillPreloadMiddleware::new(names, cwd)` 内部：
1. 调用 `resolve_skill_dirs(cwd)` 获取多路径列表
2. 调用 `list_skills(&dirs)` 扫描所有 skill 元数据
3. 按 `names` 过滤，保留匹配的 skill（找不到的静默跳过，不报错）

## 实现要点

- **`BaseMessage::Ai` 的工具调用格式**：需要同时填写 `tool_calls: Vec<ToolCallRequest>` 和 `content: Vec<ContentBlock::ToolUse>`（双写，与现有 `ai_from_blocks()` 逻辑一致）
- **fake ID 生成**：使用 `format!("skill_preload_{}", index)` 形式，不依赖 UUID
- **找不到的 skill 静默跳过**：与现有 `list_skills` 行为一致，不抛出错误
- **只读操作，无 HITL**：fake 工具调用不经过 `HitlMiddleware`，因为是预注入到 state 而非真实工具调用

## 约束一致性

- **Workspace 分层**：改动限于 `rust-agent-middlewares` crate，不涉及 `rust-create-agent` 核心层，符合"禁止下层依赖上层"约束
- **Middleware Chain 模式**：`SkillPreloadMiddleware` 实现 `Middleware<S>` trait，通过 `before_agent` 钩子注入，符合横切关注点解耦原则
- **消息不可变历史**：使用 `prepend_message`（非修改），状态只追加，符合 `AgentState` 约束
- **异步优先**：`before_agent` 中的文件 IO 通过 `tokio::task::spawn_blocking` 包装，与现有 `SkillsMiddleware` 一致

## 验收标准

- [ ] `AgentFrontmatter` 支持 `skills: Vec<String>` 字段，默认空，反序列化不报错
- [ ] `SkillPreloadMiddleware::before_agent` 将指定 skill 的 SKILL.md 全文注入为 fake read_file 工具调用 + 结果消息对
- [ ] 找不到的 skill 名称静默跳过，不影响其余 skill 的注入
- [ ] `SubAgentTool::invoke` 当 `frontmatter.skills` 非空时正确注册 `SkillPreloadMiddleware`
- [ ] 注入消息的顺序正确：Human init → Ai ToolUse → Tool ToolResult(×N)
- [ ] 与 `AnthropicAdapter` / `OpenAIAdapter` 兼容（不产生非法消息序列）
- [ ] 单元测试覆盖 `SkillPreloadMiddleware`（正常注入 + 部分 skill 不存在 + skill 列表为空）
