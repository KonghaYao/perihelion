use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    Frame,
};

use perihelion_widgets::{BorderedPanel, ScrollState, ScrollableArea};

use crate::app::{App, DetailAction, McpPanelView};
use crate::ui::main_ui::highlight_line_spans;
use crate::ui::theme;

use rust_agent_middlewares::mcp::{ClientStatus, ConfigSource, OAuthStatus, ServerInfo};

/// MCP 管理面板渲染
pub(crate) fn render_mcp_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(panel) = app.mcp_panel.as_ref() else {
        return;
    };

    if panel.view.is_server_list() {
        render_server_list(f, app, area);
    } else {
        render_server_detail(f, app, area);
    }
}

fn render_server_list(f: &mut Frame, app: &mut App, area: Rect) {
    // Phase 1: 读取面板数据并构建所有行（不可变借用 panel）
    let (mut lines, scroll_offset) = {
        let panel = match app.mcp_panel.as_ref() {
            Some(p) => p,
            None => return,
        };
        let scroll_offset = panel.scroll_offset;
        let cursor = panel.cursor;
        let servers = &panel.servers;
        let mut lines: Vec<Line> = Vec::new();

        // 服务器计数
        let count = servers.len();
        lines.push(Line::from(Span::styled(
            format!("  {} servers", count),
            Style::default().fg(theme::MUTED),
        )));
        lines.push(Line::from(""));

        // 按来源分组
        let (project_servers, user_servers) = partition_by_source(servers);

        if !project_servers.is_empty() {
            let header_text = match &project_servers[0].source {
                Some(ConfigSource::Project(path)) => format!("  Project MCPs ({})", path.display()),
                _ => "  Project MCPs".to_string(),
            };
            lines.push(Line::from(Span::styled(
                header_text,
                Style::default().fg(theme::MUTED),
            )));
            render_server_group(
                &mut lines,
                &project_servers,
                cursor,
                &project_start_offset(servers),
            );
            lines.push(Line::from(""));
        }

        if !user_servers.is_empty() {
            let header_text = match &user_servers[0].source {
                Some(ConfigSource::Global(path)) => format!("  User MCPs ({})", path.display()),
                _ => "  User MCPs".to_string(),
            };
            lines.push(Line::from(Span::styled(
                header_text,
                Style::default().fg(theme::MUTED),
            )));
            render_server_group(
                &mut lines,
                &user_servers,
                cursor,
                &user_start_offset(servers),
            );
            lines.push(Line::from(""));
        }

        if servers.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  No MCP servers configured. Edit .mcp.json or settings.json",
                Style::default().fg(theme::MUTED),
            )));
        }

        (lines, scroll_offset)
    }; // panel 借用在此结束

    // Phase 2: 渲染 BorderedPanel 边框（标题在边框线上）
    let inner = BorderedPanel::new(Span::styled(
        " Manage MCP servers ",
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    // Phase 3: 写入元数据和渲染内容（可变借用 app）
    app.sessions[app.active].core.panel_area = Some(inner);
    app.sessions[app.active].core.panel_scroll_offset = 0;
    app.sessions[app.active].core.panel_plain_lines = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    apply_panel_selection(app, &mut lines, inner);

    let mut scroll_state = ScrollState::with_offset(scroll_offset);
    ScrollableArea::new(Text::from(lines))
        .scrollbar_style(Style::default().fg(theme::MUTED))
        .render(f, inner, &mut scroll_state);
}

fn render_server_group(
    lines: &mut Vec<Line>,
    servers: &[&rust_agent_middlewares::mcp::ServerInfo],
    global_cursor: usize,
    start_offset: &usize,
) {
    for (i, server) in servers.iter().enumerate() {
        let flat_idx = *start_offset + i;
        let is_cursor = flat_idx == global_cursor;
        let cursor_char = if is_cursor { "❯ " } else { "  " };

        let (icon, icon_style, status_text) = match (&server.status, &server.oauth_status) {
            (ClientStatus::Connected, _) => ("✔", Style::default().fg(theme::SAGE), "connected"),
            (_, OAuthStatus::NeedsAuthorization) => (
                "△",
                Style::default().fg(theme::WARNING),
                "needs authentication",
            ),
            (ClientStatus::Failed(_), _) => ("◯", Style::default().fg(theme::MUTED), "disabled"),
            (ClientStatus::Disconnected, _) => ("◯", Style::default().fg(theme::MUTED), "disabled"),
        };

        let name_style = if is_cursor {
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };

        lines.push(Line::from(vec![
            Span::styled(
                cursor_char.to_string(),
                Style::default().fg(theme::THINKING),
            ),
            Span::styled(server.name.clone(), name_style),
            Span::styled(" · ", Style::default().fg(theme::MUTED)),
            Span::styled(icon.to_string(), icon_style),
            Span::styled(format!(" {}", status_text), icon_style),
        ]));
    }
}

fn render_server_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let (server_name, tools, resources, actions, show_tools, cursor, scroll_offset) = {
        let panel = match app.mcp_panel.as_ref() {
            Some(p) => p,
            None => return,
        };
        let McpPanelView::ServerDetail {
            server_name,
            tools,
            resources,
            actions,
            show_tools,
        } = &panel.view
        else {
            return;
        };
        (
            server_name.clone(),
            tools.clone(),
            resources.clone(),
            actions.clone(),
            *show_tools,
            panel.cursor,
            panel.scroll_offset,
        )
    };

    // 渲染 BorderedPanel 边框（标题在边框线上）
    let inner = BorderedPanel::new(Span::styled(
        format!(" {} ", server_name),
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    let mut lines: Vec<Line> = Vec::new();

    // 获取当前 server info
    let server_info = app
        .mcp_panel
        .as_ref()
        .and_then(|p| p.servers.iter().find(|s| s.name == server_name));

    let label_width = 18;

    // Status 行
    if let Some(info) = server_info {
        let (status_icon, status_style) = match (&info.status, &info.oauth_status) {
            (ClientStatus::Connected, _) => ("✔", Style::default().fg(theme::SAGE)),
            (_, OAuthStatus::NeedsAuthorization) => ("△", Style::default().fg(theme::WARNING)),
            _ => ("◯", Style::default().fg(theme::MUTED)),
        };
        let status_label = match (&info.status, &info.oauth_status) {
            (ClientStatus::Connected, _) => "connected",
            (_, OAuthStatus::NeedsAuthorization) => "needs authentication",
            _ => "disabled",
        };
        lines.push(detail_line(
            label_width,
            "Status:",
            &format!("{} {}", status_icon, status_label),
            status_style,
        ));
    }

    // Auth 行
    if let Some(info) = server_info {
        let (auth_icon, auth_label, auth_style) = match &info.oauth_status {
            OAuthStatus::Authorized => ("✔", "authenticated", Style::default().fg(theme::SAGE)),
            OAuthStatus::NeedsAuthorization => (
                "△",
                "needs authentication",
                Style::default().fg(theme::WARNING),
            ),
            OAuthStatus::None => ("—", "none", Style::default().fg(theme::MUTED)),
        };
        lines.push(detail_line(
            label_width,
            "Auth:",
            &format!("{} {}", auth_icon, auth_label),
            auth_style,
        ));
    }

    // URL 行
    if let Some(info) = server_info {
        if let Some(url) = &info.url {
            lines.push(detail_line(
                label_width,
                "URL:",
                url,
                Style::default().fg(theme::TEXT),
            ));
        }
    }

    // Config location 行
    if let Some(info) = server_info {
        if let Some(source) = &info.source {
            let path_str = match source {
                ConfigSource::Project(p) | ConfigSource::Global(p) => p.display().to_string(),
            };
            lines.push(detail_line(
                label_width,
                "Config location:",
                &path_str,
                Style::default().fg(theme::TEXT),
            ));
        }
    }

    // Capabilities 行
    let mut capabilities = Vec::new();
    if !tools.is_empty() {
        capabilities.push("tools");
    }
    if !resources.is_empty() {
        capabilities.push("resources");
    }
    lines.push(detail_line(
        label_width,
        "Capabilities:",
        &capabilities.join(", "),
        Style::default().fg(theme::TEXT),
    ));

    // Tools 行
    lines.push(detail_line(
        label_width,
        "Tools:",
        &format!("{} tools", tools.len()),
        Style::default().fg(theme::TEXT),
    ));

    // 展开工具列表
    if show_tools {
        for (i, tool) in tools.iter().enumerate() {
            lines.push(Line::from(Span::styled(
                format!("      {}. {}", i + 1, tool.name),
                Style::default().fg(theme::MUTED),
            )));
        }
    }

    lines.push(Line::from(""));

    // Action 菜单
    for (i, action) in actions.iter().enumerate() {
        let is_cursor = i == cursor;
        let cursor_char = if is_cursor { "❯ " } else { "  " };
        let num = i + 1;
        let label = match action {
            DetailAction::ViewTools => {
                if show_tools {
                    "Hide tools"
                } else {
                    "View tools"
                }
            }
            DetailAction::ReAuthenticate => "Re-authenticate",
            DetailAction::ClearAuth => "Clear authentication",
            DetailAction::Reconnect => "Reconnect",
            DetailAction::Disable => "Disable",
        };
        let style = if is_cursor {
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };
        lines.push(Line::from(vec![
            Span::styled(
                cursor_char.to_string(),
                Style::default().fg(theme::THINKING),
            ),
            Span::styled(format!("{}. {}", num, label), style),
        ]));
    }

    // 存储面板元数据
    app.sessions[app.active].core.panel_area = Some(inner);
    app.sessions[app.active].core.panel_scroll_offset = 0;
    app.sessions[app.active].core.panel_plain_lines = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    apply_panel_selection(app, &mut lines, inner);

    let mut scroll_state = ScrollState::with_offset(scroll_offset);
    ScrollableArea::new(Text::from(lines))
        .scrollbar_style(Style::default().fg(theme::MUTED))
        .render(f, inner, &mut scroll_state);
}

/// 生成对齐的 key-value 行
fn detail_line<'a>(label_width: usize, label: &str, value: &str, value_style: Style) -> Line<'a> {
    let padded = format!("  {:<width$}", label, width = label_width);
    Line::from(vec![
        Span::styled(padded, Style::default().fg(theme::MUTED)),
        Span::styled(value.to_string(), value_style),
    ])
}

/// 将服务器按来源分组：(project_servers, user_servers)
fn partition_by_source(
    servers: &[rust_agent_middlewares::mcp::ServerInfo],
) -> (
    Vec<&rust_agent_middlewares::mcp::ServerInfo>,
    Vec<&rust_agent_middlewares::mcp::ServerInfo>,
) {
    let mut project = Vec::new();
    let mut user = Vec::new();
    for s in servers {
        match &s.source {
            Some(ConfigSource::Project(_)) => project.push(s),
            Some(ConfigSource::Global(_)) => user.push(s),
            None => user.push(s), // 无来源的归入 user 组
        }
    }
    (project, user)
}

/// 计算 project 组在 flat servers 列表中的起始偏移
fn project_start_offset(_servers: &[ServerInfo]) -> usize {
    0
}

/// 计算 user 组在 flat servers 列表中的起始偏移
fn user_start_offset(servers: &[ServerInfo]) -> usize {
    servers
        .iter()
        .filter(|s| matches!(s.source, Some(ConfigSource::Project(_))))
        .count()
}

fn apply_panel_selection(app: &mut App, lines: &mut Vec<Line>, area: Rect) {
    if app.sessions[app.active].core.panel_selection.is_active() {
        let sel = &app.sessions[app.active].core.panel_selection;
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
    use rust_agent_middlewares::mcp::{ClientStatus, ConfigSource, ServerInfo};

    use crate::app::{App, McpPanel, McpPanelView};

    fn make_server(name: &str, status: ClientStatus) -> ServerInfo {
        ServerInfo {
            name: name.to_string(),
            transport_type: "stdio".to_string(),
            status,
            tool_count: 3,
            resource_count: 2,
            oauth_status: Default::default(),
            source: None,
            url: None,
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
        assert!(snap.contains("No MCP servers"), "空 MCP 面板应显示引导文字");
    }

    #[tokio::test]
    async fn test_mcp_panel_server_list_with_items() {
        let handle = render_mcp_panel(vec![
            make_server("test-connected", ClientStatus::Connected),
            make_server("test-failed", ClientStatus::Failed("timeout".into())),
        ]);
        let snap = handle.snapshot().join("\n");
        assert!(snap.contains("test-connected"), "MCP 面板应显示服务器名称");
        assert!(snap.contains("connected"), "MCP 面板应显示 connected 状态");
    }

    #[tokio::test]
    async fn test_mcp_panel_detail_action_menu() {
        let (mut app, mut handle) = App::new_headless(120, 30);
        let mut srv = make_server("test-srv", ClientStatus::Connected);
        srv.transport_type = "http".to_string();
        srv.url = Some("https://example.com/mcp".to_string());
        app.mcp_panel = Some(McpPanel::new(vec![srv]));
        app.mcp_panel_enter();

        match &app.mcp_panel.as_ref().unwrap().view {
            McpPanelView::ServerDetail { actions, .. } => {
                assert!(
                    actions
                        .iter()
                        .any(|a| matches!(a, crate::app::DetailAction::ReAuthenticate)),
                    "HTTP 服务器应有 ReAuthenticate action"
                );
            }
            _ => panic!("应进入 ServerDetail 视图"),
        }

        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        let snap = handle.snapshot().join("\n");
        assert!(snap.contains("test-srv"), "详情页应显示服务器名");
    }

    #[tokio::test]
    async fn test_mcp_panel_grouped_by_source() {
        let mut project_srv = make_server("project-srv", ClientStatus::Connected);
        project_srv.source = Some(ConfigSource::Project(std::path::PathBuf::from(
            "/project/.mcp.json",
        )));
        let mut global_srv = make_server("global-srv", ClientStatus::Connected);
        global_srv.source = Some(ConfigSource::Global(std::path::PathBuf::from(
            "/home/.zen-code/settings.json",
        )));

        let handle = render_mcp_panel(vec![project_srv, global_srv]);
        let snap = handle.snapshot().join("\n");
        assert!(snap.contains("project-srv"), "应显示项目级服务器");
        assert!(snap.contains("global-srv"), "应显示全局服务器");
    }
}
