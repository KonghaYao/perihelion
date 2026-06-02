use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    Frame,
};

use peri_widgets::{BorderedPanel, ScrollState, ScrollableArea};

use crate::{
    app::{
        plugin_panel::{MarketplaceViewStatus, PluginItemType, PluginPanel, PluginPanelView},
        App,
    },
    ui::theme,
};

use peri_middlewares::plugin::InstallScope;

use super::truncate_display;

pub(crate) fn render_list(f: &mut Frame, panel: &PluginPanel, app: &mut App, area: Rect) {
    let (lines, scroll_offset, cursor_row) = {
        let scroll_offset = panel.scroll_offset();
        let mut lines: Vec<Line> = Vec::new();
        let mut cursor_row = 0; // 光标所在行号（不含 Tab 行）

        // Tab 行
        let tab_labels: Vec<Span> = PluginPanelView::ALL
            .iter()
            .map(|v| {
                let label = v.label();
                let is_active = panel.view == *v;
                let style = if is_active {
                    Style::default()
                        .fg(theme::TEXT)
                        .bg(theme::THINKING)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::MUTED)
                };
                Span::styled(format!(" {} ", label), style)
            })
            .collect();
        lines.push(Line::from(tab_labels));
        lines.push(Line::from(""));

        // 根据视图渲染内容
        match panel.view {
            PluginPanelView::Installed => {
                let indices = panel.visible_indices();
                let cursor_idx = indices.get(panel.cursor()).copied();
                let table_header_height = 3; // 表头行 + 空行

                if indices.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "  No plugins installed".to_string(),
                        Style::default().fg(theme::MUTED),
                    )));
                } else {
                    // 表头
                    lines.push(Line::from(vec![
                        Span::styled(
                            "  Plugin",
                            Style::default()
                                .fg(theme::MUTED)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "                  Type  Scope      Status  Marketplace",
                            Style::default().fg(theme::MUTED),
                        ),
                    ]));
                    lines.push(Line::from(""));

                    // 直接遍历所有可见条目，不分组显示标题
                    for (row_idx, &idx) in indices.iter().enumerate() {
                        if let Some(entry) = panel.entries.get(idx) {
                            let is_cursor = cursor_idx == Some(idx);
                            if is_cursor {
                                cursor_row = table_header_height + row_idx;
                            }
                            let cursor_char = if is_cursor { "\u{276F} " } else { "  " };

                            let type_label = match entry.plugin_type {
                                PluginItemType::Plugin => "Plugin",
                                PluginItemType::Mcp => "MCP    ",
                            };

                            let (status_icon, status_style) = if entry.enabled {
                                ("\u{2714} ", Style::default().fg(theme::SAGE))
                            } else {
                                ("  ", Style::default().fg(theme::MUTED))
                            };

                            let name_style = if is_cursor {
                                Style::default()
                                    .fg(theme::THINKING)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme::TEXT)
                            };

                            let scope_label = match entry.scope {
                                InstallScope::User => "User  ",
                                InstallScope::Project => "Project",
                                InstallScope::Local => "Local ",
                            };

                            // 表格行：光标 + 名称 + 类型 + 作用域 + 状态 + marketplace（右对齐）
                            let name_width = 18;
                            let display_name = truncate_display(&entry.name, name_width);
                            let name_padding = " ".repeat(name_width.saturating_sub(
                                unicode_width::UnicodeWidthStr::width(display_name.as_str()),
                            ));

                            let marketplace_text = if !entry.marketplace.is_empty() {
                                entry.marketplace.clone()
                            } else {
                                String::new()
                            };

                            lines.push(Line::from(vec![
                                Span::styled(
                                    cursor_char.to_string(),
                                    Style::default().fg(theme::THINKING),
                                ),
                                Span::styled(display_name, name_style),
                                Span::styled(name_padding, Style::default()),
                                Span::styled(type_label, Style::default().fg(theme::MUTED)),
                                Span::styled("  ", Style::default()),
                                Span::styled(scope_label, Style::default().fg(theme::MUTED)),
                                Span::styled("  ", Style::default()),
                                Span::styled(status_icon.to_string(), status_style),
                                Span::styled("  ", Style::default()),
                                Span::styled(marketplace_text, Style::default().fg(theme::MUTED)),
                            ]));
                        }
                    }
                }
            }
            PluginPanelView::Errors => {
                let indices = panel.visible_indices();
                let cursor_idx = indices.get(panel.cursor()).copied();
                let table_header_height = 3; // 表头行 + 空行

                if indices.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "  No errors".to_string(),
                        Style::default().fg(theme::SAGE),
                    )));
                } else {
                    // 表头
                    lines.push(Line::from(vec![
                        Span::styled(
                            "  Plugin",
                            Style::default()
                                .fg(theme::MUTED)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "                  Scope  Error",
                            Style::default().fg(theme::MUTED),
                        ),
                    ]));
                    lines.push(Line::from(""));

                    for (row_idx, &idx) in indices.iter().enumerate() {
                        if let Some(entry) = panel.entries.get(idx) {
                            let is_cursor = cursor_idx == Some(idx);
                            if is_cursor {
                                cursor_row = table_header_height + row_idx;
                            }
                            let cursor_char = if is_cursor { "\u{276F} " } else { "  " };

                            let name_style = if is_cursor {
                                Style::default()
                                    .fg(theme::THINKING)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme::TEXT)
                            };

                            let scope_label = match entry.scope {
                                InstallScope::User => "User  ",
                                InstallScope::Project => "Project",
                                InstallScope::Local => "Local ",
                            };

                            let error_text = entry.load_error.as_deref().unwrap_or("Unknown error");

                            lines.push(Line::from(vec![
                                Span::styled(
                                    cursor_char.to_string(),
                                    Style::default().fg(theme::THINKING),
                                ),
                                Span::styled(truncate_display(&entry.name, 18), name_style),
                                Span::styled("  ", Style::default()),
                                Span::styled(scope_label, Style::default().fg(theme::MUTED)),
                                Span::styled("  ", Style::default()),
                                Span::styled(
                                    error_text.to_string(),
                                    Style::default().fg(theme::ERROR),
                                ),
                            ]));
                        }
                    }
                }
            }
            PluginPanelView::Discover => {
                lines.push(Line::from(Span::styled(
                    "  Discover",
                    Style::default().fg(theme::MUTED),
                )));
            }
            PluginPanelView::Marketplaces => {
                cursor_row = if panel.marketplace_confirm_delete.is_some() {
                    2
                } else if panel.marketplace_list.cursor() == 0 {
                    2
                } else {
                    5 + (panel.marketplace_list.cursor() - 1) * 4
                };

                if let Some(confirm_idx) = panel.marketplace_confirm_delete {
                    if let Some(mkt) = panel.marketplace_entries.get(confirm_idx) {
                        lines.push(Line::from(""));
                        lines.push(Line::from(vec![
                            Span::styled(
                                "  \u{786E}\u{8BA4}\u{8981}\u{79FB}\u{9664} marketplace ",
                                Style::default().fg(theme::TEXT),
                            ),
                            Span::styled(
                                mkt.name.clone(),
                                Style::default()
                                    .fg(theme::THINKING)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(" ?", Style::default().fg(theme::TEXT)),
                        ]));
                        lines.push(Line::from(""));
                        lines.push(Line::from(vec![
                            Span::styled("  \u{6309}\u{4E0B} ", Style::default().fg(theme::MUTED)),
                            Span::styled(
                                "Enter",
                                Style::default()
                                    .fg(theme::ACCENT)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                " \u{786E}\u{8BA4}\u{FF0C}",
                                Style::default().fg(theme::MUTED),
                            ),
                            Span::styled(
                                "Esc",
                                Style::default()
                                    .fg(theme::ACCENT)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(" \u{53D6}\u{6D88}", Style::default().fg(theme::MUTED)),
                        ]));
                    }
                } else {
                    let is_add_cursor = panel.marketplace_list.cursor() == 0;
                    let add_style = if is_add_cursor {
                        Style::default()
                            .fg(theme::THINKING)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme::TEXT)
                    };
                    let add_cursor = if is_add_cursor { "\u{276F} " } else { "  " };
                    lines.push(Line::from(vec![
                        Span::styled(add_cursor.to_string(), Style::default().fg(theme::THINKING)),
                        Span::styled("Add Marketplace", add_style),
                    ]));
                    lines.push(Line::from(vec![
                        Span::styled("     ".to_string(), Style::default()),
                        Span::styled(
                            "\u{6DFB}\u{52A0}\u{65B0}\u{7684} marketplace \u{6E90}",
                            Style::default().fg(theme::MUTED),
                        ),
                    ]));
                    lines.push(Line::from(""));

                    if panel.marketplace_entries.is_empty() {
                        lines.push(Line::from(Span::styled(
                            "  No marketplaces configured",
                            Style::default().fg(theme::MUTED),
                        )));
                    } else {
                        for (i, mkt) in panel.marketplace_entries.iter().enumerate() {
                            let is_cursor = panel.marketplace_list.cursor() == i + 1;
                            let is_updating = panel.marketplace_updating.contains(&mkt.name);
                            let cursor_char = if is_cursor { "\u{276F} " } else { "  " };

                            let name_style = if is_cursor {
                                Style::default()
                                    .fg(theme::THINKING)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(theme::TEXT)
                            };

                            let (status_text, status_style) = if is_updating {
                                ("updating\u{2026}", Style::default().fg(theme::WARNING))
                            } else {
                                match mkt.status {
                                    MarketplaceViewStatus::Fresh
                                    | MarketplaceViewStatus::Cached => {
                                        ("cached", Style::default().fg(theme::SAGE))
                                    }
                                    MarketplaceViewStatus::Fetching => {
                                        ("fetching\u{2026}", Style::default().fg(theme::WARNING))
                                    }
                                    MarketplaceViewStatus::Stale => {
                                        ("stale", Style::default().fg(theme::WARNING))
                                    }
                                    MarketplaceViewStatus::Failed => {
                                        ("failed", Style::default().fg(theme::ERROR))
                                    }
                                }
                            };

                            lines.push(Line::from(vec![
                                Span::styled(
                                    cursor_char.to_string(),
                                    Style::default().fg(theme::THINKING),
                                ),
                                Span::styled(mkt.name.clone(), name_style),
                            ]));

                            let mut detail_parts = vec![
                                Span::styled("     ".to_string(), Style::default()),
                                Span::styled(
                                    mkt.source_label.clone(),
                                    Style::default().fg(theme::MUTED),
                                ),
                            ];

                            detail_parts.push(Span::styled(
                                format!(" \u{00B7} {} available", mkt.plugin_count),
                                Style::default().fg(theme::MUTED),
                            ));

                            if mkt.installed_count > 0 {
                                detail_parts.push(Span::styled(
                                    format!(" \u{00B7} {} installed", mkt.installed_count),
                                    Style::default().fg(theme::SAGE),
                                ));
                            }

                            lines.push(Line::from(detail_parts));

                            let mut status_parts = vec![
                                Span::styled("     ", Style::default()),
                                Span::styled(status_text.to_string(), status_style),
                            ];

                            if let Some(ref updated) = mkt.last_updated {
                                status_parts.push(Span::styled(
                                    format!(" \u{00B7} Updated {}", updated),
                                    Style::default().fg(theme::MUTED),
                                ));
                            }

                            let auto_label = if mkt.auto_update { "on" } else { "off" };
                            status_parts.push(Span::styled(
                                format!(" \u{00B7} auto-update: {}", auto_label),
                                Style::default().fg(theme::MUTED),
                            ));

                            lines.push(Line::from(status_parts));
                            lines.push(Line::from(""));
                        }
                    }
                }
            }
        }

        (lines, scroll_offset, cursor_row)
    };

    let inner = BorderedPanel::new(Span::styled(
        " Plugins ",
        Style::default()
            .fg(theme::THINKING)
            .add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .render(f, area);

    app.session_mgr.current_mut().ui.panel_area = Some(inner);
    app.session_mgr.current_mut().ui.panel_scroll_offset = 0;
    app.session_mgr.current_mut().ui.panel_plain_lines = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect();

    let visible_height = inner.height.saturating_sub(1);
    let mut scroll_state = ScrollState::with_offset(scroll_offset);
    scroll_state.ensure_visible(cursor_row as u16, visible_height);

    if let Some(p) = app.global_panels.get_mut::<PluginPanel>() {
        p.set_scroll_offset(scroll_state.offset());
    }

    app.session_mgr.current_mut().ui.panel_scrollbar_metrics =
        ScrollableArea::new(Text::from(lines))
            .scrollbar_style(Style::default().fg(theme::MUTED))
            .render(f, inner, &mut scroll_state);
}
