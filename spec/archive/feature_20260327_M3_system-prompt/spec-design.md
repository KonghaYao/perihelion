# Plan M3：消除 PrependSystemMiddleware 排序约束

> 优先级：小工作量，立即消除隐患
> 涉及 crate：rust-create-agent / rust-agent-middlewares / rust-agent-tui

---

## 问题描述

`PrependSystemMiddleware` 必须是最后一个 `add_middleware`，因为 `before_agent` 按注册顺序执行，
而 `prepend_message` 的语义要求它在所有其他 before_agent 之后执行，否则 system prompt 不在消息列表最前。

这是一个隐式约束，注释中虽有提示，但新增中间件时极易被破坏。

```rust
// 当前调用方（rust-agent-tui/src/app/agent.rs）
.add_middleware(Box::new(FilesystemMiddleware::new()))
.add_middleware(Box::new(TerminalMiddleware::new()))
.add_middleware(Box::new(TodoMiddleware::new(todo_tx)))
.add_middleware(Box::new(hitl))
.add_middleware(Box::new(subagent))
// ↑ 如果有人在这行之后再加一个中间件，system prompt 就不在最前了
.add_middleware(Box::new(PrependSystemMiddleware::new(system_prompt)))  // 必须最后
```

---

## 方案（推荐方案 A）

### 方案 A：ReActAgent 专用 `with_system_prompt()` 方法

在 executor 内部固定处理，不依赖中间件注册顺序。

**核心改动：**

```rust
// rust-create-agent/src/agent/executor.rs

pub struct ReActAgent<S, L> {
    // 新增字段
    system_prompt: Option<String>,
    // ...其他字段不变
}

impl<S, L> ReActAgent<S, L> {
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }
}

// execute() 内部，run_before_agent 之后固定 prepend
async fn execute_inner(&self, input: AgentInput, state: &mut S, ...) {
    // ...
    self.chain.run_before_agent(state).await?;

    // 固定在所有 before_agent 之后 prepend，不受中间件顺序影响
    if let Some(ref prompt) = self.system_prompt {
        state.prepend_message(BaseMessage::system(prompt.clone()));
    }

    // ... LLM 循环
}
```

**调用方简化：**

```rust
// 之前
.add_middleware(Box::new(PrependSystemMiddleware::new(system_prompt)))

// 之后
.with_system_prompt(system_prompt)
```

---

### 方案 B（备选）：Middleware priority 排序

```rust
pub trait Middleware<S: State>: Send + Sync {
    /// 执行优先级，数字越小越先执行，默认 0
    fn priority(&self) -> i32 { 0 }
}

// PrependSystemMiddleware 实现
fn priority(&self) -> i32 { i32::MAX }  // 永远最后执行
```

`MiddlewareChain::add()` 改为按 priority 有序插入。

**不推荐原因**：增加 Middleware trait 复杂度；所有现有中间件要考虑 priority 语义；
方案 A 更符合"系统提示是 agent 构建者的关注点，不是中间件"的语义。

---

## 变更文件清单

### 1. `rust-create-agent/src/agent/executor.rs`
- 新增 `system_prompt: Option<String>` 字段
- 新增 `with_system_prompt(prompt)` builder 方法
- `execute()` 中，`run_before_agent` 之后固定 prepend system message

### 2. `rust-agent-tui/src/app/agent.rs`
- 删除 `.add_middleware(Box::new(PrependSystemMiddleware::new(system_prompt)))`
- 改为 `.with_system_prompt(system_prompt)`

### 3. `rust-agent-middlewares/src/subagent/tool.rs`
- 子 agent 构建处同样替换为 `.with_system_prompt(...)`
- `PrependSystemMiddleware` 的 import 可删除（如果无其他用途）

### 4. `rust-agent-middlewares/src/middleware/prepend_system.rs`（可选）
- 若无其他调用方，可标记 `#[deprecated]` 或删除
- 建议保留并加注释：高级场景（动态 prompt 等）仍可使用中间件方式

---

## 预期效果

```rust
// 新的 agent 构建方式，顺序无约束，任意位置添加中间件均安全
ReActAgent::new(model)
    .with_system_prompt(system_prompt)   // ← 不依赖位置
    .max_iterations(500)
    .add_middleware(Box::new(FilesystemMiddleware::new()))
    .add_middleware(Box::new(TerminalMiddleware::new()))
    .add_middleware(Box::new(TodoMiddleware::new(todo_tx)))
    .add_middleware(Box::new(hitl))
    .add_middleware(Box::new(subagent))
    // 未来新增中间件不会破坏 system prompt 位置
    .with_event_handler(handler)
```

---

## 工作量估计

- executor.rs 改动：约 15 行
- agent.rs 改动：删 1 行加 1 行
- subagent/tool.rs 改动：同上
- 合计：**小（1-2 小时）**
