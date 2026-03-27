# Feature: 20260325_F001 - subagent-middleware-injection

## 需求背景

子 agent（通过 `launch_agent` 工具委派）在 `SubAgentTool::invoke` 中组装时，缺少父 agent 中间件链的三个关键中间件：

- **`AgentsMdMiddleware`**：子 agent 无法读取 `AGENTS.md` / `CLAUDE.md`，不了解项目规范与约束
- **`SkillsMiddleware`**：子 agent 无法感知项目可用 skills，不能调用专项技能
- **`TodoMiddleware`**：子 agent 没有 `todo_write` 工具，无法管理任务列表

这导致子 agent 的上下文信息不完整，与父 agent 行为不一致。

## 目标

- 补全子 agent 缺失的三个中间件，使其与父 agent 上下文一致
- 最小化改动，不引入新的 API 变更
- `TodoMiddleware` 的通知 channel 静默丢弃（不向 TUI 透传），实现最简单

## 方案设计

### 修改位置

唯一修改文件：`rust-agent-middlewares/src/subagent/tool.rs`，在 `SubAgentTool::invoke` 方法的 `agent_builder` 组装段（当前第 190 行附近）。

### 中间件注册顺序

子 agent 新的中间件链，与父 agent 对齐：

```
AgentsMdMiddleware::new()
  └─ before_agent: 读取 {cwd}/AGENTS.md 或 CLAUDE.md，前插为 System 消息

SkillsMiddleware::new().with_global_config()
  └─ before_agent: 扫描 skills 目录，注入 skills 摘要 System 消息

TodoMiddleware::new(todo_tx)  ← todo_rx 立即丢弃，send 失败静默忽略
  └─ collect_tools: 提供 todo_write 工具

PrependSystemMiddleware::new(system_content)  ← 最后注册（仅 system_builder 有值时）
  └─ before_agent: 最后执行 prepend → 系统提示位于消息列表最前
```

![子 agent 中间件链示意](./images/01-middleware-chain.png)

### 代码改动

**新增 import（tool.rs 顶部）：**

```rust
use crate::agents_md::AgentsMdMiddleware;
use crate::middleware::todo::TodoMiddleware;
use crate::skills::SkillsMiddleware;
use tokio::sync::mpsc;
```

**invoke 方法修改（在 PrependSystemMiddleware 之前插入）：**

```rust
let mut agent_builder = ReActAgent::new(llm).max_iterations(max_iterations);

// 补充父 agent 缺失的中间件
agent_builder = agent_builder
    .add_middleware(Box::new(AgentsMdMiddleware::new()))
    .add_middleware(Box::new(SkillsMiddleware::new().with_global_config()))
    .add_middleware(Box::new(TodoMiddleware::new({
        let (tx, _rx) = mpsc::channel(8); // _rx 丢弃，通知静默忽略
        tx
    })));

// PrependSystemMiddleware 最后注册（确保系统提示在消息列表最前）
if let Some(ref builder) = self.system_builder {
    ...
}
```

### 有意保留的省略项

| 中间件 | 省略原因 |
|--------|---------|
| `HumanInTheLoopMiddleware` | 子 agent 自动执行，不应阻塞等待人工审批 |
| `SubAgentMiddleware` | 防止子 agent 递归调用 `launch_agent` |
| `AskUserTool` | 子 agent 自动完成，无需交互式询问用户 |

## 实现要点

- `SkillsMiddleware::with_global_config()` 会从 `~/.zen-code/settings.json` 加载全局 skills 目录，与父 agent 行为一致
- `mpsc::channel(8)` 的 `_rx` 变量在 let 绑定后立即离开作用域被丢弃；子 agent 若调用 `todo_write`，`TodoWriteTool` 的 `notify_tx.send(...)` 会因 channel 已关闭而返回错误，该错误不影响工具返回结果（工具本身仍正常执行写入逻辑）
- 注册顺序严格保持：`AgentsMdMiddleware` → `SkillsMiddleware` → `TodoMiddleware` → `PrependSystemMiddleware`（最后），以确保 `before_agent` prepend 顺序正确，系统提示始终处于消息列表最前

## 约束一致性

- **下层禁止依赖上层**（`rust-agent-middlewares` → `rust-create-agent`）：本改动仅在 `rust-agent-middlewares` 内部使用自身的中间件，无约束违反
- **`AgentsMdMiddleware`、`SkillsMiddleware`、`TodoMiddleware`** 均已在 `rust-agent-middlewares` 中实现，无需新增依赖
- `tokio::sync::mpsc` 已在工作空间中使用，无新增依赖

## 验收标准

- [ ] 子 agent 执行时，若 `{cwd}` 存在 `AGENTS.md` 或 `CLAUDE.md`，其内容作为 System 消息注入子 agent 上下文
- [ ] 子 agent 执行时，若存在 skills，skills 摘要作为 System 消息注入子 agent 上下文
- [ ] 子 agent 可使用 `todo_write` 工具，写入成功，TUI 不崩溃（通知静默丢弃）
- [ ] 父 agent 的 HITL 审批、`ask_user`、`launch_agent` 工具不被注入子 agent（防递归 + 防阻塞）
- [ ] 现有子 agent 相关测试全部通过（`cargo test -p rust-agent-middlewares`）
