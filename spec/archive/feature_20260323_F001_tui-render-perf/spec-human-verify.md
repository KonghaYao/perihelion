# TUI 渲染性能优化（双线程架构）人工验收清单

**生成时间:** 2026-03-23 18:00
**关联计划:** spec/feature_20260323_F001_tui-render-perf/spec-plan.md
**关联设计:** spec/feature_20260323_F001_tui-render-perf/spec-design.md

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 检查 Rust 工具链可用: `rustc --version`
- [ ] [AUTO] 检查 Cargo 可用: `cargo --version`
- [ ] [AUTO] 编译 rust-agent-tui: `cargo build -p rust-agent-tui 2>&1 | tail -3`
- [ ] [AUTO] 检查 ANTHROPIC_API_KEY 或 OPENAI_API_KEY 已设置（至少一个）: `env | grep -E "ANTHROPIC_API_KEY|OPENAI_API_KEY" | wc -l`

### 测试数据准备

- [ ] [MANUAL] 确保工作目录为项目根目录（包含 Cargo.toml）
- [ ] [MANUAL] 准备 2~3 条测试提问，例如："你好，简单介绍一下自己"、"列出当前目录的文件"

---

## 验收项目

### 场景 1：架构与编译验证

> 验证渲染线程模块代码结构正确，所有自动化检查通过。

#### - [x] 1.1 编译无 error，单元测试全通过

- **来源:** Task 1/2 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"` → 期望: 无任何输出（无 error）
  2. [A] `cargo test -p rust-agent-tui --bin agent-tui -- render_thread 2>&1 | tail -5` → 期望: 输出包含 "2 passed"，"0 failed"
  3. [A] `cargo build -p rust-agent-tui 2>&1 | grep "Finished"` → 期望: 输出包含 "Finished"
- **异常排查:**
  - 如果编译失败: 检查 `rust-agent-tui/src/ui/render_thread.rs` 是否存在，`parking_lot = "0.12"` 是否已添加到 Cargo.toml
  - 如果测试失败: 检查 `spawn_render_thread` 内部的 tokio::spawn 是否在 tokio runtime 中执行

#### - [x] 1.2 渲染线程架构代码结构正确

- **来源:** Task 3/4 检查步骤
- **操作步骤:**
  1. [A] `grep -c "app.view_messages" rust-agent-tui/src/ui/main_ui.rs` → 期望: 输出 `0`（render_messages 不再直接遍历 view_messages）
  2. [A] `grep -c "last_render_version\|cache_updated\|agent_updated" rust-agent-tui/src/main.rs` → 期望: 输出 `3` 或以上（按需重绘逻辑已就位）
  3. [A] `grep -n "render_tx" rust-agent-tui/src/app/mod.rs | wc -l` → 期望: 输出 `6` 或以上（消息路径均已发送 RenderEvent）
- **异常排查:**
  - 如果 app.view_messages 仍在 main_ui.rs 中: render_messages() 改造未完成，检查 Task 4
  - 如果 render_tx 引用数量不足: poll_agent() 中某些事件分支缺少 RenderEvent 发送，检查 Task 3

---

### 场景 2：基本 TUI 功能

> 启动 TUI，验证基本消息发送与流式回复功能正常工作。

#### - [x] 2.1 启动无崩溃，初始界面正常显示

- **来源:** Task 5 端到端验收 #1（基础部分）
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"` → 期望: 无输出（编译成功）
  2. [H] 运行 `cargo run -p rust-agent-tui`，观察 TUI 是否正常启动，显示标题栏"🦀 Rust Agent TUI"、输入框和状态栏 → 是/否
  3. [H] 观察初始系统消息是否正确显示（"Rust Agent TUI 已启动 | ... | 工作目录: ..."），无乱码、无错位 → 是/否
- **异常排查:**
  - 如果启动崩溃: 运行 `RUST_LOG=debug cargo run -p rust-agent-tui 2>&1 | head -30` 查看 panic 信息
  - 如果界面空白: 检查 `App::new()` 中 `spawn_render_thread` 和初始消息发送是否正常执行

#### - [x] 2.2 流式输出逐字渲染正常

- **来源:** Task 5 端到端验收 #1（流式部分）+ spec-design 流式输出增量渲染
- **操作步骤:**
  1. [H] 在输入框中输入"你好，简单介绍一下你自己"并按 Enter 发送，观察 Agent 回复是否以流式方式逐字出现（而非等待全部生成后一次显示） → 是/否
  2. [H] 流式输出过程中，观察消息区域顶部是否显示"◆ Agent  …"（含省略号流式指示器），回复完成后省略号消失 → 是/否
- **异常排查:**
  - 如果回复一次性显示（非流式）: 检查 `AppendChunk` 事件是否正确发送到渲染线程
  - 如果出现 panic 或 TUI 崩溃: 查看终端错误输出，检查 render_thread.rs 中 `AppendChunk` 处理逻辑

---

### 场景 3：渲染性能

> 验证消息增多后滚动和输入仍然流畅，无可感知卡顿。

#### - [x] 3.1 多条消息时滚动流畅无卡顿

- **来源:** Task 5 端到端验收 #2 + spec-design 目标"100 条消息时滚动流畅"
- **操作步骤:**
  1. [H] 在 TUI 中连续发送 5 条以上消息（可复制粘贴发送），等待 Agent 全部回复完毕，消息区有足够内容可滚动 → 是/否
  2. [H] 使用鼠标滚轮或按 PageUp/PageDown 快速上下滚动，观察滚动是否即时响应，无明显延迟或跳帧感 → 是/否
- **异常排查:**
  - 如果滚动卡顿: 检查 `render_messages()` 是否真的从 RenderCache 读取（而非重新遍历 view_messages），检查 Task 4
  - 如果滚动位置异常: 检查 `scroll_offset` 和 `scroll_follow` 的更新逻辑

#### - [x] 3.2 打字输入无延迟

- **来源:** Task 5 端到端验收 #3 + spec-design 目标"输入无延迟"
- **操作步骤:**
  1. [H] 在有多条历史消息（5 条以上）的情况下，在输入框中快速连续打字（如连续输入 20+ 个字符），观察输入框中字符是否即时出现，无可感知延迟 → 是/否
  2. [H] 打字过程中同时观察消息区域：消息区内容稳定不闪烁（无因输入导致的消息区重绘抖动） → 是/否
- **异常排查:**
  - 如果输入有延迟: 检查主循环中 `terminal.draw()` 是否仍在每帧无条件调用，检查按需重绘逻辑是否生效
  - 如果消息区闪烁: 检查 `needs_redraw`/`cache_updated` 的判断条件，避免键盘事件触发不必要的消息区重渲染

---

### 场景 4：功能兼容性

> 验证双线程架构改造后，所有原有功能仍然正常工作。

#### - [x] 4.1 HITL 审批弹窗功能正常

- **来源:** Task 5 端到端验收 #4 + spec-design 兼容性（HITL 审批弹窗）
- **操作步骤:**
  1. [H] 发送一条会触发文件操作的任务，例如："请在当前目录创建一个名为 test_hitl.txt 的文件，内容为 hello"，等待 HITL 审批弹窗弹出 → 是/否
  2. [H] 弹窗弹出后，按 y 键全部批准，观察 Agent 是否继续执行并完成任务（界面恢复正常，无卡死） → 是/否
- **异常排查:**
  - 如果弹窗未弹出: 检查 `YOLO_MODE` 环境变量是否被意外设置，或 HITL channel 是否正常工作
  - 如果审批后界面卡死: 检查 `hitl_confirm()` 的 oneshot 发送是否正常，检查 Task 3 中弹窗事件路径

#### - [x] 4.2 清空对话与历史加载功能正常

- **来源:** Task 5 端到端验收 #5 + spec-design 兼容性（清空/历史）
- **操作步骤:**
  1. [H] 在有多条消息的情况下，在输入框中输入 `/clear` 并按 Enter，观察消息区是否立即清空（只剩空白区域） → 是/否
  2. [H] 重新发送一条消息确认新对话正常工作后，输入 `/history` 打开历史对话浏览器，选择一条历史对话（按 Enter），观察历史消息是否正确加载并显示（无乱码、无缺失） → 是/否
- **异常排查:**
  - 如果清空后仍显示旧消息: 检查 `new_thread()` 中是否发送了 `RenderEvent::Clear`
  - 如果历史消息加载后显示空白: 检查 `open_thread()` 中是否发送了 `RenderEvent::LoadHistory`，检查 Task 3

#### - [x] 4.3 终端 resize 后渲染正确

- **来源:** Task 5 端到端验收 #6 + spec-design 兼容性（Resize）
- **操作步骤:**
  1. [H] 在 TUI 正常运行且有多条消息显示时，用鼠标拖拽终端窗口边缘改变窗口大小（变宽或变窄），观察消息区内容是否随窗口宽度自动重新排版（行包装正确调整） → 是/否
  2. [H] resize 后持续观察 2~3 秒，确认界面稳定无乱码、无残影、无 panic → 是/否
- **异常排查:**
  - 如果 resize 后消息区空白: 检查 `event.rs` 中 `Event::Resize` 处理是否发送了 `RenderEvent::Resize(w)`
  - 如果 resize 后行包装未更新: 检查 render_thread.rs 中 `Resize` 事件的全量重渲染逻辑（`rebuild_all()`）

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 | 1.1 | 编译无 error，单元测试全通过 | 3 个 | 0 个 | ✅ | 自动验证通过 |
| 场景 1 | 1.2 | 渲染线程架构代码结构正确 | 3 个 | 0 个 | ✅ | 自动验证通过 |
| 场景 2 | 2.1 | 启动无崩溃，初始界面正常显示 | 1 个 | 2 个 | ✅ | |
| 场景 2 | 2.2 | 流式输出逐字渲染正常 | 0 个 | 2 个 | ✅ | 流式效果不明显但可接受；省略号指示器 bug 已修复 |
| 场景 3 | 3.1 | 多条消息时滚动流畅无卡顿 | 0 个 | 2 个 | ✅ | 有轻微延迟感但可接受 |
| 场景 3 | 3.2 | 打字输入无延迟 | 0 个 | 2 个 | ✅ | |
| 场景 4 | 4.1 | HITL 审批弹窗功能正常 | 0 个 | 2 个 | ✅ | |
| 场景 4 | 4.2 | 清空对话与历史加载功能正常 | 0 个 | 2 个 | ✅ | |
| 场景 4 | 4.3 | 终端 resize 后渲染正确 | 0 个 | 2 个 | ✅ | |

**验收结论:** ✅ 全部通过
