use super::Theme;
use ratatui::style::Color;

/// 项目默认深色主题
///
/// 色值与 peri-tui/src/ui/theme.rs 的常量一一对应。
/// 业务特有常量（TOOL_NAME=SAGE, SUB_AGENT=SAGE, MODEL_INFO=#A0825F）
/// 保留在 TUI 层，不在此处定义。
#[derive(Debug, Clone)]
pub struct DarkTheme;

impl Theme for DarkTheme {
    fn accent(&self) -> Color {
        Color::Rgb(215, 119, 87)
    } // ACCENT #D77757
    fn success(&self) -> Color {
        Color::Rgb(78, 186, 101)
    } // SAGE #4EBA65
    fn warning(&self) -> Color {
        Color::Rgb(255, 193, 7)
    } // WARNING #FFC107
    fn error(&self) -> Color {
        Color::Rgb(255, 107, 128)
    } // ERROR #FF6B80
    fn thinking(&self) -> Color {
        Color::Rgb(162, 169, 228)
    } // THINKING #A2A9E4（与 TUI theme::THINKING 一致）
    fn text(&self) -> Color {
        Color::Rgb(255, 255, 255)
    } // TEXT #FFFFFF
    fn muted(&self) -> Color {
        Color::Rgb(153, 153, 153)
    } // MUTED #999999
    fn dim(&self) -> Color {
        Color::Rgb(80, 80, 80)
    } // DIM #505050
    fn border(&self) -> Color {
        Color::Rgb(80, 80, 80)
    } // BORDER #505050
    fn border_active(&self) -> Color {
        Color::Rgb(215, 119, 87)
    } // = accent #D77757
    fn popup_bg(&self) -> Color {
        Color::Rgb(0, 0, 0)
    } // POPUP_BG #000000
    fn cursor_bg(&self) -> Color {
        Color::Rgb(38, 38, 38)
    } // CURSOR_BG #262626
    fn loading(&self) -> Color {
        Color::Rgb(147, 165, 255)
    } // LOADING #93A5FF

    fn user_bg(&self) -> Color {
        Color::Rgb(55, 55, 55)
    } // USER_BG #373737

    fn bash_border(&self) -> Color {
        Color::Rgb(253, 93, 177)
    } // BASH_BORDER #FD5DB1
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("presets_test.rs");
}
