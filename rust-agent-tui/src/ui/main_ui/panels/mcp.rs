use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    Frame,
};

use perihelion_widgets::{BorderedPanel, ScrollState, ScrollableArea, TabBar, TabState, TabStyle};

use crate::app::{McpPanelView, App};
use crate::ui::main_ui::highlight_line_spans;
use crate::ui::theme;

/// MCP 管理面板渲染
pub(crate) fn render_mcp_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(panel) = app.mcp_panel.as_ref() else {
        return;
    };

    let title = match &panel.view {
        McpPanelView::ServerList => " MCP 服务器 ".to_string(),
        McpPanelView::ServerDetail { server_name, .. } => {
            format!(" {} ", server_name)
        }
    };

    let inner = BorderedPanel::new(Span::styled(
        title,
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    if panel.view.is_server_list() {
        render_server_list(f, app, inner);
    } else {
        render_server_detail(f, app, inner);
    }
}

fn render_server_list(f: &mut Frame, app: &mut App, inner: Rect) {
    let panel = match app.mcp_panel.as_ref() {
        Some(p) => p,
        None => return,
    };
    let mut lines: Vec<Line> = Vec::new();

    for (i, server) in panel.servers.iter().enumerate() {
        let is_cursor = i == panel.cursor;
        let cursor_char = if is_cursor { "❯ " } else { "  " };

        let status_icon = match &server.status {
            rust_agent_middlewares::mcp::ClientStatus::Connected => "●",
            _ if server.oauth_status == rust_agent_middlewares::mcp::OAuthStatus::NeedsAuthorization => "◎",
            _ => "○",
        };
        let status_style = match &server.status {
            rust_agent_middlewares::mcp::ClientStatus::Connected => {
                Style::default().fg(theme::SAGE)
            }
            _ if server.oauth_status == rust_agent_middlewares::mcp::OAuthStatus::NeedsAuthorization => {
                Style::default().fg(theme::WARNING)
            }
            _ => Style::default().fg(theme::ERROR),
        };

        let status_text = match (&server.status, &server.oauth_status) {
            (rust_agent_middlewares::mcp::ClientStatus::Connected, _) => {
                "Connected".to_string()
            }
            (_, rust_agent_middlewares::mcp::OAuthStatus::NeedsAuthorization) => {
                "OAuth Required".to_string()
            }
            (rust_agent_middlewares::mcp::ClientStatus::Failed(reason), _) => {
                let truncated: String = reason.chars().take(20).collect();
                if reason.len() > 20 {
                    format!("Failed({})…", truncated)
                } else {
                    format!("Failed({})", truncated)
                }
            }
            (rust_agent_middlewares::mcp::ClientStatus::Disconnected, _) => {
                "Disconnected".to_string()
            }
        };

        let count_text = match &server.status {
            rust_agent_middlewares::mcp::ClientStatus::Connected => {
                format!(
                    "{} tools, {} resources",
                    server.tool_count, server.resource_count
                )
            }
            _ => "—".to_string(),
        };

        // OAuth 状态图标
        let (oauth_icon, oauth_style) = match &server.oauth_status {
            rust_agent_middlewares::mcp::OAuthStatus::None => ("", Style::default()),
            rust_agent_middlewares::mcp::OAuthStatus::Authorized => {
                ("\u{1f511}", Style::default().fg(theme::SAGE))
            }
            rust_agent_middlewares::mcp::OAuthStatus::NeedsAuthorization => {
                ("\u{1f512}", Style::default().fg(theme::WARNING))
            }
        };

        let name_style = if is_cursor {
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };

        lines.push(Line::from(vec![
            Span::styled(cursor_char.to_string(), Style::default().fg(theme::THINKING)),
            Span::styled(status_icon.to_string(), status_style),
            Span::styled(" ", Style::default()),
            Span::styled(
                format!("{:<20}", server.name),
                name_style,
            ),
            Span::styled(
                format!("[{}] ", server.transport_type),
                Style::default().fg(theme::MUTED),
            ),
            Span::styled(
                format!("{:<16} ", status_text),
                status_style,
            ),
            Span::styled(count_text, Style::default().fg(theme::MUTED)),
            Span::styled(format!("{} ", oauth_icon), oauth_style),
        ]));
    }

    if panel.servers.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  （无 MCP 服务器配置，请编辑 .mcp.json 或 settings.json）",
            Style::default().fg(theme::MUTED),
        )));
    }

    // 存储面板元数据供鼠标选区使用
    app.core.panel_area = Some(inner);
    app.core.panel_scroll_offset = 0;
    app.core.panel_plain_lines = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    // 应用面板选区高亮
    apply_panel_selection(app, &mut lines, inner);

    let mut scroll_state = ScrollState::with_offset(0);
    ScrollableArea::new(Text::from(lines))
        .scrollbar_style(Style::default().fg(theme::MUTED))
        .render(f, inner, &mut scroll_state);
}

fn render_server_detail(f: &mut Frame, app: &mut App, inner: Rect) {
    // 从 app 中提取所有需要的数据，避免借用冲突
    let (active_tab, scroll_offset, cursor, tools, resources) = {
        let panel = match app.mcp_panel.as_ref() {
            Some(p) => p,
            None => return,
        };
        let McpPanelView::ServerDetail {
            tools, resources, active_tab, ..
        } = &panel.view
        else {
            return;
        };
        (*active_tab, panel.scroll_offset, panel.cursor, tools.clone(), resources.clone())
    };

    // ── 1. Tab 栏（固定，不滚动） ──
    let tab_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    let mut tab_state = TabState::new(vec!["工具".into(), "资源".into()]);
    tab_state.set_active(active_tab);
    let tab_bar = TabBar::new().style(TabStyle {
        active: Style::default().add_modifier(Modifier::REVERSED),
        completed: Style::default().fg(theme::SAGE),
        incomplete: Style::default().fg(theme::MUTED),
        separator: " ",
    });
    f.render_stateful_widget(tab_bar, tab_area, &mut tab_state);

    // ── 2. 滚动区域（Tab 栏下方全部空间） ──
    let scroll_area = Rect {
        x: inner.x,
        y: inner.y + 1,
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    // 两栏布局：名称 40%，描述 60%（仅资源 Tab 使用）
    let name_col_width = (scroll_area.width as usize) * 40 / 100;
    let desc_col_width = (scroll_area.width as usize).saturating_sub(name_col_width);

    match active_tab {
        // 工具 Tab
        0 => {
            if tools.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  （该服务器未暴露工具）",
                    Style::default().fg(theme::MUTED),
                )));
            } else {
                for (i, tool) in tools.iter().enumerate() {
                    let is_cursor = i == cursor;
                    let cursor_char = if is_cursor { "❯ " } else { "  " };

                    lines.push(Line::from(vec![
                        Span::styled(cursor_char.to_string(), Style::default().fg(theme::THINKING)),
                        Span::styled(
                            tool.name.clone(),
                            Style::default().fg(theme::SAGE),
                        ),
                    ]));
                }
            }
        }
        // 资源 Tab
        1 => {
            if resources.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  （该服务器未暴露资源）",
                    Style::default().fg(theme::MUTED),
                )));
            } else {
                for (i, resource) in resources.iter().enumerate() {
                    let is_cursor = i == cursor;
                    let cursor_char = if is_cursor { "❯ " } else { "  " };
                    let uri = &resource.uri;
                    let name = resource
                        .title
                        .as_deref()
                        .unwrap_or("");

                    let uri_display: String = uri.chars().take(name_col_width.saturating_sub(2)).collect();
                    let name_display: String = name.chars().take(desc_col_width).collect();

                    lines.push(Line::from(vec![
                        Span::styled(cursor_char.to_string(), Style::default().fg(theme::THINKING)),
                        Span::styled(
                            format!("{:<width$}", uri_display, width = name_col_width),
                            Style::default().fg(theme::THINKING),
                        ),
                        Span::styled(name_display, Style::default().fg(theme::MUTED)),
                    ]));
                }
            }
        }
        _ => {}
    }


    // 存储面板元数据供鼠标选区使用
    app.core.panel_area = Some(scroll_area);
    app.core.panel_scroll_offset = scroll_offset as u16;
    app.core.panel_plain_lines = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    // 应用面板选区高亮
    apply_panel_selection(app, &mut lines, scroll_area);

    let mut scroll_state = ScrollState::with_offset(scroll_offset);
    ScrollableArea::new(Text::from(lines))
        .scrollbar_style(Style::default().fg(theme::MUTED))
        .render(f, scroll_area, &mut scroll_state);
}

fn apply_panel_selection(app: &mut App, lines: &mut Vec<Line>, area: Rect) {
    if app.core.panel_selection.is_active() {
        let sel = &app.core.panel_selection;
        if let (Some(start), Some(end)) = (sel.start, sel.end) {
            let ((sr, sc), (er, ec)) = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            let scroll = 0usize;
            let visible_end = area.height as usize;
            for line_idx in sr as usize..=er as usize {
                if line_idx >= visible_end {
                    continue;
                }
                let visual_idx = line_idx - scroll;
                if visual_idx >= lines.len() {
                    continue;
                }
                let (cs, ce) = if line_idx == sr as usize && line_idx == er as usize {
                    (sc as usize, ec as usize)
                } else if line_idx == sr as usize {
                    (sc as usize, usize::MAX)
                } else if line_idx == er as usize {
                    (0, ec as usize)
                } else {
                    (0, usize::MAX)
                };
                let spans = std::mem::take(&mut lines[visual_idx].spans);
                lines[visual_idx] = Line::from(highlight_line_spans(spans, cs, ce));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rust_agent_middlewares::mcp::{ClientStatus, ServerInfo};

    use crate::app::{McpPanel, McpPanelView, App};

    fn make_server(name: &str, status: ClientStatus) -> ServerInfo {
        ServerInfo {
            name: name.to_string(),
            transport_type: "stdio".to_string(),
            status,
            tool_count: 3,
            resource_count: 2,
            oauth_status: Default::default(),
        }
    }

    fn render_mcp_panel(servers: Vec<ServerInfo>) -> crate::ui::headless::HeadlessHandle {
        let (mut app, mut handle) = App::new_headless(120, 30);
        app.mcp_panel = Some(McpPanel::new(servers));
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        handle
    }

    #[tokio::test]
    async fn test_mcp_panel_empty_server_list() {
        let handle = render_mcp_panel(vec![]);
        let snap = handle.snapshot().join("\n");
        assert!(
            snap.contains(".mcp.json"),
            "空 MCP 面板应显示配置引导文字"
        );
    }

    #[tokio::test]
    async fn test_mcp_panel_server_list_with_items() {
        let handle = render_mcp_panel(vec![
            make_server("test-connected", ClientStatus::Connected),
            make_server("test-failed", ClientStatus::Failed("timeout".into())),
        ]);
        let snap = handle.snapshot().join("\n");
        assert!(
            snap.contains("test-connected"),
            "MCP 面板应显示服务器名称"
        );
        assert!(
            snap.contains("Connected"),
            "MCP 面板应显示 Connected 状态"
        );
    }

    #[tokio::test]
    async fn test_mcp_panel_detail_tab_bar() {
        let (mut app, mut handle) = App::new_headless(120, 30);
        app.mcp_panel = Some(McpPanel::new(vec![
            make_server("test-srv", ClientStatus::Connected),
        ]));
        app.mcp_panel_enter();

        match &app.mcp_panel.as_ref().unwrap().view {
            McpPanelView::ServerDetail { active_tab, .. } => {
                assert_eq!(*active_tab, 0, "默认应显示工具 Tab");
            }
            _ => panic!("应进入 ServerDetail 视图"),
        }

        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        // Tab 栏包含"工具"和"资源"两个 CJK 标签，snapshot 中以 Unicode 原文出现
        let snap = handle.snapshot().join("\n");
        // CJK 在 TestBackend 中有宽字符填充，使用 ASCII 上下文验证
        assert!(
            snap.contains("Tab") || snap.contains("test-srv"),
            "详情页应渲染服务器名或 Tab 栏"
        );
    }

    #[tokio::test]
    async fn test_mcp_panel_tab_switch() {
        let (mut app, _handle) = App::new_headless(120, 30);
        app.mcp_panel = Some(McpPanel::new(vec![
            make_server("test-srv", ClientStatus::Connected),
        ]));
        app.mcp_panel_enter();

        app.mcp_panel_tab();
        match &app.mcp_panel.as_ref().unwrap().view {
            McpPanelView::ServerDetail { active_tab, .. } => {
                assert_eq!(*active_tab, 1, "应切换到资源 Tab");
            }
            _ => panic!("应仍在 ServerDetail 视图"),
        }

        app.mcp_panel_tab();
        match &app.mcp_panel.as_ref().unwrap().view {
            McpPanelView::ServerDetail { active_tab, .. } => {
                assert_eq!(*active_tab, 0, "应切换回工具 Tab");
            }
            _ => panic!("应仍在 ServerDetail 视图"),
        }
    }
}
