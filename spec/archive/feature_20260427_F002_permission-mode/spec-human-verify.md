# Permission Mode 人工验收清单

**生成时间:** 2026-04-27 22:00
**关联计划:** spec/feature_20260427_F002_permission-mode/spec-plan.md
**关联设计:** spec/feature_20260427_F002_permission-mode/spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 编译全量 workspace: `cargo build --workspace 2>&1 | tail -5`
- [ ] [AUTO] 验证测试框架可用: `cargo test --workspace 2>&1 | tail -5`

---

## 验收项目

### 场景 1：PermissionMode 枚举与 SharedPermissionMode 基础类型

#### - [ ] 1.1 SharedPermissionMode 单元测试全部通过
- **来源:** spec-plan.md Task 1 / spec-design.md §权限模式定义
- **目的:** 枚举循环、原子读写、并发安全
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- shared_mode::tests 2>&1 | tail -15` → 期望包含: `test result: ok`

#### - [ ] 1.2 shared_mode.rs 文件存在且导出正确
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认基础类型文件已创建并通过 prelude 导出
- **操作步骤:**
  1. [A] `grep -n 'PermissionMode\|SharedPermissionMode' rust-agent-middlewares/src/lib.rs` → 期望包含: `pub use hitl` 和 `pub use crate::hitl` 均含新类型

### 场景 2：AutoClassifier 分类器

#### - [ ] 2.1 AutoClassifier 单元测试全部通过
- **来源:** spec-plan.md Task 2 / spec-design.md §Auto 分类器接口
- **目的:** ALLOW/DENY/UNSURE 分类、LLM 失败降级、缓存命中与过期
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- auto_classifier::tests 2>&1 | tail -15` → 期望包含: `test result: ok`

#### - [ ] 2.2 Auto 模式分类器已在 TUI 层接入
- **来源:** spec-plan.md Task 4（后续扩展）+ 实际实现补充
- **目的:** 确认 run_universal_agent 中 LlmAutoClassifier 已注入而非传 None
- **操作步骤:**
  1. [A] `grep -A3 'LlmAutoClassifier::new' rust-agent-tui/src/app/agent.rs` → 期望包含: `LlmAutoClassifier::new`
  2. [A] `grep -n 'auto_classifier' rust-agent-tui/src/app/agent.rs | grep -v '//'` → 期望包含: `Some(Arc::new(`

### 场景 3：HITL 多模式决策

#### - [ ] 3.1 HITL 全量单元测试通过
- **来源:** spec-plan.md Task 3 / spec-design.md §HumanInTheLoopMiddleware 改造
- **目的:** 5 种模式决策、is_edit_tool、process_batch 混合场景
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- hitl::tests 2>&1 | tail -15` → 期望包含: `test result: ok`

#### - [ ] 3.2 is_edit_tool 和 with_shared_mode 函数存在
- **来源:** spec-plan.md Task 3 检查步骤
- **目的:** 确认公共 API 完整
- **操作步骤:**
  1. [A] `grep -n 'pub fn is_edit_tool\|pub fn with_shared_mode' rust-agent-middlewares/src/hitl/mod.rs` → 期望包含: 两行均存在

#### - [ ] 3.3 disabled() 和 new() 向后兼容
- **来源:** spec-plan.md Task 3 / spec-design.md §实现要点
- **目的:** 旧构造函数行为不变
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- hitl::tests::test_disabled_allows_all hitl::tests::test_approve_passes_through 2>&1 | tail -5` → 期望包含: `test result: ok`

### 场景 4：TUI 共享状态注入

#### - [ ] 4.1 App struct 和 AgentRunConfig 包含 permission_mode 字段
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认共享状态在完整链路中传递
- **操作步骤:**
  1. [A] `grep -n 'permission_mode' rust-agent-tui/src/app/mod.rs | head -5` → 期望包含: 结构体字段和 new() 初始化
  2. [A] `grep -n 'permission_mode' rust-agent-tui/src/app/agent.rs | head -8` → 期望包含: AgentRunConfig 字段、解构、with_shared_mode 调用
  3. [A] `grep -n 'permission_mode' rust-agent-tui/src/app/agent_ops.rs` → 期望包含: `permission_mode: self.permission_mode.clone()`

#### - [ ] 4.2 main.rs 初始模式从环境变量解析
- **来源:** spec-plan.md Task 4 / spec-design.md §YOLO_MODE 环境变量兼容
- **目的:** YOLO_MODE/CLI 参数决定初始模式
- **操作步骤:**
  1. [A] `grep -n 'initial_mode\|permission_mode.store' rust-agent-tui/src/main.rs` → 期望包含: `app.permission_mode.store(initial_mode)`

#### - [ ] 4.3 run_universal_agent 使用 with_shared_mode 而非 from_env
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认 HITL 中间件通过新模式构造
- **操作步骤:**
  1. [A] `grep -c 'from_env' rust-agent-tui/src/app/agent.rs` → 期望精确: `0`
  2. [A] `grep -c 'with_shared_mode' rust-agent-tui/src/app/agent.rs` → 期望包含: `1`

### 场景 5：Shift+Tab 键绑定与状态栏显示

#### - [ ] 5.1 BackTab 键绑定拦截存在
- **来源:** spec-plan.md Task 5 / spec-design.md §Shift+Tab 键绑定
- **目的:** Shift+Tab 触发模式循环切换
- **操作步骤:**
  1. [A] `grep -n 'BackTab\|permission_mode.cycle' rust-agent-tui/src/event.rs` → 期望包含: `BackTab` 和 `permission_mode.cycle()`

#### - [ ] 5.2 状态栏渲染权限模式标签
- **来源:** spec-plan.md Task 5 / spec-design.md §状态栏显示
- **目的:** 状态栏第一位显示当前模式名称和颜色
- **操作步骤:**
  1. [A] `grep -n 'PermissionMode\|permission_mode.load' rust-agent-tui/src/ui/main_ui/status_bar.rs` → 期望包含: match 5 种模式 + load() 调用
  2. [A] `grep -c 'DEFAULT\|AUTO-EDIT\|AUTO\|YOLO\|NO-ASK' rust-agent-tui/src/ui/main_ui/status_bar.rs` → 期望精确: `5`

#### - [ ] 5.3 Headless 测试全部通过（含权限模式测试）
- **来源:** spec-plan.md Task 4+5 检查步骤
- **目的:** 端到端验证状态栏渲染、模式切换、初始模式
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- headless::tests 2>&1 | tail -15` → 期望包含: `test result: ok`

#### - [ ] 5.4 mode_highlight_until 字段存在
- **来源:** spec-plan.md Task 5 / spec-design.md §TUI 层集成
- **目的:** 模式切换后 1.5s 闪烁高亮
- **操作步骤:**
  1. [A] `grep -n 'mode_highlight_until' rust-agent-tui/src/app/mod.rs` → 期望包含: 结构体字段和 new() 初始化
  2. [A] `grep -n 'mode_highlight_until' rust-agent-tui/src/app/panel_ops.rs` → 期望包含: `mode_highlight_until: None`

### 场景 6：全量回归与兼容性

#### - [ ] 6.1 Workspace 全量测试通过
- **来源:** spec-plan.md Task 6 / spec-design.md §验收标准
- **目的:** 确保无回归
- **操作步骤:**
  1. [A] `cargo test --workspace 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [ ] 6.2 YOLO_MODE 环境变量兼容性
- **来源:** spec-design.md §YOLO_MODE 环境变量兼容
- **目的:** YOLO_MODE=true 时 disabled() 行为不变
- **操作步骤:**
  1. [A] `YOLO_MODE=true cargo test -p rust-agent-middlewares --lib -- hitl::tests::test_disabled_allows_all 2>&1 | tail -5` → 期望包含: `test result: ok`

---

## 验收后清理

本功能无后台服务启动，无需清理。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | SharedPermissionMode 单元测试 | 1 | 0 | ⬜ |
| 场景 1 | 1.2 | 基础类型文件与导出 | 1 | 0 | ⬜ |
| 场景 2 | 2.1 | AutoClassifier 单元测试 | 1 | 0 | ⬜ |
| 场景 2 | 2.2 | Auto 分类器 TUI 层接入 | 2 | 0 | ⬜ |
| 场景 3 | 3.1 | HITL 全量单元测试 | 1 | 0 | ⬜ |
| 场景 3 | 3.2 | is_edit_tool/with_shared_mode 存在 | 1 | 0 | ⬜ |
| 场景 3 | 3.3 | 旧构造函数向后兼容 | 1 | 0 | ⬜ |
| 场景 4 | 4.1 | 共享状态完整链路 | 3 | 0 | ⬜ |
| 场景 4 | 4.2 | 初始模式环境变量解析 | 1 | 0 | ⬜ |
| 场景 4 | 4.3 | with_shared_mode 替代 from_env | 2 | 0 | ⬜ |
| 场景 5 | 5.1 | BackTab 键绑定 | 1 | 0 | ⬜ |
| 场景 5 | 5.2 | 状态栏模式标签渲染 | 2 | 0 | ⬜ |
| 场景 5 | 5.3 | Headless 测试全部通过 | 1 | 0 | ⬜ |
| 场景 5 | 5.4 | mode_highlight_until 字段 | 2 | 0 | ⬜ |
| 场景 6 | 6.1 | Workspace 全量回归 | 1 | 0 | ⬜ |
| 场景 6 | 6.2 | YOLO_MODE 兼容性 | 1 | 0 | ⬜ |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
