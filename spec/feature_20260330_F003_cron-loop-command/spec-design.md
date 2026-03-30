# Feature: 20260330_F003 - cron-loop-command

## 需求背景

当前 TUI 仅支持用户手动输入消息触发 Agent 执行。用户希望 Agent 能按定时计划自动执行任务（如每 5 分钟检查服务健康、每日凌晨生成报告）。需要：

1. `/loop` 命令：用户或 AI 注册定时任务，指示 Agent 定时执行指定 prompt
2. `/cron` 命令：查看和管理已注册的定时任务
3. cron 任务仅存储在内存中，TUI 重启后清空

## 目标

- 提供标准 5 段 cron 表达式的定时任务注册能力
- AI 可通过工具调用 `cron_register` 创建定时任务（无需 HITL 审批）
- 用户可通过 `/loop` 命令快速注册、`/cron` 命令管理任务
- Agent 正忙时跳过本次触发，不排队不中断

## 方案设计

### 整体架构

采用 **B+C 组合架构**：`rust-agent-middlewares` 提供 CronMiddleware（通用 cron 工具），`rust-agent-tui` 用独立 CronManager task 驱动定时触发。

![定时任务架构](./images/01-architecture.png)

```
┌─ rust-agent-middlewares ─────────────────────────┐
│  CronMiddleware                                   │
│    ├─ cron_register  (BaseTool)                   │
│    ├─ cron_list      (BaseTool)                   │
│    └─ cron_remove    (BaseTool)                   │
│    └─ CronScheduler (内存任务表 + tick 计算)      │
└──────────────────────────────────────────────────┘
         │ trigger_tx (mpsc::unbounded)
         ↓
┌─ rust-agent-tui ─────────────────────────────────┐
│  CronManager Task (tokio::spawn)                  │
│    └─ 每 1s tick → 发送 CronTrigger               │
│                                                   │
│  App.poll_agent()                                 │
│    └─ 收到 CronTrigger →                          │
│         if !loading: submit_message(prompt)       │
│         if loading:  跳过                         │
│                                                   │
│  /loop <cron_expr> <prompt> → CronScheduler.register() │
│  /cron → CronPanel 面板                           │
└──────────────────────────────────────────────────┘
```

### 数据模型

```rust
// rust-agent-middlewares/src/cron/mod.rs

/// 定时任务
struct CronTask {
    id: String,              // UUID v7
    expression: String,      // 标准 5 段 cron 表达式，如 "*/5 * * * *"
    prompt: String,          // 触发时作为用户输入提交
    next_fire: Option<DateTime<Utc>>,  // 下次触发时间
    enabled: bool,           // 是否启用
}

/// 触发事件（由 CronManager 发送到 App）
struct CronTrigger {
    task_id: String,
    prompt: String,
}

/// 定时任务调度器（纯内存）
struct CronScheduler {
    tasks: HashMap<String, CronTask>,
    trigger_tx: mpsc::UnboundedSender<CronTrigger>,
}
```

### CronMiddleware 工具设计

`CronMiddleware` 实现 `Middleware<S>` trait，通过 `collect_tools` 提供三个工具：

#### `cron_register`（注册定时任务）

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `expression` | string | 是 | 标准 5 段 cron 表达式 |
| `prompt` | string | 是 | 触发时提交的用户输入 |

返回示例：`"已注册定时任务 0192a3b4（*/5 * * * *），prompt: 检查服务健康状态"`

AI 和用户均可调用。不在 HITL 审批清单中。

#### `cron_list`（列出所有任务）

无参数。

返回示例：
```
当前有 2 个定时任务：
1. [0192a3b4] */5 * * * * | 下次: 14:35 | ✓启用 | 检查服务健康状态
2. [0192a5c6] 0 9 * * *   | 下次: 明天 09:00 | ✓启用 | 生成每日报告
```

#### `cron_remove`（删除任务）

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | string | 是 | 任务 ID |

返回示例：`"已删除定时任务 0192a3b4"`

### CronScheduler 核心逻辑

```rust
impl CronScheduler {
    /// 注册新任务
    fn register(&mut self, expression: &str, prompt: &str) -> Result<String> {
        // 1. 解析 cron 表达式（使用 croner 库）
        // 2. 计算下次触发时间
        // 3. 生成 UUID v7 作为 ID
        // 4. 存入 tasks HashMap
        // 5. 返回 task_id
    }

    /// 删除任务
    fn remove(&mut self, id: &str) -> bool { ... }

    /// 每秒调用：检查是否有任务到时触发
    fn tick(&mut self) {
        let now = Utc::now();
        for task in self.tasks.values_mut() {
            if !task.enabled { continue; }
            if let Some(next) = task.next_fire {
                if now >= next {
                    let _ = self.trigger_tx.send(CronTrigger {
                        task_id: task.id.clone(),
                        prompt: task.prompt.clone(),
                    });
                    // 计算并更新下次触发时间
                    task.next_fire = calculate_next_fire(&task.expression, now);
                }
            }
        }
    }

    /// 获取所有任务（供 /cron 面板和 cron_list 工具使用）
    fn list_tasks(&self) -> &[CronTask] { ... }
}
```

### TUI 层 CronManager Task

```
App::new() 时：
  1. let (trigger_tx, trigger_rx) = mpsc::unbounded_channel::<CronTrigger>();
  2. let scheduler = CronScheduler::new(trigger_tx);
  3. let scheduler_clone = scheduler.clone();  // Arc<Mutex<CronScheduler>>
  4. tokio::spawn(cron_manager_task(scheduler_clone));

async fn cron_manager_task(scheduler: Arc<Mutex<CronScheduler>>) {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        scheduler.lock().tick();
    }
}
```

### TUI 事件处理

在 `poll_agent()` 中增加 CronTrigger 消费：

```rust
// agent_ops.rs - poll_agent() 末尾
if let Some(ref mut rx) = self.cron.trigger_rx {
    while let Ok(trigger) = rx.try_recv() {
        if !self.core.loading {
            self.submit_message(trigger.prompt);
        }
        // else: Agent 正忙，静默跳过
    }
}
```

### TUI 命令

#### `/loop <cron_expression> <prompt>`

- 格式：`/loop */5 * * * * 检查服务健康状态并汇报`
- cron 表达式为前 5 个空格分隔的 token，剩余部分为 prompt
- 调用 `CronScheduler::register()` 直接注册
- 成功后在消息流中显示：`⏰ 已注册定时任务 {id}（*/5 * * * *）`
- 失败时显示错误：`❌ cron 表达式无效: ...`

#### `/cron`

- 打开 CronPanel 面板，显示所有已注册任务
- 面板操作：
  - `↑/↓` 或 `j/k`：导航
  - `d`：删除选中任务
  - `Enter`：切换 enabled/disabled
  - `Esc`：关闭面板

![Cron 面板交互流程](./images/02-cron-panel-flow.png)

### 中间件链集成

在 `run_universal_agent()` 中将 CronMiddleware 插入中间件链：

```rust
// agent.rs
let cron_middleware = CronMiddleware::new(scheduler.clone());

let executor = ReActAgent::new(model)
    // ... 现有中间件 ...
    .add_middleware(Box::new(cron_middleware))
    // ...
```

CronMiddleware 放在 SubAgentMiddleware 之前、HITL 之前，确保 cron 工具对 AI 可用但不受 HITL 拦截。

## 实现要点

1. **cron 表达式解析**：使用 `croner` crate（纯 Rust，支持标准 5 段 cron），计算下次触发时间
2. **线程安全**：`CronScheduler` 通过 `Arc<Mutex<CronScheduler>>` 共享，CronManager task 每秒获取锁做 tick，工具调用时获取锁做 register/remove/list
3. **跳过策略**：Agent loading 时静默跳过，不输出日志不打扰用户；下次 tick 会重新检查
4. **时区**：使用 UTC 计算，显示时转为本地时间
5. **内存上限**：限制最多 20 个 cron 任务，超出时返回错误提示
6. **cron 表达式解析失败**：工具返回明确错误信息，AI 可调整重试

## 新增依赖

| crate | 用途 | 添加到 |
|-------|------|--------|
| `croner` | 标准 cron 表达式解析与下次触发时间计算 | `rust-agent-middlewares` |

## 约束一致性

- **Workspace 分层**：CronMiddleware 放在 `rust-agent-middlewares`，符合「下层不依赖上层」约束
- **内存任务表**：不涉及持久化，不违反「消息不可变历史」约束
- **事件驱动通信**：通过 mpsc channel 发送 CronTrigger，符合「事件驱动 TUI 通信」架构决策
- **工具系统**：cron_register/cron_list/cron_remove 遵循 `BaseTool` trait 接口
- **不在 HITL 拦截清单中**：cron 操作仅注册定时任务，不直接执行敏感操作；触发时通过 submit_message 正常走 HITL 流程（如果启用了审批模式）

## 验收标准

- [ ] `/loop */1 * * * * hello` 注册后，1 分钟内 Agent 自动提交 "hello" 并执行
- [ ] Agent 正忙时触发被静默跳过，不崩溃不卡顿
- [ ] `/cron` 面板显示所有已注册任务，支持导航/删除/切换启用
- [ ] AI 通过 `cron_register` 工具成功创建定时任务（无需 HITL 审批）
- [ ] AI 通过 `cron_list` 查看已注册任务
- [ ] AI 通过 `cron_remove` 删除任务
- [ ] TUI 重启后所有 cron 任务清空
- [ ] 任务数上限 20 个，超出时返回错误
- [ ] cron 表达式格式错误时返回明确错误信息
