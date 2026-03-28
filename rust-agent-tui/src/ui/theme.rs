/// TUI 统一颜色主题（对应 TUI-STYLE.md 风格指南 v1.0）
///
/// 设计哲学：极简锋利，单色制胜。
/// 背景透明——不使用任何 bg() 颜色（弹窗光标行除外）。
/// 信息层级只用亮度和 BOLD 区分，颜色只表达状态。
use ratatui::style::Color;

// ── 强调色（单一主色）────────────────────────────────────────────────────────

/// 橙红 — 唯一主交互色，激活边框/光标/关键操作，对应 #FF6B2B
pub const ACCENT: Color = Color::Rgb(255, 107, 43);

// ── 功能色 ───────────────────────────────────────────────────────────────────

/// 哑光绿 — 成功/工具名/在线状态，对应 #6EB56A
pub const SAGE: Color = Color::Rgb(110, 181, 106);

/// 琥珀黄 — 运行中/警告/keybind 提示，对应 #C8942A
pub const WARNING: Color = Color::Rgb(200, 148, 42);

/// 暗红 — 错误/拒绝，对应 #CC463E
pub const ERROR: Color = Color::Rgb(204, 70, 62);

/// 亮紫罗兰 — 推理/CoT 思考内容，对应 #A78BFA
pub const THINKING: Color = Color::Rgb(167, 139, 250);

// ── 文字层级（三级亮度）──────────────────────────────────────────────────────

/// 主文字 — 需要立即看到的内容，对应 #DACED0（冷白偏暖）
pub const TEXT: Color = Color::Rgb(218, 206, 208);

/// 次要文字 — 标签、路径、辅助信息，对应 #8C7D78
pub const MUTED: Color = Color::Rgb(140, 125, 120);

/// 极弱文字 — 占位、已完成项、分隔符，对应 #483E3A
pub const DIM: Color = Color::Rgb(72, 62, 58);

// ── 边框 ─────────────────────────────────────────────────────────────────────

/// 空闲边框 — 极低对比，只做功能性分隔，对应 #302620
pub const BORDER: Color = Color::Rgb(48, 38, 32);

/// 激活边框 — 输入框/当前 panel focus 状态
pub const BORDER_ACTIVE: Color = ACCENT;

// ── 弹窗专用 ─────────────────────────────────────────────────────────────────

/// 弹窗底色（Clear 后的背景），对应 #0A0806
pub const POPUP_BG: Color = Color::Rgb(10, 8, 6);

/// 光标行背景（列表选中行），对应 #261608
pub const CURSOR_BG: Color = Color::Rgb(38, 22, 10);

/// Loading 专用色 — 电光青，对应 #22D3EE，在暗色终端最显眼
pub const LOADING: Color = Color::Rgb(34, 211, 238);

// ── 语义别名 ─────────────────────────────────────────────────────────────────

/// 工具名颜色（= SAGE）
pub const TOOL_NAME: Color = SAGE;

/// SubAgent 颜色（= SAGE）
pub const SUB_AGENT: Color = SAGE;

/// 模型信息颜色 — 棕金，对应 #A0825F（状态栏模型名，不抢眼）
pub const MODEL_INFO: Color = Color::Rgb(160, 130, 95);
