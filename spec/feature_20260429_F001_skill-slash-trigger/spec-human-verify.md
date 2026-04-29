# Skills 触发键统一到 / 命名空间 人工验收清单

**生成时间:** 2026-04-29
**关联计划:** spec/feature_20260429_F001_skill-slash-trigger/spec-plan.md
**关联设计:** spec/feature_20260429_F001_skill-slash-trigger/spec-design.md

> 所有验收项均可自动化验证，无需人类参与。仍将生成清单用于自动执行。

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 编译项目: `cargo build -p rust-agent-tui -p rust-agent-middlewares 2>&1 | tail -5`
- [ ] [AUTO] 验证测试框架可用: `cargo test -p rust-agent-tui --lib -- test_snapshot_row_count 2>&1 | tail -5`

### 测试数据准备
- 无需额外测试数据（headless 测试在代码中自包含）

---

## 验收项目

### 场景 1：提示浮层合并

#### - [x] 1.1 `render_skill_hint` 函数已移除
- **来源:** spec-plan.md Task 1 检查步骤 / spec-design.md §2
- **目的:** 确认旧 Skills 浮层函数已删除
- **操作步骤:**
  1. [A] `grep -c 'render_skill_hint' rust-agent-tui/src/ui/main_ui/popups/hints.rs rust-agent-tui/src/ui/main_ui.rs` → 期望精确: `0`

#### - [x] 1.2 `render_command_hint` 函数已移除
- **来源:** spec-plan.md Task 1 检查步骤 / spec-design.md §2
- **目的:** 确认旧命令浮层函数已删除
- **操作步骤:**
  1. [A] `grep -c 'render_command_hint' rust-agent-tui/src/ui/main_ui/popups/hints.rs rust-agent-tui/src/ui/main_ui.rs` → 期望精确: `0`

#### - [x] 1.3 `render_unified_hint` 已在调用点使用
- **来源:** spec-plan.md Task 1 检查步骤
- **目的:** 确认统一浮层已接入渲染
- **操作步骤:**
  1. [A] `grep 'render_unified_hint' rust-agent-tui/src/ui/main_ui.rs` → 期望包含: `popups::hints::render_unified_hint(f, app, chunks[4]);`

#### - [x] 1.4 提示浮层 headless 测试通过
- **来源:** spec-plan.md Task 1 检查步骤 / spec-design.md §2
- **目的:** 确认浮层合并功能正确
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- test_unified_hint 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 2：Tab 补全合并

#### - [x] 2.1 `hint_candidates_count` 中 `#` 分支已移除
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认 `#` 前缀不再产生候选
- **操作步骤:**
  1. [A] `grep -n 'starts_with.*#' rust-agent-tui/src/app/hint_ops.rs` → 期望精确: ``（无输出）

#### - [x] 2.2 `hint_complete` 中 `#` 分支已移除
- **来源:** spec-plan.md Task 2 检查步骤
- **目的:** 确认 `#` 前缀不再处理补全
- **操作步骤:**
  1. [A] `grep -n 'starts_with.*#' rust-agent-tui/src/app/hint_ops.rs` → 期望精确: ``（无输出）

#### - [x] 2.3 hint_ops 单元测试通过
- **来源:** spec-plan.md Task 2 检查步骤 / spec-design.md §3
- **目的:** 确认候选计数和补全逻辑正确
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- hint_ops::tests 2>&1 | tail -15` → 期望包含: `test result: ok`

---

### 场景 3：Enter 触发逻辑 + 消息解析

#### - [x] 3.1 event.rs 中 Skill fallback 逻辑存在
- **来源:** spec-plan.md Task 3 检查步骤 / spec-design.md §4
- **目的:** 确认命令未命中时 fallback 到 Skill
- **操作步骤:**
  1. [A] `grep -n 'skills.iter().find' rust-agent-tui/src/event.rs` → 期望包含: `skills.iter().find`

#### - [x] 3.2 agent_ops.rs 中 `#` 前缀已替换为 `/`
- **来源:** spec-plan.md Task 3 检查步骤 / spec-design.md §5
- **目的:** 确认消息解析使用 `/` 前缀
- **操作步骤:**
  1. [A] `grep -n "starts_with('#')" rust-agent-tui/src/app/agent_ops.rs` → 期望精确: ``（无输出）
  2. [A] `grep -n "starts_with('/')" rust-agent-tui/src/app/agent_ops.rs` → 期望包含: `starts_with`

#### - [x] 3.3 "未知命令"文案已更新为"未知命令或 Skill"
- **来源:** spec-plan.md Task 3 检查步骤 / spec-design.md §4
- **目的:** 确认错误提示覆盖 Skill 场景
- **操作步骤:**
  1. [A] `grep -n '未知命令或 Skill' rust-agent-tui/src/event.rs` → 期望包含: `未知命令或 Skill`

#### - [x] 3.4 Enter 触发链路 headless 测试通过
- **来源:** spec-plan.md Task 3 检查步骤 / spec-design.md §4
- **目的:** 确认命令优先 → Skill fallback → 无匹配报错
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- test_enter_skill 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 3.5 agent_ops 单元测试通过（消息解析）
- **来源:** spec-plan.md Task 3 检查步骤 / spec-design.md §5
- **目的:** 确认 `/skill-name` token 提取正确
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- agent_ops::tests 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 4：提示词与文案更新

#### - [x] 4.1 skills/mod.rs 中不再包含 `#skill_name`
- **来源:** spec-plan.md Task 4 检查步骤 / spec-design.md §6
- **目的:** 确认旧格式引用已清除
- **操作步骤:**
  1. [A] `grep -n '#skill_name' rust-agent-middlewares/src/skills/mod.rs` → 期望精确: ``（无输出）

#### - [x] 4.2 skills/mod.rs 包含 `/skill-name`
- **来源:** spec-plan.md Task 4 检查步骤 / spec-design.md §6
- **目的:** 确认新格式引用已写入
- **操作步骤:**
  1. [A] `grep -n '/skill-name' rust-agent-middlewares/src/skills/mod.rs` → 期望包含: `/skill-name`

#### - [x] 4.3 tips.rs 中不再包含 `# 前缀` 相关 Skills 提示
- **来源:** spec-plan.md Task 4 检查步骤 / spec-design.md §7
- **目的:** 确认旧 TUI 文案已清除
- **操作步骤:**
  1. [A] `grep -n '#' rust-agent-tui/src/ui/tips.rs | grep -i skill` → 期望精确: ``（无输出）

#### - [x] 4.4 tips.rs 包含更新后的合并提示文案
- **来源:** spec-plan.md Task 4 检查步骤 / spec-design.md §7
- **目的:** 确认新文案包含命令和 Skills
- **操作步骤:**
  1. [A] `grep -n '命令和 Skills' rust-agent-tui/src/ui/tips.rs` → 期望包含: `命令和 Skills`

#### - [x] 4.5 tips.rs Tab 补全提示顺序已更新
- **来源:** spec-plan.md Task 4 检查步骤 / spec-design.md §7
- **目的:** 确认 Tab 提示顺序为命令在前
- **操作步骤:**
  1. [A] `grep -n '命令或 Skills 提示中补全' rust-agent-tui/src/ui/tips.rs` → 期望包含: `命令或 Skills 提示中补全`

#### - [x] 4.6 skills 测试通过
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认提示词测试验证新旧格式
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- skills::tests 2>&1 | tail -10` → 期望包含: `test result: ok`

#### - [x] 4.7 tips 测试通过
- **来源:** spec-plan.md Task 4 检查步骤
- **目的:** 确认文案测试验证新旧内容
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui --lib -- tips::tests 2>&1 | tail -10` → 期望包含: `test result: ok`

---

### 场景 5：边界与回归

#### - [x] 5.1 全量测试通过
- **来源:** spec-plan.md Task 5 / spec-design.md 验收标准
- **目的:** 确认无回归问题
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui -p rust-agent-middlewares 2>&1 | tail -20` → 期望包含: `test result: ok`

#### - [x] 5.2 `#` 前缀已从代码中完全移除
- **来源:** spec-plan.md Task 5 / spec-design.md §9 边界情况
- **目的:** 确认所有 `#` 前缀判断已替换
- **操作步骤:**
  1. [A] `grep -rn "starts_with('#')" rust-agent-tui/src/ rust-agent-middlewares/src/` → 期望精确: ``（无输出）

#### - [x] 5.3 旧文案（`#skill_name`、`# 前缀`）已全部移除
- **来源:** spec-plan.md Task 5 / spec-design.md §6 §7
- **目的:** 确认无残留旧格式引用
- **操作步骤:**
  1. [A] `grep -rn '#skill_name\|# 前缀' rust-agent-middlewares/src/skills/ rust-agent-tui/src/ui/tips.rs` → 期望精确: ``（无输出，或仅出现在测试断言消息中）

---

## 验收后清理

无后台服务需要清理。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | render_skill_hint 已移除 | 1 | 0 | ✅ |
| 场景 1 | 1.2 | render_command_hint 已移除 | 1 | 0 | ✅ |
| 场景 1 | 1.3 | render_unified_hint 已接入 | 1 | 0 | ✅ |
| 场景 1 | 1.4 | 浮层 headless 测试通过 | 1 | 0 | ✅ |
| 场景 2 | 2.1 | candidates_count # 分支移除 | 1 | 0 | ✅ |
| 场景 2 | 2.2 | hint_complete # 分支移除 | 1 | 0 | ✅ |
| 场景 2 | 2.3 | hint_ops 单元测试通过 | 1 | 0 | ✅ |
| 场景 3 | 3.1 | Skill fallback 逻辑存在 | 1 | 0 | ✅ |
| 场景 3 | 3.2 | agent_ops # → / 前缀替换 | 2 | 0 | ✅ |
| 场景 3 | 3.3 | 未知命令文案更新 | 1 | 0 | ✅ |
| 场景 3 | 3.4 | Enter 触发链路测试通过 | 1 | 0 | ✅ |
| 场景 3 | 3.5 | agent_ops 消息解析测试通过 | 1 | 0 | ✅ |
| 场景 4 | 4.1 | #skill_name 已清除 | 1 | 0 | ✅ |
| 场景 4 | 4.2 | /skill-name 已写入 | 1 | 0 | ✅ |
| 场景 4 | 4.3 | tips # 前缀文案已清除 | 1 | 0 | ✅ |
| 场景 4 | 4.4 | tips 合并提示文案存在 | 1 | 0 | ✅ |
| 场景 4 | 4.5 | tips Tab 补全顺序更新 | 1 | 0 | ✅ |
| 场景 4 | 4.6 | skills 测试通过 | 1 | 0 | ✅ |
| 场景 4 | 4.7 | tips 测试通过 | 1 | 0 | ✅ |
| 场景 5 | 5.1 | 全量测试无回归 | 1 | 0 | ✅ |
| 场景 5 | 5.2 | # 前缀判断完全移除 | 1 | 0 | ✅ |
| 场景 5 | 5.3 | 旧文案完全移除 | 1 | 0 | ✅ |

**验收结论:** ✅ 全部通过 / ⬜ 存在问题
