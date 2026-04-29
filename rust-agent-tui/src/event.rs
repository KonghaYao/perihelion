use anyhow::Result;
use base64::Engine as _;
use ratatui::crossterm::event::{self, Event, KeyEventKind, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use tui_textarea::{Input, Key};
use std::time::Duration;

use crate::app::model_panel::{AliasTab, ROW_HAIKU, ROW_LOGIN, ROW_OPUS, ROW_SONNET, ROW_THINKING};
use crate::app::{App, MessageViewModel, PendingAttachment};
use rust_create_agent::messages::BaseMessage;
use crate::ui::render_thread::RenderEvent;

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
    if let Some(text) = app.core.text_selection.selected_text.take() {
        let char_count = text.chars().count();
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(&text);
        }
        app.core.copy_char_count = char_count;
        app.core.copy_message_until = Some(
            std::time::Instant::now() + std::time::Duration::from_millis(2000)
        );
        app.core.text_selection.clear();
        return true;
    }
    false
}

/// 将面板选区文本复制到系统剪贴板。返回 true 表示成功复制。
fn copy_panel_selection_to_clipboard(app: &mut App) -> bool {
    if let Some(text) = app.core.panel_selection.selected_text.take() {
        let char_count = text.chars().count();
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(&text);
        }
        app.core.copy_char_count = char_count;
        app.core.copy_message_until = Some(
            std::time::Instant::now() + std::time::Duration::from_millis(2000)
        );
        app.core.panel_selection.clear();
        return true;
    }
    false
}

pub async fn next_event(app: &mut App) -> Result<Option<Action>> {
    if !event::poll(Duration::from_millis(50))? {
        return Ok(None);
    }

    let ev = event::read()?;

    match ev {
        Event::Resize(w, _) => {
            let _ = app.core.render_tx.send(RenderEvent::Resize(w));
            app.core.text_selection.clear();
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
                let _new_mode = app.permission_mode.cycle();
                app.mode_highlight_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
                return Ok(Some(Action::Redraw));
            }

            // macOS: Cmd+C (Super+C) 复制选区文本（遵循系统剪贴板快捷键惯例）
            // tui_textarea::Input 没有 super 字段，需在转换前从原始 key_event 检测。
            if key_event.code == KeyCode::Char('c')
                && key_event.modifiers.contains(KeyModifiers::SUPER)
            {
                if copy_selection_to_clipboard(app) {
                    return Ok(Some(Action::Redraw));
                }
            }

            // 全局复制：有选区时 Ctrl+C 优先复制，不被任何面板拦截
            if key_event.code == KeyCode::Char('c')
                && key_event.modifiers.contains(KeyModifiers::CONTROL)
                && !key_event.modifiers.contains(KeyModifiers::SHIFT)
            {
                // 优先级：消息区选区 > 面板选区 > textarea 选区
                if copy_selection_to_clipboard(app) {
                    return Ok(Some(Action::Redraw));
                }
                if copy_panel_selection_to_clipboard(app) {
                    return Ok(Some(Action::Redraw));
                }
                if app.core.textarea.is_selecting() {
                    app.core.textarea.copy();
                    let text = app.core.textarea.yank_text();
                    if !text.is_empty() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(&text);
                        }
                        let char_count = text.chars().count();
                        app.core.copy_char_count = char_count;
                        app.core.copy_message_until = Some(
                            std::time::Instant::now() + std::time::Duration::from_millis(2000)
                        );
                        app.core.textarea.cancel_selection();
                        return Ok(Some(Action::Redraw));
                    }
                }
            }

            let input = Input::from(ev);

            // Setup 向导：优先拦截所有按键事件
            if app.setup_wizard.is_some() {
                let input_clone = input.clone();
                if let Some(ref mut wizard) = app.setup_wizard {
                    if let Some(action) = crate::app::setup_wizard::handle_setup_wizard_key(wizard, input_clone) {
                        match action {
                            crate::app::setup_wizard::SetupWizardAction::SaveAndClose => {
                                let wizard = app.setup_wizard.take().unwrap();
                                match crate::app::setup_wizard::save_setup(&wizard) {
                                    Ok(cfg) => app.refresh_after_setup(cfg),
                                    Err(e) => {
                                        let msg = MessageViewModel::from_base_message(
                                            &BaseMessage::system(format!("Setup save failed: {}", e)),
                                            &[],
                                        );
                                        let _ = app.core.render_tx.send(RenderEvent::AddMessage(msg));
                                    }
                                }
                            }
                            crate::app::setup_wizard::SetupWizardAction::Skip => {
                                app.setup_wizard = None;
                            }
                            crate::app::setup_wizard::SetupWizardAction::Redraw => {}
                        }
                    }
                }
                return Ok(Some(Action::Redraw));
            }

            // Thread 浏览面板优先处理
            if app.core.thread_browser.is_some() {
                handle_thread_browser(app, input);
                return Ok(Some(Action::Redraw));
            }

            // CronPanel 优先处理
            if app.cron.cron_panel.is_some() {
                handle_cron_panel(app, input);
                return Ok(Some(Action::Redraw));
            }

            // /agents 面板优先处理
            if app.core.agent_panel.is_some() {
                handle_agent_panel(app, input);
                return Ok(Some(Action::Redraw));
            }

            // /login 面板优先处理
            if app.core.login_panel.is_some() {
                handle_login_panel(app, input);
                return Ok(Some(Action::Redraw));
            }

            // /model 面板优先处理
            if app.core.model_panel.is_some() {
                handle_model_panel(app, input);
                return Ok(Some(Action::Redraw));
            }

            // AskUser 批量弹窗
            if matches!(&app.agent.interaction_prompt, Some(crate::app::InteractionPrompt::Questions(_))) {
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
                    // 文字输入（自定义输入模式下）
                    Input {
                        key: Key::Backspace,
                        ..
                    } => app.ask_user_pop_char(),
                    Input {
                        key: Key::Char(c),
                        ctrl: false,
                        alt: false,
                        ..
                    } => {
                        app.ask_user_push_char(c);
                    }
                    _ => {}
                }
                return Ok(Some(Action::Redraw));
            }

            // HITL 批量弹窗激活时，优先处理弹窗按键
            if matches!(&app.agent.interaction_prompt, Some(crate::app::InteractionPrompt::Approval(_))) {
                match input {
                    Input {
                        key: Key::Char('c'),
                        ctrl: true,
                        ..
                    } => return Ok(Some(Action::Quit)),

                    // 上下移动光标
                    Input { key: Key::Up, .. } => app.hitl_move(-1),
                    Input { key: Key::Down, .. } => app.hitl_move(1),

                    // 空格/t：切换当前项
                    Input {
                        key: Key::Char(' '),
                        ..
                    }
                    | Input {
                        key: Key::Char('t'),
                        ..
                    } => app.hitl_toggle(),

                    // y / A：全部批准并确认
                    Input {
                        key: Key::Char('y'),
                        ..
                    } => app.hitl_approve_all(),

                    // n / N：全部拒绝并确认
                    Input {
                        key: Key::Char('n'),
                        ..
                    } => app.hitl_reject_all(),

                    // Enter：按当前各项选择确认
                    Input {
                        key: Key::Enter, ..
                    } => app.hitl_confirm(),

                    _ => {}
                }
                return Ok(Some(Action::Redraw));
            }

            match input {
                // Ctrl+C：有选区时复制优先，无选区时中断/退出
                // Ctrl+C：无选区时中断/退出（有选区的复制已在全局拦截处理）
                Input {
                    key: Key::Char('c'),
                    ctrl: true,
                    ..
                } => {
                    if app.core.loading {
                        app.interrupt();
                    } else {
                        return Ok(Some(Action::Quit));
                    }
                }
                Input { key: Key::Esc, .. } if !app.core.loading => return Ok(Some(Action::Quit)),

                // Up：浮层导航 > 历史恢复（仅首行）> textarea 光标
                Input { key: Key::Up, .. } if !app.core.loading => {
                    let hint_count = app.hint_candidates_count();
                    if app.core.hint_cursor.is_some() && hint_count > 0 {
                        let cur = app.core.hint_cursor.unwrap();
                        app.core.hint_cursor = if cur == 0 {
                            Some(hint_count - 1)
                        } else {
                            Some(cur - 1)
                        };
                    } else {
                        let (row, _col) = app.core.textarea.cursor();
                        if row == 0 {
                            app.history_up();
                        } else {
                            app.core.textarea.input(Input { key: Key::Up, ctrl: false, alt: false, shift: false });
                        }
                    }
                }

                // Down：浮层导航 > 历史恢复（仅末行）> textarea 光标
                Input { key: Key::Down, .. } if !app.core.loading => {
                    let hint_count = app.hint_candidates_count();
                    if app.core.hint_cursor.is_some() && hint_count > 0 {
                        let cur = app.core.hint_cursor.unwrap();
                        app.core.hint_cursor = if cur + 1 >= hint_count {
                            Some(0)
                        } else {
                            Some(cur + 1)
                        };
                    } else if app.core.history_index.is_some() {
                        app.history_down();
                    } else {
                        let (row, _col) = app.core.textarea.cursor();
                        let last_row = app.core.textarea.lines().len().saturating_sub(1);
                        if row >= last_row {
                            app.history_down();
                        } else {
                            app.core.textarea.input(Input { key: Key::Down, ctrl: false, alt: false, shift: false });
                        }
                    }
                }


                // Ctrl+V：优先尝试粘贴剪贴板图片，失败则回退到粘贴文字
                Input { key: Key::Char('v'), ctrl: true, .. } if !app.core.loading => {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        if let Ok(img) = clipboard.get_image() {
                            let (w, h) = (img.width as u32, img.height as u32);
                            if let Ok((b64, sz)) = rgba_to_png_base64(w, h, &img.bytes) {
                                let n = app.core.pending_attachments.len() + 1;
                                app.add_pending_attachment(PendingAttachment {
                                    label: format!("clipboard_{}.png", n),
                                    media_type: "image/png".to_string(),
                                    base64_data: b64,
                                    size_bytes: sz,
                                });
                            }
                        } else if let Ok(text) = clipboard.get_text() {
                            let text = text.replace('\r', "\n");
                            app.core.textarea.insert_str(&text);
                        }
                    }
                }

                // Tab：提示浮层候选导航与补全
                Input {
                    key: Key::Tab,
                    shift: false,
                    ..
                } if !app.core.loading => {
                    let count = app.hint_candidates_count();
                    if count > 0 {
                        match app.core.hint_cursor {
                            Some(cur) if cur + 1 < count => {
                                app.core.hint_cursor = Some(cur + 1);
                            }
                            Some(_) => {
                                // 已在最后一个，循环到第一个
                                app.core.hint_cursor = Some(0);
                            }
                            None => {
                                // 首次按 Tab，选中第一个
                                app.core.hint_cursor = Some(0);
                            }
                        }
                    }
                }

                // Enter 在提示浮层激活时：确认选中
                Input {
                    key: Key::Enter, ..
                } if !app.core.loading && app.core.hint_cursor.is_some() => {
                    app.hint_complete();
                }

                // Alt+Enter：插入换行
                Input {
                    key: Key::Enter,
                    alt: true,
                    ..
                } => {
                    app.core.textarea.input(Input {
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
                    let text = app.core.textarea.lines().join("\n");
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        if app.core.loading {
                            // Loading 状态：缓冲消息
                            app.core.pending_messages.push(text);
                            app.update_textarea_hint();
                        } else if text.starts_with('/') {
                            app.core.textarea = crate::app::build_textarea(false);
                            // 命令模式：取出 registry 避免借用冲突
                            let registry = std::mem::take(&mut app.core.command_registry);
                            let known = registry.dispatch(app, &text);
                            app.core.command_registry = registry;
                            if known {
                                // 命令命中，结束
                            } else {
                                // 命令未命中，尝试 Skill 匹配
                                let skill_name: String = text.trim_start_matches('/')
                                    .chars()
                                    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                                    .collect();
                                if let Some(_skill) = app.core.skills.iter().find(|s| s.name == skill_name) {
                                    // Skill 命中：将整条消息提交给 agent
                                    return Ok(Some(Action::Submit(text)));
                                } else {
                                    // 完全无匹配
                                    app.core.view_messages.push(MessageViewModel::system(format!(
                                        "未知命令或 Skill: {}  （输入 /help 查看可用命令）",
                                        text
                                    )));
                                }
                            }
                        } else {
                            app.core.textarea = crate::app::build_textarea(false);
                            return Ok(Some(Action::Submit(text)));
                        }
                    }
                }

                Input {
                    key: Key::PageUp, ..
                } => {
                    for _ in 0..10 {
                        app.scroll_up();
                    }
                }
                Input {
                    key: Key::PageDown, ..
                } => {
                    for _ in 0..10 {
                        app.scroll_down();
                    }
                }

                // Del：删除最后一个待发送附件（有附件时优先消费 Del）
                Input { key: Key::Delete, .. } if !app.core.loading && !app.core.pending_attachments.is_empty() => {
                    app.pop_pending_attachment();
                }

                // 拦截普通 Enter，避免 textarea 默认换行；允许 loading 时输入
                input if input.key != Key::Enter => {
                    // 退出历史浏览
                    if app.core.history_index.is_some() {
                        app.exit_history();
                    }
                    app.core.textarea.input(input);
                    // 输入内容变化时：有候选自动选中第一个，无候选则清零
                    if !app.core.loading {
                        app.core.hint_cursor = if app.hint_candidates_count() > 0 {
                            Some(0)
                        } else {
                            None
                        };
                    }
                }

                _ => {}
            }
        }
        Event::Paste(text) => {
            // 粘贴文本处理
            // 某些终端（如 VSCode）在 bracketed paste 中使用 \r 而非 \n 作为换行符
            let text = text.replace('\r', "\n");

            // setup_wizard 打开时粘贴到当前字段
            if app.setup_wizard.is_some() {
                let wizard = app.setup_wizard.as_mut().unwrap();
                wizard.paste_text(&text);
                return Ok(Some(Action::Redraw));
            }

            // login_panel 打开时粘贴到面板当前字段
            if app.core.login_panel.is_some() {
                app.core.login_panel.as_mut().unwrap().paste_text(&text);
                return Ok(Some(Action::Redraw));
            }

            // model_panel 打开时粘贴到面板当前字段
            if app.core.model_panel.is_some() {
                app.core.model_panel.as_mut().unwrap().paste_text(&text);
                return Ok(Some(Action::Redraw));
            }

            // 其他情况粘贴到 textarea
            app.core.textarea.insert_str(&text);
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => app.scroll_up(),
            MouseEventKind::ScrollDown => app.scroll_down(),
            MouseEventKind::Down(MouseButton::Left) => {
                // 面板区域：开始面板选区
                if let Some(area) = app.core.panel_area {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let content_row = mouse.row - area.y + app.core.panel_scroll_offset;
                        let col = mouse.column - area.x;
                        app.core.panel_selection.start_drag(content_row, col);
                        app.core.text_selection.clear();
                        // 不再处理其他区域的选区
                        return Ok(Some(Action::Redraw));
                    }
                }
                if let Some(area) = app.core.messages_area {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let visual_row = mouse.row - area.y + app.core.scroll_offset;
                        let visual_col = mouse.column - area.x;
                        app.core.text_selection.start_drag(visual_row, visual_col);
                    }
                }
                // 输入框区域：开始 textarea 选区
                if let Some(area) = app.core.textarea_area {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let row = (mouse.row - area.y).saturating_sub(1) as usize; // 跳过顶部边框
                        let col = mouse.column.saturating_sub(area.x) as usize;
                        app.core.textarea.move_cursor(tui_textarea::CursorMove::Jump(row as u16, col as u16));
                        app.core.textarea.start_selection();
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // 面板选区拖拽
                if app.core.panel_selection.dragging {
                    if let Some(area) = app.core.panel_area {
                        let content_row = mouse.row.saturating_sub(area.y)
                            .saturating_add(app.core.panel_scroll_offset);
                        let col = mouse.column.saturating_sub(area.x);
                        app.core.panel_selection.update_drag(content_row, col);
                    }
                }
                if app.core.text_selection.dragging {
                    if let Some(area) = app.core.messages_area {
                        let visual_row = mouse.row.saturating_sub(area.y)
                            .saturating_add(app.core.scroll_offset);
                        let visual_col = mouse.column.saturating_sub(area.x);
                        app.core.text_selection.update_drag(visual_row, visual_col);
                    }
                }
                // 输入框区域：扩展 textarea 选区
                if app.core.textarea.is_selecting() {
                    if let Some(area) = app.core.textarea_area {
                        if mouse.row >= area.y && mouse.row < area.y + area.height {
                            let row = (mouse.row - area.y).saturating_sub(1) as usize;
                            let col = mouse.column.saturating_sub(area.x) as usize;
                            app.core.textarea.move_cursor(tui_textarea::CursorMove::Jump(row as u16, col as u16));
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // 面板选区松开
                if app.core.panel_selection.dragging {
                    app.core.panel_selection.end_drag();
                    let sel = &app.core.panel_selection;
                    if let (Some(start), Some(end)) = (sel.start, sel.end) {
                        let text = crate::app::text_selection::extract_panel_text(
                            start, end, &app.core.panel_plain_lines,
                        );
                        app.core.panel_selection.set_selected_text(text);
                    }
                }
                if app.core.text_selection.dragging {
                    app.core.text_selection.end_drag();
                    let ts = &app.core.text_selection;
                    if let (Some(start), Some(end)) = (ts.start, ts.end) {
                        let usable_width = app.core.messages_area
                            .map(|a| a.width.saturating_sub(1))
                            .unwrap_or(0);
                        let cache = app.core.render_cache.read();
                        let text = crate::app::text_selection::extract_selected_text(
                            start, end, &cache.wrap_map, usable_width,
                        );
                        drop(cache);
                        app.core.text_selection.set_selected_text(text);
                    }
                }
                // textarea 选区在 mouse up 时不做额外处理，保持 tui_textarea 的选区状态
            }
            _ => {}
        },
        _ => {}
    }

    Ok(Some(Action::Redraw))
}

// ─── Thread 浏览面板键盘处理 ──────────────────────────────────────────────────

fn handle_thread_browser(app: &mut App, input: Input) {
    match input {
        Input {
            key: Key::Char('c'),
            ctrl: true,
            ..
        } => {}
        Input { key: Key::Esc, .. } => {
            // Esc 关闭面板，回到当前对话
            app.core.thread_browser = None;
            app.core.panel_selection.clear();
            app.core.panel_area = None;
        }
        Input { key: Key::Up, .. } => {
            if let Some(b) = app.core.thread_browser.as_mut() {
                b.move_cursor(-1);
                b.scroll_offset = crate::app::ensure_cursor_visible(b.cursor as u16, b.scroll_offset, 10);
            }
        }
        Input { key: Key::Down, .. } => {
            if let Some(b) = app.core.thread_browser.as_mut() {
                b.move_cursor(1);
                b.scroll_offset = crate::app::ensure_cursor_visible(b.cursor as u16, b.scroll_offset, 10);
            }
        }
        Input {
            key: Key::Enter, ..
        } => {
            if let Some(b) = app.core.thread_browser.as_mut() {
                if b.is_new() {
                    app.new_thread();
                } else if let Some(id) = b.selected_id().cloned() {
                    app.open_thread(id);
                }
            }
        }
        Input {
            key: Key::Char('d'),
            ctrl: false,
            alt: false,
            ..
        } => {
            if let Some(b) = app.core.thread_browser.as_mut() {
                b.delete_selected();
            }
        }
        _ => {}
    }
}

// ─── /agents 面板键盘处理 ──────────────────────────────────────────────────────

fn handle_agent_panel(app: &mut App, input: Input) {
    match input {
        Input {
            key: Key::Char('c'),
            ctrl: true,
            ..
        } => {}
        Input { key: Key::Esc, .. } => {
            app.close_agent_panel();
            app.core.panel_selection.clear();
            app.core.panel_area = None;
        }
        Input { key: Key::Up, .. } => {
            app.agent_panel_move_up();
        }
        Input { key: Key::Down, .. } => {
            app.agent_panel_move_down();
        }
        Input {
            key: Key::Enter, ..
        } => {
            // Enter 确认选择当前 agent（或取消选择）
            app.agent_panel_confirm();
        }
        _ => {}
    }
}

// ─── /login 面板键盘处理 ──────────────────────────────────────────────────────

fn handle_login_panel(app: &mut App, input: Input) {
    use crate::app::login_panel::LoginPanelMode;

    let mode = match app.core.login_panel.as_ref() {
        Some(p) => p.mode.clone(),
        None => return,
    };

    match mode {
        LoginPanelMode::Browse => match input {
            Input { key: Key::Esc, .. } => {
                app.close_login_panel();
            }
            Input { key: Key::Up, .. } => {
                app.core.login_panel.as_mut().unwrap().move_cursor(-1);
            }
            Input { key: Key::Down, .. } => {
                app.core.login_panel.as_mut().unwrap().move_cursor(1);
            }
            Input { key: Key::Char('e'), ctrl: false, alt: false, .. }
            | Input { key: Key::Enter, .. } => {
                app.core.login_panel.as_mut().unwrap().enter_edit();
            }
            Input { key: Key::Char('n'), ctrl: false, alt: false, .. } => {
                app.core.login_panel.as_mut().unwrap().enter_new();
            }
            Input { key: Key::Char('d'), ctrl: false, alt: false, .. } => {
                app.core.login_panel.as_mut().unwrap().request_delete();
            }
            Input { key: Key::Char(' '), .. } => {
                app.login_panel_select_provider();
            }
            _ => {}
        },
        LoginPanelMode::Edit | LoginPanelMode::New => match input {
            Input { key: Key::Esc, .. } => {
                app.core.login_panel.as_mut().unwrap().mode = LoginPanelMode::Browse;
            }
            Input { key: Key::Char('v'), ctrl: true, .. } => {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        app.core.login_panel.as_mut().unwrap().paste_text(&text);
                    }
                }
            }
            Input { key: Key::Up, .. } => {
                app.core.login_panel.as_mut().unwrap().field_prev();
            }
            Input { key: Key::Down, .. } => {
                app.core.login_panel.as_mut().unwrap().field_next();
            }
            Input { key: Key::Tab, shift: false, .. } => {
                app.core.login_panel.as_mut().unwrap().field_next();
            }
            Input { key: Key::Tab, shift: true, .. } => {
                app.core.login_panel.as_mut().unwrap().field_prev();
            }
            Input { key: Key::Left, .. } | Input { key: Key::Right, .. } => {
                let field = app.core.login_panel.as_ref().unwrap().edit_field.clone();
                if field == crate::app::login_panel::LoginEditField::Type {
                    app.core.login_panel.as_mut().unwrap().cycle_type();
                }
            }
            Input { key: Key::Char(' '), .. } => {
                let field = app.core.login_panel.as_ref().unwrap().edit_field.clone();
                if field == crate::app::login_panel::LoginEditField::Type {
                    app.core.login_panel.as_mut().unwrap().cycle_type();
                } else {
                    app.core.login_panel.as_mut().unwrap().push_char(' ');
                }
            }
            Input { key: Key::Enter, .. } => {
                app.login_panel_apply_edit();
            }
            Input { key: Key::Backspace, .. } => {
                app.core.login_panel.as_mut().unwrap().pop_char();
            }
            Input { key: Key::Char(c), ctrl: false, alt: false, .. } => {
                app.core.login_panel.as_mut().unwrap().push_char(c);
            }
            _ => {}
        },
        LoginPanelMode::ConfirmDelete => match input {
            Input { key: Key::Enter, .. } => {
                app.login_panel_confirm_delete();
            }
            Input { key: Key::Esc, .. } => {
                app.core.login_panel.as_mut().unwrap().cancel_delete();
            }
            _ => {}
        },
    }
}

// ─── /model 面板键盘处理 ──────────────────────────────────────────────────────

fn handle_model_panel(app: &mut App, input: Input) {
    match input {
        Input { key: Key::Esc, .. } => {
            app.close_model_panel();
        }
        Input { key: Key::Up, .. } => {
            app.core.model_panel.as_mut().unwrap().move_cursor(-1);
        }
        Input { key: Key::Down, .. } => {
            app.core.model_panel.as_mut().unwrap().move_cursor(1);
        }
        Input { key: Key::Char(' '), .. } => {
            app.core.model_panel.as_mut().unwrap().toggle_thinking();
        }
        Input { key: Key::Enter, .. } => {
            let cursor = app.core.model_panel.as_ref().unwrap().cursor;
            match cursor {
                ROW_LOGIN => {
                    app.close_model_panel();
                    app.open_login_panel();
                }
                ROW_THINKING => {
                    app.core.model_panel.as_mut().unwrap().toggle_thinking();
                }
                ROW_OPUS => {
                    app.core.model_panel.as_mut().unwrap().active_tab = AliasTab::Opus;
                    app.model_panel_confirm();
                }
                ROW_SONNET => {
                    app.core.model_panel.as_mut().unwrap().active_tab = AliasTab::Sonnet;
                    app.model_panel_confirm();
                }
                ROW_HAIKU => {
                    app.core.model_panel.as_mut().unwrap().active_tab = AliasTab::Haiku;
                    app.model_panel_confirm();
                }
                _ => {}
            }
        }
        Input { key: Key::Char('v'), ctrl: true, .. } => {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                if let Ok(text) = clipboard.get_text() {
                    app.core.model_panel.as_mut().unwrap().paste_text(&text);
                }
            }
        }
        Input { key: Key::Backspace, .. } => {
            app.core.model_panel.as_mut().unwrap().pop_char();
        }
        Input { key: Key::Char(c), ctrl: false, alt: false, .. } => {
            app.core.model_panel.as_mut().unwrap().push_char(c);
        }
        _ => {}
    }
}

fn handle_cron_panel(app: &mut App, input: Input) {
    match input {
        Input {
            key: Key::Char('c'),
            ctrl: true,
            ..
        } => {
            // Ctrl+C 在面板中不退出，忽略
        }
        Input {
            key: Key::Up, ..
        } => {
            app.cron_panel_move_up();
        }
        Input {
            key: Key::Down, ..
        } => {
            app.cron_panel_move_down();
        }
        Input {
            key: Key::Enter, ..
        } => {
            app.cron_panel_toggle();
        }
        Input {
            key: Key::Esc, ..
        } => {
            app.cron_panel_close();
            app.core.panel_selection.clear();
            app.core.panel_area = None;
        }
        _ => {}
    }
}
