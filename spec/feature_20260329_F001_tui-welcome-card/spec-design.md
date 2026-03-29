# Feature: 20260329_F001 - tui-welcome-card

## 需求背景

当前 TUI 启动后聊天区为空白，用户首次看到的是空荡荡的界面，缺乏品牌感和引导性。需要一个 Welcome Card 组件，在空消息时显示品牌 Logo + 问候语 + 基础信息，提升首次体验。

## 目标

- 空消息时在聊天区显示 Welcome Card，发送第一条消息后自然消失
- ASCII Art 品牌Logo + 副标题 + 功能亮点 + 命令快捷提示
- 单布局自适应：宽屏完整展示，窄屏降级（跳过 Logo 仅显示文字）
- 垂直 + 水平居中显示
- 复用现有 theme 颜色体系，不引入新颜色

## 方案设计

### 整体架构

新增独立模块 `ui/welcome.rs`，封装 welcome card 渲染逻辑。在 `main_ui::render_messages` 中，当 `view_messages.is_empty()` 时调用 `welcome::render_welcome()` 替代正常消息渲染。

不新增 App 状态字段——显示条件纯由 `view_messages.is_empty()` 决定。

### 渲染位置与触发

调用点：`main_ui.rs` 的 `render_messages()` 函数开头：

```rust
fn render_messages(f: &mut Frame, app: &mut App, area: Rect) {
    if app.view_messages.is_empty() {
        welcome::render_welcome(f, app, area);
        return;
    }
    // ... 现有渲染逻辑
}
```

### 内容布局

垂直布局（居中显示）：

```
┌─────────────────────────────────────────────┐
│                                             │
│              ███╗   ██╗███████╗██╗  ██╗     │  ← ASCII Art Logo (ACCENT + BOLD)
│              ████╗  ██║██╔════╝╚██╗██╔╝     │     6 行
│              ██╔██╗ ██║█████╗   ╚███╔╝      │
│              ██║╚██╗██║██╔══╝   ██╔██╗      │
│              ██║ ╚████║███████╗██╔╝ ██╗     │
│              ╚═╝  ╚═══╝╚══════╝╚═╝  ╚═╝     │
│                                             │
│           Perihelion Agent Framework         │  ← 副标题 (MUTED)
│                                             │
│         ────── What can I do? ──────         │  ← 分隔线 + 问候语 (DIM)
│                                             │
│    • Ask me to code, debug, or refactor      │  ← 功能亮点 (TEXT)
│    • /model  /history  /help  /compact       │  ← 命令提示 (WARNING 标识快捷键)
│    • #skill-name to activate skills          │
│                                             │
└─────────────────────────────────────────────┘
```

### 自适应策略

- **所有行**使用 `Line::centered()` 水平居中
- **窄屏降级**（`area.width < 50`）：跳过 ASCII Art Logo，仅显示文字版 "Perihelion" 标题
- **垂直居中**：计算内容总行数，用 `Paragraph::scroll((vertical_offset, 0))` 实现垂直居中
- **高度不足**：内容超出可见区域时，只显示能显示的部分（从顶部开始），不做滚动

### 文件结构

```
rust-agent-tui/src/ui/
├── mod.rs              # 添加 pub mod welcome;
├── welcome.rs          # 新增：Welcome Card 渲染逻辑
├── main_ui.rs          # 修改：render_messages 中添加空消息判断
├── theme.rs            # 不修改，复用现有颜色
└── ...
```

### 接口设计

```rust
// ui/welcome.rs

/// 渲染 Welcome Card（空消息时替代聊天区内容）
pub(crate) fn render_welcome(f: &mut Frame, app: &App, area: Rect)
```

参数说明：
- `f: &mut Frame` — ratatui Frame
- `app: &App` — 读取 `app.skills.len()` 等信息用于动态内容
- `area: Rect` — 聊天区可用区域

### 颜色方案

严格复用 `theme.rs` 已有颜色：

| 元素 | 颜色 | 样式 |
|------|------|------|
| ASCII Art Logo | `theme::ACCENT` | BOLD |
| 副标题 | `theme::MUTED` | — |
| 分隔线 + 问候语 | `theme::DIM` | — |
| 功能亮点文字 | `theme::TEXT` | — |
| 命令快捷键 | `theme::WARNING` | — |
| 功能亮点符号（•）| `theme::ACCENT` | — |

### 动态内容

从 `app` 读取以下信息动态显示：
- `app.skills.len()` → 显示可用 Skills 数量
- `app.zen_config` → 显示当前模型别名（如 "★Opus"）

## 实现要点

1. **ASCII Art Logo**：硬编码 6 行 ASCII art（"PERIHELION" 或项目名），宽度约 46 字符
2. **居中算法**：`Line::centered()` 水平居中 + `scroll` 偏移垂直居中
3. **窄屏降级**：`area.width < 50` 时跳过 Logo 行，改用单行文字标题
4. **垂直居中计算**：`vertical_offset = (area.height.saturating_sub(content_height)) / 2`
5. **Headless 测试**：可在 `headless.rs` 中新增测试，验证空 App 渲染包含 welcome 内容

## 约束一致性

- 符合 `constraints.md` 中 TUI 框架选型（ratatui ≥0.30）
- 符合编码规范（snake_case 函数命名，`pub(crate)` 可见性）
- 符合文件组织规范（每模块一文件）
- 符合事件驱动 TUI 通信约束（无共享可变状态，纯渲染函数）
- 无架构偏离

## 验收标准

- [ ] TUI 启动后，空消息时聊天区显示 Welcome Card（ASCII Logo + 副标题 + 功能亮点 + 命令提示）
- [ ] 内容水平和垂直居中
- [ ] 发送第一条消息后 Welcome Card 消失，正常显示消息流
- [ ] 窄屏（<50 列）时降级显示，不截断或错乱
- [ ] /clear 后重新显示 Welcome Card
- [ ] 复用 theme.rs 颜色，不引入新颜色常量
- [ ] 新增 headless 测试验证 welcome card 渲染
