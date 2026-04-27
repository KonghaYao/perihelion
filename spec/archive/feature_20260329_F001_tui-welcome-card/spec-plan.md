# TUI Welcome Card 执行计划

**目标:** 在 TUI 空消息时显示品牌 Welcome Card（ASCII Logo + 副标题 + 功能亮点 + 命令提示），发送消息后自动消失

**技术栈:** Rust, ratatui ≥0.30, 现有 theme 颜色体系

**设计文档:** spec/feature_20260329_F001_tui-welcome-card/spec-design.md

---

### Task 1: Welcome Card 渲染模块

**涉及文件:**
- 新建: `rust-agent-tui/src/ui/welcome.rs`
- 修改: `rust-agent-tui/src/ui/mod.rs`

**执行步骤:**
- [x] 创建 `ui/welcome.rs`，实现 `render_welcome(f, app, area)` 函数
  - 构建 ASCII Art Logo（6 行，"PERIHELION"，宽度约 46 字符），ACCENT + BOLD 样式
  - 构建副标题行 "Perihelion Agent Framework"，MUTED 色
  - 构建分隔线行 "────── What can I do? ──────"，DIM 色
  - 构建功能亮点行（• Ask me to code, debug, or refactor 等），TEXT 色 + ACCENT 符号
  - 构建命令提示行（/model /history /help /compact），WARNING 色快捷键
  - 构建动态内容行（Skills 数量、当前模型别名），从 `app.skills.len()` 和 `app.zen_config` 读取
  - 所有行使用 `Line::centered()` 水平居中
  - 窄屏降级：`area.width < 50` 时跳过 ASCII Art Logo，改用单行文字标题 "Perihelion"
  - 垂直居中：计算 `content_height`，`vertical_offset = (area.height - content_height) / 2`
  - 使用 `Paragraph::new(Text::from(lines)).scroll((vertical_offset, 0))` 渲染
- [x] 在 `ui/mod.rs` 添加 `pub mod welcome;`

**检查步骤:**
- [x] 验证 welcome.rs 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 "Compiling rust-agent-tui" 且无 error

---

### Task 2: 集成到主渲染流程

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/main_ui.rs`

**执行步骤:**
- [x] 在 `main_ui.rs` 顶部添加 `use super::welcome;`
- [x] 在 `render_messages()` 函数开头添加空消息判断分支
  - `if app.view_messages.is_empty() { welcome::render_welcome(f, app, area); return; }`
  - 确保在 spinner 计算之前 return，避免空消息时执行无意义的渲染逻辑

**检查步骤:**
- [x] 验证编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 无 error
- [x] 验证已有测试不受影响
  - `cargo test -p rust-agent-tui 2>&1 | tail -10`
  - 预期: 所有测试通过

---

### Task 3: Headless 测试

**涉及文件:**
- 修改: `rust-agent-tui/src/ui/headless.rs`

**执行步骤:**
- [x] 新增 `test_welcome_card_renders_when_empty` 测试
  - 创建空 App（默认 view_messages 为空）
  - 直接 `terminal.draw(main_ui::render)` 渲染
  - 断言 snapshot 包含 "Perihelion" 关键字
  - 断言 snapshot 包含命令提示（如 "/help" 或 "/model"）
- [x] 新增 `test_welcome_card_hidden_after_message` 测试
  - 创建空 App
  - push 一条 AssistantChunk + Done
  - 渲染后断言 snapshot 不包含 "Perihelion"（welcome 已被消息替代）
  - 断言包含消息内容
- [x] 新增 `test_welcome_card_narrow_screen` 测试
  - 创建 `App::new_headless(40, 24)`（窄屏 <50 列）
  - 渲染后断言不包含 ASCII Art（如 "██" 或 "╚═"）
  - 断言仍包含文字版标题

**检查步骤:**
- [x] 验证新增测试全部通过
  - `cargo test -p rust-agent-tui -- test_welcome 2>&1 | tail -10`
  - 预期: 3 个测试全部 pass
- [x] 验证全量测试不受影响
  - `cargo test -p rust-agent-tui 2>&1 | tail -10`
  - 预期: 所有测试通过

---

### Task 4: Welcome Card Acceptance

**Prerequisites:**
- Start command: `cargo run -p rust-agent-tui`
- Test data setup: 无需额外数据
- Other environment preparation: 终端窗口宽度 ≥80 列

**End-to-end verification:**

1. 宽屏空消息显示 Welcome Card
   - `cargo test -p rust-agent-tui -- test_welcome_card_renders_when_empty 2>&1`
   - Expected: 测试通过，snapshot 包含 "Perihelion"、"/help"、"/model" 等关键文本
   - On failure: check Task 1 [welcome.rs 渲染逻辑] 或 Task 2 [main_ui.rs 集成]

2. 发送消息后 Welcome Card 消失
   - `cargo test -p rust-agent-tui -- test_welcome_card_hidden_after_message 2>&1`
   - Expected: 测试通过，snapshot 不包含 "Perihelion" welcome 文本，包含消息内容
   - On failure: check Task 2 [render_messages 分支逻辑]

3. 窄屏降级显示
   - `cargo test -p rust-agent-tui -- test_welcome_card_narrow_screen 2>&1`
   - Expected: 测试通过，不包含 ASCII Art 字符，包含文字标题
   - On failure: check Task 1 [窄屏降级逻辑]

4. 全量测试回归
   - `cargo test -p rust-agent-tui 2>&1`
   - Expected: 所有测试通过，无 regression
   - On failure: check 对应失败测试关联的 Task
