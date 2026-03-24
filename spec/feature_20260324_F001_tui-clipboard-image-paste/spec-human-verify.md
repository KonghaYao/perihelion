# TUI 剪贴板粘贴图片 人工验收清单

**生成时间:** 2026-03-24 15:00
**关联计划:** [spec-plan.md](./spec-plan.md)
**关联设计:** [spec-design.md](./spec-design.md)

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 确认项目在正确目录: `test -f rust-agent-tui/Cargo.toml && echo "OK"`
- [ ] [AUTO] 编译确认无错误: `cargo build -p rust-agent-tui 2>&1 | grep -E "^error" | head -5`
- [ ] [AUTO] 确认 API Key 环境文件存在（运行时验证需要）: `test -f rust-agent-tui/.env && echo "found" || echo "missing (需手动配置)"`

### 人工测试数据准备
- [ ] [MANUAL] 准备一张图片到系统剪贴板：在 macOS 上使用 Cmd+Ctrl+Shift+4 截取任意屏幕区域（截图会自动进入剪贴板），或右键图片文件 → 复制图片
- [ ] [MANUAL] 准备一段纯文字到剪贴板：用 Cmd+C 复制任意一段文字（如 "hello world"）

---

## 验收项目

### 场景 1：静态代码验证

#### - [x] 1.1 依赖声明正确
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -E "arboard|^png|^base64" rust-agent-tui/Cargo.toml` → 期望: 三行均出现（arboard = "3"、png = "0.17"、base64 = { version = "0.22" ...}）
- **异常排查:**
  - 如果找不到某条依赖：检查 `rust-agent-tui/Cargo.toml` 是否正确写入

#### - [x] 1.2 PendingAttachment 结构体与 App 字段完整
- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -n "PendingAttachment\|pending_attachments" rust-agent-tui/src/app/mod.rs | wc -l` → 期望: 输出 ≥ 6（struct 定义 + 字段 + new() + new_headless() + new_thread() + 方法）
  2. [A] `grep -n "pub struct PendingAttachment\|add_pending_attachment\|pop_pending_attachment" rust-agent-tui/src/app/mod.rs` → 期望: 找到结构体定义和两个辅助方法
- **异常排查:**
  - 如果缺少方法：检查 `rust-agent-tui/src/app/mod.rs` 的 "─── Attachment 操作" 区块是否存在

#### - [x] 1.3 Ctrl+V 拦截逻辑完整
- **来源:** Task 2 检查步骤 + spec-design.md 事件处理
- **操作步骤:**
  1. [A] `grep -n "Char('v').*ctrl: true\|rgba_to_png_base64\|get_image\|get_text" rust-agent-tui/src/event.rs` → 期望: 找到 Ctrl+V 分支、rgba_to_png_base64 调用、get_image()、get_text() 四处
  2. [A] `grep -n "fn rgba_to_png_base64" rust-agent-tui/src/event.rs` → 期望: 找到函数定义（含 png::Encoder）
- **异常排查:**
  - 如果 rgba_to_png_base64 缺失：查看 event.rs 文件顶部是否有该函数
  - 如果 get_text fallback 缺失：查看 Ctrl+V 分支的 `else if` 路径

#### - [x] 1.4 Del 键删除逻辑
- **来源:** Task 2 检查步骤 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -n "Key::Delete\|pop_pending_attachment" rust-agent-tui/src/event.rs` → 期望: Key::Delete 分支调用 pop_pending_attachment()，且带有 `!app.pending_attachments.is_empty()` 守卫
- **异常排查:**
  - 如果找不到：检查 event.rs 中 `input if input.key != Key::Enter` 之前是否有 Del 分支

#### - [x] 1.5 附件栏 Layout 6-slot 结构
- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `grep -c "Constraint::" rust-agent-tui/src/ui/main_ui.rs` → 期望: 输出包含 ≥ 6（对应 6 个 Layout slot）
  2. [A] `grep -n "attachment_height\|render_attachment_bar" rust-agent-tui/src/ui/main_ui.rs` → 期望: attachment_height 定义+Constraint 使用共 2 处，render_attachment_bar 调用+定义共 2 处
- **异常排查:**
  - 如果 Layout 只有 5 个 Constraint：main_ui.rs 中 Layout 未插入附件栏 slot

---

### 场景 2：编译与测试

#### - [x] 2.1 编译无错误无 unused 警告
- **来源:** Tasks 1-4 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | grep -E "^error|^warning.*unused"` → 期望: 无任何输出
- **异常排查:**
  - 如果有 error：按照错误信息定位到对应文件修复
  - 如果有 unused 警告：检查新增代码是否有未使用的变量

#### - [x] 2.2 run_universal_agent 签名变更为 AgentInput
- **来源:** Task 4 检查步骤
- **操作步骤:**
  1. [A] `grep -A 3 "fn run_universal_agent" rust-agent-tui/src/app/agent.rs` → 期望: 第二个参数为 `input: AgentInput`（非 `input: String`）
  2. [A] `grep -n "AgentInput::text(input)\|let agent_input = AgentInput::text" rust-agent-tui/src/app/agent.rs` → 期望: 无输出（旧构建代码已移除）
- **异常排查:**
  - 如果签名仍为 String：检查 agent.rs 第 19 行是否已修改

#### - [x] 2.3 全量测试通过（43 个测试）
- **来源:** Task 4 检查步骤 + spec-design.md 验收标准
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | tail -3` → 期望: 输出包含 "test result: ok. 43 passed; 0 failed"
- **异常排查:**
  - 如果测试失败：运行 `cargo test -p rust-agent-tui -- --nocapture 2>&1 | grep "FAILED"` 查看失败的具体测试

---

### 场景 3：TUI 运行时 - 图片粘贴

> 此场景需要实际启动 TUI 程序进行验证。在验证前确保已按"验收前准备"配置 API Key。
> 启动命令：在项目根目录运行 `cargo run -p rust-agent-tui`

#### - [x] 3.1 剪贴板有图片时 Ctrl+V 在输入框上方显示附件栏
- **来源:** spec-design.md 验收标准 - 第 1 条
- **操作步骤:**
  1. [H] 按照"准备"中步骤，将一张图片复制到剪贴板（截图或复制图片）→ 是否已复制到剪贴板？ 是/否
  2. [H] 在 TUI 输入框中按 `Ctrl+V` → 输入框上方是否出现带蓝色边框、标题为"待发送附件"的附件栏？ 是/否
  3. [H] 附件栏内是否显示类似 `[img clipboard_1.png 24KB]` 格式的标签？ 是/否
- **异常排查:**
  - 如果没有附件栏出现：检查 arboard 是否成功读取剪贴板图片（macOS 可能需要授权终端访问剪贴板）
  - 如果出现 Ctrl+V 键盘输入了 "v" 字符：说明 Ctrl+V 拦截分支有问题，检查 event.rs 中的 Ctrl+V 处理顺序

#### - [x] 3.2 剪贴板无图片时 Ctrl+V 走文字粘贴路径
- **来源:** spec-design.md 验收标准 - 第 2 条
- **操作步骤:**
  1. [H] 将纯文字（如 "hello world"）复制到剪贴板（Cmd+C）→ 是否已复制？ 是/否
  2. [H] 在 TUI 输入框中按 `Ctrl+V` → 文字是否正常插入 textarea（不出现附件栏）？ 是/否
- **异常排查:**
  - 如果文字未插入：检查 event.rs 中 get_text() fallback 分支

---

### 场景 4：TUI 运行时 - 附件管理

> 继续使用场景 3 中已启动的 TUI。

#### - [x] 4.1 Del 键删除最后一个附件，全部删除后附件栏消失
- **来源:** spec-design.md 验收标准 - 第 3 条 + spec-design.md 验收标准 - 第 4 条
- **操作步骤:**
  1. [A] `grep -n "is_empty.*0\|pending_attachments.is_empty" rust-agent-tui/src/ui/main_ui.rs | head -3` → 期望: attachment_height 在 is_empty() 时为 0（逻辑保证）
  2. [H] 确保已通过 Ctrl+V 添加至少一张图片，按 `Del` 键 → 最后一张图片标签是否从附件栏中消失？ 是/否
  3. [H] 继续按 `Del` 直到清空所有附件 → 附件栏（蓝色边框区域）是否整体消失，布局是否回退到原始状态？ 是/否
- **异常排查:**
  - 如果 Del 键无反应：检查附件栏是否因 loading 状态被禁用，或检查 event.rs 中 Del 处理的 `!app.pending_attachments.is_empty()` 守卫

---

### 场景 5：TUI 运行时 - 多模态消息提交

> 继续使用场景 3/4 中已启动的 TUI。

#### - [x] 5.1 发送含图片消息后附件栏消失，消息区显示图片摘要
- **来源:** spec-design.md 验收标准 - 第 5、6 条
- **操作步骤:**
  1. [A] `grep -n "pending_attachments\|mem::take" rust-agent-tui/src/app/mod.rs | grep "take\|clear" | head -5` → 期望: 找到 `std::mem::take(&mut self.pending_attachments)` 和 new_thread() 中的 clear()（代码层保证提交后清空）
  2. [H] 通过 Ctrl+V 添加一张图片，输入文字（如"请描述这张图片"），按 Enter 提交 → 消息区是否出现一条用户消息，内容为 "请描述这张图片 [🖼 1 张图片]"？ 是/否
  3. [H] 消息提交后，输入框上方的附件栏是否立即消失？ 是/否

#### - [ ] 5.2 LLM 能接收到多模态内容（仅在配置了支持 vision 的模型时验证）
- **来源:** spec-design.md 验收标准 - 第 5 条
- **操作步骤:**
  1. [H] 提交含图片的消息后，等待 LLM 响应 → LLM 的回复内容中是否包含对图片内容的描述（例如描述了截图中的文字、界面元素等）？ 是/否
- **异常排查:**
  - 如果 LLM 未描述图片：确认使用的是支持 vision 的模型（如 claude-3-5-sonnet、gpt-4o）
  - 如果 API 报错：检查模型是否支持 image 类型的 ContentBlock

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 静态代码 | 1.1 | 依赖声明正确 | 1 | 0 | ✅ | |
| 场景 1 | 1.2 | PendingAttachment 结构体完整 | 2 | 0 | ✅ | |
| 场景 1 | 1.3 | Ctrl+V 拦截逻辑完整 | 2 | 0 | ✅ | |
| 场景 1 | 1.4 | Del 键删除逻辑 | 1 | 0 | ✅ | |
| 场景 1 | 1.5 | 附件栏 Layout 6-slot | 2 | 0 | ✅ | |
| 场景 2 编译测试 | 2.1 | 编译无错误无 unused 警告 | 1 | 0 | ✅ | |
| 场景 2 | 2.2 | AgentInput 签名变更 | 2 | 0 | ✅ | |
| 场景 2 | 2.3 | 全量测试通过 43 个 | 1 | 0 | ✅ | |
| 场景 3 图片粘贴 | 3.1 | 有图片时附件栏出现 | 0 | 3 | ✅ | |
| 场景 3 | 3.2 | 无图片时走文字路径 | 0 | 2 | ✅ | |
| 场景 4 附件管理 | 4.1 | Del 删除附件/栏消失 | 1 | 2 | ✅ | |
| 场景 5 消息提交 | 5.1 | 提交后附件清空+摘要 | 1 | 2 | ✅ | |
| 场景 5 | 5.2 | LLM 收到多模态内容 | 0 | 1 | ⏭️ | 跳过：模型不支持 vision |

**验收结论:** ✅ 12 项通过，1 项跳过（vision 模型未配置）
