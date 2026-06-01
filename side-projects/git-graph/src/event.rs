use crate::app::{
    App, ConfirmAction, Focus, InputAction, InputDialog, Overlay, StatusSubPanel, ToastStyle,
};
use crate::git::remote::{self, RemoteOp, RemoteResult};
use crate::ui::sidebar::status_panel;
use crate::ui::toolbar::{GlobalAction, ToolbarAction};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

/// 打开文件编辑器（替代旧的 preview_file 逻辑）
fn open_file_in_editor(app: &mut App, relative_path: &str) {
    if let Some(wd) = app.repo.repo().workdir() {
        let abs_path = wd.join(relative_path);
        match crate::editor::TextEditor::open(abs_path) {
            Ok(ed) => {
                app.editor = Some(ed);
                app.preview_file = None;
            }
            Err(e) => {
                app.show_toast(format!("无法打开文件: {}", e), ToastStyle::Error);
            }
        }
    }
}

pub fn handle_event(app: &mut App, event: crossterm::event::Event) -> anyhow::Result<()> {
    match event {
        crossterm::event::Event::Key(key) => handle_key(app, key.code, key.modifiers),
        crossterm::event::Event::Mouse(mouse) => handle_mouse(app, mouse),
        _ => {}
    }
    Ok(())
}

fn handle_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    // 编辑器激活时，优先路由到编辑器
    if let Some(ref mut editor) = app.editor {
        let consumed = crate::editor::input::handle_key(editor, code, mods);
        if consumed {
            let area = app.editor_area;
            let gutter_w = crate::editor::render::gutter_width(editor.line_count());
            // content_width = area.width - gutter - separator - scrollbar
            let content_w = area.width.saturating_sub(gutter_w + 1 + 1) as usize;
            editor.scroll_to_cursor(area.height.saturating_sub(1) as usize); // -1 status bar
            editor.scroll_to_cursor_x(content_w);
            return;
        }
        // Esc 或其他未消费键 → 关闭编辑器
        if code == KeyCode::Esc {
            app.editor = None;
            return;
        }
        return; // 编辑器激活时忽略其他键
    }

    // 确认弹窗优先拦截
    if app.confirm_message.is_some() {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                execute_confirm_action(app);
                app.confirm_message = None;
                app.confirm_action = None;
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                // PullRebase 时 n = 用 merge；其他操作 n = 取消
                if let Some(ConfirmAction::PullRebase) = &app.confirm_action {
                    spawn_remote(app, RemoteOp::Pull, None);
                }
                app.confirm_message = None;
                app.confirm_action = None;
            }
            KeyCode::Esc => {
                app.confirm_message = None;
                app.confirm_action = None;
            }
            _ => {}
        }
        return;
    }

    // InputDialog 弹窗输入模式（tag / branch）
    if app.overlay == Overlay::InputDialog {
        match code {
            KeyCode::Esc => {
                app.input_dialog = None;
                app.overlay = Overlay::None;
            }
            KeyCode::Enter => {
                if let Some(dialog) = app.input_dialog.take() {
                    if !dialog.value.is_empty() {
                        if let Some(oid) = app.selected_oid {
                            let result = match dialog.action {
                                InputAction::CreateTag => app.repo.create_tag(oid, &dialog.value),
                                InputAction::CreateBranch => {
                                    app.repo.create_branch(oid, &dialog.value)
                                }
                            };
                            match result {
                                Ok(()) => {
                                    let label = match dialog.action {
                                        InputAction::CreateTag => "标签",
                                        InputAction::CreateBranch => "分支",
                                    };
                                    app.show_toast(
                                        format!("已创建{} {}", label, dialog.value),
                                        ToastStyle::Success,
                                    );
                                    let _ = app.reload();
                                }
                                Err(e) => {
                                    app.show_toast(format!("创建失败: {}", e), ToastStyle::Error);
                                }
                            }
                        }
                    }
                }
                app.input_dialog = None;
                app.overlay = Overlay::None;
            }
            KeyCode::Left => {
                if let Some(dialog) = &mut app.input_dialog {
                    dialog.cursor_pos = dialog.cursor_pos.saturating_sub(1);
                }
            }
            KeyCode::Right => {
                if let Some(dialog) = &mut app.input_dialog {
                    dialog.cursor_pos = dialog
                        .cursor_pos
                        .min(dialog.value.chars().count().saturating_sub(1));
                }
            }
            KeyCode::Home => {
                if let Some(dialog) = &mut app.input_dialog {
                    dialog.cursor_pos = 0;
                }
            }
            KeyCode::End => {
                if let Some(dialog) = &mut app.input_dialog {
                    dialog.cursor_pos = dialog.value.chars().count();
                }
            }
            KeyCode::Backspace => {
                if let Some(dialog) = &mut app.input_dialog {
                    if dialog.cursor_pos > 0 {
                        let pos = dialog.cursor_pos;
                        let byte_pos = dialog.value.char_indices().nth(pos - 1).map(|(i, _)| i);
                        let byte_next = dialog
                            .value
                            .char_indices()
                            .nth(pos)
                            .map(|(i, _)| i)
                            .unwrap_or(dialog.value.len());
                        if let Some(bp) = byte_pos {
                            dialog.value.drain(bp..byte_next);
                            dialog.cursor_pos -= 1;
                        }
                    }
                }
            }
            KeyCode::Delete => {
                if let Some(dialog) = &mut app.input_dialog {
                    let pos = dialog.cursor_pos;
                    let len = dialog.value.chars().count();
                    if pos < len {
                        let byte_pos = dialog
                            .value
                            .char_indices()
                            .nth(pos)
                            .map(|(i, _)| i)
                            .unwrap_or(dialog.value.len());
                        let byte_next = dialog
                            .value
                            .char_indices()
                            .nth(pos + 1)
                            .map(|(i, _)| i)
                            .unwrap_or(dialog.value.len());
                        dialog.value.drain(byte_pos..byte_next);
                    }
                }
            }
            KeyCode::Char(c) => {
                if let Some(dialog) = &mut app.input_dialog {
                    let byte_pos = dialog
                        .value
                        .char_indices()
                        .nth(dialog.cursor_pos)
                        .map(|(i, _)| i)
                        .unwrap_or(dialog.value.len());
                    dialog.value.insert(byte_pos, c);
                    dialog.cursor_pos += 1;
                }
            }
            _ => {}
        }
        return;
    }

    // SearchBar 输入模式
    if app.overlay == Overlay::SearchBar {
        match code {
            KeyCode::Esc => {
                app.search_query = None;
                app.overlay = Overlay::None;
            }
            KeyCode::Enter => {
                // 跳转到第一个匹配的 commit（hash 前缀或 message 子串）
                if let Some(query) = &app.search_query {
                    let q = query.to_ascii_lowercase();
                    let found = app.layout.rows.iter().position(|r| {
                        if let Some(oid) = r.oid {
                            let hash = format!("{:.7}", oid).to_ascii_lowercase();
                            if hash.starts_with(&q) {
                                return true;
                            }
                            // 尝试加载 commit message 进行匹配
                            if let Ok(detail) = app.repo.commit_detail(oid) {
                                if detail.message.to_ascii_lowercase().contains(&q) {
                                    return true;
                                }
                            }
                        }
                        false
                    });
                    if let Some(idx) = found {
                        app.select(idx);
                        ensure_selected_visible(app);
                    }
                }
                app.overlay = Overlay::None;
            }
            KeyCode::Backspace => {
                if let Some(query) = &mut app.search_query {
                    query.pop();
                    if query.is_empty() {
                        app.search_query = None;
                    }
                }
            }
            KeyCode::Char(c) => {
                app.search_query.get_or_insert_with(String::new).push(c);
            }
            _ => {}
        }
        return;
    }

    // FileSearch 文件搜索弹窗
    if app.overlay == Overlay::FileSearch {
        match code {
            KeyCode::Esc => {
                app.file_search_query = None;
                app.file_search_cursor = 0;
                app.file_search_results.clear();
                app.file_search_selected = 0;
                app.overlay = Overlay::None;
            }
            KeyCode::Enter => {
                if let Some(idx) = app.file_search_results.get(app.file_search_selected) {
                    if let Some(path) = app.all_tracked_files.get(*idx).cloned() {
                        open_file_in_editor(app, &path);
                    }
                }
                app.file_search_query = None;
                app.file_search_cursor = 0;
                app.file_search_results.clear();
                app.file_search_selected = 0;
                app.overlay = Overlay::None;
            }
            KeyCode::Up => {
                app.file_search_selected = app.file_search_selected.saturating_sub(1);
            }
            KeyCode::Down => {
                let max = app.file_search_results.len().saturating_sub(1);
                app.file_search_selected = (app.file_search_selected + 1).min(max);
            }
            KeyCode::Left => {
                app.file_search_cursor = app.file_search_cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                if let Some(q) = &app.file_search_query {
                    app.file_search_cursor = app.file_search_cursor.min(q.chars().count());
                }
            }
            KeyCode::Home => {
                app.file_search_cursor = 0;
            }
            KeyCode::End => {
                if let Some(q) = &app.file_search_query {
                    app.file_search_cursor = q.chars().count();
                }
            }
            KeyCode::Backspace => {
                if let Some(query) = &mut app.file_search_query {
                    if app.file_search_cursor > 0 {
                        let pos = app.file_search_cursor;
                        let bp = query.char_indices().nth(pos - 1).map(|(i, _)| i);
                        let bn = query
                            .char_indices()
                            .nth(pos)
                            .map(|(i, _)| i)
                            .unwrap_or(query.len());
                        if let Some(b) = bp {
                            query.drain(b..bn);
                            app.file_search_cursor -= 1;
                        }
                    }
                    if query.is_empty() {
                        app.file_search_query = None;
                    }
                }
                app.update_file_search_results();
            }
            KeyCode::Delete => {
                if let Some(query) = &mut app.file_search_query {
                    let pos = app.file_search_cursor;
                    let len = query.chars().count();
                    if pos < len {
                        let bp = query
                            .char_indices()
                            .nth(pos)
                            .map(|(i, _)| i)
                            .unwrap_or(query.len());
                        let bn = query
                            .char_indices()
                            .nth(pos + 1)
                            .map(|(i, _)| i)
                            .unwrap_or(query.len());
                        query.drain(bp..bn);
                    }
                }
                app.update_file_search_results();
            }
            KeyCode::Char(c) => {
                let query = app.file_search_query.get_or_insert_with(String::new);
                let bp = query
                    .char_indices()
                    .nth(app.file_search_cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(query.len());
                query.insert(bp, c);
                app.file_search_cursor += 1;
                app.update_file_search_results();
            }
            _ => {}
        }
        return;
    }

    // overlay 打开时处理
    if app.overlay == Overlay::BranchList
        || app.overlay == Overlay::TagList
        || app.overlay == Overlay::StashList
    {
        match code {
            KeyCode::Esc => {
                app.overlay = Overlay::None;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let max = match app.overlay {
                    Overlay::BranchList => app.repo.branch_names().unwrap_or_default().len(),
                    Overlay::TagList => app.repo.tag_names_list().unwrap_or_default().len(),
                    Overlay::StashList => app.stash_map.values().flatten().count(),
                    _ => 0,
                };
                if max > 0 {
                    app.overlay_selected = (app.overlay_selected + 1).min(max - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.overlay_selected = app.overlay_selected.saturating_sub(1);
            }
            // TagList 专属操作
            KeyCode::Char('d') if app.overlay == Overlay::TagList => {
                let tags = app.repo.tag_names_list().unwrap_or_default();
                if let Some(tag) = tags.get(app.overlay_selected) {
                    let name = tag.clone();
                    app.confirm_message = Some(format!("是否删除标签 '{}'？", name));
                    app.confirm_action = Some(ConfirmAction::DeleteTag(name));
                    app.overlay = Overlay::ConfirmDialog;
                }
            }
            KeyCode::Char('p') if app.overlay == Overlay::TagList => {
                let tags = app.repo.tag_names_list().unwrap_or_default();
                if let Some(tag) = tags.get(app.overlay_selected) {
                    let name = tag.clone();
                    match app.repo.push_tag(&name) {
                        Ok(()) => {
                            app.show_toast(format!("已推送标签 {}", name), ToastStyle::Success);
                        }
                        Err(e) => {
                            app.show_toast(format!("推送标签失败: {}", e), ToastStyle::Error);
                        }
                    }
                    app.overlay = Overlay::None;
                }
            }
            _ => {}
        }
        return;
    }

    // Ctrl+u / Ctrl+d / Ctrl+C
    if mods.contains(KeyModifiers::CONTROL) {
        match code {
            KeyCode::Char('c') => {
                app.quit();
                return;
            }
            KeyCode::Char('u') => {
                let delta = app.viewport_height.min(3);
                if app.selected_idx >= delta {
                    app.select(app.selected_idx - delta);
                }
                ensure_selected_visible(app);
                return;
            }
            KeyCode::Char('d') => {
                let delta = app.viewport_height.min(3);
                let new_idx =
                    (app.selected_idx + delta).min(app.layout.rows.len().saturating_sub(1));
                app.select(new_idx);
                ensure_selected_visible(app);
                return;
            }
            _ => {}
        }
    }

    // Sidebar 焦点处理
    match app.focus {
        Focus::FileTree => match code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.file_tree_state.move_cursor(-1);
                return;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.file_tree_state.move_cursor(1);
                return;
            }
            KeyCode::Enter => {
                let idx = app.file_tree_state.cursor();
                if let Some(result) = app.file_tree_state.toggle(idx) {
                    if result.needs_load {
                        let children = crate::app::scan_dir_children(&result.path);
                        app.file_tree_state.set_children(&result.path, children);
                        app.file_tree_state.toggle(idx);
                    }
                }
                return;
            }
            _ => {}
        },
        Focus::Status => {
            match code {
                KeyCode::Up | KeyCode::Char('k') => {
                    let files = match app.status_sub_panel {
                        StatusSubPanel::Staged => &app.staged_visible_files,
                        StatusSubPanel::Changes => &app.changes_visible_files,
                    };
                    if !files.is_empty() && app.status_file_index > 0 {
                        app.status_file_index -= 1;
                    }
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let files = match app.status_sub_panel {
                        StatusSubPanel::Staged => &app.staged_visible_files,
                        StatusSubPanel::Changes => &app.changes_visible_files,
                    };
                    if !files.is_empty() && app.status_file_index + 1 < files.len() {
                        app.status_file_index += 1;
                    }
                    return;
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    // 切换到 Staged 子面板
                    app.status_sub_panel = StatusSubPanel::Staged;
                    if app.status_file_index >= app.staged_visible_files.len() {
                        app.status_file_index = app.staged_visible_files.len().saturating_sub(1);
                    }
                    return;
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    // 切换到 Changes 子面板
                    app.status_sub_panel = StatusSubPanel::Changes;
                    if app.status_file_index >= app.changes_visible_files.len() {
                        app.status_file_index = app.changes_visible_files.len().saturating_sub(1);
                    }
                    return;
                }
                KeyCode::Enter => {
                    // 选中文件打开编辑器
                    let files = match app.status_sub_panel {
                        StatusSubPanel::Staged => &app.staged_visible_files,
                        StatusSubPanel::Changes => &app.changes_visible_files,
                    };
                    if let Some(path) = files.get(app.status_file_index).cloned() {
                        open_file_in_editor(app, &path);
                    }
                    return;
                }
                _ => {
                    if !matches!(code, KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab) {
                        return;
                    }
                }
            }
        }
        _ => {}
    }

    match code {
        KeyCode::Tab => {
            app.focus = match app.focus {
                Focus::FileTree => Focus::Status,
                Focus::Status => Focus::Graph,
                Focus::Graph => Focus::Detail,
                Focus::Detail => Focus::FileTree,
            };
        }
        KeyCode::BackTab => {
            app.focus = match app.focus {
                Focus::FileTree => Focus::Detail,
                Focus::Status => Focus::FileTree,
                Focus::Graph => Focus::Status,
                Focus::Detail => Focus::Graph,
            };
        }
        KeyCode::Char('q') if !mods.contains(KeyModifiers::CONTROL) => app.quit(),
        KeyCode::Char('m') if !mods.contains(KeyModifiers::CONTROL) => {
            app.mouse_enabled = !app.mouse_enabled;
            use crate::app::ToastStyle;
            if app.mouse_enabled {
                app.show_toast("鼠标已启用（滚动/点击）".to_string(), ToastStyle::Info);
            } else {
                app.show_toast(
                    "鼠标已禁用（终端选择复制可用）".to_string(),
                    ToastStyle::Info,
                );
            }
        }
        KeyCode::Char('b') => {
            app.overlay = Overlay::BranchList;
        }
        KeyCode::Char('t') => {
            app.overlay = Overlay::TagList;
        }
        KeyCode::Char('s') => {
            app.overlay = Overlay::StashList;
        }
        KeyCode::Char('/') => {
            app.overlay = Overlay::SearchBar;
            app.search_query = Some(String::new());
        }
        KeyCode::Char('p') if mods.contains(KeyModifiers::CONTROL) => {
            app.file_search_query = Some(String::new());
            app.file_search_cursor = 0;
            app.file_search_selected = 0;
            app.file_search_results.clear();
            if app.all_tracked_files.is_empty() {
                match app.repo.list_all_files() {
                    Ok(files) => {
                        app.all_tracked_files_lower =
                            files.iter().map(|f| f.to_ascii_lowercase()).collect();
                        app.all_tracked_files = files;
                    }
                    Err(e) => {
                        app.show_toast(format!("文件扫描失败: {}", e), ToastStyle::Error);
                    }
                }
            }
            app.update_file_search_results();
            app.overlay = Overlay::FileSearch;
        }
        KeyCode::Char('f') if !mods.contains(KeyModifiers::CONTROL) => {
            spawn_remote(app, RemoteOp::Fetch, None);
        }
        KeyCode::Char('P') => {
            spawn_remote(app, RemoteOp::Pull, None);
        }
        KeyCode::Char('p') if !mods.contains(KeyModifiers::CONTROL) => {
            spawn_remote(app, RemoteOp::Push, None);
        }
        KeyCode::Up | KeyCode::Char('k') if app.selected_idx > 0 => {
            app.select(app.selected_idx - 1);
            ensure_selected_visible(app);
        }
        KeyCode::Down | KeyCode::Char('j')
            if !app.layout.rows.is_empty() && app.selected_idx < app.layout.rows.len() - 1 =>
        {
            app.select(app.selected_idx + 1);
            ensure_selected_visible(app);
        }
        KeyCode::Enter if app.overlay == Overlay::None => {
            app.focus = Focus::Detail;
        }
        KeyCode::Esc => {
            if app.preview_file.is_some() {
                app.preview_file = None;
                app.focus = Focus::Status;
                return;
            }
            if app.overlay != Overlay::None {
                app.overlay = Overlay::None;
            } else if app.focus != Focus::Graph {
                app.focus = Focus::Graph;
            }
        }
        _ => {}
    }
}

fn handle_mouse(app: &mut App, mouse: MouseEvent) {
    // 编辑器激活时，鼠标事件优先路由到编辑器
    if let Some(ref mut editor) = app.editor {
        let gutter_w = crate::editor::render::gutter_width(editor.line_count());
        let consumed = crate::editor::input::handle_mouse(editor, mouse, app.editor_area, gutter_w);
        if consumed {
            if matches!(mouse.kind, MouseEventKind::Drag(_)) {
                let area = app.editor_area;
                let content_w = area.width.saturating_sub(gutter_w + 1 + 1) as usize;
                editor.scroll_to_cursor(area.height.saturating_sub(1) as usize);
                editor.scroll_to_cursor_x(content_w);
            }
            return;
        }
    }

    // 确认弹窗优先处理：检测 [Y]es / [N]o 按钮点击
    if app.confirm_message.is_some() {
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            let area = app.frame_area;
            let popup_width = 50u16.min(area.width);
            let popup_height = 5u16;
            let px = (area.width.saturating_sub(popup_width)) / 2;
            let py = (area.height.saturating_sub(popup_height)) / 2;
            // inner 区域 = popup 去掉边框（各 1 列/行）
            let inner_x = px + 1;
            let inner_y = py + 1;
            // 按钮在第 3 行（0-indexed）= inner_y + 2
            let btn_row = inner_y + 2;
            if mouse.row == btn_row {
                // 居中对齐：计算行内容宽度
                // " [Y]es " (7) + "  " (2) + " [N]o " (6) = 15
                let content_width = 15u16;
                let inner_w = popup_width.saturating_sub(2);
                let line_start = inner_x + (inner_w.saturating_sub(content_width)) / 2;
                let yes_start = line_start;
                let yes_end = yes_start + 7; // " [Y]es "
                let no_start = yes_end + 2; // "  " 分隔
                let no_end = no_start + 6; // " [N]o "
                if mouse.column >= yes_start && mouse.column < yes_end {
                    execute_confirm_action(app);
                    app.confirm_message = None;
                    app.confirm_action = None;
                    return;
                }
                if mouse.column >= no_start && mouse.column < no_end {
                    // No: PullRebase 时 n = merge，其他取消
                    if let Some(ConfirmAction::PullRebase) = &app.confirm_action {
                        spawn_remote(app, RemoteOp::Pull, None);
                    }
                    app.confirm_message = None;
                    app.confirm_action = None;
                    return;
                }
            }
            // 点击弹窗外部区域：关闭弹窗
            let in_popup = mouse.column >= px
                && mouse.column < px + popup_width
                && mouse.row >= py
                && mouse.row < py + popup_height;
            if !in_popup {
                app.confirm_message = None;
                app.confirm_action = None;
                app.overlay = Overlay::None;
            }
        }
        return;
    }

    // overlay 打开时，拦截鼠标事件防止穿透到底层面板
    if app.overlay != Overlay::None {
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            app.overlay = Overlay::None;
        }
        return;
    }

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            scroll_panel(app, mouse.column, mouse.row, ScrollDirection::Up);
        }
        MouseEventKind::ScrollDown => {
            scroll_panel(app, mouse.column, mouse.row, ScrollDirection::Down);
        }
        MouseEventKind::Down(MouseButton::Left) => {
            let mods = mouse.modifiers;

            // Ctrl+左键：在 sidebar 区域复制文件路径
            if mods.contains(KeyModifiers::CONTROL) {
                let sa = app.sidebar_area;
                if mouse.column >= sa.x
                    && mouse.column < sa.x + sa.width
                    && mouse.row >= sa.y
                    && mouse.row < sa.y + sa.height
                {
                    let sl = &app.sidebar_layout;
                    let panels: [(ratatui::layout::Rect, &Option<status_panel::PanelLayout>); 2] = [
                        (sl.staged_inner, &sl.staged_layout),
                        (sl.changes_inner, &sl.changes_layout),
                    ];
                    for (inner, panel_layout) in &panels {
                        if mouse.row < inner.y || mouse.row >= inner.y + inner.height {
                            continue;
                        }
                        let rel_row = mouse.row.saturating_sub(inner.y);
                        if let Some(ref layout) = panel_layout {
                            for &(row, ref path, _is_dir) in &layout.path_rows {
                                if rel_row == row {
                                    let path = path.clone();
                                    let shift = mods.contains(KeyModifiers::SHIFT);
                                    copy_path_to_clipboard(app, &path, shift);
                                    return;
                                }
                            }
                        }
                    }
                }
                // Ctrl+点击不在 sidebar 路径行上，继续走正常逻辑
            }

            // 普通左键点击
            // 先检查全局工具栏
            if let Some(idx) = app.global_toolbar_state.hit_test(mouse.column, mouse.row) {
                let buttons = crate::ui::toolbar::global_buttons();
                if let Some(btn) = buttons.get(idx) {
                    handle_global_action(app, btn.action);
                }
                return;
            }

            // 检查 commit 工具栏点击
            if let Some(idx) = app.toolbar_state.hit_test(mouse.column, mouse.row) {
                let buttons = crate::ui::toolbar::commit_buttons(app);
                if let Some(btn) = buttons.get(idx) {
                    handle_toolbar_action(app, btn.action);
                }
                return;
            }
            // 检查 sidebar 区域点击（Staged / Changes 双面板）
            let sa = app.sidebar_area;
            if mouse.column >= sa.x
                && mouse.column < sa.x + sa.width
                && mouse.row >= sa.y
                && mouse.row < sa.y + sa.height
            {
                app.focus = Focus::Status;
                let sl = &app.sidebar_layout;
                let panels: [(
                    ratatui::layout::Rect,
                    &Option<status_panel::PanelLayout>,
                    bool,
                ); 2] = [
                    (sl.staged_inner, &sl.staged_layout, true),
                    (sl.changes_inner, &sl.changes_layout, false),
                ];

                for (inner, panel_layout, is_staged) in &panels {
                    if mouse.row < inner.y || mouse.row >= inner.y + inner.height {
                        continue;
                    }
                    let rel_row = mouse.row.saturating_sub(inner.y);
                    if let Some(ref layout) = panel_layout {
                        // 按钮点击检测
                        for &(btn_row, btn_x, btn_type, ref path) in &layout.button_rows {
                            if rel_row == btn_row {
                                let abs_btn_x = inner.x + btn_x;
                                if mouse.column >= abs_btn_x && mouse.column < abs_btn_x + 2 {
                                    let git_path = path.as_str();
                                    match btn_type {
                                        status_panel::StatusButton::Stage => {
                                            if let Err(e) = app.repo.stage_file(git_path) {
                                                app.show_toast(
                                                    format!("暂存失败: {}", e),
                                                    ToastStyle::Error,
                                                );
                                            } else {
                                                let _ = app.reload();
                                            }
                                        }
                                        status_panel::StatusButton::Unstage => {
                                            if let Err(e) = app.repo.unstage_file(git_path) {
                                                app.show_toast(
                                                    format!("取消暂存失败: {}", e),
                                                    ToastStyle::Error,
                                                );
                                            } else {
                                                let _ = app.reload();
                                            }
                                        }
                                        status_panel::StatusButton::Delete => {
                                            if git_path.ends_with('/') {
                                                // 目录：丢弃变更而非删除目录
                                                if let Err(e) =
                                                    app.repo.discard_dir_changes(git_path)
                                                {
                                                    app.show_toast(
                                                        format!("丢弃变更失败: {}", e),
                                                        ToastStyle::Error,
                                                    );
                                                } else {
                                                    app.show_toast(
                                                        format!("已丢弃 {} 的变更", git_path),
                                                        ToastStyle::Success,
                                                    );
                                                    let _ = app.reload();
                                                }
                                            } else {
                                                // 文件：先尝试 git rm（已跟踪），失败则直接删除（未跟踪）
                                                if app.repo.delete_tracked_file(git_path).is_err() {
                                                    if let Err(e) =
                                                        app.repo.delete_untracked_file(git_path)
                                                    {
                                                        app.show_toast(
                                                            format!("删除失败: {}", e),
                                                            ToastStyle::Error,
                                                        );
                                                    } else {
                                                        app.show_toast(
                                                            format!("已删除 {}", git_path),
                                                            ToastStyle::Success,
                                                        );
                                                        let _ = app.reload();
                                                    }
                                                } else {
                                                    app.show_toast(
                                                        format!("已删除 {}", git_path),
                                                        ToastStyle::Success,
                                                    );
                                                    let _ = app.reload();
                                                }
                                            }
                                        }
                                    }
                                    app.dirty = true;
                                    return;
                                }
                            }
                        }
                        // 目录行点击
                        for &(dir_row, ref key) in &layout.dir_rows {
                            if rel_row == dir_row {
                                if app.status_dir_collapsed.contains(key) {
                                    app.status_dir_collapsed.remove(key);
                                } else {
                                    app.status_dir_collapsed.insert(key.clone());
                                }
                            }
                        }
                        // 文件行点击 → 预览
                        for &(path_row, ref path, is_dir) in &layout.path_rows {
                            if !is_dir && rel_row == path_row {
                                app.status_sub_panel = if *is_staged {
                                    StatusSubPanel::Staged
                                } else {
                                    StatusSubPanel::Changes
                                };
                                // 更新光标索引
                                let files = if *is_staged {
                                    &app.staged_visible_files
                                } else {
                                    &app.changes_visible_files
                                };
                                if let Some(idx) = files.iter().position(|f| f == path) {
                                    app.status_file_index = idx;
                                }
                                let path_owned = path.clone();
                                open_file_in_editor(app, &path_owned);
                                app.dirty = true;
                                return;
                            }
                        }
                        return;
                    }
                }
                return;
            }
            // 检查是否在 graph 面板区域内
            let ga = app.graph_area;
            if mouse.column >= ga.x
                && mouse.column < ga.x + ga.width
                && mouse.row >= ga.y
                && mouse.row < ga.y + ga.height
            {
                let offset_y = app.graph_inner_y;
                if mouse.row >= offset_y {
                    let row = (mouse.row - offset_y) as usize;
                    let target_idx = app.scroll_offset + row;
                    if target_idx < app.layout.rows.len() {
                        let graph_row = &app.layout.rows[target_idx];
                        // cells 区域结束列（每 cell 占 2 列 + 左边框 1）
                        let cells_end = ga.x + 1 + graph_row.cells.len() as u16 * 2;
                        // 精确检测 badge 点击：cells_end + 1(空格) 起，每个 badge = " branch " + " "(分隔)
                        let mut badge_x = cells_end + 1;
                        for branch in &graph_row.branches {
                            let badge_width = branch.len() as u16 + 2; // " " + branch + " "
                            if mouse.column >= badge_x && mouse.column < badge_x + badge_width {
                                let b = branch.clone();
                                app.confirm_message = Some(format!("是否 checkout 到 '{}'？", b));
                                app.confirm_action = Some(ConfirmAction::CheckoutBranch(b));
                                app.overlay = Overlay::ConfirmDialog;
                                break;
                            }
                            badge_x += badge_width + 1; // +1 分隔空格
                        }
                        app.select(target_idx);
                        ensure_selected_visible(app);
                    }
                }
            }
        }
        _ => {}
    }
}

enum ScrollDirection {
    Up,
    Down,
}

/// 根据鼠标位置判断滚动哪个面板
fn scroll_panel(app: &mut App, col: u16, row: u16, dir: ScrollDirection) {
    let delta = 3;
    let sa = app.sidebar_area;

    // sidebar 区域？
    if col >= sa.x && col < sa.x + sa.width && row >= sa.y && row < sa.y + sa.height {
        if row < app.sidebar_split_y {
            // Staged 面板滚动
            let max = app.staged_total_lines.saturating_sub(app.staged_viewport);
            match dir {
                ScrollDirection::Up => {
                    let new = app.staged_scroll.saturating_sub(delta);
                    if new != app.staged_scroll {
                        app.staged_scroll = new;
                        app.dirty = true;
                    }
                }
                ScrollDirection::Down => {
                    let new = (app.staged_scroll + delta).min(max);
                    if new != app.staged_scroll {
                        app.staged_scroll = new;
                        app.dirty = true;
                    }
                }
            }
        } else {
            // Changes 面板滚动
            let max = app.changes_total_lines.saturating_sub(app.changes_viewport);
            match dir {
                ScrollDirection::Up => {
                    let new = app.changes_scroll.saturating_sub(delta);
                    if new != app.changes_scroll {
                        app.changes_scroll = new;
                        app.dirty = true;
                    }
                }
                ScrollDirection::Down => {
                    let new = (app.changes_scroll + delta).min(max);
                    if new != app.changes_scroll {
                        app.changes_scroll = new;
                        app.dirty = true;
                    }
                }
            }
        }
        app.dirty = true;
        return;
    }

    let ga = app.graph_area;
    if col >= ga.x && col < ga.x + ga.width && row >= ga.y && row < ga.y + ga.height {
        // Graph 面板
        match dir {
            ScrollDirection::Up => {
                let new = app.scroll_offset.saturating_sub(delta as usize);
                if new != app.scroll_offset {
                    app.scroll_offset = new;
                    app.dirty = true;
                }
            }
            ScrollDirection::Down => {
                let max = app.layout.rows.len().saturating_sub(app.viewport_height);
                let new = (app.scroll_offset + delta as usize).min(max);
                if new != app.scroll_offset {
                    app.scroll_offset = new;
                    app.dirty = true;
                }
            }
        }
        return;
    }

    let da = app.detail_area;
    if col >= da.x && col < da.x + da.width && row >= da.y && row < da.y + da.height {
        // Detail 面板
        let max = app.detail_total_lines.saturating_sub(app.detail_viewport);
        match dir {
            ScrollDirection::Up => {
                let new = app.detail_scroll.saturating_sub(delta);
                if new != app.detail_scroll {
                    app.detail_scroll = new;
                    app.dirty = true;
                }
            }
            ScrollDirection::Down => {
                let new = (app.detail_scroll + delta).min(max);
                if new != app.detail_scroll {
                    app.detail_scroll = new;
                    app.dirty = true;
                }
            }
        }
    }
}

fn ensure_selected_visible(app: &mut App) {
    if app.selected_idx < app.scroll_offset {
        app.scroll_offset = app.selected_idx;
    } else if app.selected_idx >= app.scroll_offset + app.viewport_height {
        app.scroll_offset = app.selected_idx - app.viewport_height + 1;
    }
}

fn handle_toolbar_action(app: &mut App, action: ToolbarAction) {
    match action {
        ToolbarAction::Checkout => {
            if let Some(oid) = app.selected_oid {
                if let Err(e) = app.repo.checkout(oid) {
                    app.show_toast(format!("checkout 失败: {}", e), ToastStyle::Error);
                } else {
                    let _ = app.reload();
                }
            }
        }
        ToolbarAction::CreateTag => {
            app.input_dialog = Some(InputDialog {
                title: "Create Tag".to_string(),
                value: String::new(),
                cursor_pos: 0,
                action: InputAction::CreateTag,
            });
            app.overlay = Overlay::InputDialog;
        }
        ToolbarAction::CreateBranch => {
            app.input_dialog = Some(InputDialog {
                title: "Create Branch".to_string(),
                value: String::new(),
                cursor_pos: 0,
                action: InputAction::CreateBranch,
            });
            app.overlay = Overlay::InputDialog;
        }
        ToolbarAction::Merge => {
            if let Some(oid) = app.selected_oid {
                if let Err(e) = app.repo.merge(oid) {
                    app.show_toast(format!("merge 失败: {}", e), ToastStyle::Error);
                } else {
                    let _ = app.reload();
                }
            }
        }
        ToolbarAction::CherryPick => {
            if let Some(oid) = app.selected_oid {
                if let Err(e) = app.repo.cherry_pick(oid) {
                    app.show_toast(format!("cherry-pick 失败: {}", e), ToastStyle::Error);
                } else {
                    let _ = app.reload();
                }
            }
        }
        ToolbarAction::Reset => {
            if let Some(oid) = app.selected_oid {
                app.confirm_message = Some(format!("确认 reset --hard 到 {:.7}?", oid));
                app.confirm_action = Some(ConfirmAction::ResetHard(oid));
                app.overlay = Overlay::ConfirmDialog;
            }
        }
        ToolbarAction::DeleteBranch => {
            if let Some(detail) = &app.selected_detail {
                if let Some(branch) = detail.branches.first() {
                    app.confirm_message = Some(format!("确认删除分支 {}?", branch));
                    app.confirm_action = Some(ConfirmAction::DeleteBranch(branch.clone()));
                    app.overlay = Overlay::ConfirmDialog;
                }
            }
        }
        ToolbarAction::StashPop => {
            if let Some(oid) = app.selected_oid {
                if let Some(stashes) = app.stash_map.get(&oid) {
                    if let Some(stash) = stashes.first() {
                        if let Err(e) = app.repo.stash_pop(stash.index) {
                            app.show_toast(format!("stash pop 失败: {}", e), ToastStyle::Error);
                        } else {
                            let _ = app.reload();
                        }
                    }
                }
            }
        }
        ToolbarAction::StashDrop => {
            if let Some(oid) = app.selected_oid {
                if let Some(stashes) = app.stash_map.get(&oid) {
                    if let Some(stash) = stashes.first() {
                        app.confirm_message = Some(format!("确认删除 stash@{}?", stash.index));
                        app.confirm_action = Some(ConfirmAction::StashDrop(stash.index));
                        app.overlay = Overlay::ConfirmDialog;
                    }
                }
            }
        }
    }
}

fn execute_confirm_action(app: &mut App) {
    let action = app.confirm_action.clone();
    match action {
        Some(ConfirmAction::ResetHard(oid)) => {
            if let Err(e) = app.repo.reset_hard(oid) {
                app.show_toast(format!("reset 失败: {}", e), ToastStyle::Error);
            } else {
                let _ = app.reload();
            }
        }
        Some(ConfirmAction::DeleteBranch(name)) => {
            if let Err(e) = app.repo.delete_branch(&name) {
                app.show_toast(format!("删除分支失败: {}", e), ToastStyle::Error);
            } else {
                let _ = app.reload();
            }
        }
        Some(ConfirmAction::StashDrop(index)) => {
            if let Err(e) = app.repo.stash_drop(index) {
                app.show_toast(format!("stash drop 失败: {}", e), ToastStyle::Error);
            } else {
                let _ = app.reload();
            }
        }
        Some(ConfirmAction::DeleteTag(name)) => {
            if let Err(e) = app.repo.delete_tag(&name) {
                app.show_toast(format!("删除标签失败: {}", e), ToastStyle::Error);
            } else {
                app.show_toast(format!("已删除标签 {}", name), ToastStyle::Success);
                let _ = app.reload();
            }
        }
        Some(ConfirmAction::ForcePush) => {
            // TODO: 实现 force push
        }
        Some(ConfirmAction::PushSetUpstream(branch)) => {
            spawn_remote(app, RemoteOp::PushSetUpstream, Some(branch));
        }
        Some(ConfirmAction::PullRebase) => {
            // y=rebase, 其他=merge
            spawn_remote(app, RemoteOp::PullRebase, None);
        }
        Some(ConfirmAction::CheckoutBranch(branch)) => {
            if let Err(e) = app.repo.checkout_branch(&branch) {
                app.show_toast(format!("checkout 失败: {}", e), ToastStyle::Error);
            } else {
                app.show_toast(format!("已切换到 {}", branch), ToastStyle::Success);
                let _ = app.reload();
            }
        }
        None => {}
    }
    app.overlay = Overlay::None;
}

fn spawn_remote(app: &mut App, op: RemoteOp, branch: Option<String>) {
    let workdir = match app.repo.repo().workdir().map(|p| p.to_path_buf()) {
        Some(d) => d,
        None => {
            app.show_toast("bare 仓库不支持远程操作".to_string(), ToastStyle::Error);
            return;
        }
    };
    app.show_toast(format!("{}ing...", op), ToastStyle::Info);
    let result_rx = app.remote_result_rx.clone();
    let handle = remote::spawn_remote_op(workdir, op, branch);
    std::thread::spawn(move || {
        let result = handle.join().unwrap_or(RemoteResult {
            operation: op,
            success: false,
            message: "thread panicked".to_string(),
        });
        let msg = if result.success {
            format!("{} {}", result.operation, result.message.trim())
        } else {
            format!("{} 失败: {}", result.operation, result.message.trim())
        };
        if let Ok(mut rx) = result_rx.lock() {
            *rx = Some(msg);
        }
    });
}

fn handle_global_action(app: &mut App, action: GlobalAction) {
    match action {
        GlobalAction::RemoteFetch => spawn_remote(app, RemoteOp::Fetch, None),
        GlobalAction::RemotePull => {
            if app.repo.head_branch().is_none() {
                app.show_toast("detached HEAD，无法 pull".to_string(), ToastStyle::Error);
            } else {
                app.confirm_message = Some("Pull 方式？y=rebase, n=merge".to_string());
                app.confirm_action = Some(ConfirmAction::PullRebase);
                app.overlay = Overlay::ConfirmDialog;
            }
        }
        GlobalAction::RemotePush => {
            if app.repo.has_upstream() {
                spawn_remote(app, RemoteOp::Push, None);
            } else if let Some(branch) = app.repo.head_branch() {
                app.confirm_message = Some(format!(
                    "分支 '{}' 没有 upstream，是否 push -u origin {}？",
                    branch, branch
                ));
                app.confirm_action = Some(ConfirmAction::PushSetUpstream(branch));
                app.overlay = Overlay::ConfirmDialog;
            } else {
                app.show_toast("detached HEAD，无法 push".to_string(), ToastStyle::Error);
            }
        }
        GlobalAction::ToggleBranches => {
            app.overlay = Overlay::BranchList;
        }
        GlobalAction::ToggleTags => {
            app.overlay = Overlay::TagList;
        }
        GlobalAction::ToggleStash => {
            app.overlay = Overlay::StashList;
        }
    }
}

/// 复制文件路径到剪贴板并显示 toast
fn copy_path_to_clipboard(app: &mut App, relative_path: &str, use_relative: bool) {
    let path = if use_relative {
        relative_path.to_string()
    } else {
        // 拼接绝对路径
        match app.repo.repo().workdir() {
            Some(wd) => {
                let abs = wd.join(relative_path);
                abs.to_string_lossy().to_string()
            }
            None => relative_path.to_string(),
        }
    };

    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        if let Err(e) = clipboard.set_text(&path) {
            app.show_toast(format!("复制失败: {}", e), ToastStyle::Error);
            return;
        }
        app.show_toast(format!("已复制 {}", path), ToastStyle::Success);
    } else {
        app.show_toast("无法访问剪贴板".to_string(), ToastStyle::Error);
    }
}
