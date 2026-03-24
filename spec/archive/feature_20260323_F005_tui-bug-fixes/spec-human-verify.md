# TUI Bug Fixes 人工验收清单

**生成时间:** 2026-03-23
**关联计划:** spec-plan.md
**关联设计:** spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 编译项目: `cargo build -p rust-agent-tui 2>&1 | tail -3`
- [ ] [AUTO] 确认已配置 API Key: `test -n "$ANTHROPIC_API_KEY" -o -n "$OPENAI_API_KEY" && echo "OK" || echo "MISSING"`
- [ ] [AUTO] 全量测试通过: `cargo test -p rust-agent-tui 2>&1 | tail -5`

### 测试数据准备
- [ ] 准备一段含有 3 行以上的多行文本用于粘贴测试（例如复制本文件的某几行）

---

## 验收项目

### 场景 1：弹窗/面板内容超长滚动

#### - [x] 1.1 AskUser 弹窗超长内容可滚动
- **来源:** spec-design.md 验收标准 + Task 1
- **操作步骤:**
  1. [A] `grep -c 'area.height \* 4 / 5' rust-agent-tui/src/ui/main_ui.rs` → 期望: 输出 >= 4（四个弹窗函数均有高度限制）
  2. [A] `grep -c 'scroll_offset' rust-agent-tui/src/app/mod.rs` → 期望: 输出 >= 3（AskUserBatchPrompt 声明 + 初始化 + 使用）
  3. [H] 启动 TUI（`cargo run -p rust-agent-tui`），发送一条会触发 Agent 提问的消息（如 "帮我创建一个文件，先问我几个问题"），当 AskUser 弹窗出现时，观察弹窗是否被限制在屏幕高度 80% 以内，按 ↑↓ 是否能滚动查看所有选项 → 是/否
- **异常排查:**
  - 如果弹窗溢出屏幕: 检查 `render_ask_user_popup` 中 `popup_height` 是否包含 `.min(area.height * 4 / 5)`
  - 如果 ↑↓ 无法滚动: 检查 `ask_user_move` 是否调用了 `ensure_cursor_visible` 更新 scroll_offset

#### - [x] 1.2 Model/Agents/Thread 面板超长内容可滚动
- **来源:** spec-design.md 验收标准 + Task 1
- **操作步骤:**
  1. [A] `grep -c 'scroll_offset' rust-agent-tui/src/app/agent_panel.rs rust-agent-tui/src/app/model_panel.rs rust-agent-tui/src/thread/browser.rs` → 期望: 每个文件至少 1 处
  2. [H] 启动 TUI，输入 `/model` 打开模型面板，观察面板是否正常渲染且不溢出；输入 `/agents` 打开 Agent 面板，确认同样有高度限制 → 是/否
- **异常排查:**
  - 如果面板溢出: 检查对应 render 函数中 `popup_height` 的 `.min(area.height * 4 / 5)` 约束

### 场景 2：粘贴多行文本

#### - [x] 2.1 粘贴含换行符文本不触发提交
- **来源:** spec-design.md 验收标准 + Task 2
- **操作步骤:**
  1. [A] `grep -n 'EnableBracketedPaste' rust-agent-tui/src/main.rs` → 期望: 至少 2 处（初始化 Enable + 退出 Disable）
  2. [A] `grep -n 'Event::Paste' rust-agent-tui/src/event.rs` → 期望: 至少 1 处匹配
  3. [H] 启动 TUI，复制一段含换行的多行文本（至少 3 行），粘贴到输入框。观察：文本是否完整保留在输入框内（显示多行），且未触发消息提交（消息区无新用户消息出现） → 是/否
- **异常排查:**
  - 如果粘贴仍然触发提交: 确认终端支持 bracketed paste（macOS Terminal.app 可能不支持，建议用 iTerm2 或 WezTerm 测试）
  - 如果粘贴后仅显示第一行: 检查 `Event::Paste` 分支是否正确调用了 `insert_str`

### 场景 3：Loading 状态输入缓冲

#### - [x] 3.1 Loading 状态下输入框可编辑
- **来源:** spec-design.md 验收标准 + Task 3
- **操作步骤:**
  1. [A] `grep -c 'pending_messages' rust-agent-tui/src/app/mod.rs` → 期望: >= 3（声明、初始化、使用）
  2. [H] 启动 TUI，发送一条需要较长处理时间的消息（如 "用 bash 运行 sleep 5 并输出结果"）。在 Agent 运行中（输入框边框为黄色 "处理中…" 状态），尝试在输入框中键入文字。观察：能否正常输入文字（光标移动、字符出现） → 是/否
- **异常排查:**
  - 如果无法输入: 检查 `event.rs` 中字符输入分支的 `!app.loading` guard 是否已移除

#### - [x] 3.2 缓冲消息时标题显示计数
- **来源:** spec-design.md 验收标准 + Task 3
- **操作步骤:**
  1. [A] `grep -n '已缓存' rust-agent-tui/src/app/mod.rs` → 期望: 1 处匹配（`build_textarea` 函数中）
  2. [H] 在 Agent 运行中键入一些文字并按 Enter。观察：输入框是否清空，且输入框标题是否变为 "处理中… (已缓存 1 条)"。再输入一条按 Enter，标题是否变为 "处理中… (已缓存 2 条)" → 是/否
- **异常排查:**
  - 如果标题未更新: 检查 `update_textarea_hint` 是否在 `pending_messages.push` 后被调用

#### - [x] 3.3 Agent 完成后自动合并发送缓冲消息
- **来源:** spec-design.md 验收标准 + Task 3
- **操作步骤:**
  1. [A] `grep -A15 'AgentEvent::Done' rust-agent-tui/src/app/mod.rs | grep 'pending_messages'` → 期望: 至少 1 行匹配
  2. [A] `grep -A15 'AgentEvent::Error' rust-agent-tui/src/app/mod.rs | grep 'pending_messages'` → 期望: 至少 1 行匹配
  3. [H] 在 Agent 运行中缓存 1-2 条消息（按 Enter），等待 Agent 完成。观察：Agent 完成后是否自动开始处理缓冲消息（消息区出现合并的用户消息，且 Agent 开始新一轮执行） → 是/否
- **异常排查:**
  - 如果缓冲消息未发送: 检查 `handle_agent_event` 的 `Done` 分支中 `pending_messages` 检查逻辑

#### - [x] 3.4 无缓冲消息时正常结束
- **来源:** spec-design.md 验收标准 + Task 4 Acceptance
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | tail -5` → 期望: 所有测试通过（N passed, 0 failed），无 panic
  2. [H] 启动 TUI，发送一条简单消息（如 "你好"），等待 Agent 完成。观察：完成后输入框恢复正常（青色边框 "输入" 标题），无空消息被自动发送 → 是/否
- **异常排查:**
  - 如果自动发送空消息: 检查 `pending_messages.is_empty()` 判断是否正确

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 弹窗滚动 | 1.1 | AskUser 弹窗超长滚动 | 2 | 1 | ⬜ | |
| 弹窗滚动 | 1.2 | Model/Agents/Thread 面板滚动 | 1 | 1 | ⬜ | |
| 粘贴换行 | 2.1 | 粘贴多行文本不触发提交 | 2 | 1 | ⬜ | |
| 输入缓冲 | 3.1 | Loading 状态输入框可编辑 | 1 | 1 | ⬜ | |
| 输入缓冲 | 3.2 | 缓冲消息标题显示计数 | 1 | 1 | ⬜ | |
| 输入缓冲 | 3.3 | Agent 完成后合并发送 | 2 | 1 | ⬜ | |
| 输入缓冲 | 3.4 | 无缓冲消息时正常结束 | 1 | 1 | ⬜ | |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
