use anyhow::Result;
use base64::Engine as _;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use std::time::Duration;
use tui_textarea::{Input, Key};

use crate::app::model_panel::{AliasTab, ROW_EFFORT, ROW_HAIKU, ROW_OPUS, ROW_SONNET};
use crate::app::{App, MessageViewModel, PendingAttachment};
use crate::ui::render_thread::RenderEvent;
use rust_create_agent::messages::BaseMessage;

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
    if let Some(text) = app.sessions[app.active].core.text_selection.selected_text.take() {
        let char_count = text.chars().count();
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(&text);
        }
        app.sessions[app.active].core.copy_char_count = char_count;
        app.sessions[app.active].core.copy_message_until =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(2000));
        app.sessions[app.active].core.text_selection.clear();
        return true;
    }
    false
}

/// 将面板选区文本复制到系统剪贴板。返回 true 表示成功复制。
fn copy_panel_selection_to_clipboard(app: &mut App) -> bool {
    if let Some(text) = app.sessions[app.active].core.panel_selection.selected_text.take() {
        let char_count = text.chars().count();
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(&text);
        }
        app.sessions[app.active].core.copy_char_count = char_count;
        app.sessions[app.active].core.copy_message_until =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(2000));
        app.sessions[app.active].core.panel_selection.clear();
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
            let _ = app.sessions[app.active].core.render_tx.send(RenderEvent::Resize(w));
            app.sessions[app.active].core.text_selection.clear();
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
                app.mode_highlight_until =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
                return Ok(Some(Action::Redraw));
            }

            // Alt+M 循环切换模型别名（opus → sonnet → haiku → opus）
            // macOS 默认 Alt 作为字符修饰键，Alt+M 发送 'µ' 且不带 ALT 修饰符
            if matches!(key_event.code, KeyCode::Char('µ'))
                || (key_event.modifiers.contains(KeyModifiers::ALT)
                    && matches!(key_event.code, KeyCode::Char('m')))
            {
                if let Some(cfg) = app.zen_config.as_mut() {
                    let aliases = ["opus", "sonnet", "haiku"];
                    let current = cfg.config.active_alias.as_str();
                    let idx = aliases.iter().position(|&a| a == current).unwrap_or(0);
                    let next = aliases[(idx + 1) % aliases.len()];
                    cfg.config.active_alias = next.to_string();
                    if let Err(e) = App::save_config(cfg, app.config_path_override.as_deref()) {
                        app.sessions[app.active].core
                            .view_messages
                            .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
                    }
                    if let Some(p) = crate::app::agent::LlmProvider::from_config(cfg) {
                        app.provider_name = p.display_name().to_string();
                        app.model_name = p.model_name().to_string();
                    }
                    app.model_highlight_until =
                        Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
                }
                return Ok(Some(Action::Redraw));
            }

            // macOS: Cmd+C (Super+C) 复制选区文本（遵循系统剪贴板快捷键惯例）
            // tui_textarea::Input 没有 super 字段，需在转换前从原始 key_event 检测。
            if key_event.code == KeyCode::Char('c')
                && key_event.modifiers.contains(KeyModifiers::SUPER)
                && copy_selection_to_clipboard(app)
            {
                return Ok(Some(Action::Redraw));
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
                if app.sessions[app.active].core.textarea.is_selecting() {
                    app.sessions[app.active].core.textarea.copy();
                    let text = app.sessions[app.active].core.textarea.yank_text();
                    if !text.is_empty() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(&text);
                        }
                        let char_count = text.chars().count();
                        app.sessions[app.active].core.copy_char_count = char_count;
                        app.sessions[app.active].core.copy_message_until = Some(
                            std::time::Instant::now() + std::time::Duration::from_millis(2000),
                        );
                        app.sessions[app.active].core.textarea.cancel_selection();
                        return Ok(Some(Action::Redraw));
                    }
                }
            }

            let input = Input::from(ev);

            // Setup 向导：优先拦截所有按键事件
            if app.setup_wizard.is_some() {
                let input_clone = input.clone();
                if let Some(ref mut wizard) = app.setup_wizard {
                    if let Some(action) =
                        crate::app::setup_wizard::handle_setup_wizard_key(wizard, input_clone)
                    {
                        match action {
                            crate::app::setup_wizard::SetupWizardAction::SaveAndClose => {
                                let wizard = app.setup_wizard.take().unwrap();
                                match crate::app::setup_wizard::save_setup(&wizard) {
                                    Ok(cfg) => app.refresh_after_setup(cfg),
                                    Err(e) => {
                                        let msg = MessageViewModel::from_base_message(
                                            &BaseMessage::system(format!("配置保存失败: {}", e)),
                                            &[],
                                        );
                                        let _ =
                                            app.sessions[app.active].core.render_tx.send(RenderEvent::AddMessage(msg));
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
            if app.sessions[app.active].core.thread_browser.is_some() {
                handle_thread_browser(app, input);
                return Ok(Some(Action::Redraw));
            }

            // CronPanel 优先处理
            if app.cron.cron_panel.is_some() {
                handle_cron_panel(app, input);
                return Ok(Some(Action::Redraw));
            }

            // OAuth 弹窗优先处理
            if app.oauth_prompt.is_some() {
                handle_oauth_prompt(app, input);
                return Ok(Some(Action::Redraw));
            }

            // MCP 面板优先处理
            if app.mcp_panel.is_some() {
                handle_mcp_panel(app, input);
                return Ok(Some(Action::Redraw));
            }

            // /agents 面板优先处理
            if app.sessions[app.active].core.agent_panel.is_some() {
                handle_agent_panel(app, input);
                return Ok(Some(Action::Redraw));
            }

            // /login 面板优先处理
            if app.sessions[app.active].core.login_panel.is_some() {
                handle_login_panel(app, input);
                return Ok(Some(Action::Redraw));
            }

            // /model 面板优先处理
            if app.sessions[app.active].core.model_panel.is_some() {
                handle_model_panel(app, input);
                return Ok(Some(Action::Redraw));
            }

            // /config 配置面板优先处理
            if app.sessions[app.active].core.config_panel.is_some() {
                handle_config_panel(app, input);
                return Ok(Some(Action::Redraw));
            }

            // /cost & /context 状态面板优先处理
            if app.status_panel.is_some() {
                handle_status_panel(app, input);
                return Ok(Some(Action::Redraw));
            }

            // /memory 面板优先处理
            if app.memory_panel.is_some() {
                handle_memory_panel(app, &input);
                // Enter 时打开编辑器（避免借用冲突，Enter 在 handle_memory_panel 中不处理）
                if matches!(
                    input,
                    Input {
                        key: Key::Enter,
                        ..
                    }
                ) {
                    if let Err(e) = app.memory_panel_open_editor() {
                        tracing::error!("Failed to open editor: {}", e);
                    }
                }
                return Ok(Some(Action::Redraw));
            }

            // AskUser 批量弹窗
            if matches!(
                &app.sessions[app.active].agent.interaction_prompt,
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
                &app.sessions[app.active].agent.interaction_prompt,
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
                    if app.sessions[app.active].core.loading {
                        app.interrupt();
                    } else {
                        return Ok(Some(Action::Quit));
                    }
                }
                Input { key: Key::Esc, .. } if !app.sessions[app.active].core.loading => return Ok(Some(Action::Quit)),

                // Up：浮层导航 > 历史恢复（仅首行）> textarea 光标
                Input { key: Key::Up, .. } if !app.sessions[app.active].core.loading => {
                    let hint_count = app.hint_candidates_count();
                    if hint_count > 0 {
                        let cur = app.sessions[app.active].core.hint_cursor.unwrap_or(0);
                        app.sessions[app.active].core.hint_cursor = if cur == 0 {
                            Some(hint_count - 1)
                        } else {
                            Some(cur - 1)
                        };
                    } else {
                        let (row, _col) = app.sessions[app.active].core.textarea.cursor();
                        if row == 0 {
                            app.history_up();
                        } else {
                            app.sessions[app.active].core.textarea.input(Input {
                                key: Key::Up,
                                ctrl: false,
                                alt: false,
                                shift: false,
                            });
                        }
                    }
                }

                // Down：浮层导航 > 历史恢复（仅末行）> textarea 光标
                Input { key: Key::Down, .. } if !app.sessions[app.active].core.loading => {
                    let hint_count = app.hint_candidates_count();
                    if hint_count > 0 {
                        let cur = app.sessions[app.active].core.hint_cursor.unwrap_or(hint_count - 1);
                        app.sessions[app.active].core.hint_cursor = if cur + 1 >= hint_count {
                            Some(0)
                        } else {
                            Some(cur + 1)
                        };
                    } else if app.sessions[app.active].core.history_index.is_some() {
                        app.history_down();
                    } else {
                        let (row, _col) = app.sessions[app.active].core.textarea.cursor();
                        let last_row = app.sessions[app.active].core.textarea.lines().len().saturating_sub(1);
                        if row >= last_row {
                            app.history_down();
                        } else {
                            app.sessions[app.active].core.textarea.input(Input {
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
                } if !app.sessions[app.active].core.loading => {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        if let Ok(img) = clipboard.get_image() {
                            let (w, h) = (img.width as u32, img.height as u32);
                            if let Ok((b64, sz)) = rgba_to_png_base64(w, h, &img.bytes) {
                                let n = app.sessions[app.active].core.pending_attachments.len() + 1;
                                app.add_pending_attachment(PendingAttachment {
                                    label: format!("clipboard_{}.png", n),
                                    media_type: "image/png".to_string(),
                                    base64_data: b64,
                                    size_bytes: sz,
                                });
                            }
                        } else if let Ok(text) = clipboard.get_text() {
                            let text = text.replace('\r', "\n");
                            app.sessions[app.active].core.textarea.insert_str(&text);
                        }
                    }
                }

                // Tab：提示浮层候选导航与补全
                Input {
                    key: Key::Tab,
                    shift: false,
                    ..
                } if !app.sessions[app.active].core.loading => {
                    let count = app.hint_candidates_count();
                    if count > 0 {
                        match app.sessions[app.active].core.hint_cursor {
                            Some(cur) if cur + 1 < count => {
                                app.sessions[app.active].core.hint_cursor = Some(cur + 1);
                            }
                            Some(_) => {
                                // 已在最后一个，循环到第一个
                                app.sessions[app.active].core.hint_cursor = Some(0);
                            }
                            None => {
                                // 首次按 Tab，选中第一个
                                app.sessions[app.active].core.hint_cursor = Some(0);
                            }
                        }
                    }
                }

                // Enter 在有候选项时：确认选中（无选中则默认第一项）
                Input {
                    key: Key::Enter, ..
                } if !app.sessions[app.active].core.loading && app.hint_candidates_count() > 0 => {
                    if app.sessions[app.active].core.hint_cursor.is_none() {
                        app.sessions[app.active].core.hint_cursor = Some(0);
                    }
                    app.hint_complete();
                }

                // Alt+Enter：插入换行
                Input {
                    key: Key::Enter,
                    alt: true,
                    ..
                } => {
                    app.sessions[app.active].core.textarea.input(Input {
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
                    let text = app.sessions[app.active].core.textarea.lines().join("\n");
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        if app.sessions[app.active].core.loading {
                            // Loading 状态：缓冲消息
                            app.sessions[app.active].core.pending_messages.push(text);
                            app.update_textarea_hint();
                        } else if text.starts_with('/') {
                            app.sessions[app.active].core.textarea = crate::app::build_textarea(false);
                            // 命令模式：取出 registry 避免借用冲突
                            let registry = std::mem::take(&mut app.sessions[app.active].core.command_registry);
                            let known = registry.dispatch(app, &text);
                            app.sessions[app.active].core.command_registry = registry;
                            if known {
                                // 命令命中，结束
                            } else {
                                // 命令未命中，尝试 Skill 匹配
                                let skill_name: String = text
                                    .trim_start_matches('/')
                                    .chars()
                                    .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                                    .collect();
                                if let Some(_skill) =
                                    app.sessions[app.active].core.skills.iter().find(|s| s.name == skill_name)
                                {
                                    // Skill 命中：将整条消息提交给 agent
                                    return Ok(Some(Action::Submit(text)));
                                } else {
                                    // 区分"前缀歧义"和"完全未知"
                                    let prefix = text.trim_start_matches('/').to_string();
                                    let cmd_matches = app.sessions[app.active].core.command_registry.match_prefix(&prefix);
                                    let error_msg = if cmd_matches.len() > 1 {
                                        let names: Vec<&str> =
                                            cmd_matches.iter().map(|(n, _)| *n).collect();
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
                                    app.sessions[app.active].core.view_messages.push(MessageViewModel::system(error_msg));
                                }
                            }
                        } else {
                            app.sessions[app.active].core.textarea = crate::app::build_textarea(false);
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
                Input {
                    key: Key::Delete, ..
                } if !app.sessions[app.active].core.loading && !app.sessions[app.active].core.pending_attachments.is_empty() => {
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
                        app.sessions[app.active].core.textarea.input(input);
                    }
                }

                // 拦截普通 Enter，避免 textarea 默认换行；允许 loading 时输入
                input if input.key != Key::Enter => {
                    // 退出历史浏览
                    if app.sessions[app.active].core.history_index.is_some() {
                        app.exit_history();
                    }
                    app.sessions[app.active].core.textarea.input(input);
                    // 输入内容变化时：重置光标（不预选，等用户按 Tab/上下键激活）
                    if !app.sessions[app.active].core.loading {
                        app.sessions[app.active].core.hint_cursor = None;
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
            if app.sessions[app.active].core.login_panel.is_some() {
                app.sessions[app.active].core.login_panel.as_mut().unwrap().paste_text(&text);
                return Ok(Some(Action::Redraw));
            }

            // model_panel 打开时拦截粘贴（面板无文本输入字段）
            if app.sessions[app.active].core.model_panel.is_some() {
                return Ok(Some(Action::Redraw));
            }

            // config_panel 打开时粘贴到当前编辑字段
            if app.sessions[app.active].core.config_panel.is_some() {
                if let Some(panel) = app.sessions[app.active].core.config_panel.as_mut() {
                    panel.paste_text(&text);
                }
                return Ok(Some(Action::Redraw));
            }

            // thread_browser / agent_panel / cron_panel 打开时拦截粘贴，
            // 防止文本进入后台 textarea（这些面板无文本输入字段）
            if app.sessions[app.active].core.thread_browser.is_some()
                || app.sessions[app.active].core.agent_panel.is_some()
                || app.cron.cron_panel.is_some()
                || app.mcp_panel.is_some()
                || app.status_panel.is_some()
                || app.memory_panel.is_some()
            {
                return Ok(Some(Action::Redraw));
            }

            // 其他情况粘贴到 textarea
            app.sessions[app.active].core.textarea.insert_str(&text);
        }
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::ScrollUp => {
                // MCP 面板区域滚轮滚动面板，否则滚动消息区
                if let Some(area) = app.sessions[app.active].core.panel_area {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                        && app.mcp_panel.is_some()
                    {
                        app.mcp_panel_scroll_up(3);
                        return Ok(Some(Action::Redraw));
                    }
                }
                app.scroll_up();
            }
            MouseEventKind::ScrollDown => {
                if let Some(area) = app.sessions[app.active].core.panel_area {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                        && app.mcp_panel.is_some()
                    {
                        app.mcp_panel_scroll_down(3);
                        return Ok(Some(Action::Redraw));
                    }
                }
                app.scroll_down();
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // 多 session：点击非 active session 列区域时切换焦点
                if app.sessions.len() > 1 {
                    for (i, area) in app.session_areas.iter().enumerate() {
                        if mouse.column >= area.x
                            && mouse.column < area.x + area.width
                            && mouse.row >= area.y
                            && mouse.row < area.y + area.height
                            && i != app.active
                        {
                            app.active = i;
                            return Ok(Some(Action::Redraw));
                        }
                    }
                }
                // 面板区域：开始面板选区
                if let Some(area) = app.sessions[app.active].core.panel_area {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let content_row = mouse.row - area.y + app.sessions[app.active].core.panel_scroll_offset;
                        let col = mouse.column - area.x;
                        app.sessions[app.active].core.panel_selection.start_drag(content_row, col);
                        app.sessions[app.active].core.text_selection.clear();
                        // 不再处理其他区域的选区
                        return Ok(Some(Action::Redraw));
                    }
                }
                if let Some(area) = app.sessions[app.active].core.messages_area {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let visual_row = mouse.row - area.y + app.sessions[app.active].core.scroll_offset;
                        let visual_col = mouse.column - area.x;
                        app.sessions[app.active].core.text_selection.start_drag(visual_row, visual_col);
                    }
                }
                // 输入框区域：开始 textarea 选区
                if let Some(area) = app.sessions[app.active].core.textarea_area {
                    if mouse.row >= area.y
                        && mouse.row < area.y + area.height
                        && mouse.column >= area.x
                        && mouse.column < area.x + area.width
                    {
                        let row = (mouse.row - area.y).saturating_sub(1) as usize; // 跳过顶部边框
                        let col = mouse.column.saturating_sub(area.x) as usize;
                        app.sessions[app.active].core
                            .textarea
                            .move_cursor(tui_textarea::CursorMove::Jump(row as u16, col as u16));
                        app.sessions[app.active].core.textarea.start_selection();
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // 面板选区拖拽
                if app.sessions[app.active].core.panel_selection.dragging {
                    if let Some(area) = app.sessions[app.active].core.panel_area {
                        let content_row = mouse
                            .row
                            .saturating_sub(area.y)
                            .saturating_add(app.sessions[app.active].core.panel_scroll_offset);
                        let col = mouse.column.saturating_sub(area.x);
                        app.sessions[app.active].core.panel_selection.update_drag(content_row, col);
                    }
                }
                if app.sessions[app.active].core.text_selection.dragging {
                    if let Some(area) = app.sessions[app.active].core.messages_area {
                        let visual_row = mouse
                            .row
                            .saturating_sub(area.y)
                            .saturating_add(app.sessions[app.active].core.scroll_offset);
                        let visual_col = mouse.column.saturating_sub(area.x);
                        app.sessions[app.active].core.text_selection.update_drag(visual_row, visual_col);
                    }
                }
                // 输入框区域：扩展 textarea 选区
                if app.sessions[app.active].core.textarea.is_selecting() {
                    if let Some(area) = app.sessions[app.active].core.textarea_area {
                        if mouse.row >= area.y && mouse.row < area.y + area.height {
                            let row = (mouse.row - area.y).saturating_sub(1) as usize;
                            let col = mouse.column.saturating_sub(area.x) as usize;
                            app.sessions[app.active].core
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
                if app.sessions[app.active].core.panel_selection.dragging {
                    app.sessions[app.active].core.panel_selection.end_drag();
                    let sel = &app.sessions[app.active].core.panel_selection;
                    if let (Some(start), Some(end)) = (sel.start, sel.end) {
                        let text = crate::app::text_selection::extract_panel_text(
                            start,
                            end,
                            &app.sessions[app.active].core.panel_plain_lines,
                        );
                        app.sessions[app.active].core.panel_selection.set_selected_text(text);
                    }
                }
                if app.sessions[app.active].core.text_selection.dragging {
                    app.sessions[app.active].core.text_selection.end_drag();
                    let ts = &app.sessions[app.active].core.text_selection;
                    if let (Some(start), Some(end)) = (ts.start, ts.end) {
                        let usable_width = app
                            .sessions[app.active].core
                            .messages_area
                            .map(|a| a.width.saturating_sub(1))
                            .unwrap_or(0);
                        let cache = app.sessions[app.active].core.render_cache.read();
                        let text = crate::app::text_selection::extract_selected_text(
                            start,
                            end,
                            &cache.wrap_map,
                            usable_width,
                        );
                        drop(cache);
                        app.sessions[app.active].core.text_selection.set_selected_text(text);
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
    // 确认删除模式下只处理 Enter（确认）和其他键（取消）
    if app.sessions[app.active]
        .core
        .thread_browser
        .as_ref()
        .is_some_and(|b| b.confirm_delete)
    {
        match input {
            Input {
                key: Key::Enter, ..
            } => {
                if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                    b.confirm_delete = false;
                    if let Some(title) = b.delete_selected() {
                        app.sessions[app.active].core
                            .view_messages
                            .push(MessageViewModel::system(format!("已删除对话: {}", title)));
                    }
                }
            }
            _ => {
                if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                    b.confirm_delete = false;
                }
            }
        }
        return;
    }

    // 搜索框聚焦时的输入处理
    let search_focused = app.sessions[app.active]
        .core
        .thread_browser
        .as_ref()
        .is_some_and(|b| b.search_focused);

    if search_focused {
        match input {
            Input {
                key: Key::Char('c'),
                ctrl: true,
                ..
            } => {}
            Input { key: Key::Esc, .. } => {
                if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                    if !b.search_query.value().is_empty() {
                        // 清空搜索
                        b.search_query.set_value(String::new());
                        b.refresh_filter();
                    } else {
                        // 关闭面板
                        app.sessions[app.active].core.thread_browser = None;
                        app.sessions[app.active].core.panel_selection.clear();
                        app.sessions[app.active].core.panel_area = None;
                    }
                }
            }
            Input {
                key: Key::Char('v'),
                ctrl: true,
                ..
            } => {
                if let Ok(text) = arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
                    if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                        b.search_query.paste(&text);
                        b.refresh_filter();
                    }
                }
            }
            Input {
                key: Key::Char(c), ..
            } => {
                if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                    b.search_query.insert(c);
                    b.refresh_filter();
                }
            }
            Input {
                key: Key::Backspace,
                ..
            } => {
                if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                    b.search_query.backspace();
                    b.refresh_filter();
                }
            }
            Input {
                key: Key::Delete, ..
            } => {
                if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                    b.search_query.delete();
                    b.refresh_filter();
                }
            }
            Input { key: Key::Left, .. } => {
                if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                    b.search_query.cursor_left();
                }
            }
            Input {
                key: Key::Right, ..
            } => {
                if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                    b.search_query.cursor_right();
                }
            }
            Input { key: Key::Home, .. } => {
                if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                    b.search_query.cursor_home();
                }
            }
            Input { key: Key::End, .. } => {
                if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                    b.search_query.cursor_end();
                }
            }
            // ↓ / Tab 切换到列表模式
            Input { key: Key::Down, .. } | Input { key: Key::Tab, .. } => {
                if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                    b.search_focused = false;
                }
            }
            // Enter：打开选中的 thread
            Input {
                key: Key::Enter, ..
            } => {
                if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                    if let Some(id) = b.selected_id().cloned() {
                        app.open_thread_with_feedback(id);
                    }
                }
            }
            _ => {}
        }
        return;
    }

    // 列表模式
    match input {
        Input {
            key: Key::Char('c'),
            ctrl: true,
            ..
        } => {}
        Input { key: Key::Esc, .. } => {
            // Esc 关闭面板
            app.sessions[app.active].core.thread_browser = None;
            app.sessions[app.active].core.panel_selection.clear();
            app.sessions[app.active].core.panel_area = None;
        }
        Input { key: Key::Up, .. } => {
            let visible = app.sessions[app.active]
                .core
                .panel_area
                .map(|a| a.height.saturating_sub(1))
                .unwrap_or(10);
            if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                b.move_cursor(-1);
                // 每个 item 占 3 视觉行（标题 + 元数据 + 空行）
                let visual_row = b.cursor as u16 * 3;
                // panel_area 已经是 list_area（不含搜索框），减去快捷键 1 行
                b.scroll_offset =
                    crate::app::ensure_cursor_visible(visual_row, b.scroll_offset, visible);
            }
        }
        Input { key: Key::Down, .. } => {
            let visible = app.sessions[app.active]
                .core
                .panel_area
                .map(|a| a.height.saturating_sub(1))
                .unwrap_or(10);
            let visible = app.sessions[app.active]
                .core
                .panel_area
                .map(|a| a.height.saturating_sub(1))
                .unwrap_or(10);
            if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                b.move_cursor(1);
                let visual_row = b.cursor as u16 * 3;
                b.scroll_offset =
                    crate::app::ensure_cursor_visible(visual_row, b.scroll_offset, visible);
            }
        }
        Input {
            key: Key::Enter, ..
        } => {
            if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                if let Some(id) = b.selected_id().cloned() {
                    app.open_thread_with_feedback(id);
                }
            }
        }
        Input {
            key: Key::Char('d'),
            ctrl: true,
            ..
        } => {
            if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                if b.total() > 0 {
                    b.confirm_delete = true;
                }
            }
        }
        // / 或 Tab 切换到搜索框
        Input {
            key: Key::Char('/'),
            ..
        }
        | Input { key: Key::Tab, .. } => {
            if let Some(b) = app.sessions[app.active].core.thread_browser.as_mut() {
                b.search_focused = true;
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
            app.sessions[app.active].core.panel_selection.clear();
            app.sessions[app.active].core.panel_area = None;
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

    let mode = match app.sessions[app.active].core.login_panel.as_ref() {
        Some(p) => p.mode.clone(),
        None => return,
    };

    match mode {
        LoginPanelMode::Browse => match input {
            Input { key: Key::Esc, .. } => {
                app.close_login_panel();
            }
            Input { key: Key::Up, .. } => {
                app.sessions[app.active].core.login_panel.as_mut().unwrap().move_cursor(-1);
            }
            Input { key: Key::Down, .. } => {
                app.sessions[app.active].core.login_panel.as_mut().unwrap().move_cursor(1);
            }
            Input {
                key: Key::Enter, ..
            } => {
                app.login_panel_select_provider();
            }
            Input {
                key: Key::Tab,
                shift: false,
                ..
            } => {
                app.sessions[app.active].core.login_panel.as_mut().unwrap().enter_edit();
            }
            Input {
                key: Key::Char('n'),
                ctrl: true,
                ..
            } => {
                app.sessions[app.active].core.login_panel.as_mut().unwrap().enter_new();
            }
            Input {
                key: Key::Char('d'),
                ctrl: true,
                ..
            } => {
                app.sessions[app.active].core.login_panel.as_mut().unwrap().request_delete();
            }
            _ => {}
        },
        LoginPanelMode::Edit | LoginPanelMode::New => {
            let is_type_field = app.sessions[app.active].core.login_panel.as_ref().unwrap().edit_field
                == crate::app::login_panel::LoginEditField::Type;

            match input {
                Input { key: Key::Esc, .. } => {
                    app.sessions[app.active].core.login_panel.as_mut().unwrap().mode = LoginPanelMode::Browse;
                }
                Input {
                    key: Key::Char('v'),
                    ctrl: true,
                    ..
                } => {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        if let Ok(text) = clipboard.get_text() {
                            app.sessions[app.active].core.login_panel.as_mut().unwrap().paste_text(&text);
                        }
                    }
                }
                Input { key: Key::Up, .. } => {
                    app.sessions[app.active].core.login_panel.as_mut().unwrap().field_prev();
                }
                Input { key: Key::Down, .. } => {
                    app.sessions[app.active].core.login_panel.as_mut().unwrap().field_next();
                }
                Input {
                    key: Key::Tab,
                    shift: false,
                    ..
                } => {
                    app.sessions[app.active].core.login_panel.as_mut().unwrap().field_next();
                }
                Input {
                    key: Key::Tab,
                    shift: true,
                    ..
                } => {
                    app.sessions[app.active].core.login_panel.as_mut().unwrap().field_prev();
                }
                Input { key: Key::Left, .. }
                | Input {
                    key: Key::Right, ..
                } if is_type_field => {
                    app.sessions[app.active].core.login_panel.as_mut().unwrap().cycle_type();
                }
                Input {
                    key: Key::Char(' '),
                    ..
                } => {
                    if is_type_field {
                        app.sessions[app.active].core.login_panel.as_mut().unwrap().cycle_type();
                    } else if let Some((buf, cursor)) =
                        app.sessions[app.active].core.login_panel.as_mut().unwrap().active_field()
                    {
                        crate::app::handle_edit_key(
                            buf,
                            cursor,
                            Input {
                                key: Key::Char(' '),
                                ctrl: false,
                                alt: false,
                                shift: false,
                            },
                        );
                    }
                }
                Input {
                    key: Key::Enter, ..
                } => {
                    app.login_panel_apply_edit();
                }
                _ => {
                    if !is_type_field {
                        if let Some((buf, cursor)) =
                            app.sessions[app.active].core.login_panel.as_mut().unwrap().active_field()
                        {
                            crate::app::handle_edit_key(buf, cursor, input);
                        }
                    }
                }
            }
        }
        LoginPanelMode::ConfirmDelete => match input {
            Input {
                key: Key::Enter, ..
            } => {
                app.login_panel_confirm_delete();
            }
            Input { key: Key::Esc, .. } => {
                app.sessions[app.active].core.login_panel.as_mut().unwrap().cancel_delete();
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
            app.sessions[app.active].core.model_panel.as_mut().unwrap().move_cursor(-1);
        }
        Input { key: Key::Down, .. } => {
            app.sessions[app.active].core.model_panel.as_mut().unwrap().move_cursor(1);
        }
        Input {
            key: Key::Char(' ') | Key::Enter,
            ..
        } => {
            let cursor = app.sessions[app.active].core.model_panel.as_ref().unwrap().cursor;
            match cursor {
                ROW_OPUS => {
                    app.sessions[app.active].core.model_panel.as_mut().unwrap().active_tab = AliasTab::Opus;
                    app.model_panel_confirm();
                }
                ROW_SONNET => {
                    app.sessions[app.active].core.model_panel.as_mut().unwrap().active_tab = AliasTab::Sonnet;
                    app.model_panel_confirm();
                }
                ROW_HAIKU => {
                    app.sessions[app.active].core.model_panel.as_mut().unwrap().active_tab = AliasTab::Haiku;
                    app.model_panel_confirm();
                }
                ROW_EFFORT => {
                    app.sessions[app.active].core.model_panel.as_mut().unwrap().cycle_effort(false);
                }
                _ => {}
            }
        }
        Input { key: Key::Left, .. } => {
            app.sessions[app.active].core.model_panel.as_mut().unwrap().cycle_effort(true);
        }
        Input {
            key: Key::Right, ..
        } => {
            app.sessions[app.active].core.model_panel.as_mut().unwrap().cycle_effort(false);
        }
        _ => {}
    }
}

fn handle_config_panel(app: &mut App, input: Input) {
    use crate::app::config_panel::{ConfigEditField, ConfigPanel, ConfigPanelMode};
    let Some(panel) = app.sessions[app.active].core.config_panel.as_mut() else {
        return;
    };
    match panel.mode {
        ConfigPanelMode::Browse => match input {
            Input { key: Key::Up, .. } => {
                if panel.cursor > 0 {
                    panel.cursor -= 1;
                } else {
                    panel.cursor = ConfigPanel::field_count() - 1;
                }
            }
            Input { key: Key::Down, .. } => {
                panel.cursor = (panel.cursor + 1) % ConfigPanel::field_count();
            }
            Input {
                key: Key::Enter, ..
            } => {
                panel.enter_edit();
            }
            Input { key: Key::Esc, .. } => {
                app.sessions[app.active].core.config_panel = None;
            }
            _ => {}
        },
        ConfigPanelMode::Edit => match input {
            Input { key: Key::Esc, .. } => {
                panel.mode = ConfigPanelMode::Browse;
            }
            Input {
                key: Key::Enter, ..
            } => {
                app.config_panel_apply();
            }
            Input { key: Key::Up, .. } => {
                panel.field_prev();
            }
            Input { key: Key::Down, .. } => {
                panel.field_next();
            }
            Input {
                key: Key::Char(' '),
                ctrl: false,
                ..
            } => match panel.edit_field {
                ConfigEditField::Autocompact => panel.cycle_autocompact(),
                ConfigEditField::Proactiveness => panel.cycle_proactiveness(),
                _ => {
                    if let Some((buf, cursor)) = panel.active_field() {
                        crate::app::handle_edit_key(
                            buf,
                            cursor,
                            Input {
                                key: Key::Char(' '),
                                ctrl: false,
                                alt: false,
                                shift: false,
                            },
                        );
                    }
                }
            },
            Input {
                key: Key::Left,
                ctrl: false,
                ..
            }
            | Input {
                key: Key::Right,
                ctrl: false,
                ..
            } => match panel.edit_field {
                ConfigEditField::Autocompact => panel.cycle_autocompact(),
                ConfigEditField::Proactiveness => panel.cycle_proactiveness(),
                _ => {
                    if let Some((buf, cursor)) = panel.active_field() {
                        crate::app::handle_edit_key(buf, cursor, input);
                    }
                }
            },
            _ => {
                if let Some((buf, cursor)) = panel.active_field() {
                    crate::app::handle_edit_key(buf, cursor, input);
                }
            }
        },
    }
}

fn handle_status_panel(app: &mut App, input: Input) {
    match input {
        Input { key: Key::Esc, .. } => {
            app.status_panel = None;
        }
        Input { key: Key::Left, .. } => {
            if let Some(panel) = &mut app.status_panel {
                panel.tab.prev();
            }
        }
        Input {
            key: Key::Right, ..
        } => {
            if let Some(panel) = &mut app.status_panel {
                panel.tab.next();
            }
        }
        _ => {}
    }
}

fn handle_memory_panel(app: &mut App, input: &Input) {
    let Some(panel) = app.memory_panel.as_mut() else {
        return;
    };
    match *input {
        Input { key: Key::Up, .. } => {
            panel.move_cursor_up();
        }
        Input { key: Key::Down, .. } => {
            panel.move_cursor_down();
        }
        Input {
            key: Key::Enter, ..
        } => {
            // 由调用方处理打开编辑器（避免借用冲突），此处不执行操作
        }
        Input { key: Key::Esc, .. } => {
            app.memory_panel = None;
        }
        _ => {}
    }
}

fn handle_cron_panel(app: &mut App, input: Input) {
    // 确认删除模式下只处理 Enter（确认）和 Esc（取消）
    if app
        .cron
        .cron_panel
        .as_ref()
        .is_some_and(|p| p.confirm_delete)
    {
        match input {
            Input {
                key: Key::Enter, ..
            } => {
                app.cron_panel_confirm_delete();
            }
            _ => {
                app.cron_panel_cancel_delete();
            }
        }
        return;
    }

    match input {
        Input {
            key: Key::Char('c'),
            ctrl: true,
            ..
        } => {
            // Ctrl+C 在面板中不退出，忽略
        }
        Input { key: Key::Up, .. } => {
            app.cron_panel_move_up();
        }
        Input { key: Key::Down, .. } => {
            app.cron_panel_move_down();
        }
        Input {
            key: Key::Enter, ..
        } => {
            app.cron_panel_toggle();
        }
        Input { key: Key::Esc, .. } => {
            app.cron_panel_close();
            app.sessions[app.active].core.panel_selection.clear();
            app.sessions[app.active].core.panel_area = None;
        }
        Input {
            key: Key::Char('d'),
            ctrl: true,
            ..
        } => {
            app.cron_panel_request_delete();
        }
        _ => {}
    }
}

fn handle_mcp_panel(app: &mut App, input: Input) {
    // 确认删除模式下只处理 Enter（确认）和其他键（取消）
    if app
        .mcp_panel
        .as_ref()
        .is_some_and(|p| p.confirm_delete.is_some())
    {
        match input {
            Input {
                key: Key::Enter, ..
            } => {
                app.mcp_panel_confirm_delete();
            }
            _ => {
                app.mcp_panel_cancel_delete();
            }
        }
        return;
    }

    let is_server_list = app
        .mcp_panel
        .as_ref()
        .is_none_or(|p| p.view.is_server_list());

    match input {
        Input {
            key: Key::Char('c'),
            ctrl: true,
            ..
        } => {
            // Ctrl+C 在面板中不退出，忽略
        }
        Input { key: Key::Up, .. } => {
            app.mcp_panel_move_up();
        }
        Input { key: Key::Down, .. } => {
            app.mcp_panel_move_down();
        }
        Input {
            key: Key::Enter, ..
        } => {
            app.mcp_panel_enter();
        }
        Input { key: Key::Esc, .. } => {
            if is_server_list {
                app.mcp_panel_close();
                app.sessions[app.active].core.panel_selection.clear();
                app.sessions[app.active].core.panel_area = None;
            } else {
                app.mcp_panel_back();
            }
        }
        Input {
            key: Key::Char('r'),
            ctrl: true,
            ..
        } => {
            if is_server_list {
                app.mcp_panel_reconnect();
            }
        }
        Input {
            key: Key::Char('d'),
            ctrl: true,
            ..
        } => {
            if is_server_list {
                app.mcp_panel_request_delete();
            }
        }
        _ => {}
    }
}

fn handle_oauth_prompt(app: &mut App, input: Input) {
    use crate::app::handle_edit_key;
    let prompt = match app.oauth_prompt.as_mut() {
        Some(p) => p,
        None => return,
    };
    match input {
        Input {
            key: Key::Enter, ..
        } => {
            if prompt.submit() {
                app.oauth_prompt = None;
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
            app.oauth_prompt = None;
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
