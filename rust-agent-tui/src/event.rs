use anyhow::Result;
use base64::Engine as _;
use ratatui::crossterm::event::{self, Event, KeyEventKind, MouseEventKind};
use ratatui_textarea::{Input, Key};
use std::time::Duration;

use crate::app::model_panel::ModelPanelMode;
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

pub async fn next_event(app: &mut App) -> Result<Option<Action>> {
    if !event::poll(Duration::from_millis(50))? {
        return Ok(None);
    }

    let ev = event::read()?;

    match ev {
        Event::Resize(w, _) => {
            let _ = app.core.render_tx.send(RenderEvent::Resize(w));
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
                    Input { key: Key::Up, .. }
                    | Input {
                        key: Key::Char('k'),
                        ..
                    } => app.hitl_move(-1),
                    Input { key: Key::Down, .. }
                    | Input {
                        key: Key::Char('j'),
                        ..
                    } => app.hitl_move(1),

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
                Input {
                    key: Key::Char('c'),
                    ctrl: true,
                    ..
                } => {
                    if app.core.loading {
                        // loading 时：中断 Agent（不退出）
                        app.interrupt();
                    } else {
                        return Ok(Some(Action::Quit));
                    }
                }
                Input { key: Key::Esc, .. } if !app.core.loading => return Ok(Some(Action::Quit)),

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
                            app.core.textarea = crate::app::build_textarea(false, 0);
                            // 命令模式：取出 registry 避免借用冲突
                            let registry = std::mem::take(&mut app.core.command_registry);
                            let known = registry.dispatch(app, &text);
                            app.core.command_registry = registry;
                            if !known {
                                app.core.view_messages.push(MessageViewModel::system(format!(
                                    "未知命令: {}  （输入 /help 查看可用命令）",
                                    text
                                )));
                            }
                        } else {
                            app.core.textarea = crate::app::build_textarea(false, 0);
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

                // Shift+T：切换工具调用消息的显示
                Input {
                    key: Key::Char('T'),
                    shift: true,
                    ..
                } => {
                    app.toggle_collapsed_messages();
                }

                // Del：删除最后一个待发送附件（有附件时优先消费 Del）
                Input { key: Key::Delete, .. } if !app.core.loading && !app.core.pending_attachments.is_empty() => {
                    app.pop_pending_attachment();
                }

                // 拦截普通 Enter，避免 textarea 默认换行；允许 loading 时输入
                input if input.key != Key::Enter => {
                    app.core.textarea.input(input);
                    // 输入内容变化时重置提示光标
                    if !app.core.loading {
                        app.core.hint_cursor = None;
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
        }
        Input { key: Key::Up, .. }
        | Input {
            key: Key::Char('k'),
            ..
        } => {
            if let Some(b) = app.core.thread_browser.as_mut() {
                b.move_cursor(-1);
                b.scroll_offset = crate::app::ensure_cursor_visible(b.cursor as u16, b.scroll_offset, 10);
            }
        }
        Input { key: Key::Down, .. }
        | Input {
            key: Key::Char('j'),
            ..
        } => {
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
        }
        Input { key: Key::Up, .. }
        | Input {
            key: Key::Char('k'),
            ..
        } => {
            app.agent_panel_move_up();
        }
        Input { key: Key::Down, .. }
        | Input {
            key: Key::Char('j'),
            ..
        } => {
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

// ─── /model 面板键盘处理 ──────────────────────────────────────────────────────

fn handle_model_panel(app: &mut App, input: Input) {
    use crate::app::model_panel::{AliasEditField, EditField};

    // 用只读借用提前获取 mode（借用立即释放），后续各分支可自由重借 app.core.model_panel
    let mode = match app.core.model_panel.as_ref() {
        Some(p) => p.mode.clone(),
        None => return,
    };

    match mode {
        // ── 别名配置主界面 ────────────────────────────────────────────────────
        ModelPanelMode::AliasConfig => match input {
            Input { key: Key::Esc, .. } => {
                app.close_model_panel();
            }
            Input { key: Key::Char('v'), ctrl: true, .. } => {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        app.core.model_panel.as_mut().unwrap().paste_text(&text);
                    }
                }
            }
            // Tab / Shift+Tab：切换 Alias Tab（Opus / Sonnet / Haiku）
            Input { key: Key::Tab, shift: false, .. } => {
                app.core.model_panel.as_mut().unwrap().tab_next();
            }
            Input { key: Key::Tab, shift: true, .. } => {
                app.core.model_panel.as_mut().unwrap().tab_prev();
            }
            // ↓：切换到下一个编辑字段（Provider → ModelId）
            Input { key: Key::Down, .. } => {
                app.core.model_panel.as_mut().unwrap().alias_field_next();
            }
            // ↑：切换到上一个编辑字段（ModelId → Provider）
            Input { key: Key::Up, .. } => {
                app.core.model_panel.as_mut().unwrap().alias_field_prev();
            }
            // Space：循环切换 Provider（当 alias_edit_field == Provider 时）
            Input { key: Key::Char(' '), .. } => {
                let field = app.core.model_panel.as_ref().unwrap().alias_edit_field.clone();
                if field == AliasEditField::Provider {
                    app.core.model_panel.as_mut().unwrap().cycle_alias_provider();
                }
            }
            // Enter：激活当前 Tab（写入 active_alias）并保存
            Input { key: Key::Enter, .. } => {
                app.model_panel_activate_tab();
            }
            // s：保存当前 Tab 的 provider/model 配置
            Input { key: Key::Char('s'), ctrl: false, alt: false, .. } => {
                app.model_panel_save_alias();
            }
            // p：进入 provider 管理子面板
            Input { key: Key::Char('p'), ctrl: false, alt: false, .. } => {
                app.core.model_panel.as_mut().unwrap().mode = ModelPanelMode::Browse;
            }
            // Backspace：删除当前 Tab 的 model_id 末字符
            Input { key: Key::Backspace, .. } => {
                app.core.model_panel.as_mut().unwrap().pop_alias_char();
            }
            // 字符输入：写入 model_id 缓冲（当 alias_edit_field == ModelId 时）
            Input { key: Key::Char(c), ctrl: false, alt: false, .. } => {
                app.core.model_panel.as_mut().unwrap().push_alias_char(c);
            }
            _ => {}
        },
        // ── Provider 管理浏览 ─────────────────────────────────────────────────
        ModelPanelMode::Browse => match input {
            Input { key: Key::Esc, .. } => {
                // 回到别名配置主界面
                app.core.model_panel.as_mut().unwrap().mode = ModelPanelMode::AliasConfig;
            }
            Input { key: Key::Up, .. }
            | Input { key: Key::Char('k'), .. } => {
                app.core.model_panel.as_mut().unwrap().move_cursor(-1);
            }
            Input { key: Key::Down, .. }
            | Input { key: Key::Char('j'), .. } => {
                app.core.model_panel.as_mut().unwrap().move_cursor(1);
            }
            Input { key: Key::Enter, .. } => {
                app.model_panel_confirm_select();
            }
            Input { key: Key::Char('e'), .. } => {
                app.core.model_panel.as_mut().unwrap().enter_edit();
            }
            Input { key: Key::Char('n'), .. } => {
                app.core.model_panel.as_mut().unwrap().enter_new();
            }
            Input { key: Key::Char('d'), .. } => {
                app.core.model_panel.as_mut().unwrap().request_delete();
            }
            _ => {}
        },
        // ── Provider 编辑/新建 ────────────────────────────────────────────────
        ModelPanelMode::Edit | ModelPanelMode::New => match input {
            Input { key: Key::Esc, .. } => {
                app.core.model_panel.as_mut().unwrap().mode = ModelPanelMode::Browse;
            }
            Input { key: Key::Char('v'), ctrl: true, .. } => {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        app.core.model_panel.as_mut().unwrap().paste_text(&text);
                    }
                }
            }
            Input { key: Key::Tab, shift: false, .. } => {
                app.core.model_panel.as_mut().unwrap().field_next();
            }
            Input { key: Key::Tab, shift: true, .. } => {
                app.core.model_panel.as_mut().unwrap().field_prev();
            }
            Input { key: Key::Char(' '), .. } => {
                let field = app.core.model_panel.as_ref().unwrap().edit_field();
                if field == EditField::ProviderType {
                    app.core.model_panel.as_mut().unwrap().cycle_type();
                } else if field == EditField::ThinkingBudget {
                    app.core.model_panel.as_mut().unwrap().toggle_thinking();
                } else {
                    app.core.model_panel.as_mut().unwrap().push_char(' ');
                }
            }
            Input { key: Key::Enter, .. } => {
                app.model_panel_apply_edit();
            }
            Input { key: Key::Backspace, .. } => {
                app.core.model_panel.as_mut().unwrap().pop_char();
            }
            Input { key: Key::Char(c), ctrl: false, alt: false, .. } => {
                app.core.model_panel.as_mut().unwrap().push_char(c);
            }
            _ => {}
        },
        // ── 删除确认 ──────────────────────────────────────────────────────────
        ModelPanelMode::ConfirmDelete => match input {
            Input { key: Key::Char('y'), .. } => {
                app.model_panel_confirm_delete();
            }
            Input { key: Key::Char('n'), .. }
            | Input { key: Key::Esc, .. } => {
                app.core.model_panel.as_mut().unwrap().cancel_delete();
            }
            _ => {}
        },
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
        }
        | Input {
            key: Key::Char('k'),
            ctrl: false,
            ..
        } => {
            app.cron_panel_move_up();
        }
        Input {
            key: Key::Down, ..
        }
        | Input {
            key: Key::Char('j'),
            ctrl: false,
            ..
        } => {
            app.cron_panel_move_down();
        }
        Input {
            key: Key::Enter, ..
        } => {
            app.cron_panel_toggle();
        }
        Input {
            key: Key::Char('d'),
            ctrl: false,
            ..
        } => {
            app.cron_panel_delete();
        }
        Input {
            key: Key::Esc, ..
        } => {
            app.cron_panel_close();
        }
        _ => {}
    }
}
