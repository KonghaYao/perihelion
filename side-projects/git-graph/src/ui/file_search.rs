use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub fn draw_file_search(f: &mut Frame, area: Rect, app: &App) {
    let popup_width = 60u16.min(area.width);
    let max_visible = 10usize;
    let result_count = app.file_search_results.len();
    let inner_height = 2 + result_count.min(max_visible) + 1;
    let popup_height = ((inner_height + 2).min(20) as u16).min(area.height);
    let x = (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    f.render_widget(Clear, popup_area);

    let query = app.file_search_query.as_deref().unwrap_or("");
    let cursor_pos = app.file_search_cursor;

    // 输入行
    let before: String = query.chars().take(cursor_pos).collect();
    let cursor_char = query.chars().nth(cursor_pos).unwrap_or(' ');
    let after: String = query.chars().skip(cursor_pos + 1).collect();
    let input_line = Line::from(vec![
        Span::styled(" 🔍 ", Style::default().fg(Color::Cyan)),
        Span::styled(before, Style::default().fg(Color::White)),
        Span::styled(
            cursor_char.to_string(),
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ),
        Span::styled(after, Style::default().fg(Color::White)),
    ]);

    // 分隔线
    let sep = Line::from(Span::styled(
        "─".repeat(popup_width as usize - 2),
        Style::default().fg(Color::DarkGray),
    ));

    // 结果行（虚拟滚动）
    let visible = popup_height as usize - 5; // 减去边框+输入+分隔+提示
    let scroll = if app.file_search_selected >= visible {
        app.file_search_selected - visible + 1
    } else {
        0
    };

    let mut lines: Vec<Line> = vec![input_line, sep];
    for i in scroll..app.file_search_results.len() {
        if lines.len() >= popup_height as usize - 2 - 1 {
            // 留 1 行给提示
            break;
        }
        let path = &app.file_search_results[i];
        let (dir, file) = split_path(path);
        if i == app.file_search_selected {
            lines.push(Line::from(vec![
                Span::styled(" ▸ ", Style::default().fg(Color::Cyan)),
                Span::styled(
                    dir.clone(),
                    Style::default().fg(Color::Cyan).bg(Color::Rgb(38, 79, 120)),
                ),
                Span::styled(
                    file.clone(),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Rgb(38, 79, 120))
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(dir, Style::default().fg(Color::DarkGray)),
                Span::styled(file, Style::default().fg(Color::White)),
            ]));
        }
    }

    if app.file_search_results.is_empty() && !query.is_empty() {
        lines.push(Line::from(Span::styled(
            " 无匹配文件",
            Style::default().fg(Color::DarkGray),
        )));
    }

    // 提示
    lines.push(Line::from(Span::styled(
        " ↑↓ 导航 · Enter 打开 · Esc 关闭",
        Style::default().fg(Color::DarkGray),
    )));

    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" 文件搜索 ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(para, popup_area);
}

fn split_path(path: &str) -> (String, String) {
    match path.rfind('/') {
        Some(pos) => {
            let (dir, file) = path.split_at(pos + 1);
            (dir.to_string(), file.to_string())
        }
        None => (String::new(), path.to_string()),
    }
}
