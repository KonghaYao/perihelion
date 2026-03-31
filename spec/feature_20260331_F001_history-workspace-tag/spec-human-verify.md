# 历史面板工作区过滤 人工验收清单

**生成时间:** 2026-03-31 17:05
**关联计划:** `spec/feature_20260331_F001_history-workspace-tag/spec-plan.md`
**关联设计:** `spec/feature_20260331_F001_history-workspace-tag/spec-design.md`

---

## 验收前准备

### 环境要求
- [x] [AUTO] 编译项目: `cargo build -p rust-agent-tui 2>&1 | tail -3`

---

## 验收项目

### 场景 1：单元测试验证

#### - [x] 1.1 全量测试通过
- **来源:** spec-plan.md Task 2 验收步骤 1
- **目的:** 确认所有测试（含新增的过滤逻辑单元测试）均通过
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | tail -5` → 期望包含: all passed

#### - [x] 1.2 过滤逻辑单元测试
- **来源:** spec-plan.md Task 1 单元测试
- **目的:** 确认 3 个过滤场景（匹配、无匹配、全匹配）正确
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui -- filter_keeps_matching_cwd 2>&1 | tail -1` → 期望包含: ok
  2. [A] `cargo test -p rust-agent-tui -- filter_returns_empty_when_no_match 2>&1 | tail -1` → 期望包含: ok
  3. [A] `cargo test -p rust-agent-tui -- filter_keeps_all_when_all_match 2>&1 | tail -1` → 期望包含: ok

---

### 场景 2：历史面板工作区过滤

#### - [x] 2.1 历史面板只显示当前工作区对话
- **来源:** spec-plan.md Task 2 验收步骤 2 / spec-design.md 验收标准
- **目的:** 确认 `/history` 面板按 cwd 过滤，标题包含当前路径
- **操作步骤:**
  1. [H] 启动 `cargo run -p rust-agent-tui`，输入 `/history`，观察面板标题和列表内容 → 是/否

---

### 场景 3：功能不受影响

#### - [x] 3.1 新建对话功能正常
- **来源:** spec-plan.md Task 2 验收步骤 3 / spec-design.md 验收标准
- **目的:** 确认新增过滤逻辑不影响新建对话流程
- **操作步骤:**
  1. [H] 在历史面板中选择"新建对话"，确认对话被正确清空并创建 → 是/否

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | 全量测试通过 | 1 | 0 | ✅ |
| 场景 1 | 1.2 | 过滤逻辑单元测试 | 3 | 0 | ✅ |
| 场景 2 | 2.1 | 历史面板只显示当前工作区对话 | 0 | 1 | ✅ |
| 场景 3 | 3.1 | 新建对话功能正常 | 0 | 1 | ✅ |

**验收结论:** ✅ 全部通过
