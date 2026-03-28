# ask-user-question-align 人工验收清单

**生成时间:** 2026-03-28 00:00
**关联计划:** [spec-plan.md](./spec-plan.md)
**关联设计:** [spec-design.md](./spec-design.md)

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 确认 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 全 workspace 编译通过: `cargo build 2>&1 | grep "^error" | wc -l`

### 测试说明

本 feature 为纯代码结构变更（工具名称重命名、Schema 对齐、TUI/前端展示调整），无需启动外部服务。所有自动化验收项均为静态源码检查 + 编译验证 + 前端文件内容检查。TUI 弹窗的视觉展示需人工运行 TUI 后目视验证。

---

## 验收项目

### 场景 1：工具名称与 Schema 对齐

#### - [x] 1.1 工具名称已重命名为 `ask_user_question`

- **来源:** Task 2 检查步骤 + Task 5 Acceptance 场景2
- **操作步骤:**
  1. [A] 检查中间件工具名 → `grep -r '"ask_user_question"' rust-agent-middlewares/src/ask_user/mod.rs rust-agent-middlewares/src/tools/ask_user_tool.rs` → 期望: 两个文件各至少有 1 处匹配
  2. [A] 确认旧工具名已完全移除 → `grep -r '"ask_user"' rust-agent-middlewares/src/ask_user/ rust-agent-middlewares/src/tools/ask_user_tool.rs 2>/dev/null || echo "无残留"` → 期望: 输出 `无残留`（无残留旧名称）
- **异常排查:**
  - 若仍有 `"ask_user"` 残留: 检查 `rust-agent-middlewares/src/ask_user/mod.rs` 中 `name: "ask_user_question"` 是否正确

#### - [x] 1.2 旧字段 `allow_custom_input` / `placeholder` / `type` 已完全移除

- **来源:** Task 1 执行步骤 + Task 5 Acceptance 场景3 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] 检查核心库数据结构中无旧字段 → `grep -r "allow_custom_input\|\.placeholder\b" rust-create-agent/src/interaction/ rust-create-agent/src/ask_user/ 2>/dev/null || echo "无残留"` → 期望: `无残留`
  2. [A] 检查中间件和 TUI 中无旧字段引用 → `grep -r "allow_custom_input\|\.placeholder\b" rust-agent-middlewares/src/ rust-agent-tui/src/ 2>/dev/null || echo "无残留"` → 期望: `无残留`
- **异常排查:**
  - 若有残留: 检查 `ask_user_prompt.rs` 中是否存在 `allow_custom_input` 字段访问

#### - [x] 1.3 新字段 `header` 和 `description` 已添加到数据结构

- **来源:** Task 1 执行步骤 + spec-design.md 数据结构变更
- **操作步骤:**
  1. [A] 确认 `QuestionItem` 含 `header` → `grep -n "pub header:" rust-create-agent/src/interaction/mod.rs` → 期望: 找到 `pub header: String`
  2. [A] 确认 `QuestionOption` 含 `description` → `grep -n "pub description:" rust-create-agent/src/interaction/mod.rs rust-create-agent/src/ask_user/mod.rs` → 期望: 两个文件中均有 `pub description: Option<String>`
- **异常排查:**
  - 若字段缺失: 重新检查 `rust-create-agent/src/interaction/mod.rs` 的结构体定义

---

### 场景 2：TUI 弹窗展示

#### - [x] 2.1 TUI 弹窗 Tab 行使用 `header` 字段

- **来源:** Task 3 执行步骤 + spec-design.md TUI 展示变更
- **操作步骤:**
  1. [A] 确认源码中 Tab 渲染使用 `header` → `grep -n "header" rust-agent-tui/src/ui/main_ui/popups/ask_user.rs` → 期望: 找到 `q.data.header` 相关引用
  2. [H] 本地运行 TUI（`cargo run -p rust-agent-tui -- -y`），触发一个带 `ask_user_question` 调用的 Agent，观察弹出的问题弹窗 Tab 行是否显示 `header` 字段内容（而非截取 description 前8字）→ 是/否
  3. [H] 确认 Tab 行格式为 `"✓/空格 {header文字}"` 而非 `"Q1: {截取描述}"` → 是/否
- **异常排查:**
  - 若 Tab 仍显示旧格式: 检查 `rust-agent-tui/src/ui/main_ui/popups/ask_user.rs` 中 tab_spans 构建逻辑

#### - [x] 2.2 TUI 弹窗选项列表在 `label` 下方展示 `description`

- **来源:** Task 3 执行步骤 + spec-design.md TUI 展示变更
- **操作步骤:**
  1. [A] 确认源码中选项渲染含 description 逻辑 → `grep -n "opt.description\|DarkGray" rust-agent-tui/src/ui/main_ui/popups/ask_user.rs` → 期望: 找到 `opt.description` 判断和 `DarkGray` 颜色设置
  2. [H] 运行 TUI，触发包含 `description` 字段的选项（如 `{label: "选项A", description: "说明文字"}`），确认每个有 description 的选项下方以 DarkGray 颜色缩进展示说明文字 → 是/否
- **异常排查:**
  - 若 description 未显示: 确认传入的 `AskUserQuestionData.options[].description` 不为 `None`

#### - [x] 2.3 TUI 弹窗自定义输入行始终显示（不受旧 `allow_custom_input` 控制）

- **来源:** Task 3 执行步骤 + spec-design.md TUI 展示变更
- **操作步骤:**
  1. [A] 确认 `total_rows()` 始终 +1 → `grep -n "total_rows\|+ 1" rust-agent-tui/src/app/ask_user_prompt.rs | head -10` → 期望: `total_rows` 返回 `options.len() + 1`（无条件判断）
  2. [H] 运行 TUI，触发 `ask_user_question` 弹窗，确认弹窗底部始终有自定义输入框（无论选项数量多少）→ 是/否
- **异常排查:**
  - 若自定义输入框消失: 检查 `ask_user_prompt.rs` 中是否残留 `allow_custom_input` 条件

---

### 场景 3：前端 AskUserDialog 展示

#### - [x] 3.1 前端弹窗显示 `header` 芯片标签

- **来源:** Task 4 检查步骤 + spec-design.md 前端展示变更
- **操作步骤:**
  1. [A] 确认 JS 含 header chip → `grep -c "ask-user-header-chip" rust-relay-server/web/components/AskUserDialog.js` → 期望: 输出 `1` 或以上
  2. [A] 确认 CSS 含 header chip 样式 → `grep -c "ask-user-header-chip" rust-relay-server/web/components/AskUserDialog.css` → 期望: 输出 `1` 或以上
  3. [H] 在浏览器中打开 Relay Server 前端（需先启动 relay server），触发一个含 `header` 字段的 ask_user_question 请求，确认弹窗中每个问题上方有小的芯片标签显示 header 内容 → 是/否
- **异常排查:**
  - 若芯片未显示: 检查 `AskUserDialog.js` 中 `q.header && html\`<span class="ask-user-header-chip">\`` 逻辑
  - 若样式不对: 检查 `AskUserDialog.css` 中 `.ask-user-header-chip` 定义

#### - [x] 3.2 前端弹窗选项展示 `description`

- **来源:** Task 4 检查步骤 + spec-design.md 前端展示变更
- **操作步骤:**
  1. [A] 确认 JS 含 opt-desc 类 → `grep -c "ask-user-opt-desc" rust-relay-server/web/components/AskUserDialog.js` → 期望: 输出 `2` 或以上（radio + checkbox 两处）
  2. [H] 在浏览器前端中，触发含 `description` 字段的 ask_user_question 问题，确认选项 label 下方以较小灰色字体展示 description 内容 → 是/否
- **异常排查:**
  - 若 description 未显示: 确认 Relay Server 传给前端的 JSON 消息中包含 `options[].description` 字段

---

### 场景 4：编译与集成

#### - [x] 4.1 全 workspace 编译通过，无编译错误

- **来源:** Task 5 Acceptance 场景1 + Task 1/2/3/4 检查步骤
- **操作步骤:**
  1. [A] 全量编译检查 → `cargo build 2>&1 | grep "^error"` → 期望: 无输出（无任何 `error:` 行）
  2. [A] 运行所有测试 → `cargo test 2>&1 | grep "FAILED\|test result"` → 期望: 所有 crate 均 `test result: ok`，无 `FAILED`
- **异常排查:**
  - 若编译失败: 根据错误信息定位到具体 crate，检查数据结构字段名是否与实际访问一致
  - 若测试失败: 运行 `cargo test -p [失败的crate] -- --nocapture` 查看详细输出

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| Schema 对齐 | 1.1 | 工具名称已重命名 | 2 | 0 | ✅ | |
| Schema 对齐 | 1.2 | 旧字段已完全移除 | 2 | 0 | ✅ | |
| Schema 对齐 | 1.3 | 新字段已添加 | 2 | 0 | ✅ | |
| TUI 展示 | 2.1 | Tab 行用 header | 1 | 2 | ✅ | |
| TUI 展示 | 2.2 | 选项 description 展示 | 1 | 1 | ✅ | |
| TUI 展示 | 2.3 | 自定义输入始终显示 | 1 | 1 | ✅ | |
| 前端展示 | 3.1 | header chip 注入 | 2 | 1 | ✅ | |
| 前端展示 | 3.2 | 选项 description 注入 | 1 | 1 | ✅ | |
| 编译集成 | 4.1 | 全量编译+测试通过 | 2 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
