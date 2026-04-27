# Cron Loop Command 人工验收清单

**生成时间:** 2026-03-30 16:00
**关联计划:** spec/feature_20260330_F003_cron-loop-command/spec-plan.md
**关联设计:** spec/feature_20260330_F003_cron-loop-command/spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 编译项目: `cargo build`
- [ ] [AUTO] 运行全量测试: `cargo test`

### 测试数据准备
- [ ] 无需额外测试数据，使用内置工具和 TUI

---

## 验收项目

### 场景 1：CronScheduler 核心数据结构与逻辑

#### - [x] 1.1 CronScheduler 单元测试全部通过
- **来源:** spec-plan.md Task 1
- **目的:** 验证注册、删除、tick 触发等核心逻辑
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- cron` → 期望包含: `test result: ok`
---

### 场景 2：CronMiddleware + 三个工具

#### - [x] 2.1 CronMiddleware 编译通过
- **来源:** spec-plan.md Task 2
- **目的:** 确认中间件和工具集成无误
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-middlewares` → 期望包含: `Finished`

#### - [x] 2.2 Cron 工具单元测试通过
- **来源:** spec-plan.md Task 2
- **目的:** 验证 cron_register/cron_list/cron_remove 工具行为
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- cron::tools` → 期望包含: `test result: ok`
---

### 场景 3：TUI 集成编译与已有测试

#### - [x] 3.1 TUI 编译通过
- **来源:** spec-plan.md Task 3 + Task 4
- **目的:** 确认 CronState、CronTrigger 消费、中间件链集成无误
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui` → 期望包含: `Finished`

#### - [x] 3.2 TUI 已有测试不受影响
- **来源:** spec-plan.md Task 3 + Task 4
- **目的:** 回归验证 cron 集成未破坏现有功能
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui` → 期望包含: `test result: ok`
---

### 场景 4：/loop 和 /cron 命令注册

#### - [x] 4.1 /loop 和 /cron 命令出现在帮助列表
- **来源:** spec-plan.md Task 5
- **目的:** 确认命令已正确注册
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- command::tests::test_list_returns_all` → 期望包含: `test result: ok`
  2. [A] `cargo test -p rust-agent-tui --lib -- command::tests::test_list_returns_all 2>&1 | grep -i "cron\|loop"` → 期望包含: 无报错（命令在 list 中）

#### - [x] 4.2 命令逻辑单元测试
- **来源:** spec-plan.md Task 5
- **目的:** 验证 loop_cmd 注册成功/失败路径
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- loop_cmd` → 期望包含: `test result: ok`
---

### 场景 5：CronPanel UI 渲染

#### - [x] 5.1 CronPanel headless 渲染测试
- **来源:** spec-plan.md Task 6
- **目的:** 验证 CronPanel UI 渲染输出包含面板标题
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- test_cron_panel_render` → 期望包含: `test result: ok`
---

### 场景 6：边界与回归

#### - [x] 6.1 任务上限（20 个）检查
- **来源:** spec-plan.md Task 1 / spec-design.md §实现要点
- **目的:** 确认超出上限返回错误
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- test_max_tasks` → 期望包含: `test result: ok`

#### - [x] 6.2 无效 cron 表达式拒绝
- **来源:** spec-plan.md Task 1 / spec-design.md §实现要点
- **目的:** 确认无效表达式返回明确错误
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- test_register_invalid_expression` → 期望包含: `test result: ok`

#### - [x] 6.3 disabled 任务不触发
- **来源:** spec-plan.md Task 1 / spec-design.md §CronScheduler
- **目的:** 确认禁用任务被 tick 跳过
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- test_tick_skips_disabled` → 期望包含: `test result: ok`

#### - [x] 6.4 toggle 切换启用/禁用
- **来源:** spec-plan.md Task 1
- **目的:** 确认 toggle 正确反转 enabled 状态
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- test_toggle` → 期望包含: `test result: ok`
---

### 场景 7：端到端交互验证

#### - [x] 7.1 /loop 自然语言注册定时任务
- **来源:** spec-plan.md Task 7 步骤 1 / spec-design.md §TUI 命令
- **目的:** 确认用户可通过自然语言描述让 Agent 自动注册 cron 任务
- **操作步骤:**
  1. [H] 启动 TUI（`cargo run -p rust-agent-tui`），输入 `/loop 每隔5分钟提醒我喝水`，查看消息流 → Agent 调用 cron_register 成功注册 是/否

#### - [x] 7.2 /cron 面板显示所有任务
- **来源:** spec-plan.md Task 7 步骤 3 / spec-design.md §/cron
- **目的:** 确认面板正确展示任务列表
- **操作步骤:**
  1. [H] 在 TUI 中通过 /loop 注册 2-3 个 cron 任务后，输入 `/cron`，查看面板 → 显示任务列表且含 cron 表达式/状态/prompt 是/否

#### - [x] 7.3 /cron 面板操作（导航/删除/切换）
- **来源:** spec-plan.md Task 7 步骤 4 / spec-design.md §/cron
- **目的:** 确认面板键盘交互正常
- **操作步骤:**
  1. [H] 在 `/cron` 面板中按 `Enter` 切换任务启用/禁用 → 状态图标变化 是/否
  2. [H] 按 `d` 删除任务 → 任务被移除 是/否
  3. [H] 按 `Esc` 关闭面板 → 面板消失 是/否

#### - [x] 7.4 定时触发自动提交
- **来源:** spec-plan.md Task 7 步骤 1 / spec-design.md §TUI 事件处理
- **目的:** 确认 cron 到时后自动触发 Agent 执行
- **操作步骤:**
  1. [H] 通过 `/loop 每分钟执行一次 ping` 注册任务后等待 1 分钟 → Agent 自动提交 "ping" 并开始执行 是/否

#### - [x] 7.5 Agent 正忙时跳过触发
- **来源:** spec-plan.md Task 7 步骤 2 / spec-design.md §实现要点
- **目的:** 确认 loading 时静默跳过，不崩溃不卡顿
- **操作步骤:**
  1. [H] 注册每分钟 cron 任务，在 Agent 执行长任务时等待触发 → 不崩溃不卡顿 是/否

#### - [x] 7.6 TUI 重启后 cron 任务清空
- **来源:** spec-design.md §约束一致性 / §实现要点
- **目的:** 确认任务仅存内存，重启后为空
- **操作步骤:**
  1. [H] 注册 cron 任务后重启 TUI → `/cron` 显示"无定时任务" 是/否

---

## 验收后清理

无需清理后台服务（TUI 为前台进程，关闭即可）。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | CronScheduler 单元测试 | 1 | 0 | ✅ |
| 场景 2 | 2.1 | CronMiddleware 编译 | 1 | 0 | ✅ |
| 场景 2 | 2.2 | Cron 工具单元测试 | 1 | 0 | ✅ |
| 场景 3 | 3.1 | TUI 编译 | 1 | 0 | ✅ |
| 场景 3 | 3.2 | TUI 已有测试回归 | 1 | 0 | ✅ |
| 场景 4 | 4.1 | 命令出现在帮助列表 | 2 | 0 | ✅ |
| 场景 4 | 4.2 | 命令逻辑单元测试 | 1 | 0 | ✅ |
| 场景 5 | 5.1 | CronPanel headless 渲染 | 1 | 0 | ✅ |
| 场景 6 | 6.1 | 任务上限检查 | 1 | 0 | ✅ |
| 场景 6 | 6.2 | 无效表达式拒绝 | 1 | 0 | ✅ |
| 场景 6 | 6.3 | disabled 不触发 | 1 | 0 | ✅ |
| 场景 6 | 6.4 | toggle 切换 | 1 | 0 | ✅ |
| 场景 7 | 7.1 | /loop 自然语言注册 | 0 | 1 | ✅ |
| 场景 7 | 7.2 | /cron 面板展示 | 0 | 1 | ✅ |
| 场景 7 | 7.3 | 面板键盘操作 | 0 | 3 | ✅ |
| 场景 7 | 7.4 | 定时触发自动提交 | 0 | 1 | ✅ |
| 场景 7 | 7.5 | 正忙时跳过触发 | 0 | 1 | ✅ |
| 场景 7 | 7.6 | 重启后任务清空 | 0 | 1 | ✅ |

**验收结论:** ✅ 全部通过
