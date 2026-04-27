# Feature: 20260427_F002 - permission-mode

## 需求背景

当前 HITL 审批系统只有两种状态：YOLO（全放行）和审批模式（所有敏感工具弹窗）。这导致用户要么每次操作都要点击确认，要么完全放弃安全审核。Claude Code 通过 Shift+Tab 在多种权限模式间切换，在安全性和效率之间取得了良好平衡。我们需要类似的多级权限模式系统，让用户根据场景灵活调整 Agent 的自主程度。

## 目标

- 支持 5 种权限模式：`default` / `acceptEdits` / `auto` / `bypassPermissions` / `dontAsk`
- Shift+Tab 循环切换模式，状态栏实时显示当前模式
- 模式切换仅影响后续工具调用，不中断正在执行的操作
- `auto` 模式使用 LLM 分类器决定工具调用的放行/拒绝
- `acceptEdits` 模式自动放行文件编辑类工具，其他敏感操作仍需审批
- 不引入 Plan 模式，Relay 远程端暂不感知权限模式

## 方案设计

### 权限模式定义

新增 `PermissionMode` 枚举，位于 `rust-agent-middlewares/src/hitl/shared_mode.rs`：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PermissionMode {
    /// 所有敏感工具弹窗审批（默认）
    Default = 0,
    /// 自动放行文件编辑类工具，其他敏感操作仍需审批
    AcceptEdits = 1,
    /// 使用 LLM 分类器自动决定放行/拒绝
    Auto = 2,
    /// 跳过所有审批（当前 YOLO 行为）
    BypassPermissions = 3,
    /// 自动拒绝所有审批请求
    DontAsk = 4,
}
```

**循环切换顺序：** `Default → AcceptEdits → Auto → BypassPermissions → DontAsk → Default`

**模式行为表：**

| 模式 | 敏感工具行为 | 非敏感工具 | ask_user_question |
|------|-------------|-----------|-------------------|
| `Default` | 弹窗审批 | 直接放行 | 弹窗问答 |
| `AcceptEdits` | 编辑类自动放行，其他弹窗 | 直接放行 | 弹窗问答 |
| `Auto` | LLM 分类器决定 | 直接放行 | 弹窗问答 |
| `BypassPermissions` | 全部自动放行 | 直接放行 | 弹窗问答 |
| `DontAsk` | 全部自动拒绝 | 直接放行 | 弹窗问答（不受影响，见说明） |

> **说明：** `ask_user_question` 不通过 HITL `before_tool` 拦截，而是通过 `AskUserTool` 直接调用 `broker`。`DontAsk` 模式仅自动拒绝需要审批的敏感工具调用，`ask_user_question` 的行为在各模式下均不受权限模式影响（始终弹窗问答）。后续可扩展在 `AskUserTool` 中增加权限模式感知逻辑。

### 共享状态传递

使用 `Arc<AtomicU8>` 在 TUI 线程和 Agent task 之间共享当前权限模式：

```rust
// rust-agent-middlewares/src/hitl/shared_mode.rs
pub struct SharedPermissionMode {
    inner: AtomicU8,
}

impl SharedPermissionMode {
    pub fn new(mode: PermissionMode) -> Arc<Self> {
        Arc::new(Self { inner: AtomicU8::new(mode as u8) })
    }
    pub fn load(&self) -> PermissionMode {
        let v = self.inner.load(Ordering::Relaxed);
        PermissionMode::try_from(v).unwrap_or(PermissionMode::Default)
    }
    pub fn store(&self, mode: PermissionMode) {
        self.inner.store(mode as u8, Ordering::Relaxed);
    }
    /// 循环切换到下一个模式
    pub fn cycle(&self) -> PermissionMode {
        loop {
            let current = self.inner.load(Ordering::Relaxed);
            let next = (current + 1) % 5;
            // CAS 循环，防止并发竞争
            if self.inner.compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                return PermissionMode::try_from(next).unwrap_or(PermissionMode::Default);
            }
        }
    }
}
```

### HumanInTheLoopMiddleware 改造

将 `HumanInTheLoopMiddleware` 从"启用/禁用"二态改为读取 `SharedPermissionMode` 动态决策：

```rust
pub struct HumanInTheLoopMiddleware {
    broker: Option<Arc<dyn UserInteractionBroker>>,
    mode: Option<Arc<SharedPermissionMode>>,
    requires_approval: fn(&str) -> bool,
    /// Auto 模式的 LLM 分类器（可选，懒初始化）
    auto_classifier: Option<Arc<dyn AutoClassifier>>,
}
```

`broker` 和 `mode` 均为 `Option`，保持 `disabled()` 和 `new()` 的向后兼容（`mode=None` 时走原有逻辑）。`with_shared_mode()` 构造函数同时设置两者为 `Some`。

**`before_tool` 决策流程：**

```
before_tool(call):
  if !requires_approval(call.name):
    return Ok(call)  // 非敏感工具，所有模式都放行

  match mode.load():
    BypassPermissions → Ok(call)
    DontAsk           → Err(ToolRejected)
    AcceptEdits       → if is_edit_tool(call.name) { Ok(call) }
                        else { broker.request(Approval) }
    Auto              → auto_classifier.classify(call).await
                         → Allow → Ok(call)
                         → Deny  → Err(ToolRejected)
                         → Unsure → broker.request(Approval)
    Default           → broker.request(Approval)
```

**编辑工具判断函数（`acceptEdits` 模式使用）：**

```rust
fn is_edit_tool(tool_name: &str) -> bool {
    tool_name.starts_with("write_")
        || tool_name.starts_with("edit_")
        || tool_name == "folder_operations"
}
```

`bash`、`launch_agent`、`delete_*`、`rm_*` 仍需弹窗审批。

### Auto 分类器接口

新增 trait，位于 `rust-agent-middlewares/src/hitl/auto_classifier.rs`：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    Allow,
    Deny,
    Unsure,
}

#[async_trait]
pub trait AutoClassifier: Send + Sync {
    /// 根据工具名称和输入判断是否放行
    async fn classify(&self, tool_name: &str, tool_input: &serde_json::Value) -> Classification;
}
```

**LLM 分类器实现（`LlmAutoClassifier`）：**

使用当前会话的同一个 LLM，构造一个短 system prompt 进行分类：

```rust
pub struct LlmAutoClassifier {
    model: Arc<parking_lot::Mutex<Box<dyn BaseModel>>>,
}

// 分类 prompt 示例（极简，降低延迟）：
// "判断以下工具调用是否安全。工具: {name}, 输入: {input}
//  只回答 ALLOW / DENY / UNSURE，不要解释。"
```

为避免频繁调用 LLM 带来延迟，分类器需缓存近期结果（相同工具名 + 相似输入 → 复用决策）。

### TUI 层集成

#### 1. App struct 新增字段

```rust
// rust-agent-tui/src/app/mod.rs
pub struct App {
    // ... 已有字段
    pub permission_mode: Arc<SharedPermissionMode>,
    /// 权限模式切换后的闪烁高亮截止时间，None 表示不闪烁
    pub mode_highlight_until: Option<std::time::Instant>,
}
```

#### 2. Shift+Tab 键绑定

在 `event.rs` 的 `handle_key_event` 中拦截 `Shift+Tab`：

```rust
KeyEvent { code: Tab, modifiers: SHIFT, .. } => {
    let _new_mode = app.permission_mode.cycle();
    app.mode_highlight_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
}
```

`mode_highlight_until` 字段控制状态栏闪烁高亮的截止时间，渲染时检查 `Instant::now() < until` 决定是否激活高亮样式，超时后自动恢复，无需额外定时器。

#### 3. 状态栏显示

在状态栏区域显示当前模式名称，使用不同颜色区分：

| 模式 | 显示文本 | 颜色 |
|------|---------|------|
| `Default` | `DEFAULT` | 白色 |
| `AcceptEdits` | `AUTO-EDIT` | 绿色 |
| `Auto` | `AUTO` | 青色 |
| `BypassPermissions` | `YOLO` | 黄色 |
| `DontAsk` | `NO-ASK` | 红色 |

模式切换时，状态栏文本闪烁高亮 1.5 秒后恢复正常亮度。

#### 4. Agent 构建时传入共享状态

`run_universal_agent` 中创建 HITL middleware 时注入 `SharedPermissionMode`：

```rust
// rust-agent-tui/src/app/agent.rs
let hitl = HumanInTheLoopMiddleware::with_shared_mode(
    broker.clone(),
    default_requires_approval,
    shared_permission_mode.clone(),  // 从 AgentRunConfig 传入
    None,                            // auto_classifier 暂传 None，后续可扩展
);
```

`AgentRunConfig` 新增字段：

```rust
pub struct AgentRunConfig {
    // ... 已有字段
    pub permission_mode: Arc<SharedPermissionMode>,
}
```

### YOLO_MODE 环境变量兼容

保留 `YOLO_MODE` 环境变量作为初始模式的决定因素：

- `YOLO_MODE` 未设置或 `true` → 初始模式 `BypassPermissions`（行为不变）
- `YOLO_MODE=false` → 初始模式 `Default`（行为不变）
- `-a` / `--approve` CLI 参数 → 初始模式 `Default`

启动后用户可随时通过 Shift+Tab 切换，环境变量仅决定初始值。

## 实现要点

1. **原子性保证：** `SharedPermissionMode` 使用 `AtomicU8` + `Ordering::Relaxed`，无需 Mutex，零锁竞争。`repr(u8)` 保证枚举值在 0-4 范围内，`try_from` 处理异常值回退到 `Default`。

2. **Auto 分类器延迟：** `auto` 模式下每个敏感工具调用前多一次 LLM 请求（~200-500ms）。可通过小模型（haiku）或本地规则预筛降低延迟。分类器缓存相同工具名 + input hash 的决策结果，有效期 5 分钟。

3. **Middleware 层改造范围：** `HumanInTheLoopMiddleware` 的 `before_tool` 和 `process_batch` 都需读取共享模式。`process_batch` 批量处理时需逐个读取模式（因为模式可能在批量处理期间被切换）。

4. **不破坏现有测试：** 现有 HITL 测试使用 `disabled()` 和 `new()` 构造，新增 `with_shared_mode()` 方法，保留旧构造函数的兼容性。`disabled()` 等价于 `BypassPermissions` 模式。

5. **Relay 暂不感知：** 本期设计中 `SharedPermissionMode` 不传递到 Relay client。Relay Web 前端的审批弹窗行为不受影响（仍通过 `InteractionRequest` 事件触发）。后续可扩展通过 Relay 消息同步模式状态。

6. **新增文件清单：**
   - `rust-agent-middlewares/src/hitl/auto_classifier.rs` — AutoClassifier trait + LlmAutoClassifier
   - `rust-agent-middlewares/src/hitl/shared_mode.rs` — SharedPermissionMode
   - 修改 `rust-agent-middlewares/src/hitl/mod.rs` — 重构 HumanInTheLoopMiddleware
   - 修改 `rust-agent-tui/src/app/mod.rs` — App 新增 permission_mode 字段
   - 修改 `rust-agent-tui/src/app/agent.rs` — AgentRunConfig 新增字段，传入共享状态
   - 修改 `rust-agent-tui/src/event.rs` — Shift+Tab 键绑定
   - 修改 `rust-agent-tui/src/ui/` — 状态栏渲染当前模式
   - 修改 `rust-agent-tui/src/main.rs` — 初始模式从环境变量解析

## 约束一致性

- **Middleware Chain 模式：** 权限决策逻辑封装在 `HumanInTheLoopMiddleware` 内部，不侵入 ReAct 执行器，与现有架构一致。
- **事件驱动 TUI 通信：** 模式切换通过 `SharedPermissionMode::cycle()` 原子更新共享状态，状态栏在下一帧渲染时自动读取最新模式值。模式切换后的闪烁高亮通过 `App::mode_highlight_until: Option<Instant>` 控制（1.5 秒后自动失效），无需额外事件或定时器。唯一的共享可变状态 `Arc<AtomicU8>` 是无锁原子操作，不违反"禁止共享可变状态"约束的精神。
- **Workspace 分层：** `PermissionMode` 和 `SharedPermissionMode` 定义在 `rust-agent-middlewares`（中间件层），不放在 `rust-create-agent`（核心层），避免核心层感知权限模式。`AutoClassifier` trait 也在中间件层。
- **异步优先：** `AutoClassifier::classify` 使用 `async-trait`，与现有 trait 风格一致。
- **错误处理：** 分类器调用失败时降级为 `Unsure`，走弹窗审批路径，遵循 fail-safe 原则。

## 验收标准

- [ ] Shift+Tab 在 5 种模式间循环切换，状态栏实时显示当前模式
- [ ] `Default` 模式下敏感工具弹窗审批，非敏感工具直接放行
- [ ] `AcceptEdits` 模式下 `write_*`/`edit_*`/`folder_operations` 自动放行，`bash`/`launch_agent` 仍弹窗
- [ ] `Auto` 模式使用 LLM 分类器自动决定， Unsure 时回退到弹窗
- [ ] `BypassPermissions` 模式等同于当前 YOLO 行为（全放行）
- [ ] `DontAsk` 模式自动拒绝所有敏感工具调用
- [ ] 模式切换仅影响后续工具调用，不中断正在执行或等待审批的工具
- [ ] `YOLO_MODE` 环境变量和 `-a` CLI 参数仍决定初始模式，启动后可自由切换
- [ ] 现有 HITL 单元测试全部通过
- [ ] `HumanInTheLoopMiddleware::disabled()` 和 `::new()` 保持向后兼容
