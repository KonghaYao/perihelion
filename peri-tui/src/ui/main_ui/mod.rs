mod attachment;
pub(crate) mod message_area;
pub(crate) mod panels;
mod popups;
mod status_bar;
mod sticky_header;

pub(crate) use message_area::highlight_line_spans;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;
use crate::ui::theme;

pub fn render(f: &mut Frame, app: &mut App) {
    // Setup 向导：全屏覆盖，优先于所有正常界面
    if app.global_ui.setup_wizard.is_some() {
        popups::setup_wizard::render_setup_wizard(f, app);
        return;
    }

    let area = f.area();

    if app.session_mgr.sessions.len() > 1 {
        // ── 多 Session 分栏布局 ──
        // 外层：水平切分（各 session 列）+ 底部共享状态栏
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // session 列区域
                Constraint::Length(3), // 共享状态栏
            ])
            .split(area);

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Percentage(50);
                app.session_mgr.sessions.len()
            ])
            .split(outer[0]);

        app.session_mgr.session_areas = cols.iter().copied().collect();

        // 先渲染非 active session（不设置光标位置），再渲染 active session
        for (i, col_area) in cols.iter().enumerate() {
            if i == app.session_mgr.active {
                continue;
            }
            render_session_column(f, app, i, *col_area, false);
        }
        render_session_column(
            f,
            app,
            app.session_mgr.active,
            cols[app.session_mgr.active],
            true,
        );

        status_bar::render_status_bar(f, app, outer[1]);
    } else {
        // ── 单 Session 布局（原有行为）──
        render_session_column(f, app, 0, area, true);
    }
}

/// 渲染单个 session 列（含垂直布局拆分）
fn render_session_column(
    f: &mut Frame,
    app: &mut App,
    session_idx: usize,
    area: Rect,
    is_active: bool,
) {
    // 临时切换 active 以便现有 render 函数使用 app.session_mgr.sessions[app.session_mgr.active]
    let prev_active = app.session_mgr.active;
    app.session_mgr.active = session_idx;

    // 多 session 模式：外层 block 作为聚焦指示
    let area = if app.session_mgr.sessions.len() > 1 {
        let border_color = if is_active {
            theme::ACCENT
        } else {
            theme::BORDER_DIM
        };
        let outer_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));
        let inner = outer_block.inner(area);
        f.render_widget(outer_block, area);
        inner
    } else {
        area
    };

    // 动态输入框高度
    let line_count = app.session_mgr.sessions[session_idx]
        .ui
        .textarea
        .lines()
        .len() as u16;
    let input_height = (line_count + 2).min(area.height * 2 / 5).max(3);

    // 缓冲消息高度（loading 时在输入框上方显示待发送消息）
    let pending_count = app.session_mgr.sessions[session_idx]
        .messages
        .pending_messages
        .len();
    let queued_height: u16 =
        if pending_count > 0 && app.session_mgr.sessions[session_idx].ui.loading {
            (pending_count as u16).min(3)
        } else {
            0
        };

    // 附件栏高度
    let attachment_height: u16 = if app.session_mgr.sessions[session_idx]
        .metadata
        .pending_attachments
        .is_empty()
    {
        0
    } else {
        3
    };

    // 底部展开区高度
    let panel_height = active_panel_height(app, area.height, area.width);

    // Sticky header 高度
    let sticky_header_height: u16 = app.session_mgr.sessions[session_idx]
        .metadata
        .last_human_message
        .as_ref()
        .map(|msg| {
            let width = area.width.saturating_sub(2).max(1);
            let lines = sticky_header::estimate_header_lines(msg, width);
            lines as u16
        })
        .unwrap_or(0);

    let status_bar_height = if app.session_mgr.sessions.len() > 1 {
        0
    } else {
        3
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(sticky_header_height),
            Constraint::Min(1),
            Constraint::Length(attachment_height),
            Constraint::Length(panel_height),
            Constraint::Length(queued_height),
            Constraint::Length(input_height),
            Constraint::Length(status_bar_height),
        ])
        .split(area);

    message_area::render_messages(f, app, chunks[0], chunks[1]);
    attachment::render_attachment_bar(f, app, chunks[2]);

    // 底部展开区
    if panel_height > 0 {
        let panel_area = chunks[3];
        match &app.session_mgr.sessions[session_idx]
            .agent
            .interaction_prompt
        {
            Some(crate::app::InteractionPrompt::Approval(_)) => {
                popups::hitl::render_hitl_popup(f, app, panel_area);
            }
            Some(crate::app::InteractionPrompt::Questions(_)) => {
                popups::ask_user::render_ask_user_popup(f, app, panel_area);
            }
            None => {}
        }
        if app.global_ui.oauth_prompt.is_some() {
            popups::oauth::render_oauth_popup(f, app, panel_area);
        }
        // PanelManager 统一渲染分发：session 面板优先，global 面板次之
        if app.session_mgr.sessions[session_idx]
            .agent
            .interaction_prompt
            .is_none()
            && app.global_ui.oauth_prompt.is_none()
        {
            if app.session_mgr.sessions[session_idx]
                .session_panels
                .is_any_open()
            {
                let mut state = app.session_mgr.sessions[session_idx]
                    .session_panels
                    .take_active()
                    .expect("is_any_open was true");
                state.render(f, app, panel_area);
                app.session_mgr.sessions[session_idx]
                    .session_panels
                    .put_active(state);
            } else if app.global_panels.is_any_open() {
                let mut state = app
                    .global_panels
                    .take_active()
                    .expect("is_any_open was true");
                state.render(f, app, panel_area);
                app.global_panels.put_active(state);
            }
        }
    }

    // 缓冲消息预览（loading 时在输入框上方显示待发送消息）
    if queued_height > 0 {
        let queued_area = chunks[4];
        let msgs = &app.session_mgr.sessions[session_idx]
            .messages
            .pending_messages;
        let visible_count = (pending_count).min(queued_height as usize);
        let pending_style = Style::default().fg(theme::MUTED).bg(theme::USER_BG);
        for (i, msg) in msgs.iter().take(visible_count).enumerate() {
            let line_area = Rect {
                x: queued_area.x + 2,
                y: queued_area.y + i as u16,
                width: queued_area.width.saturating_sub(2),
                height: 1,
            };
            // 截断到可用宽度（字符级安全）
            let max_chars = line_area.width as usize;
            let display: String = msg.chars().take(max_chars.saturating_sub(3)).collect();
            let suffix = if msg.chars().count() > max_chars.saturating_sub(3) {
                "…"
            } else {
                ""
            };
            f.render_widget(
                Paragraph::new(format!("{}{}", display, suffix)).style(pending_style),
                line_area,
            );
        }
        if pending_count > visible_count {
            let more_area = Rect {
                x: queued_area.x + 2,
                y: queued_area.y + visible_count as u16,
                width: queued_area.width.saturating_sub(2),
                height: 1,
            };
            f.render_widget(
                Paragraph::new(format!("… +{} more", pending_count - visible_count))
                    .style(pending_style),
                more_area,
            );
        }
    }

    // 输入框：非聚焦 session 或应用失焦时隐藏光标（移除 REVERSED 修饰符）
    let textarea_ref = &app.session_mgr.sessions[session_idx].ui.textarea;
    // 应用失焦 或 当前 session 未激活 → 隐藏光标
    let should_hide_cursor = !app.focused || !is_active;
    if should_hide_cursor {
        // 克隆并移除光标的 REVERSED 样式，使光标不可见
        let mut ta = textarea_ref.clone();
        // 将光标样式设置为与普通文本相同（无 REVERSED），光标字符将融入背景
        ta.set_cursor_style(Style::default().fg(theme::DIM));
        f.render_widget(&ta, chunks[5]);
    } else {
        f.render_widget(textarea_ref, chunks[5]);
    }
    app.session_mgr.sessions[session_idx].ui.textarea_area = Some(chunks[5]);

    // ❯ 前缀
    let prompt_x = chunks[5].x;
    let prompt_y = chunks[5].y + 1;
    let prompt_area = Rect {
        x: prompt_x,
        y: prompt_y,
        width: 2,
        height: 1,
    };
    let loading = app.session_mgr.sessions[session_idx].ui.loading;
    let prompt_color = if !is_active || loading {
        theme::MUTED
    } else {
        theme::TEXT
    };
    let prompt_style = Style::default().fg(prompt_color).add_modifier(if loading {
        Modifier::empty()
    } else {
        Modifier::BOLD
    });
    f.render_widget(Paragraph::new("❯").style(prompt_style), prompt_area);

    // 统一命令/Skills 提示条 或 @ 提及弹窗（互斥）
    if app.session_mgr.sessions[session_idx].ui.at_mention.active {
        crate::app::at_mention::popup::render_at_mention_popup(
            f,
            &app.session_mgr.sessions[session_idx].ui.at_mention,
            chunks[5],
        );
    } else {
        popups::hints::render_unified_hint(f, app, chunks[5]);
    }

    // 单 session 模式下渲染状态栏
    if app.session_mgr.sessions.len() == 1 {
        status_bar::render_status_bar(f, app, chunks[6]);
    }

    // 恢复原始 active
    app.session_mgr.active = prev_active;
}

/// 计算底部展开区所需高度（无激活面板时返回 0）
fn active_panel_height(app: &App, screen_height: u16, screen_width: u16) -> u16 {
    // plugin 面板可以占 70%，AskUser 弹窗允许 75%（选项多/文字长需要更多空间），其他最多 60%
    let is_plugin_panel = app.global_panels.is_active(crate::app::PanelKind::Plugin);
    let has_ask_user = matches!(
        &app.session_mgr.sessions[app.session_mgr.active]
            .agent
            .interaction_prompt,
        Some(crate::app::InteractionPrompt::Questions(_))
    );
    let max_h = if is_plugin_panel {
        screen_height * 70 / 100
    } else if has_ask_user {
        screen_height * 3 / 4
    } else {
        screen_height * 3 / 5
    };
    let raw = if let Some(h) = app.session_mgr.sessions[app.session_mgr.active]
        .session_panels
        .dispatch_desired_height(screen_height, screen_width)
    {
        h
    } else if let Some(h) = app
        .global_panels
        .dispatch_desired_height(screen_height, screen_width)
    {
        h
    } else if let Some(crate::app::InteractionPrompt::Approval(p)) = &app.session_mgr.sessions
        [app.session_mgr.active]
        .agent
        .interaction_prompt
    {
        (p.items.len() as u16 * 2 + 5).max(5)
    } else if app.global_ui.oauth_prompt.is_some() {
        9 // 标题1 + 提示1 + URL1 + 空行1 + 输入框1 + 错误1 + 快捷键1 + 边框2
    } else if let Some(crate::app::InteractionPrompt::Questions(p)) = &app.session_mgr.sessions
        [app.session_mgr.active]
        .agent
        .interaction_prompt
    {
        let cur = &p.questions[p.active_tab];
        // BorderedPanel 无左右边框，内容区宽度 = screen_width；滚动条占 1 列
        let panel_width = screen_width.saturating_sub(1) as usize;
        popups::ask_user_height::ask_user_content_height(&cur.data, panel_width).max(8)
    } else {
        0
    };
    raw.min(max_h)
}
