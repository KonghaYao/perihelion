# /compact 上下文压缩指令 人工验收清单

**生成时间:** 2026-03-24 00:00
**关联计划:** ./spec-plan.md
**关联设计:** ./spec-design.md

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 确认 Rust 工具链可用: `rustc --version`
- [ ] [AUTO] 编译 rust-agent-tui: `cargo build -p rust-agent-tui 2>&1 | grep -E "^error" | head -5`
- [ ] [AUTO] 确认 API Key 已配置（至少一个）: `(test -n "$ANTHROPIC_API_KEY" || test -n "$OPENAI_API_KEY") && echo "已配置" || echo "未配置"`
- [ ] [MANUAL] 准备一个终端窗口用于运行 TUI（本清单中 [H] 步骤均需在此窗口操作）

### 测试数据准备

TUI 是无状态应用，无需额外数据库 seed。每个 [H] 场景均从干净状态启动。

---

## 验收项目

### 场景 1：代码结构验证

#### - [x] 1.1 编译无错误

- **来源:** Task 1/2/3 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | grep -E "^error"` → 期望: 无输出（无编译错误）
  2. [A] `cargo build -p rust-agent-tui 2>&1 | grep "non-exhaustive"` → 期望: 无输出（AgentEvent match 覆盖完整）
- **异常排查:**
  - 若出现 `cannot find function compact_task`：检查 `rust-agent-tui/src/app/agent.rs` 末尾是否存在 `pub async fn compact_task`
  - 若出现 `non-exhaustive patterns`：检查 `handle_agent_event` 中是否遗漏了 `CompactDone`/`CompactError` 分支

#### - [x] 1.2 关键符号存在性

- **来源:** Task 1/2/3 检查步骤
- **操作步骤:**
  1. [A] `grep -n "CompactDone\|CompactError" rust-agent-tui/src/app/mod.rs` → 期望: 至少 3 行输出（枚举定义 + 两个处理臂）
  2. [A] `grep -n "pub async fn compact_task" rust-agent-tui/src/app/agent.rs` → 期望: 找到函数定义行（如 `204:pub async fn compact_task`）
  3. [A] `grep -n "fn name\|fn description\|fn execute" rust-agent-tui/src/command/compact.rs` → 期望: 3 行输出，分别对应三个 Command trait 方法
  4. [A] `grep -n "无可压缩的上下文" rust-agent-tui/src/app/mod.rs` → 期望: 找到对应字符串（空历史保护逻辑存在）
- **异常排查:**
  - 若 `compact_task` 不存在：检查 `rust-agent-tui/src/app/agent.rs` 末尾
  - 若 `compact.rs` 的方法缺少：检查 `rust-agent-tui/src/command/compact.rs`

#### - [x] 1.3 全量测试不回归

- **来源:** Task 4 端到端验证场景 1 & 7
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | tail -3` → 期望: 输出包含 `test result: ok`，无 `FAILED`
  2. [A] `cargo test -p rust-create-agent --lib 2>&1 | tail -3` → 期望: 输出包含 `test result: ok`，无 `FAILED`
- **异常排查:**
  - 若 TUI 测试失败：`cargo test -p rust-agent-tui -- --nocapture 2>&1 | grep FAILED`
  - 若核心库测试失败：说明修改破坏了 rust-create-agent，检查 `app/mod.rs` 的 import 变更

---

### 场景 2：命令路由与前缀匹配

#### - [x] 2.1 compact 命令注册正确

- **来源:** Task 3 检查步骤、Task 4 场景 2
- **操作步骤:**
  1. [A] `grep -c "CompactCommand" rust-agent-tui/src/command/mod.rs` → 期望: `1`（恰好注册一次）
  2. [A] `grep -n "pub mod compact" rust-agent-tui/src/command/mod.rs` → 期望: 找到 `pub mod compact;`
  3. [A] `grep -rn "fn name" rust-agent-tui/src/command/*.rs | grep -v "mod.rs"` → 期望: 无其他命令以 `co` 开头（`clear` 开头是 `cl`，`compact` 唯一以 `co` 开头）
- **异常排查:**
  - 若 count 为 0：检查 `command/mod.rs` 是否已 `r.register(Box::new(compact::CompactCommand));`

#### - [x] 2.2 /help 列出 compact 命令描述

- **来源:** Task 3 检查步骤 4
- **操作步骤:**
  1. [A] `grep -n '"compact"\|压缩对话上下文' rust-agent-tui/src/command/compact.rs` → 期望: 输出包含 name 返回 "compact" 和 description 描述行
  2. [H] 启动 TUI：`cargo run -p rust-agent-tui -- -y`，在输入框输入 `/help` 并按 Enter，观察消息列表中是否出现 `compact` 及其描述"压缩对话上下文（调用 LLM 生成摘要）" → 是/否
- **异常排查:**
  - 若 [H] 未显示 compact：确认 `default_registry()` 中已注册，重新 `cargo build`

---

### 场景 3：TUI 行为验证

> 以下 [H] 步骤均需启动 TUI：`cargo run -p rust-agent-tui -- -y`（YOLO 模式）
> 前提：已配置 `ANTHROPIC_API_KEY` 或 `OPENAI_API_KEY`

#### - [x] 3.1 空历史边界保护

- **来源:** spec-design.md 验收标准 §历史消息为空
- **操作步骤:**
  1. [A] `grep -n "agent_state_messages.is_empty" rust-agent-tui/src/app/mod.rs` → 期望: 找到该判断（保护逻辑存在于代码中）
  2. [A] `grep -n "set_loading\|loading.*true" rust-agent-tui/src/app/mod.rs | head -5` → 期望: `start_compact` 中 `set_loading(true)` 位于 `is_empty()` 判断之后
  3. [H] 启动 TUI，不发送任何消息，直接输入 `/compact` 并按 Enter。观察：消息列表中是否出现"无可压缩的上下文"提示，且输入框保持可用（没有变成黄色"处理中…"状态） → 是/否
- **异常排查:**
  - 若 TUI 进入 loading 状态：检查 `start_compact` 中 `is_empty()` 判断是否在 `set_loading(true)` 之前

#### - [x] 3.2 执行期间 loading 状态显示

- **来源:** spec-design.md 验收标准 §执行期间 TUI 显示 loading
- **操作步骤:**
  1. [H] 启动 TUI，先发一条消息（如"你好"）与 AI 交互至少一轮，等对话完成。然后输入 `/compact` 按 Enter。观察：输入框是否立即变成黄色边框并显示"处理中…" → 是/否
  2. [H] 在 loading 期间尝试在输入框输入文字。观察：输入框是否为禁用状态（无法输入或已缓冲提示） → 是/否
- **异常排查:**
  - 若未进入 loading：检查 `start_compact` 方法中 `self.set_loading(true)` 是否被调用
  - 若可以输入：loading 状态下 `textarea` 状态是否正确更新（`build_textarea(true, ...)` 的行为）

#### - [x] 3.3 压缩成功完整流程

- **来源:** spec-design.md 验收标准 §压缩后 agent_state_messages、§view_messages 保留
- **操作步骤:**
  1. [H] 启动 TUI，先与 AI 进行超过 10 轮对话（每轮一问一答，共产生 20+ 条消息）。然后输入 `/compact` 按 Enter，等待压缩完成（loading 消失）。观察：消息列表头部是否出现"📦 上下文已压缩"提示 → 是/否
  2. [H] 压缩完成后，观察：消息列表中显示的消息数量是否不超过 11 条（最多 10 条历史 + 1 条压缩提示） → 是/否
  3. [H] 在压缩后，输入一条与之前对话相关的追问（如"你之前提到的文件叫什么名字？"），等待 AI 回答。观察：AI 的回答是否引用了之前对话中的内容（说明摘要被正确传递） → 是/否
- **异常排查:**
  - 若未出现"📦"提示：检查 `handle_agent_event(CompactDone)` 中 `view_messages.insert(0, ...)` 逻辑
  - 若显示消息超过 11 条：检查 `split_off` 截断逻辑（`keep_count = 10`）

#### - [x] 3.4 instructions 参数对摘要的影响

- **来源:** spec-design.md 验收标准 §传入 instructions 参数
- **操作步骤:**
  1. [H] 启动 TUI，与 AI 对话几轮，内容混杂（涉及文件操作和代码讨论）。然后输入 `/compact 重点保留文件路径和修改内容` 并按 Enter，等待完成。在压缩完成后，发消息"显示一下摘要内容"，观察：AI 回复的摘要中是否特别强调了文件路径相关内容 → 是/否
  2. [H] 对比不带参数的 `/compact` 结果，带参数版本的摘要是否在侧重点上有明显差异 → 是/否
- **异常排查:**
  - 若参数未生效：检查 `compact_task` 中 `instructions` 非空时是否追加到 user_content

---

### 场景 4：错误处理

#### - [x] 4.1 LLM 调用失败不修改历史

- **来源:** spec-design.md 验收标准 §LLM 调用失败
- **操作步骤:**
  1. [A] `grep -n "CompactError" rust-agent-tui/src/app/mod.rs` → 期望: 找到 CompactError 处理臂，其中无 `agent_state_messages` 赋值语句
  2. [A] `grep -A5 "CompactError(msg)" rust-agent-tui/src/app/mod.rs` → 期望: 处理臂仅 push 错误提示 + set_loading(false)，不修改 agent_state_messages
  3. [H] （可选，需要一个会失败的 API Key）临时设置无效 API Key：`ANTHROPIC_API_KEY=invalid cargo run -p rust-agent-tui`，与 AI 交互一轮后输入 `/compact`，等待错误响应。观察：消息列表中是否出现"❌ 压缩失败: ..."错误提示，且历史消息保持不变 → 是/否
- **异常排查:**
  - 若压缩失败后历史被清空：检查 `CompactError` 分支是否意外修改了 `agent_state_messages`

---

### 场景 5：端到端续接对话

#### - [x] 5.1 压缩后 LLM 能基于摘要续接

- **来源:** spec-design.md 验收标准 §压缩后继续发送消息
- **操作步骤:**
  1. [H] 启动 TUI，与 AI 对话：先告诉它"我正在开发一个 Rust 项目，项目叫 perihelion"，然后随机聊几轮。执行 `/compact`，等待完成。然后发消息"我的项目叫什么名字？"，观察：AI 是否能正确回答"perihelion" → 是/否
  2. [H] 在同一会话中，再次执行 `/compact`（连续两次压缩），再提问。观察：AI 是否仍能正确响应 → 是/否
- **异常排查:**
  - 若 AI 忘记了项目名称：检查 `CompactDone` 处理中 `agent_state_messages` 是否正确设置为 `vec![BaseMessage::system(summary)]`
  - 若两次压缩后出现问题：检查 `start_compact` 中对已存在的 system summary 消息的处理（第二次压缩时，历史只有 1 条 system 消息，`is_empty()` 为 false，应正常触发）

#### - [x] 5.2 pending_messages 缓冲机制

- **来源:** spec-design.md §实现要点（compact 期间缓冲）
- **操作步骤:**
  1. [A] `grep -n "pending_messages\|set_loading" rust-agent-tui/src/app/mod.rs | grep -A2 "start_compact"` → 期望: `start_compact` 中调用 `set_loading(true)` 会触发 `build_textarea` 的 loading 状态，pending_messages 机制已通过 set_loading 统一管理
  2. [A] `grep -n "pending_messages" rust-agent-tui/src/app/mod.rs | grep -v "^--"` → 期望: 找到 `pending_messages` 在 `Done`/`Error` 分支中的合并发送逻辑，确认 compact 完成后（`CompactDone` 调用 `set_loading(false)` + `agent_rx = None`）不会阻塞缓冲消息的处理
- **异常排查:**
  - 若 compact 结束后缓冲消息未发送：检查 `set_loading(false)` 是否触发 `pending_messages` 的检查和发送

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 代码结构 | 1.1 | 编译无错误 | 2 | 0 | ✅ | |
| 代码结构 | 1.2 | 关键符号存在性 | 4 | 0 | ✅ | |
| 代码结构 | 1.3 | 全量测试不回归 | 2 | 0 | ✅ | |
| 命令路由 | 2.1 | compact 命令注册正确 | 3 | 0 | ✅ | |
| 命令路由 | 2.2 | /help 列出 compact | 1 | 1 | ✅ | 修复 HelpCommand 未发 render_tx + 缓存 command_help_list |
| TUI 行为 | 3.1 | 空历史边界保护 | 2 | 1 | ✅ | |
| TUI 行为 | 3.2 | loading 状态显示 | 0 | 2 | ✅ | |
| TUI 行为 | 3.3 | 压缩成功完整流程 | 0 | 3 | ✅ | |
| TUI 行为 | 3.4 | instructions 参数侧重 | 0 | 2 | ✅ | |
| 错误处理 | 4.1 | LLM 失败不修改历史 | 2 | 1 | ✅ | 代码审查通过 |
| 续接对话 | 5.1 | 压缩后 LLM 能续接 | 0 | 2 | ✅ | |
| 续接对话 | 5.2 | pending_messages 缓冲 | 2 | 0 | ✅ | 修复 CompactDone/Error 未刷新缓冲消息 |

**验收结论:** ✅ 全部通过（含 3 项修复后通过）
