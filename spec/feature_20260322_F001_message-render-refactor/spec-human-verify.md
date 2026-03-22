# 消息渲染层重构 人工验收清单

**生成时间:** 2026-03-22
**关联计划:** spec/feature_20260322_F001_message-render-refactor/spec-plan.md
**关联设计:** spec/feature_20260322_F001_message-render-refactor/spec-design.md

---

## 验收前准备

### 环境要求
- [x] [AUTO] 检查 Rust 环境: `rustc --version && cargo --version`
- [x] [AUTO] 检查工作目录: `cd /Users/konghayao/code/ai/perihelion && pwd`

---

## 验收项目

### 场景 1：编译构建

#### - [x] 1.1 依赖编译通过（Task 1）
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -3` → 期望: 输出包含 `Finished`，无 `error`
- **异常排查:**
  - 如果编译失败: 检查 `rust-agent-tui/Cargo.toml` 中是否正确添加了 `tui-markdown = "0.3"`

#### - [x] 1.2 代码编译无 Warning（Task 5）
- **来源:** Task 5 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | grep -E '(warning|error)' | wc -l` → 期望: 输出 `0`
  2. [A] `cargo build -p rust-agent-tui 2>&1 | tail -1` → 期望: 输出包含 `Finished`
- **异常排查:**
  - 如果有 warning: 运行 `cargo build -p rust-agent-tui 2>&1 | grep -A3 'warning:'` 查看详情
  - 常见问题: unused imports/fields → 清理未使用的代码

#### - [x] 1.3 全量测试通过（Task 6）
- **来源:** Task 6 End-to-end verification #1
- **操作步骤:**
  1. [A] `cargo test 2>&1 | tail -5` → 期望: 输出包含 `test result: ok`
- **异常排查:**
  - 如果测试失败: 运行 `cargo test 2>&1` 查看详细失败信息

---

### 场景 2：数据模型

#### - [x] 2.1 MessageViewModel 变体完整（Task 2）
- **来源:** Task 2 检查步骤 + spec-design.md
- **操作步骤:**
  1. [A] `grep -c 'UserBubble\|AssistantBubble\|ToolBlock\|SystemNote\|TodoStatus' rust-agent-tui/src/ui/message_view.rs` → 期望: 至少 10（定义 + from_base_message 中各出现一次以上）
- **异常排查:**
  - 如果数量不足: 检查 `rust-agent-tui/src/ui/message_view.rs` 是否定义了全部 5 个变体

#### - [x] 2.2 from_base_message 覆盖全变体（Task 2）
- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -c 'BaseMessage::' rust-agent-tui/src/ui/message_view.rs` → 期望: 至少 4（Human/Ai/Tool/System 各一个 match arm）
- **异常排查:**
  - 如果数量不足: 检查 `from_base_message` 函数的 match arms 是否覆盖所有 BaseMessage 变体

---

### 场景 3：Markdown 集成

#### - [x] 3.1 tui-markdown 集成（Task 3）
- **来源:** Task 3 检查步骤 + spec-design.md
- **操作步骤:**
  1. [A] `grep -c 'tui_markdown' rust-agent-tui/src/ui/markdown.rs` → 期望: 至少 1
  2. [A] `test -f rust-agent-tui/src/ui/markdown.rs && echo "exists"` → 期望: 输出 `exists`
- **异常排查:**
  - 如果文件不存在: 检查 Task 3 执行步骤是否完成

---

### 场景 4：渲染层

#### - [x] 4.1 render_view_model 覆盖全变体（Task 4）
- **来源:** Task 4 检查步骤 + spec-design.md
- **操作步骤:**
  1. [A] `grep -c 'MessageViewModel::' rust-agent-tui/src/ui/message_render.rs` → 期望: 至少 6（每个变体 + collapsed 状态两个 arm）
- **异常排查:**
  - 如果数量不足: 检查 `render_view_model` 函数是否处理了所有 MessageViewModel 变体

#### - [x] 4.2 旧渲染函数已删除（Task 4）
- **来源:** Task 4 检查步骤
- **操作步骤:**
  1. [A] `grep -c 'fn message_to_lines' rust-agent-tui/src/ui/main_ui.rs` → 期望: 0
- **异常排查:**
  - 如果函数仍存在: 删除 `message_to_lines` 函数

---

### 场景 5：App 集成

#### - [x] 5.1 ChatMessage 完全移除（Task 5）
- **来源:** Task 5 检查步骤 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -rn 'ChatMessage' rust-agent-tui/src/ | grep -v '//' | wc -l` → 期望: 0
- **异常排查:**
  - 如果仍有引用: 检查 Task 5 的所有集成步骤是否完成

#### - [x] 5.2 view_messages 替换完成（Task 5）
- **来源:** Task 5 检查步骤
- **操作步骤:**
  1. [A] `grep -rn '\.messages' rust-agent-tui/src/ | grep -v 'view_messages' | grep -v 'agent_state_messages' | grep -v 'state_messages' | grep -v '//' | wc -l` → 期望: 0
- **异常排查:**
  - 如果仍有引用: 检查 App 结构体中的字段是否已正确替换

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | 依赖编译通过 | 1 | 0 | ✓ | |
| 场景 1 | 1.2 | 代码编译无 Warning | 2 | 0 | ✓ | 修复后通过（清理 unused imports/fields） |
| 场景 1 | 1.3 | 全量测试通过 | 1 | 0 | ✓ | |
| 场景 2 | 2.1 | MessageViewModel 变体完整 | 1 | 0 | ✓ | |
| 场景 2 | 2.2 | from_base_message 覆盖全变体 | 1 | 0 | ✓ | |
| 场景 3 | 3.1 | tui-markdown 集成 | 2 | 0 | ✓ | |
| 场景 4 | 4.1 | render_view_model 覆盖全变体 | 1 | 0 | ✓ | |
| 场景 4 | 4.2 | 旧渲染函数已删除 | 1 | 0 | ✓ | |
| 场景 5 | 5.1 | ChatMessage 完全移除 | 1 | 0 | ✓ | |
| 场景 5 | 5.2 | view_messages 替换完成 | 1 | 0 | ✓ | |

**验收结论:** ✓ 全部通过
