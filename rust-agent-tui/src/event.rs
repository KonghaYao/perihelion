use anyhow::Result;
use ratatui::crossterm::event::{self, Event, MouseEventKind};
use ratatui_textarea::{Input, Key};
use std::time::Duration;

use crate::app::model_panel::ModelPanelMode;
use crate::app::{App, MessageViewModel};
use crate::ui::render_thread::RenderEvent;

pub enum Action {
    Quit,
    Submit(String),
    Redraw,
}

pub async fn next_event(app: &mut App) -> Result<Option<Action>> {
    if !event::poll(Duration::from_millis(50))? {
        return Ok(Some(Action::Redraw));
    }

    let ev = event::read()?;

    match ev {
        Event::Resize(w, _) => {
            let _ = app.render_tx.try_send(RenderEvent::Resize(w));
        }
        Event::Key(_) => {
            let input = Input::from(ev);

            // Thread 浏览面板优先处理
            if app.thread_browser.is_some() {
                handle_thread_browser(app, input);
                return Ok(Some(Action::Redraw));
            }

            // /agents 面板优先处理
            if app.agent_panel.is_some() {
                handle_agent_panel(app, input);
                return Ok(Some(Action::Redraw));
            }

            // /model 面板优先处理
            if app.model_panel.is_some() {
                handle_model_panel(app, input);
                return Ok(Some(Action::Redraw));
            }

            // AskUser 批量弹窗
            if app.ask_user_prompt.is_some() {
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
            if app.hitl_prompt.is_some() {
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
                    if app.loading {
                        // loading 时：中断 Agent（不退出）
                        app.interrupt();
                    } else {
                        return Ok(Some(Action::Quit));
                    }
                }
                Input { key: Key::Esc, .. } if !app.loading => return Ok(Some(Action::Quit)),

                // Tab：提示浮层候选导航与补全
                Input {
                    key: Key::Tab,
                    shift: false,
                    ..
                } if !app.loading => {
                    let count = app.hint_candidates_count();
                    if count > 0 {
                        match app.hint_cursor {
                            Some(cur) if cur + 1 < count => {
                                app.hint_cursor = Some(cur + 1);
                            }
                            Some(_) => {
                                // 已在最后一个，循环到第一个
                                app.hint_cursor = Some(0);
                            }
                            None => {
                                // 首次按 Tab，选中第一个
                                app.hint_cursor = Some(0);
                            }
                        }
                    }
                }

                // Enter 在提示浮层激活时：确认选中
                Input {
                    key: Key::Enter, ..
                } if !app.loading && app.hint_cursor.is_some() => {
                    app.hint_complete();
                }

                // Alt+Enter：插入换行
                Input {
                    key: Key::Enter,
                    alt: true,
                    ..
                } if !app.loading => {
                    app.textarea.input(Input {
                        key: Key::Enter,
                        ctrl: false,
                        alt: false,
                        shift: false,
                    });
                }

                // Enter：提交
                Input {
                    key: Key::Enter, ..
                } if !app.loading => {
                    let text = app.textarea.lines().join("\n");
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        app.textarea = crate::app::build_textarea(false);
                        if text.starts_with('/') {
                            // 命令模式：取出 registry 避免借用冲突
                            let registry = std::mem::take(&mut app.command_registry);
                            let known = registry.dispatch(app, &text);
                            app.command_registry = registry;
                            if !known {
                                app.view_messages.push(MessageViewModel::system(format!(
                                    "未知命令: {}  （输入 /help 查看可用命令）",
                                    text
                                )));
                            }
                        } else {
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

                // 拦截普通 Enter，避免 textarea 默认换行
                input if !app.loading && input.key != Key::Enter => {
                    app.textarea.input(input);
                    // 输入内容变化时重置提示光标
                    app.hint_cursor = None;
                }

                _ => {}
            }
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
            // Esc 直接新建（跳过历史选择）
            app.new_thread();
        }
        Input { key: Key::Up, .. }
        | Input {
            key: Key::Char('k'),
            ..
        } => {
            if let Some(b) = app.thread_browser.as_mut() {
                b.move_cursor(-1);
            }
        }
        Input { key: Key::Down, .. }
        | Input {
            key: Key::Char('j'),
            ..
        } => {
            if let Some(b) = app.thread_browser.as_mut() {
                b.move_cursor(1);
            }
        }
        Input {
            key: Key::Enter, ..
        } => {
            if let Some(b) = app.thread_browser.as_mut() {
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
            if let Some(b) = app.thread_browser.as_mut() {
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
    use crate::app::model_panel::EditField;

    let Some(panel) = app.model_panel.as_mut() else {
        return;
    };

    match panel.mode.clone() {
        ModelPanelMode::Browse => match input {
            Input {
                key: Key::Char('c'),
                ctrl: true,
                ..
            } => {}
            Input { key: Key::Esc, .. } => {
                app.close_model_panel();
            }
            Input { key: Key::Up, .. }
            | Input {
                key: Key::Char('k'),
                ..
            } => {
                app.model_panel.as_mut().unwrap().move_cursor(-1);
            }
            Input { key: Key::Down, .. }
            | Input {
                key: Key::Char('j'),
                ..
            } => {
                app.model_panel.as_mut().unwrap().move_cursor(1);
            }
            Input {
                key: Key::Enter, ..
            } => {
                app.model_panel_confirm_select();
            }
            Input {
                key: Key::Char('e'),
                ..
            } => {
                app.model_panel.as_mut().unwrap().enter_edit();
            }
            Input {
                key: Key::Char('n'),
                ..
            } => {
                app.model_panel.as_mut().unwrap().enter_new();
            }
            Input {
                key: Key::Char('d'),
                ..
            } => {
                app.model_panel.as_mut().unwrap().request_delete();
            }
            _ => {}
        },
        ModelPanelMode::Edit | ModelPanelMode::New => match input {
            Input { key: Key::Esc, .. } => {
                app.model_panel.as_mut().unwrap().mode = ModelPanelMode::Browse;
            }
            Input {
                key: Key::Tab,
                shift: false,
                ..
            } => {
                app.model_panel.as_mut().unwrap().field_next();
            }
            Input {
                key: Key::Tab,
                shift: true,
                ..
            } => {
                app.model_panel.as_mut().unwrap().field_prev();
            }
            Input {
                key: Key::Char(' '),
                ..
            } => {
                let field = app.model_panel.as_ref().unwrap().edit_field.clone();
                if field == EditField::ProviderType {
                    app.model_panel.as_mut().unwrap().cycle_type();
                } else if field == EditField::ThinkingBudget {
                    app.model_panel.as_mut().unwrap().toggle_thinking();
                } else {
                    app.model_panel.as_mut().unwrap().push_char(' ');
                }
            }
            Input {
                key: Key::Enter, ..
            } => {
                app.model_panel_apply_edit();
            }
            Input {
                key: Key::Backspace,
                ..
            } => {
                app.model_panel.as_mut().unwrap().pop_char();
            }
            Input {
                key: Key::Char(c),
                ctrl: false,
                alt: false,
                ..
            } => {
                app.model_panel.as_mut().unwrap().push_char(c);
            }
            _ => {}
        },
        ModelPanelMode::ConfirmDelete => match input {
            Input {
                key: Key::Char('y'),
                ..
            } => {
                app.model_panel_confirm_delete();
            }
            Input {
                key: Key::Char('n'),
                ..
            }
            | Input { key: Key::Esc, .. } => {
                app.model_panel.as_mut().unwrap().cancel_delete();
            }
            _ => {}
        },
    }
}
