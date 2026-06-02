//! 键盘与鼠标事件处理：将 crossterm 事件转换为 TextEditor 操作。
//!
//! 提供 [`handle_key`]（键盘）和 [`handle_mouse`]（鼠标）两个公共入口，
//! 返回 `bool` 指示事件是否被编辑器消费。

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::TextEditor;

// ── 键盘处理 ──

/// 处理键盘事件，返回 `true` 表示编辑器已消费该事件。
///
/// `Ctrl+S` 保存文件，`Esc` 和未知按键返回 `false` 交给父级处理。
pub fn handle_key(editor: &mut TextEditor, code: KeyCode, mods: KeyModifiers) -> bool {
    // Ctrl 组合键优先判断
    if mods.contains(KeyModifiers::CONTROL) {
        return handle_ctrl(editor, code);
    }

    match code {
        KeyCode::Char(ch) => {
            editor.insert_char(ch);
            true
        }
        KeyCode::Enter => {
            editor.insert_char('\n');
            true
        }
        KeyCode::Backspace => {
            editor.delete_backward();
            true
        }
        KeyCode::Delete => {
            editor.delete_forward();
            true
        }
        KeyCode::Up => {
            editor.move_up();
            true
        }
        KeyCode::Down => {
            editor.move_down();
            true
        }
        KeyCode::Left => {
            editor.move_left();
            true
        }
        KeyCode::Right => {
            editor.move_right();
            true
        }
        KeyCode::Home => {
            editor.move_home();
            true
        }
        KeyCode::End => {
            editor.move_end();
            true
        }
        KeyCode::PageUp => {
            editor.set_scroll_y(editor.scroll_y().saturating_sub(20));
            true
        }
        KeyCode::PageDown => {
            editor.set_scroll_y(editor.scroll_y() + 20); // set_scroll_y 内部钳位
            true
        }
        KeyCode::Esc => false,
        _ => false,
    }
}

/// 处理 Ctrl 组合键。
fn handle_ctrl(editor: &mut TextEditor, code: KeyCode) -> bool {
    match code {
        KeyCode::Char('s') | KeyCode::Char('S') => {
            if let Err(e) = editor.save() {
                eprintln!("保存失败: {e}");
            }
            true
        }
        KeyCode::Char('z') | KeyCode::Char('Z') => {
            editor.undo();
            true
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            editor.redo();
            true
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            editor.select_all();
            true
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            let text = editor.selected_text();
            if !text.is_empty() {
                copy_to_clipboard(&text);
            }
            true
        }
        KeyCode::Char('x') | KeyCode::Char('X') => {
            let text = editor.selected_text();
            if !text.is_empty() {
                copy_to_clipboard(&text);
                editor.delete_selection();
            }
            true
        }
        KeyCode::Char('v') | KeyCode::Char('V') => {
            if let Some(text) = paste_from_clipboard() {
                editor.insert_text(&text);
            }
            true
        }
        _ => false,
    }
}

// ── 鼠标处理 ──

/// 处理鼠标事件，返回 `true` 表示鼠标在编辑器区域内。
///
/// `area` 为编辑器整体布局区域（含 gutter），`gutter_width` 为行号列宽度。
pub fn handle_mouse(editor: &mut TextEditor, mouse: MouseEvent, area: Rect, gutter_w: u16) -> bool {
    // 点击区域判断
    if mouse.column < area.x
        || mouse.column >= area.x + area.width
        || mouse.row < area.y
        || mouse.row >= area.y + area.height
    {
        return false;
    }

    let rel_row = (mouse.row - area.y) as usize;
    // 内容区起始 x（gutter + 分隔符）
    let content_x = area.x + gutter_w + 1;
    // 内容区宽度
    let content_width = area.width.saturating_sub(gutter_w + 1 + 1) as usize;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if mouse.column < content_x {
                // 点击 gutter → 定位到对应视觉行的逻辑行首
                let pos = editor.screen_to_cursor(rel_row, 0, content_width);
                editor.click(pos.line, 0);
            } else {
                let rel_col = mouse.column.saturating_sub(content_x) as usize;
                let pos = editor.screen_to_cursor(rel_row, rel_col, content_width);
                editor.click(pos.line, pos.col);
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if mouse.column < content_x {
                let pos = editor.screen_to_cursor(rel_row, 0, content_width);
                editor.drag(pos.line, 0);
            } else {
                let rel_col = mouse.column.saturating_sub(content_x) as usize;
                let pos = editor.screen_to_cursor(rel_row, rel_col, content_width);
                editor.drag(pos.line, pos.col);
            }
        }
        MouseEventKind::ScrollUp => {
            editor.set_scroll_y(editor.scroll_y().saturating_sub(3));
        }
        MouseEventKind::ScrollDown => {
            editor.set_scroll_y(editor.scroll_y() + 3); // set_scroll_y 内部钳位
        }
        _ => {}
    }

    true
}

// ── 剪贴板辅助 ──

/// 将文本复制到系统剪贴板。
fn copy_to_clipboard(text: &str) {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.set_text(text);
    }
}

/// 从系统剪贴板粘贴文本。
fn paste_from_clipboard() -> Option<String> {
    arboard::Clipboard::new()
        .ok()
        .and_then(|mut cb| cb.get_text().ok())
}
