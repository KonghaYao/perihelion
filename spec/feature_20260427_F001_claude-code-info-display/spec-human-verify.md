# 信息显示 Widget 化升级 人工验收清单

**生成时间:** 2026-04-28
**关联计划:** spec/feature_20260427_F001_claude-code-info-display/spec-plan.md
**关联设计:** spec/feature_20260427_F001_claude-code-info-display/spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 编译 workspace: `cargo build --workspace 2>&1 | tail -5`
- [ ] [AUTO] 运行全量测试基线: `cargo test --workspace 2>&1 | tail -10`

### 测试数据准备
- 无需额外测试数据，headless 测试和单元测试已内建

---

## 验收项目

### 场景 1: SpinnerWidget 基础功能

#### - [x] 1.1 Spinner 模块导出正确
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认 spinner 模块正确注册并重导出
- **操作步骤:**
  1. [A] `grep -c "pub use spinner" perihelion-widgets/src/lib.rs` → 期望包含: 1

#### - [x] 1.2 Spinner 单元测试全部通过
- **来源:** spec-plan.md Task 1 检查步骤 / spec-design.md §SpinnerWidget 验收标准
- **目的:** 确认 animation 帧、smooth_increment、verb 选取、format_elapsed 逻辑正确
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --lib -- spinner 2>&1 | tail -5` → 期望包含: 5 passed

---

### 场景 2: ToolCallWidget 基础功能

#### - [x] 2.1 ToolCall 模块导出正确
- **来源:** spec-plan.md Task 3 检查步骤
- **目的:** 确认 tool_call 模块正确注册并重导出
- **操作步骤:**
  1. [A] `grep -c "pub use tool_call" perihelion-widgets/src/lib.rs` → 期望包含: 1

#### - [x] 2.2 ToolCall 单元测试全部通过
- **来源:** spec-plan.md Task 3 检查步骤 / spec-design.md §ToolCallWidget 验收标准
- **目的:** 确认折叠策略、状态指示器、结果截断逻辑正确
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --lib -- tool_call 2>&1 | tail -5` → 期望包含: 6 passed

---

### 场景 3: MessageBlockWidget 基础功能

#### - [x] 3.1 MessageBlock 模块导出正确
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认 message_block 模块正确注册并重导出
- **操作步骤:**
  1. [A] `grep -c "pub use message_block" perihelion-widgets/src/lib.rs` → 期望包含: 1

#### - [x] 3.2 MessageBlock 单元测试全部通过
- **来源:** spec-plan.md Task 4 检查步骤 / spec-design.md §MessageBlockWidget 验收标准
- **目的:** 确认 diff 检测、diff 行着色、代码高亮逻辑正确
- **操作步骤:**
  1. [A] `cargo test -p perihelion-widgets --lib -- message_block 2>&1 | tail -5` → 期望包含: 5 passed

---

### 场景 4: TUI 集成验证

#### - [x] 4.1 TUI 构建通过
- **来源:** spec-plan.md Task 2/5 检查步骤
- **目的:** 确认 TUI 正确引用新 widget 依赖
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -3` → 期望包含: Finished

#### - [x] 4.2 Spinner 集成 headless 测试通过
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认 spinner 动词在状态栏中正确渲染
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- test_spinner 2>&1 | tail -5` → 期望包含: 1 passed

#### - [x] 4.3 ToolCall 集成 headless 测试通过
- **来源:** spec-plan.md Task 5 检查步骤
- **目的:** 确认 ToolCallWidget 在消息视图中正确渲染
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- test_tool_call 2>&1 | tail -5` → 期望包含: passed

---

### 场景 5: 端到端回归验证

#### - [x] 5.1 Workspace 全量测试通过
- **来源:** spec-plan.md Task 6 端到端验证
- **目的:** 确认三个 widget 的引入未引入回归
- **操作步骤:**
  1. [A] `cargo test --workspace 2>&1 | tail -10` → 期望包含: test result: ok

#### - [x] 5.2 Workspace 整体编译通过
- **来源:** spec-plan.md Task 6 端到端验证
- **目的:** 确认跨 crate 依赖链完整
- **操作步骤:**
  1. [A] `cargo build --workspace 2>&1 | tail -3` → 期望包含: Finished

---

### 场景 6: 视觉效果人工验证

#### - [x] 6.1 Spinner 动画与动词提示
- **来源:** spec-design.md §SpinnerWidget 渲染效果 / spec-plan.md Task 2
- **目的:** 确认 TUI 中 Spinner 动态显示效果正常
- **操作步骤:**
  1. [H] 运行 `cargo run -p rust-agent-tui`，发送任意消息触发 Agent 运行，观察状态栏第二行 → Spinner Braille 帧动画切换流畅、动词提示（如"搜索中"/"思考中"）正确显示 → 是/否

#### - [x] 6.2 工具调用状态指示器与折叠
- **来源:** spec-design.md §ToolCallWidget 渲染效果 / spec-plan.md Task 5
- **目的:** 确认工具调用的状态指示器和折叠行为正确
- **操作步骤:**
  1. [H] 在 TUI 中触发工具调用（如让 Agent 执行 `read_file` 和 `bash`），观察消息区域 → 运行中工具显示闪烁圆点、只读工具默认折叠、写操作工具默认展开 → 是/否

#### - [x] 6.3 代码高亮与 diff 着色效果（用户决定排除此特性）
- **来源:** spec-design.md §MessageBlockWidget Markdown 渲染增强 / spec-plan.md Task 4
- **目的:** 确认代码块语法高亮和 diff 内容颜色渲染
- **操作步骤:**
  1. [H] 让 Agent 生成包含代码块或 diff 的回复，观察渲染效果 → 代码关键字有颜色区分、diff 的 `+` 行绿色/`-` 行红色/`@@` 行蓝色 → 是/否

---

## 验收后清理

无后台服务需清理。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | Spinner 模块导出正确 | 1 | 0 | ⬜ |
| 场景 1 | 1.2 | Spinner 单元测试通过 | 1 | 0 | ⬜ |
| 场景 2 | 2.1 | ToolCall 模块导出正确 | 1 | 0 | ⬜ |
| 场景 2 | 2.2 | ToolCall 单元测试通过 | 1 | 0 | ⬜ |
| 场景 3 | 3.1 | MessageBlock 模块导出正确 | 1 | 0 | ⬜ |
| 场景 3 | 3.2 | MessageBlock 单元测试通过 | 1 | 0 | ⬜ |
| 场景 4 | 4.1 | TUI 构建通过 | 1 | 0 | ⬜ |
| 场景 4 | 4.2 | Spinner headless 测试 | 1 | 0 | ⬜ |
| 场景 4 | 4.3 | ToolCall headless 测试 | 1 | 0 | ⬜ |
| 场景 5 | 5.1 | Workspace 全量测试 | 1 | 0 | ⬜ |
| 场景 5 | 5.2 | Workspace 整体编译 | 1 | 0 | ⬜ |
| 场景 6 | 6.1 | Spinner 动画与动词提示 | 0 | 1 | ⬜ |
| 场景 6 | 6.2 | 工具调用状态指示器与折叠 | 0 | 1 | ⬜ |
| 场景 6 | 6.3 | 代码高亮与 diff 着色效果 | 0 | 1 | ⬜ |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
