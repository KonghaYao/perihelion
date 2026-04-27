# TUI Welcome Card 人工验收清单

**生成时间:** 2026-03-29 12:00
**关联计划:** spec-plan.md
**关联设计:** spec-design.md

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 检查 Rust 工具链可用: `rustc --version && cargo --version`
- [ ] [AUTO] 编译 TUI crate: `cargo build -p rust-agent-tui 2>&1 | tail -5`
- [ ] [AUTO] 检查 welcome.rs 模块存在: `test -f rust-agent-tui/src/ui/welcome.rs && echo "OK"`

### 测试数据准备
- 无需额外测试数据，使用 headless 测试模式验证

---

## 验收项目

### 场景 1：编译与测试

#### - [x] 1.1 编译通过
- **来源:** Task 1 + Task 2 检查步骤
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | tail -5` → 期望: 无 error，输出包含 "Compiling rust-agent-tui" 或 "Finished"
- **异常排查:**
  - 如果出现编译错误: 检查 `rust-agent-tui/src/ui/welcome.rs` 和 `rust-agent-tui/src/ui/main_ui.rs` 的 import 是否正确

#### - [x] 1.2 全量测试通过
- **来源:** Task 2 + Task 3 + Task 4 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | tail -10` → 期望: 所有测试 pass，无 failure
- **异常排查:**
  - 如果测试失败: 查看失败测试名称，对照对应 Task 排查

---

### 场景 2：宽屏 Welcome Card 渲染

#### - [x] 2.1 空消息渲染 Welcome Card
- **来源:** Task 4 End-to-end verification / 验收标准 1
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui -- test_welcome_card_renders_when_empty 2>&1` → 期望: 测试通过，snapshot 包含 "Perihelion"、"/help"、"/model" 关键文本
  2. [H] 在终端中运行 `cargo run -p rust-agent-tui`，观察启动后聊天区是否显示 ASCII Art Logo（包含 "██" 等方块字符）、副标题 "Perihelion Agent Framework"、功能亮点行和命令提示行 → 是/否
- **异常排查:**
  - 如果 headless 测试失败: 检查 `welcome.rs` 渲染逻辑和 `main_ui.rs` 集成分支
  - 如果 TUI 中看不到: 确认终端宽度 ≥80 列

#### - [x] 2.2 内容水平和垂直居中
- **来源:** 验收标准 2
- **操作步骤:**
  1. [H] 在终端中运行 `cargo run -p rust-agent-tui`，观察 Welcome Card 内容是否在聊天区水平和垂直方向居中显示 → 是/否
- **异常排查:**
  - 如果未垂直居中: 检查 `welcome.rs` 中 `vertical_offset` 计算逻辑
  - 如果未水平居中: 检查 `Line::centered()` 是否正确使用

#### - [x] 2.3 复用 theme.rs 颜色
- **来源:** 验收标准 6
- **操作步骤:**
  1. [A] `grep -n 'theme::' rust-agent-tui/src/ui/welcome.rs` → 期望: 输出包含 `theme::ACCENT`、`theme::MUTED`、`theme::DIM`、`theme::TEXT`、`theme::WARNING` 等引用
  2. [A] `grep -c 'Color::' rust-agent-tui/src/ui/welcome.rs` → 期望: 输出为 0（不引入新颜色常量）
- **异常排查:**
  - 如果发现 `Color::` 引用: 说明引入了非 theme 颜色，需修正为使用 `theme::*`

---

### 场景 3：消息替代行为

#### - [x] 3.1 发送消息后 Welcome Card 消失
- **来源:** Task 4 End-to-end verification / 验收标准 3
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui -- test_welcome_card_hidden_after_message 2>&1` → 期望: 测试通过，snapshot 不包含 welcome 文本，包含消息内容
  2. [H] 在终端中运行 `cargo run -p rust-agent-tui`，观察 Welcome Card 后输入任意消息发送，确认 Welcome Card 消失且正常显示消息流 → 是/否
- **异常排查:**
  - 如果 headless 测试失败: 检查 `main_ui.rs` 的 `render_messages` 分支逻辑
  - 如果 TUI 中不消失: 检查 `view_messages` 是否正确 push 了消息

#### - [x] 3.2 /clear 后重新显示 Welcome Card
- **来源:** 验收标准 5
- **操作步骤:**
  1. [A] `grep -n 'view_messages.clear\|view_messages = ' rust-agent-tui/src/app.rs | head -5` → 期望: `/clear` 命令会清空 `view_messages`，使其为空后重新触发 welcome 渲染
  2. [H] 在终端中运行 `cargo run -p rust-agent-tui`，发送一条消息后输入 `/clear`，确认 Welcome Card 重新出现 → 是/否
- **异常排查:**
  - 如果 /clear 后不显示 welcome: 确认 `view_messages` 是否被完全清空为 `Vec::new()`

---

### 场景 4：窄屏降级

#### - [x] 4.1 窄屏降级显示
- **来源:** Task 4 End-to-end verification / 验收标准 4
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui -- test_welcome_card_narrow_screen 2>&1` → 期望: 测试通过，不包含 ASCII Art 字符（如 "██" 或 "╚═"），包含文字标题
  2. [H] 将终端窗口宽度缩窄至 <50 列，运行 `cargo run -p rust-agent-tui`，确认不显示 ASCII Art Logo 但仍显示文字版标题和功能提示 → 是/否
- **异常排查:**
  - 如果 headless 测试失败: 检查 `welcome.rs` 中 `area.width < 50` 降级逻辑
  - 如果窄屏显示错乱: 检查窄屏分支是否正确跳过 Logo 行

---

### 场景 5：测试覆盖

#### - [x] 5.1 Headless 测试覆盖
- **来源:** Task 3 / 验收标准 7
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui -- test_welcome 2>&1 | tail -10` → 期望: 3 个测试全部 pass（`test_welcome_card_renders_when_empty`、`test_welcome_card_hidden_after_message`、`test_welcome_card_narrow_screen`）
- **异常排查:**
  - 如果缺少测试: 检查 `rust-agent-tui/src/ui/headless.rs` 中是否包含全部 3 个 welcome 测试函数
  - 如果测试失败: 查看具体失败信息，对照对应 Task 排查

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 编译与测试 | 1.1 | 编译通过 | 1 | 0 | ✅ | |
| 编译与测试 | 1.2 | 全量测试通过 | 1 | 0 | ✅ | |
| 宽屏渲染 | 2.1 | 空消息渲染 Welcome Card | 1 | 1 | ✅ | |
| 宽屏渲染 | 2.2 | 内容水平垂直居中 | 0 | 1 | ✅ | |
| 宽屏渲染 | 2.3 | 复用 theme.rs 颜色 | 2 | 0 | ✅ | |
| 消息替代 | 3.1 | 发送消息后消失 | 1 | 1 | ✅ | |
| 消息替代 | 3.2 | /clear 后重新显示 | 1 | 1 | ✅ | |
| 窄屏降级 | 4.1 | 窄屏降级显示 | 1 | 1 | ✅ | |
| 测试覆盖 | 5.1 | Headless 测试通过 | 1 | 0 | ✅ | |

**验收结论:** ✅ 全部通过
