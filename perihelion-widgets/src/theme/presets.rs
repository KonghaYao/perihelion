use super::Theme;
use ratatui::style::Color;

/// 项目默认深色主题
///
/// 色值与 rust-agent-tui/src/ui/theme.rs 的常量一一对应。
/// 业务特有常量（TOOL_NAME=SAGE, SUB_AGENT=SAGE, MODEL_INFO=#A0825F）
/// 保留在 TUI 层，不在此处定义。
#[derive(Debug, Clone)]
pub struct DarkTheme;

impl Theme for DarkTheme {
    fn accent(&self) -> Color {
        Color::Rgb(255, 107, 43)
    } // ACCENT #FF6B2B
    fn success(&self) -> Color {
        Color::Rgb(110, 181, 106)
    } // SAGE #6EB56A
    fn warning(&self) -> Color {
        Color::Rgb(176, 152, 120)
    } // WARNING #B09878
    fn error(&self) -> Color {
        Color::Rgb(204, 70, 62)
    } // ERROR #CC463E
    fn thinking(&self) -> Color {
        Color::Rgb(167, 139, 250)
    } // THINKING #A78BFA
    fn text(&self) -> Color {
        Color::Rgb(218, 206, 208)
    } // TEXT #DACED0
    fn muted(&self) -> Color {
        Color::Rgb(140, 125, 120)
    } // MUTED #8C7D78
    fn dim(&self) -> Color {
        Color::Rgb(72, 62, 58)
    } // DIM #483E3A
    fn border(&self) -> Color {
        Color::Rgb(48, 38, 32)
    } // BORDER #302620
    fn border_active(&self) -> Color {
        Color::Rgb(255, 107, 43)
    } // = accent
    fn popup_bg(&self) -> Color {
        Color::Rgb(10, 8, 6)
    } // POPUP_BG #0A0806
    fn cursor_bg(&self) -> Color {
        Color::Rgb(38, 22, 10)
    } // CURSOR_BG #261608
    fn loading(&self) -> Color {
        Color::Rgb(34, 211, 238)
    } // LOADING #22D3EE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme_returns_correct_colors() {
        let theme = DarkTheme;
        assert_eq!(theme.accent(), Color::Rgb(255, 107, 43));
    }

    #[test]
    fn dark_theme_trait_object_usable() {
        let theme: &dyn Theme = &DarkTheme;
        let _accent = theme.accent();
        let _success = theme.success();
        let _warning = theme.warning();
        let _error = theme.error();
        let _thinking = theme.thinking();
        let _text = theme.text();
        let _muted = theme.muted();
        let _dim = theme.dim();
        let _border = theme.border();
        let _border_active = theme.border_active();
        let _popup_bg = theme.popup_bg();
        let _cursor_bg = theme.cursor_bg();
        let _loading = theme.loading();
    }

    #[test]
    fn dark_theme_cloneable() {
        let theme = DarkTheme;
        let _cloned = theme.clone();
    }
}
