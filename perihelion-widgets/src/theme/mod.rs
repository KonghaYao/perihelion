mod presets;

pub use presets::DarkTheme;

use ratatui::style::Color;

/// 纯 UI 颜色主题 trait——不含业务语义方法
///
/// 组件通过此 trait 查询颜色，不硬编码色值。
/// 业务特有颜色（工具分级色、模型信息色等）由调用方在 TUI 层自行管理。
pub trait Theme: Send + Sync + 'static {
    // ── 强调色 ──────────────────────────────────────────────
    /// 主交互色（激活边框、光标、关键操作）
    fn accent(&self) -> Color;

    // ── 功能色 ──────────────────────────────────────────────
    /// 成功/完成色
    fn success(&self) -> Color;
    /// 次要强调/警告色
    fn warning(&self) -> Color;
    /// 错误/拒绝色
    fn error(&self) -> Color;
    /// 推理/思考色
    fn thinking(&self) -> Color;

    // ── 文字层级 ────────────────────────────────────────────
    /// 主文字（需要立即看到的内容）
    fn text(&self) -> Color;
    /// 次要文字（标签、路径、辅助信息）
    fn muted(&self) -> Color;
    /// 极弱文字（占位、已完成项、分隔符）
    fn dim(&self) -> Color;

    // ── 边框 ────────────────────────────────────────────────
    /// 空闲边框色
    fn border(&self) -> Color;
    /// 激活边框色（输入框/当前 panel focus）
    fn border_active(&self) -> Color;

    // ── 弹窗专用 ────────────────────────────────────────────
    /// 弹窗底色（Clear 后的背景）
    fn popup_bg(&self) -> Color;
    /// 光标行背景（列表选中行）
    fn cursor_bg(&self) -> Color;

    // ── 状态 ────────────────────────────────────────────────
    /// Loading 色（高辨识度状态指示）
    fn loading(&self) -> Color;
}
