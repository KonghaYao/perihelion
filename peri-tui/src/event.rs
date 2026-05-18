use anyhow::Result;
use base64::Engine as _;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use std::time::Duration;
use tui_textarea::{Input, Key};

use crate::app::panel_manager::{EventResult, PanelKind};
use crate::app::{App, MessageViewModel, PendingAttachment};
use peri_agent::messages::BaseMessage;
use ratatui::layout::Rect;

/// 在 global_panels 上执行面板 dispatch，自动处理 mem::take 借用规避。
/// 使用方式：`with_global_panels!(app, |pm, ctx| { ... })`，闭包内 pm 为 &mut PanelManager。
macro_rules! with_global_panels {
    ($app:expr, |$pm:ident, $ctx:ident| $body:expr) => {{
        let mut $pm = std::mem::take(&mut $app.global_panels);
        let mut $ctx = $crate::app::panel_manager::PanelContext {
            services: &mut $app.services,
            session_mgr: &mut $app.session_mgr,
        };
        let result = { $body };
        $app.global_panels = $pm;
        result
    }};
}

/// 在 session 的 session_panels 上执行面板 dispatch，自动处理 mem::take 借用规避。
/// 使用方式：`with_session_panels!(app, |sp, ctx| { ... })`，闭包内 sp 为 &mut PanelManager。
macro_rules! with_session_panels {
    ($app:expr, |$sp:ident, $ctx:ident| $body:expr) => {{
        let active_idx = $app.session_mgr.active;
        let mut $sp = std::mem::take(&mut $app.session_mgr.sessions[active_idx].session_panels);
        let mut $ctx = $crate::app::panel_manager::PanelContext {
            services: &mut $app.services,
            session_mgr: &mut $app.session_mgr,
        };
        let result = { $body };
        $app.session_mgr.sessions[active_idx].session_panels = $sp;
        result
    }};
}

/// 检查鼠标事件是否在指定矩形区域内
fn mouse_in_rect(mouse: &ratatui::crossterm::event::MouseEvent, area: Rect) -> bool {
    mouse.row >= area.y
        && mouse.row < area.y + area.height
        && mouse.column >= area.x
        && mouse.column < area.x + area.width
}

/// 将终端显示列坐标转换为字符串的字符索引
///
/// 终端中 CJK 等全角字符占 2 列宽，`mouse.column` 是终端列坐标，
/// 但 `CursorMove::Jump(row, col)` 的 `col` 是字符索引。
/// 此函数逐字符累加 `unicode_width`，找到不超过 `display_col` 的最大字符索引。
fn display_col_to_char_idx(line: &str, display_col: usize) -> usize {
    let mut col = 0usize;
    for (char_idx, ch) in line.chars().enumerate() {
        if col >= display_col {
            return char_idx;
        }
        col += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
    }
    // 点击位置超过行末 → 返回行末字符索引
    line.chars().count()
}

/// 将鼠标在 textarea 区域内的坐标转换为 textarea 的 (row, char_idx)
///
/// 需要处理四个偏移：
/// 1. **Block border + padding**：textarea 渲染时通过 `Block::inner(area)` 去掉
///    边框和 padding，鼠标坐标需要减去这些偏移才能得到文本区域的坐标
/// 2. **垂直滚动偏移**：当文本行数超过可视区域高度时，textarea 会垂直滚动（`top_row`），
///    可见区域内的第 0 行对应文本的第 `top_row` 行
/// 3. **水平滚动偏移**：当文本超长时，textarea 会水平滚动（`top_col`），
///    可见区域内的第 0 列对应文本的第 `top_col` 显示列
/// 4. **CJK 字符宽度**：`Jump(row, col)` 的 `col` 是字符索引而非显示列宽，
///    需要通过 `unicode_width` 逐字符累加转换
///
/// `top_row` 和 `top_col` 通过 cursor 位置反推，因为 tui_textarea 的 viewport 是私有的。
fn textarea_mouse_to_cursor(
    textarea: &tui_textarea::TextArea<'_>,
    textarea_area: ratatui::layout::Rect,
    mouse: &ratatui::crossterm::event::MouseEvent,
) -> (usize, usize) {
    // 1. 计算 inner area（去掉 border + padding）
    let inner = textarea
        .block()
        .map(|b| b.inner(textarea_area))
        .unwrap_or(textarea_area);
    let inner_width = inner.width as usize;
    let inner_height = inner.height as usize;

    // 鼠标在 inner 区域内的坐标（saturating 防止点击边框时 u16 溢出）
    let visual_row = mouse.row.saturating_sub(inner.y) as usize;
    let visual_col = mouse.column.saturating_sub(inner.x) as usize;

    // 2. 反推垂直滚动偏移（top_row）
    // tui_textarea 使用 next_scroll_top 逻辑：cursor < top_row → top_row = cursor;
    // cursor >= top_row + height → top_row = cursor + 1 - height; 否则不变
    // 由于 viewport 是私有的，我们只能从 cursor 位置反推：
    // cursor 一定在 [top_row, top_row + height) 内，所以 top_row <= cursor_row
    let (cursor_row, cursor_col) = textarea.cursor();
    let scroll_row = cursor_row.saturating_sub(inner_height.saturating_sub(1));

    // 3. 反推水平滚动偏移（top_col，以显示列为单位）
    let cursor_line = textarea
        .lines()
        .get(cursor_row)
        .map(|s| s.as_str())
        .unwrap_or("");
    let cursor_display_col: usize = cursor_line
        .chars()
        .take(cursor_col)
        .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
        .sum();
    let scroll_col = cursor_display_col.saturating_sub(inner_width.saturating_sub(1));

    // 4. 文本内的行和显示列
    let target_row = scroll_row + visual_row;
    let text_display_col = visual_col + scroll_col;

    // 5. 将显示列转换为字符索引
    let target_row = target_row.min(textarea.lines().len().saturating_sub(1));
    let target_line = textarea
        .lines()
        .get(target_row)
        .map(|s| s.as_str())
        .unwrap_or("");
    let char_idx = display_col_to_char_idx(target_line, text_display_col);

    (target_row, char_idx)
}

/// 将 RGBA 像素数据编码为 PNG，再返回 base64 字符串和 PNG 字节数
fn rgba_to_png_base64(width: u32, height: u32, rgba_bytes: &[u8]) -> Result<(String, usize)> {
    let mut png_bytes: Vec<u8> = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(rgba_bytes)?;
    }
    let size = png_bytes.len();
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    Ok((b64, size))
}

pub enum Action {
    Quit,
    Submit(String),
    Redraw,
}

/// 将选区文本复制到系统剪贴板并更新 UI 提示。返回 true 表示成功复制。
fn copy_selection_to_clipboard(app: &mut App) -> bool {
    if let Some(text) = app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .text_selection
        .selected_text
        .take()
    {
        let char_count = text.chars().count();
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(&text);
        }
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .copy_char_count = char_count;
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .copy_message_until =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(2000));
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .text_selection
            .clear();
        return true;
    }
    false
}

/// 将面板选区文本复制到系统剪贴板。返回 true 表示成功复制。
fn copy_panel_selection_to_clipboard(app: &mut App) -> bool {
    if let Some(text) = app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .panel_selection
        .selected_text
        .take()
    {
        let char_count = text.chars().count();
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(&text);
        }
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .copy_char_count = char_count;
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .copy_message_until =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(2000));
        app.session_mgr.sessions[app.session_mgr.active]
            .ui
            .panel_selection
            .clear();
        return true;
    }
    false
}

pub async fn next_event(app: &mut App) -> Result<Option<Action>> {
    // 退出待确认状态 2 秒后自动过期，快捷键栏恢复正常
    // 退出待确认状态 2 秒后自动过期，触发重绘让快捷键栏恢复正常
    if let Some(since) = app.global_ui.quit_pending_since {
        if since.elapsed() >= std::time::Duration::from_secs(1) {
            app.global_ui.quit_pending_since = None;
            return Ok(Some(Action::Redraw));
        }
    }

    // 鼠标可用性 probe：启动后首次收到任意用户输入时判定
    if app.global_ui.mouse_available.is_none() {
        // 等待首个事件（最多 1 秒），期间不计入正常 poll 超时
        if event::poll(Duration::from_secs(1))? {
            let ev = event::read()?;
            if matches!(ev, Event::Mouse(_)) {
                app.global_ui.mouse_available = Some(true);
            } else {
                // 收到键盘/resize 等非鼠标事件 → 终端很可能不支持鼠标
                //（支持鼠标的终端用户几乎必然在 1s 内触发滚轮/移动）
                app.global_ui.mouse_available = Some(false);
            }
            return handle_event(app, ev).await;
        } else {
            // 1 秒内无任何事件 → 无鼠标
            app.global_ui.mouse_available = Some(false);
            return Ok(None);
        }
    }

    if !event::poll(Duration::from_millis(50))? {
        return Ok(None);
    }

    let ev = event::read()?;

    handle_event(app, ev).await
}

/// 实际的事件处理逻辑（从 next_event 中提取，避免 probe 和正常路径重复）
async fn handle_event(app: &mut App, ev: Event) -> Result<Option<Action>> {
    match ev {
        Event::FocusGained => {
            app.focused = true;
            return Ok(Some(Action::Redraw));
        }
        Event::FocusLost => {
            app.focused = false;
            return Ok(Some(Action::Redraw));
        }
        Event::Resize(_, _) => {
            // 宽度同步改由 render_messages 渲染驱动（比较 cache.width 与 text_area.width）
            app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .text_selection
                .clear();
        }
        Event::Key(key_event) => {
            // 只处理 Press 事件，忽略 Release（防止按键重复触发）
            if key_event.kind == KeyEventKind::Release {
                return Ok(Some(Action::Redraw));
            }

            // Shift+Tab 在 crossterm 中报告为 BackTab，
            // ratatui-textarea 的 Key 枚举不处理 BackTab（映射为 Null），
            // 因此在这里提前拦截，直接处理权限模式切换。
            if matches!(key_event.code, ratatui::crossterm::event::KeyCode::BackTab) {
                let _new_mode = app.services.permission_mode.cycle();
                app.global_ui.mode_highlight_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
                return Ok(Some(Action::Redraw));
            }

            // Alt+M 循环切换模型别名（opus → sonnet → haiku → opus）
            // macOS 默认 Alt 作为字符修饰键，Alt+M 发送 'µ' 且不带 ALT 修饰符
            if matches!(key_event.code, KeyCode::Char('µ'))
                || (key_event.modifiers.contains(KeyModifiers::ALT)
                    && matches!(key_event.code, KeyCode::Char('m')))
            {
                if let Some(cfg) = app.services.peri_config.as_mut() {
                    let aliases = ["opus", "sonnet", "haiku"];
                    let current = cfg.config.active_alias.as_str();
                    let idx = aliases.iter().position(|&a| a == current).unwrap_or(0);
                    let next = aliases[(idx + 1) % aliases.len()];
                    cfg.config.active_alias = next.to_string();
                    if let Err(e) =
                        App::save_config(cfg, app.services.config_path_override.as_deref())
                    {
                        app.session_mgr.sessions[app.session_mgr.active]
                            .messages
                            .view_messages
                            .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
                    }
                    if let Some(p) = crate::app::agent::LlmProvider::from_config(cfg) {
                        app.services.provider_name = p.display_name().to_string();
                        app.services.model_name = p.model_name().to_string();
                    }
                    app.global_ui.model_highlight_until =
                        Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
                }
                return Ok(Some(Action::Redraw));
            }

            let input = Input::from(ev);

            // Setup 向导：优先拦截所有按键事件
            if app.global_ui.setup_wizard.is_some() {
                // Ctrl+C：退出流程（与正常模式一致）
                if matches!(
                    input,
                    Input {
                        key: Key::Char('c'),
                        ctrl: true,
                        ..
                    }
                ) {
                    if let Some(since) = app.global_ui.quit_pending_since {
                        if since.elapsed() < std::time::Duration::from_secs(2) {
                            return Ok(Some(Action::Quit));
                        } else {
                            app.global_ui.quit_pending_since = Some(std::time::Instant::now());
                        }
                    } else {
                        app.global_ui.quit_pending_since = Some(std::time::Instant::now());
                    }
                    return Ok(Some(Action::Redraw));
                }
                let input_clone = input.clone();
                if let Some(ref mut wizard) = app.global_ui.setup_wizard {
                    if let Some(action) =
                        crate::app::setup_wizard::handle_setup_wizard_key(wizard, input_clone)
                    {
                        match action {
                            crate::app::setup_wizard::SetupWizardAction::SaveAndClose => {
                                let wizard = app
                                    .global_ui
                                    .setup_wizard
                                    .take()
                                    .expect("setup_wizard must be Some (checked above)");
                                match crate::app::setup_wizard::save_setup(&wizard) {
                                    Ok(cfg) => app.refresh_after_setup(cfg),
                                    Err(e) => {
                                        let msg = MessageViewModel::from_base_message(
                                            &BaseMessage::system(format!("配置保存失败: {}", e)),
                                            &[],
                                        );
                                        app.session_mgr.sessions[app.session_mgr.active]
                                            .messages
                                            .view_messages
                                            .push(msg);
                                        app.render_rebuild();
                                    }
                                }
                            }
                            crate::app::setup_wizard::SetupWizardAction::Skip => {
                                let from_command = app
                                    .global_ui
                                    .setup_wizard
                                    .as_ref()
                                    .map(|w| w.from_command)
                                    .unwrap_or(false);
                                app.global_ui.setup_wizard = None;
                                if !from_command {
                                    return Ok(Some(Action::Quit));
                                }
                            }
                            crate::app::setup_wizard::SetupWizardAction::SetLanguage(lang) => {
                                let _ = app.services.lc.switch(&lang);
                            }
                            crate::app::setup_wizard::SetupWizardAction::Redraw => {}
                        }
                    }
                }
                return Ok(Some(Action::Redraw));
            }

            // ─── PanelManager 分发 ─────────────────────────────────────────────
            {
                // Session 面板：Model, Agent, Hooks, Login, Config, ThreadBrowser
                let session_kind = app.session_mgr.sessions[app.session_mgr.active]
                    .session_panels
                    .active_kind();
                if matches!(
                    session_kind,
                    Some(PanelKind::Model)
                        | Some(PanelKind::Agent)
                        | Some(PanelKind::Hooks)
                        | Some(PanelKind::Login)
                        | Some(PanelKind::Config)
                        | Some(PanelKind::ThreadBrowser)
                ) {
                    with_session_panels!(app, |sp, ctx| {
                        let result = sp.dispatch_key(input, &mut ctx);
                        let active_idx = app.session_mgr.active;
                        match result {
                            EventResult::ClosePanel => {
                                sp.close();
                                app.session_mgr.sessions[active_idx]
                                    .ui
                                    .panel_selection
                                    .clear();
                                app.session_mgr.sessions[active_idx].ui.panel_area = None;
                            }
                            EventResult::OpenThread(thread_id) => {
                                sp.close();
                                app.session_mgr.sessions[active_idx]
                                    .ui
                                    .panel_selection
                                    .clear();
                                app.session_mgr.sessions[active_idx].ui.panel_area = None;
                                // with_session_panels! 宏会在闭包结束时自动放回，
                                // 但 OpenThread 路径需要提前放回再调用 open_thread_with_feedback
                                app.session_mgr.sessions[active_idx].session_panels = sp;
                                // 提前 return，阻止宏再次放回（宏的 result 语句在 return 后不执行）
                                app.open_thread_with_feedback(thread_id);
                                return Ok(Some(Action::Redraw));
                            }
                            _ => {}
                        }
                        result
                    });
                    return Ok(Some(Action::Redraw));
                }

                // Global 面板：Status, Memory, Mcp, Cron, Plugin
                let global_kind = app.global_panels.active_kind();
                if matches!(
                    global_kind,
                    Some(PanelKind::Status)
                        | Some(PanelKind::Memory)
                        | Some(PanelKind::Mcp)
                        | Some(PanelKind::Cron)
                        | Some(PanelKind::Plugin)
                ) {
                    let active_idx = app.session_mgr.active;
                    with_global_panels!(app, |pm, ctx| {
                        let result = pm.dispatch_key(input, &mut ctx);
                        match result {
                            EventResult::ClosePanel => {
                                pm.close();
                                app.session_mgr.sessions[active_idx]
                                    .ui
                                    .panel_selection
                                    .clear();
                                app.session_mgr.sessions[active_idx].ui.panel_area = None;
                            }
                            EventResult::OpenPanel(PanelKind::Memory) => {
                                app.global_panels = pm;
                                if let Err(e) = app.memory_panel_open_editor() {
                                    tracing::error!("Failed to open editor: {}", e);
                                }
                                return Ok(Some(Action::Redraw));
                            }
                            _ => {}
                        }
                        result
                    });
                    return Ok(Some(Action::Redraw));
                }
            }

            // OAuth 弹窗优先处理
            if app.global_ui.oauth_prompt.is_some() {
                handle_oauth_prompt(app, input);
                return Ok(Some(Action::Redraw));
            }

            // AskUser 批量弹窗
            if matches!(
                &app.session_mgr.sessions[app.session_mgr.active]
                    .agent
                    .interaction_prompt,
                Some(crate::app::InteractionPrompt::Questions(_))
            ) {
                match input {
                    Input {
                        key: Key::Char('c'),
                        ctrl: true,
                        ..
                    } => return Ok(Some(Action::Quit)),
                    // Tab / Shift+Tab 切换问题
                    Input {
                        key: Key::Tab,
                        shift: false,
                        ..
                    } => app.ask_user_next_tab(),
                    Input {
                        key: Key::Tab,
                        shift: true,
                        ..
                    } => app.ask_user_prev_tab(),
                    // Enter 提交所有答案
                    Input {
                        key: Key::Enter, ..
                    } => app.ask_user_confirm(),
                    // 上下移动当前问题内的选项光标
                    Input { key: Key::Up, .. } => app.ask_user_move(-1),
                    Input { key: Key::Down, .. } => app.ask_user_move(1),
                    // Space 切换选中
                    Input {
                        key: Key::Char(' '),
                        ..
                    } => app.ask_user_toggle(),
                    // 文字输入（自定义输入模式下）— 使用公共编辑函数
                    _ => {
                        app.ask_user_edit_key(input);
                    }
                }
                return Ok(Some(Action::Redraw));
            }

            // HITL 批量弹窗激活时，优先处理弹窗按键
            if matches!(
                &app.session_mgr.sessions[app.session_mgr.active]
                    .agent
                    .interaction_prompt,
                Some(crate::app::InteractionPrompt::Approval(_))
            ) {
                match input {
                    Input {
                        key: Key::Char('c'),
                        ctrl: true,
                        ..
                    } => return Ok(Some(Action::Quit)),

                    // 上下移动光标
                    Input { key: Key::Up, .. } => app.hitl_move(-1),
                    Input { key: Key::Down, .. } => app.hitl_move(1),

                    // Space：切换当前项
                    Input {
                        key: Key::Char(' '),
                        ..
                    } => app.hitl_toggle(),

                    // Enter：按当前各项选择确认
                    Input {
                        key: Key::Enter, ..
                    } => app.hitl_confirm(),

                    // Esc：全部拒绝
                    Input { key: Key::Esc, .. } => app.hitl_reject_all(),

                    _ => {}
                }
                return Ok(Some(Action::Redraw));
            }

            match input {
                // Ctrl+C：中断 agent / 双击退出
                Input {
                    key: Key::Char('c'),
                    ctrl: true,
                    ..
                } => {
                    if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
                        // agent 运行中：优先中断，清除退出待确认状态
                        app.interrupt();
                        app.global_ui.quit_pending_since = None;
                    } else if let Some(since) = app.global_ui.quit_pending_since {
                        // 非 loading，2 秒内再次 Ctrl+C → 退出
                        if since.elapsed() < std::time::Duration::from_secs(2) {
                            return Ok(Some(Action::Quit));
                        } else {
                            // 超时，重新开始计时
                            app.global_ui.quit_pending_since = Some(std::time::Instant::now());
                        }
                    } else {
                        // 第一次 Ctrl+C，进入退出待确认状态
                        app.global_ui.quit_pending_since = Some(std::time::Instant::now());
                    }
                }

                // ESC：主界面不再退出，仅用于 loading 时清除缓冲
                Input { key: Key::Esc, .. }
                    if app.session_mgr.sessions[app.session_mgr.active].ui.loading =>
                {
                    if !app.session_mgr.sessions[app.session_mgr.active]
                        .messages
                        .pending_messages
                        .is_empty()
                    {
                        app.session_mgr.sessions[app.session_mgr.active]
                            .messages
                            .pending_messages
                            .clear();
                    }
                }

                // Up：浮层导航 > 历史恢复（仅首行）> textarea 光标
                Input { key: Key::Up, .. }
                    if !app.session_mgr.sessions[app.session_mgr.active].ui.loading =>
                {
                    let hint_count = app.hint_candidates_count();
                    if hint_count > 0 {
                        let cur = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor
                            .unwrap_or(0);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor = if cur == 0 {
                            Some(hint_count - 1)
                        } else {
                            Some(cur - 1)
                        };
                    } else {
                        let (row, _col) = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .cursor();
                        if row == 0 {
                            app.history_up();
                        } else {
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .textarea
                                .input(Input {
                                    key: Key::Up,
                                    ctrl: false,
                                    alt: false,
                                    shift: false,
                                });
                        }
                    }
                }

                // Down：浮层导航 > 历史恢复（仅末行）> textarea 光标
                Input { key: Key::Down, .. }
                    if !app.session_mgr.sessions[app.session_mgr.active].ui.loading =>
                {
                    let hint_count = app.hint_candidates_count();
                    if hint_count > 0 {
                        let cur = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor
                            .unwrap_or(hint_count - 1);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor = if cur + 1 >= hint_count {
                            Some(0)
                        } else {
                            Some(cur + 1)
                        };
                    } else if app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .history_index
                        .is_some()
                    {
                        app.history_down();
                    } else {
                        let (row, _col) = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .cursor();
                        let last_row = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .lines()
                            .len()
                            .saturating_sub(1);
                        if row >= last_row {
                            app.history_down();
                        } else {
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .textarea
                                .input(Input {
                                    key: Key::Down,
                                    ctrl: false,
                                    alt: false,
                                    shift: false,
                                });
                        }
                    }
                }

                // Ctrl+V：优先尝试粘贴剪贴板图片，失败则回退到粘贴文字
                Input {
                    key: Key::Char('v'),
                    ctrl: true,
                    ..
                } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading => {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        if let Ok(img) = clipboard.get_image() {
                            let (w, h) = (img.width as u32, img.height as u32);
                            if let Ok((b64, sz)) = rgba_to_png_base64(w, h, &img.bytes) {
                                let n = app.session_mgr.sessions[app.session_mgr.active]
                                    .metadata
                                    .pending_attachments
                                    .len()
                                    + 1;
                                app.add_pending_attachment(PendingAttachment {
                                    label: format!("clipboard_{}.png", n),
                                    media_type: "image/png".to_string(),
                                    base64_data: b64,
                                    size_bytes: sz,
                                });
                            }
                        } else if let Ok(text) = clipboard.get_text() {
                            let text = text.replace('\r', "\n");
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .textarea
                                .insert_str(&text);
                        }
                    }
                }

                // Tab：提示浮层候选导航与补全
                Input {
                    key: Key::Tab,
                    shift: false,
                    ..
                } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading => {
                    let count = app.hint_candidates_count();
                    if count > 0 {
                        match app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor
                        {
                            Some(cur) if cur + 1 < count => {
                                app.session_mgr.sessions[app.session_mgr.active]
                                    .ui
                                    .hint_cursor = Some(cur + 1);
                            }
                            Some(_) => {
                                // 已在最后一个，循环到第一个
                                app.session_mgr.sessions[app.session_mgr.active]
                                    .ui
                                    .hint_cursor = Some(0);
                            }
                            None => {
                                // 首次按 Tab，选中第一个
                                app.session_mgr.sessions[app.session_mgr.active]
                                    .ui
                                    .hint_cursor = Some(0);
                            }
                        }
                    }
                }

                // Enter 在有候选项时：确认选中（无选中则默认第一项）
                Input {
                    key: Key::Enter, ..
                } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading
                    && app.hint_candidates_count() > 0 =>
                {
                    if app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .hint_cursor
                        .is_none()
                    {
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor = Some(0);
                    }
                    app.hint_complete();
                }

                // Alt+Enter：插入换行
                Input {
                    key: Key::Enter,
                    alt: true,
                    ..
                } => {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea
                        .input(Input {
                            key: Key::Enter,
                            ctrl: false,
                            alt: false,
                            shift: false,
                        });
                }

                // Enter：提交（非 loading）或缓冲（loading）
                Input {
                    key: Key::Enter, ..
                } => {
                    let text = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea
                        .lines()
                        .join("\n");
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        if app.session_mgr.sessions[app.session_mgr.active].ui.loading {
                            // Loading 状态：缓冲消息
                            app.session_mgr.sessions[app.session_mgr.active]
                                .messages
                                .pending_messages
                                .push(text);
                            app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
                                crate::app::build_textarea(false);
                            app.update_textarea_hint();
                        } else if text.starts_with('/') {
                            app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
                                crate::app::build_textarea(false);
                            // SAFETY: command_registry 嵌套在 App 内，dispatch 需 &mut App
                            // NOTE: 必须在 take 前保存 session index，因为 dispatch 内部
                            // （如 /split）可能改变 app.session_mgr.active
                            let session_idx = app.session_mgr.active;
                            let registry = std::mem::take(
                                &mut app.session_mgr.sessions[session_idx]
                                    .commands
                                    .command_registry,
                            );
                            let known = registry.dispatch(app, &text);
                            app.session_mgr.sessions[session_idx]
                                .commands
                                .command_registry = registry;
                            if known {
                                // 命令命中，结束
                            } else {
                                // 命令未命中，尝试 Skill 匹配
                                let skill_name: String = text
                                    .trim_start_matches('/')
                                    .chars()
                                    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                                    .collect();
                                if let Some(_skill) = app.session_mgr.sessions
                                    [app.session_mgr.active]
                                    .commands
                                    .skills
                                    .iter()
                                    .find(|s| s.name == skill_name)
                                {
                                    // Skill 命中：将整条消息提交给 agent
                                    return Ok(Some(Action::Submit(text)));
                                } else {
                                    // 区分"前缀歧义"和"完全未知"
                                    let prefix = text.trim_start_matches('/').to_string();
                                    let cmd_matches = app.session_mgr.sessions
                                        [app.session_mgr.active]
                                        .commands
                                        .command_registry
                                        .match_prefix(&prefix, &app.services.lc);
                                    let error_msg = if cmd_matches.len() > 1 {
                                        let names: Vec<&str> =
                                            cmd_matches.iter().map(|(n, _)| n.as_str()).collect();
                                        format!(
                                            "命令 '{}' 匹配多个: {}  （请输入完整命令名）",
                                            text,
                                            names
                                                .iter()
                                                .map(|n| format!("/{}", n))
                                                .collect::<Vec<_>>()
                                                .join(", ")
                                        )
                                    } else {
                                        format!(
                                            "未知命令或 Skill: {}  （输入 /help 查看可用命令）",
                                            text
                                        )
                                    };
                                    app.session_mgr.sessions[app.session_mgr.active]
                                        .messages
                                        .view_messages
                                        .push(MessageViewModel::system(error_msg));
                                }
                            }
                        } else {
                            app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
                                crate::app::build_textarea(false);
                            return Ok(Some(Action::Submit(text)));
                        }
                    }
                }

                // PageUp/PageDown：VS Code 终端中 Option+Backspace 被映射为 PageUp
                // 检测 VS Code 终端环境，当 textarea 有内容时执行删除单词操作
                Input {
                    key: Key::PageUp, ..
                } if std::env::var("TERM_PROGRAM").as_deref() == Ok("vscode") => {
                    let session = &mut app.session_mgr.sessions[app.session_mgr.active];
                    let has_content = session
                        .ui
                        .textarea
                        .lines()
                        .iter()
                        .any(|line| !line.is_empty());
                    if has_content {
                        session.ui.textarea.delete_word();
                    } else {
                        for _ in 0..10 {
                            app.scroll_up();
                        }
                    }
                }
                Input {
                    key: Key::PageDown, ..
                } => {
                    for _ in 0..10 {
                        app.scroll_down();
                    }
                }

                // Ctrl+U / Ctrl+D：半页滚动（无需 PageUp/PageDown 物理键，MacBook 友好）
                Input {
                    key: Key::Char('u'),
                    ctrl: true,
                    ..
                } => {
                    for _ in 0..20 {
                        app.scroll_up();
                    }
                }
                Input {
                    key: Key::Char('d'),
                    ctrl: true,
                    ..
                } => {
                    for _ in 0..20 {
                        app.scroll_down();
                    }
                }

                // Del：删除最后一个待发送附件（有附件时优先消费 Del）
                Input {
                    key: Key::Delete, ..
                } if !app.session_mgr.sessions[app.session_mgr.active].ui.loading
                    && !app.session_mgr.sessions[app.session_mgr.active]
                        .metadata
                        .pending_attachments
                        .is_empty() =>
                {
                    app.pop_pending_attachment();
                }

                // Ctrl+N/P：切换 session 焦点
                Input {
                    key: Key::Char('n'),
                    ctrl: true,
                    ..
                } => {
                    app.switch_next_session();
                }
                Input {
                    key: Key::Char('p'),
                    ctrl: true,
                    ..
                } => {
                    app.switch_prev_session();
                }

                // Ctrl+W：关闭当前 session
                input @ Input {
                    key: Key::Char('w'),
                    ctrl: true,
                    ..
                } => {
                    if app.close_session().is_some() {
                        // session 已关闭，不继续处理
                    } else {
                        // 只有一个 session，fallback 到 textarea
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .input(input);
                    }
                }

                // 拦截普通 Enter，避免 textarea 默认换行；允许 loading 时输入
                input if input.key != Key::Enter => {
                    // 退出历史浏览
                    if app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .history_index
                        .is_some()
                    {
                        app.exit_history();
                    }
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea
                        .input(input);
                    // 输入内容变化时：重置光标（不预选，等用户按 Tab/上下键激活）
                    if !app.session_mgr.sessions[app.session_mgr.active].ui.loading {
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .hint_cursor = None;
                    }
                }

                _ => {
                    // 任何其他按键取消退出待确认状态
                    app.global_ui.quit_pending_since = None;
                }
            }
        }
        Event::Paste(text) => {
            // 粘贴文本处理
            // 某些终端（如 VSCode）在 bracketed paste 中使用 \r 而非 \n 作为换行符
            let text = text.replace('\r', "\n");

            // setup_wizard 打开时粘贴到当前字段
            if let Some(wizard) = &mut app.global_ui.setup_wizard {
                wizard.paste_text(&text);
                return Ok(Some(Action::Redraw));
            }

            // ─── PanelManager 粘贴分发（已迁移的面板）────────────────
            {
                // Session 面板：Model, Agent, Hooks, Login, Config, ThreadBrowser
                let session_kind = app.session_mgr.sessions[app.session_mgr.active]
                    .session_panels
                    .active_kind();
                if matches!(
                    session_kind,
                    Some(PanelKind::Model)
                        | Some(PanelKind::Agent)
                        | Some(PanelKind::Hooks)
                        | Some(PanelKind::Login)
                        | Some(PanelKind::Config)
                        | Some(PanelKind::ThreadBrowser)
                ) {
                    with_session_panels!(app, |sp, ctx| sp.dispatch_paste(&text, &mut ctx));
                    return Ok(Some(Action::Redraw));
                }

                // Global 面板：Status, Memory, Mcp, Cron, Plugin
                let global_kind = app.global_panels.active_kind();
                if matches!(
                    global_kind,
                    Some(PanelKind::Status)
                        | Some(PanelKind::Memory)
                        | Some(PanelKind::Mcp)
                        | Some(PanelKind::Cron)
                        | Some(PanelKind::Plugin)
                ) {
                    with_global_panels!(app, |pm, ctx| pm.dispatch_paste(&text, &mut ctx));
                    return Ok(Some(Action::Redraw));
                }
            }

            // 其他情况粘贴到 textarea
            app.session_mgr.sessions[app.session_mgr.active]
                .ui
                .textarea
                .insert_str(&text);
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => {
                let panel_area = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_area;
                if let Some(area) = panel_area {
                    if mouse_in_rect(&mouse, area) {
                        // Session 面板优先
                        let sp = &app.session_mgr.sessions[app.session_mgr.active].session_panels;
                        if sp.is_any_open() {
                            let result = with_session_panels!(app, |sp, ctx| {
                                sp.dispatch_scroll(-3, &mut ctx)
                            });
                            if result == EventResult::Consumed {
                                return Ok(Some(Action::Redraw));
                            }
                        }
                        // Global 面板
                        if app.global_panels.is_any_open() {
                            let result = with_global_panels!(app, |pm, ctx| {
                                pm.dispatch_scroll(-3, &mut ctx)
                            });
                            if result == EventResult::Consumed {
                                return Ok(Some(Action::Redraw));
                            }
                        }
                    }
                }
                app.scroll_up();
            }
            MouseEventKind::ScrollDown => {
                let panel_area = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_area;
                if let Some(area) = panel_area {
                    if mouse_in_rect(&mouse, area) {
                        // Session 面板优先
                        let sp = &app.session_mgr.sessions[app.session_mgr.active].session_panels;
                        if sp.is_any_open() {
                            let result = with_session_panels!(app, |sp, ctx| {
                                sp.dispatch_scroll(3, &mut ctx)
                            });
                            if result == EventResult::Consumed {
                                return Ok(Some(Action::Redraw));
                            }
                        }
                        // Global 面板
                        if app.global_panels.is_any_open() {
                            let result = with_global_panels!(app, |pm, ctx| {
                                pm.dispatch_scroll(3, &mut ctx)
                            });
                            if result == EventResult::Consumed {
                                return Ok(Some(Action::Redraw));
                            }
                        }
                    }
                }
                app.scroll_down();
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // 面板区域：尝试分发鼠标点击
                let panel_area = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_area;
                let mut click_consumed = false;
                if let Some(area) = panel_area {
                    if mouse_in_rect(&mouse, area) {
                        // Session 面板
                        {
                            let sp =
                                &app.session_mgr.sessions[app.session_mgr.active].session_panels;
                            if sp.is_any_open() {
                                let result = with_session_panels!(app, |sp, ctx| {
                                    sp.dispatch_mouse(mouse, area, &mut ctx)
                                });
                                if result == EventResult::Consumed {
                                    click_consumed = true;
                                }
                            }
                        }
                        // Global 面板
                        if !click_consumed && app.global_panels.is_any_open() {
                            let result = with_global_panels!(app, |pm, ctx| {
                                pm.dispatch_mouse(mouse, area, &mut ctx)
                            });
                            if result == EventResult::Consumed {
                                click_consumed = true;
                            }
                        }
                    }
                }
                if click_consumed {
                    return Ok(Some(Action::Redraw));
                }
                // 多 session：点击非 active session 列区域时切换焦点
                if app.session_mgr.sessions.len() > 1 {
                    for (i, area) in app.session_mgr.session_areas.iter().enumerate() {
                        if mouse.column >= area.x
                            && mouse.column < area.x + area.width
                            && mouse.row >= area.y
                            && mouse.row < area.y + area.height
                            && i != app.session_mgr.active
                        {
                            app.session_mgr.active = i;
                            return Ok(Some(Action::Redraw));
                        }
                    }
                }
                // 面板区域：开始面板选区
                let panel_area = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_area;
                if let Some(area) = panel_area {
                    if mouse_in_rect(&mouse, area) {
                        let content_row = mouse.row - area.y
                            + app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .panel_scroll_offset;
                        let col = mouse.column - area.x;
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .panel_selection
                            .start_drag(content_row, col);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .text_selection
                            .clear();
                        // 不再处理其他区域的选区
                        return Ok(Some(Action::Redraw));
                    }
                }
                if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .messages_area
                {
                    let scroll_offset = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .scroll_offset;
                    let scroll_follow = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .scroll_follow;

                    // 滚动到底按钮：右下角点击且用户已滚离底部
                    let btn_col_start = area.right().saturating_sub(2);
                    let btn_row_start = area.bottom().saturating_sub(2);
                    if !scroll_follow
                        && mouse.column >= btn_col_start
                        && mouse.column < area.right()
                        && mouse.row >= btn_row_start
                        && mouse.row < area.bottom()
                    {
                        app.scroll_to_bottom();
                        return Ok(Some(Action::Redraw));
                    }

                    // 滚动到顶按钮：右上角点击且用户已滚离顶部
                    if scroll_offset > 0
                        && mouse.column >= btn_col_start
                        && mouse.column < area.right()
                        && mouse.row >= area.y
                        && mouse.row < area.y.saturating_add(2)
                    {
                        app.scroll_to_top();
                        return Ok(Some(Action::Redraw));
                    }

                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let visual_row = mouse.row - area.y
                            + app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .scroll_offset;
                        let visual_col = mouse.column - area.x;
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .text_selection
                            .start_drag(visual_row, visual_col);
                    }
                }
                // 输入框区域：开始 textarea 选区
                if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea_area
                {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let session = &app.session_mgr.sessions[app.session_mgr.active];
                        let (row, col) =
                            textarea_mouse_to_cursor(&session.ui.textarea, area, &mouse);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .move_cursor(tui_textarea::CursorMove::Jump(row as u16, col as u16));
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .textarea
                            .start_selection();
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // 面板选区拖拽
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_selection
                    .dragging
                {
                    if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .panel_area
                    {
                        let content_row = mouse.row.saturating_sub(area.y).saturating_add(
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .panel_scroll_offset,
                        );
                        let col = mouse.column.saturating_sub(area.x);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .panel_selection
                            .update_drag(content_row, col);
                    }
                }
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .text_selection
                    .dragging
                {
                    if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .messages_area
                    {
                        let visual_row = mouse.row.saturating_sub(area.y).saturating_add(
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .scroll_offset,
                        );
                        let visual_col = mouse.column.saturating_sub(area.x);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .text_selection
                            .update_drag(visual_row, visual_col);
                    }
                }
                // 输入框区域：扩展 textarea 选区
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .textarea
                    .is_selecting()
                {
                    if let Some(area) = app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .textarea_area
                    {
                        if mouse.row >= area.y && mouse.row < area.y + area.height {
                            let session = &app.session_mgr.sessions[app.session_mgr.active];
                            let (row, col) =
                                textarea_mouse_to_cursor(&session.ui.textarea, area, &mouse);
                            app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .textarea
                                .move_cursor(tui_textarea::CursorMove::Jump(
                                    row as u16, col as u16,
                                ));
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // 面板选区松开
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .panel_selection
                    .dragging
                {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .panel_selection
                        .end_drag();
                    let sel = &app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .panel_selection;
                    if let (Some(start), Some(end)) = (sel.start, sel.end) {
                        let text = crate::app::text_selection::extract_panel_text(
                            start,
                            end,
                            &app.session_mgr.sessions[app.session_mgr.active]
                                .ui
                                .panel_plain_lines,
                        );
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .panel_selection
                            .set_selected_text(text);
                    }
                    copy_panel_selection_to_clipboard(app);
                }
                if app.session_mgr.sessions[app.session_mgr.active]
                    .ui
                    .text_selection
                    .dragging
                {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .text_selection
                        .end_drag();
                    let ts = &app.session_mgr.sessions[app.session_mgr.active]
                        .ui
                        .text_selection;
                    if let (Some(start), Some(end)) = (ts.start, ts.end) {
                        let usable_width = app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .messages_area
                            .map(|a| a.width.saturating_sub(1))
                            .unwrap_or(0);
                        let cache = app.session_mgr.sessions[app.session_mgr.active]
                            .messages
                            .render_cache
                            .read();
                        let text = crate::app::text_selection::extract_selected_text(
                            start,
                            end,
                            &cache.wrap_map,
                            usable_width,
                        );
                        drop(cache);
                        app.session_mgr.sessions[app.session_mgr.active]
                            .ui
                            .text_selection
                            .set_selected_text(text);
                    }
                    copy_selection_to_clipboard(app);
                }
                // textarea 选区在 mouse up 时不做额外处理，保持 tui_textarea 的选区状态
            }
            _ => {}
        },
    }

    Ok(Some(Action::Redraw))
}

fn handle_oauth_prompt(app: &mut App, input: Input) {
    use crate::app::handle_edit_key;
    let prompt = match app.global_ui.oauth_prompt.as_mut() {
        Some(p) => p,
        None => return,
    };
    match input {
        Input {
            key: Key::Enter, ..
        } => {
            if prompt.submit() {
                app.global_ui.oauth_prompt = None;
            }
        }
        Input {
            key: Key::Char('o'),
            ctrl: true,
            ..
        } => {
            let url = prompt.authorization_url.clone();
            #[cfg(unix)]
            let _ = std::process::Command::new("open").arg(&url).spawn();
            #[cfg(windows)]
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", &url])
                .spawn();
        }
        Input { key: Key::Esc, .. } => {
            app.global_ui.oauth_prompt = None;
        }
        Input {
            key: Key::Char('c'),
            ctrl: true,
            ..
        } => {
            // Ctrl+C 在弹窗中不退出，忽略
        }
        _ => {
            prompt.error_message = None;
            handle_edit_key(&mut prompt.input, &mut prompt.cursor, input);
        }
    }
}
