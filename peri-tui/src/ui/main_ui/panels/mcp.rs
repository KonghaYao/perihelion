use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    Frame,
};

use peri_widgets::{BorderedPanel, ScrollState, ScrollableArea};

use crate::{
    app::{App, DetailAction, McpPanel, McpPanelView},
    i18n::LcRegistry,
    ui::{main_ui::highlight_line_spans, theme},
};

use peri_middlewares::mcp::{ClientStatus, ConfigSource, OAuthStatus, ServerInfo};

/// MCP 管理面板渲染
pub(crate) fn render_mcp_panel(f: &mut Frame, panel: &McpPanel, app: &mut App, area: Rect) {
    if panel.view.is_server_list() {
        render_server_list(f, panel, app, area);
    } else {
        render_server_detail(f, panel, app, area);
    }
}

fn render_server_list(f: &mut Frame, panel: &McpPanel, app: &mut App, area: Rect) {
    let lc = &app.services.lc;
    // Phase 1: 读取面板数据并构建所有行（不可变借用 panel）
    let (mut lines, scroll_offset) = {
        let scroll_offset = panel.scroll_offset();
        let cursor = panel.cursor();
        let servers = &panel.servers;
        let mut lines: Vec<Line> = Vec::new();

        // 服务器计数
        let count = servers.len();
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                lc.tr_args(
                    "mcp-server-count",
                    &[("count".into(), (count as u64).into())]
                )
            ),
            Style::default().fg(theme::MUTED),
        )));
        lines.push(Line::from(""));

        // 按来源分组
        let (project_servers, user_servers) = partition_by_source(servers);

        if !project_servers.is_empty() {
            let header_text = match &project_servers[0].source {
                Some(ConfigSource::Project(path)) => lc.tr_args(
                    "mcp-section-project-path",
                    &[("path".into(), path.display().to_string().into())],
                ),
                _ => lc.tr("mcp-section-project"),
            };
            lines.push(Line::from(Span::styled(
                format!("  {}", header_text),
                Style::default().fg(theme::MUTED),
            )));
            render_server_group(
                &mut lines,
                &project_servers,
                cursor,
                &project_start_offset(servers),
                lc,
            );
            lines.push(Line::from(""));
        }

        if !user_servers.is_empty() {
            let header_text = match &user_servers[0].source {
                Some(ConfigSource::Global(path)) => lc.tr_args(
                    "mcp-section-user-path",
                    &[("path".into(), path.display().to_string().into())],
                ),
                Some(ConfigSource::Plugin) => lc.tr("mcp-section-plugin"),
                _ => lc.tr("mcp-section-user"),
            };
            lines.push(Line::from(Span::styled(
                format!("  {}", header_text),
                Style::default().fg(theme::MUTED),
            )));
            render_server_group(
                &mut lines,
                &user_servers,
                cursor,
                &user_start_offset(servers),
                lc,
            );
            lines.push(Line::from(""));
        }

        if servers.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("  {}", lc.tr("mcp-no-servers")),
                Style::default().fg(theme::MUTED),
            )));
        }

        (lines, scroll_offset)
    }; // panel 借用在此结束

    // Phase 2: 渲染 BorderedPanel 边框（标题在边框线上）
    let inner = BorderedPanel::new(Span::styled(
        format!(" {} ", lc.tr("mcp-panel-title")),
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    // Phase 3: 写入元数据和渲染内容（可变借用 app）
    app.session_mgr.current_mut().ui.panel_area = Some(inner);
    app.session_mgr.current_mut().ui.panel_scroll_offset = 0;
    app.session_mgr.current_mut().ui.panel_plain_lines = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    apply_panel_selection(app, &mut lines, inner);

    let mut scroll_state = ScrollState::with_offset(scroll_offset);
    app.session_mgr.current_mut().ui.panel_scrollbar_metrics =
        ScrollableArea::new(Text::from(lines))
            .scrollbar_style(Style::default().fg(theme::MUTED))
            .render(f, inner, &mut scroll_state);
}

fn render_server_group(
    lines: &mut Vec<Line>,
    servers: &[&peri_middlewares::mcp::ServerInfo],
    global_cursor: usize,
    start_offset: &usize,
    lc: &LcRegistry,
) {
    for (i, server) in servers.iter().enumerate() {
        let flat_idx = *start_offset + i;
        let is_cursor = flat_idx == global_cursor;
        let cursor_char = if is_cursor { "❯ " } else { "  " };

        let (icon, icon_style, status_text) = match (&server.status, &server.oauth_status) {
            (ClientStatus::Connected, _) => (
                "✔",
                Style::default().fg(theme::SAGE),
                lc.tr("mcp-status-connected"),
            ),
            (_, OAuthStatus::NeedsAuthorization) => (
                "△",
                Style::default().fg(theme::WARNING),
                lc.tr("mcp-status-needs-auth"),
            ),
            (ClientStatus::Failed(_), _) => (
                "✗",
                Style::default().fg(theme::ERROR),
                lc.tr("mcp-status-error"),
            ),
            (ClientStatus::Disabled, _) => (
                "◯",
                Style::default().fg(theme::MUTED),
                lc.tr("mcp-status-disabled"),
            ),
            (ClientStatus::Uninitialized, _) => (
                "◯",
                Style::default().fg(theme::MUTED),
                lc.tr("mcp-status-uninitialized"),
            ),
            (ClientStatus::Disconnected, _) => (
                "◯",
                Style::default().fg(theme::MUTED),
                lc.tr("mcp-status-offline"),
            ),
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

fn render_server_detail(f: &mut Frame, panel: &McpPanel, app: &mut App, area: Rect) {
    let lc = &app.services.lc;
    let (server_name, tools, resources, actions, show_tools, cursor, scroll_offset) = {
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
            panel.cursor(),
            panel.scroll_offset(),
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
    let server_info = panel.servers.iter().find(|s| s.name == server_name);

    let label_width = 18;

    // Status 行
    if let Some(info) = server_info {
        let (status_icon, status_style) = match (&info.status, &info.oauth_status) {
            (ClientStatus::Connected, _) => ("✔", Style::default().fg(theme::SAGE)),
            (ClientStatus::Disabled, _) => ("⊘", Style::default().fg(theme::MUTED)),
            (_, OAuthStatus::NeedsAuthorization) => ("△", Style::default().fg(theme::WARNING)),
            (ClientStatus::Failed(_), _) => ("✗", Style::default().fg(theme::ERROR)),
            (ClientStatus::Uninitialized, _) => ("◯", Style::default().fg(theme::MUTED)),
            _ => ("◯", Style::default().fg(theme::MUTED)),
        };
        let status_label = match (&info.status, &info.oauth_status) {
            (ClientStatus::Connected, _) => lc.tr("mcp-status-connected"),
            (ClientStatus::Disabled, _) => lc.tr("mcp-status-disabled"),
            (_, OAuthStatus::NeedsAuthorization) => lc.tr("mcp-status-needs-auth"),
            (ClientStatus::Failed(_), _) => lc.tr("mcp-status-error"),
            (ClientStatus::Uninitialized, _) => lc.tr("mcp-status-uninitialized"),
            (ClientStatus::Disconnected, _) => lc.tr("mcp-status-offline"),
        };
        lines.push(detail_line(
            label_width,
            &lc.tr("mcp-label-status"),
            &format!("{} {}", status_icon, status_label),
            status_style,
        ));
    }

    // Auth 行
    if let Some(info) = server_info {
        let (auth_icon, auth_label, auth_style) = match &info.oauth_status {
            OAuthStatus::Authorized => (
                "✔",
                lc.tr("mcp-auth-authenticated"),
                Style::default().fg(theme::SAGE),
            ),
            OAuthStatus::NeedsAuthorization => (
                "△",
                lc.tr("mcp-status-needs-auth"),
                Style::default().fg(theme::WARNING),
            ),
            OAuthStatus::None => (
                "—",
                lc.tr("mcp-auth-none"),
                Style::default().fg(theme::MUTED),
            ),
        };
        lines.push(detail_line(
            label_width,
            &lc.tr("mcp-label-auth"),
            &format!("{} {}", auth_icon, auth_label),
            auth_style,
        ));
    }

    // URL 行
    if let Some(info) = server_info {
        if let Some(url) = &info.url {
            lines.push(detail_line(
                label_width,
                &lc.tr("mcp-label-url"),
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
                ConfigSource::Plugin => {
                    if let Some(ps) = &info.plugin_source {
                        lc.tr_args(
                            "mcp-label-plugin-source",
                            &[("source".into(), ps.clone().into())],
                        )
                    } else {
                        lc.tr("mcp-label-plugin")
                    }
                }
            };
            lines.push(detail_line(
                label_width,
                &lc.tr("mcp-label-config-location"),
                &path_str,
                Style::default().fg(theme::TEXT),
            ));
        }
    }

    // Capabilities 行
    let mut capabilities = Vec::new();
    if !tools.is_empty() {
        capabilities.push(lc.tr("mcp-capability-tools"));
    }
    if !resources.is_empty() {
        capabilities.push(lc.tr("mcp-capability-resources"));
    }
    lines.push(detail_line(
        label_width,
        &lc.tr("mcp-label-capabilities"),
        &capabilities.join(", "),
        Style::default().fg(theme::TEXT),
    ));

    // Tools 行
    lines.push(detail_line(
        label_width,
        &lc.tr("mcp-label-tools"),
        &lc.tr_args(
            "mcp-label-tools-count",
            &[("count".into(), (tools.len() as i64).into())],
        ),
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
                    lc.tr("mcp-action-hide-tools")
                } else {
                    lc.tr("mcp-action-view-tools")
                }
            }
            DetailAction::ReAuthenticate => lc.tr("mcp-action-reauthenticate"),
            DetailAction::ClearAuth => lc.tr("mcp-action-clear-auth"),
            DetailAction::Reconnect => lc.tr("mcp-action-reconnect"),
            DetailAction::Disable => lc.tr("mcp-action-disable"),
            DetailAction::Enable => lc.tr("mcp-action-enable"),
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
    app.session_mgr.current_mut().ui.panel_area = Some(inner);
    app.session_mgr.current_mut().ui.panel_scroll_offset = 0;
    app.session_mgr.current_mut().ui.panel_plain_lines = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    apply_panel_selection(app, &mut lines, inner);

    let mut scroll_state = ScrollState::with_offset(scroll_offset);
    app.session_mgr.current_mut().ui.panel_scrollbar_metrics =
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
    servers: &[peri_middlewares::mcp::ServerInfo],
) -> (
    Vec<&peri_middlewares::mcp::ServerInfo>,
    Vec<&peri_middlewares::mcp::ServerInfo>,
) {
    let mut project = Vec::new();
    let mut user = Vec::new();
    for s in servers {
        match &s.source {
            Some(ConfigSource::Project(_)) => project.push(s),
            Some(ConfigSource::Global(_)) | Some(ConfigSource::Plugin) | None => user.push(s),
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
    if app.session_mgr.current_mut().ui.panel_selection.is_active() {
        let sel = &app.session_mgr.current_mut().ui.panel_selection;
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
    use crate::app::{App, DetailAction, McpPanel, McpPanelView};
    use peri_middlewares::mcp::{ClientStatus, ConfigSource, ServerInfo};
    include!("mcp_test.rs");
}
