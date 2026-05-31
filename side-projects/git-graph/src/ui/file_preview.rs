use crate::app::App;
use peri_widgets::Theme;
use ratatui::{
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

/// 渲染文件预览面板（高亮数据已在 load_preview 中预计算）
pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    // 懒加载
    if app.preview_highlighted.is_empty() && app.preview_file.is_some() {
        app.load_preview();
    }

    let theme = &app.theme;
    let (path, _) = app
        .preview_file
        .as_ref()
        .map(|(p, s)| (p.as_str(), *s))
        .unwrap_or(("", false));

    let title = format!(" {} ", path);
    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::ALL)
        .title(title)
        .title_style(Style::default().fg(theme.text()))
        .border_style(Style::default().fg(theme.border()));
    f.render_widget(block, area);

    let inner = area.inner(Margin::new(1, 1));
    if inner.height < 3 {
        return;
    }

    let viewport = inner.height.saturating_sub(1);
    let total_lines = app.preview_highlighted.len();

    // clamp
    let max_scroll = total_lines
        .saturating_sub(viewport as usize)
        .min(u16::MAX as usize) as u16;
    if app.preview_scroll > max_scroll {
        app.preview_scroll = max_scroll;
    }

    let start = app.preview_scroll as usize;
    let end = (start + viewport as usize).min(total_lines);

    // 从预计算的高亮数据构建可见行（O(1) per line，无高亮计算）
    let visible: Vec<Line> = app.preview_highlighted[start..end]
        .iter()
        .enumerate()
        .map(|(idx, segments)| {
            let line_num = start + idx + 1;
            let num_span = Span::styled(
                format!("{:>6} ", line_num),
                Style::default()
                    .fg(Color::Rgb(100, 100, 100))
                    .bg(Color::Rgb(25, 25, 35)),
            );
            let sep = Span::styled(
                "│",
                Style::default()
                    .fg(Color::Rgb(60, 60, 70))
                    .bg(Color::Rgb(20, 20, 28)),
            );
            let text_spans: Vec<Span> = segments
                .iter()
                .map(|(style, text)| Span::styled(text.clone(), *style))
                .collect();
            let mut line_spans = vec![num_span, sep, Span::raw(" ")];
            line_spans.extend(text_spans);
            Line::from(line_spans)
        })
        .collect();

    let content_area = Rect::new(inner.x, inner.y, inner.width, viewport);
    f.render_widget(
        Paragraph::new(visible).wrap(Wrap { trim: false }),
        content_area,
    );

    if app.preview_truncated {
        let note_area = Rect::new(inner.x, inner.y + viewport, inner.width, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" ... 文件过大，仅显示前 {} 行", total_lines),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ))),
            note_area,
        );
    }

    if total_lines > 0 {
        let pct = ((start as f64 / total_lines as f64) * 100.0) as u32;
        let info_str = format!("{}%  L{}/{}", pct, start + 1, total_lines);
        let info_area = Rect::new(
            inner.x + inner.width.saturating_sub(info_str.len() as u16 + 2),
            inner.y + inner.height.saturating_sub(1),
            info_str.len() as u16 + 2,
            1,
        );
        f.render_widget(
            Paragraph::new(Span::styled(
                info_str,
                Style::default().fg(Color::Rgb(100, 100, 110)),
            )),
            info_area,
        );
    }

    if total_lines as u16 > viewport {
        let scrollbar_area = Rect::new(
            inner.x + inner.width.saturating_sub(1),
            inner.y,
            1,
            viewport,
        );
        let mut scrollbar_state =
            ScrollbarState::new(max_scroll as usize).position(app.preview_scroll as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            scrollbar_area,
            &mut scrollbar_state,
        );
    }
}
