use crate::app::{App, Overlay};
use crate::ui::overlay;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();
    app.frame_area = size;

    // 底部分栏：全局工具栏(1行) + toast(1行)
    let bottom_height = if size.height > 4 { 2 } else { 0 };
    let (body_area, bottom_area) = if bottom_height > 0 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(bottom_height)])
            .split(size);
        (chunks[0], Some(chunks[1]))
    } else {
        (size, None)
    };

    // 搜索栏占底部 1 行
    let (content_area, search_area) = if app.overlay == Overlay::SearchBar && body_area.height > 2 {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(body_area);
        (chunks[0], Some(chunks[1]))
    } else {
        (body_area, None)
    };

    // 左右分栏：sidebar(25%) + 右侧(75%)
    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(content_area);
    app.sidebar_layout = crate::ui::sidebar::draw(f, h_chunks[0], app);

    // 右侧：有文件预览时显示预览，否则显示 graph + detail
    if app.preview_file.is_some() {
        // 文件预览占满右侧全部
        crate::ui::file_preview::draw(f, h_chunks[1], app);
        app.detail_area = h_chunks[1];
    } else {
        // graph(65%) + detail(35%)
        let v_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(h_chunks[1]);
        app.detail_area = v_chunks[1];
        crate::ui::graph_panel::draw(f, v_chunks[0], app);
        crate::ui::detail_panel::draw(f, v_chunks[1], app);
    }

    // 底部：全局工具栏 + toast
    if let Some(ba) = bottom_area {
        let toolbar_area = Rect::new(ba.x, ba.y, ba.width, 1);
        let toast_area = Rect::new(ba.x, ba.y + 1, ba.width, 1);
        crate::ui::toolbar::draw_global_toolbar(f, toolbar_area, app);
        crate::ui::toast::draw_toast(f, toast_area, app);
    }

    // 搜索栏
    if let Some(sb_area) = search_area {
        crate::ui::search_bar::draw_search_bar(f, sb_area, app);
    }

    // 确认弹窗
    crate::ui::confirm::draw_confirm(f, size, app);
    // 列表 overlay（覆盖在确认弹窗之上）
    overlay::draw_overlay(f, size, app);
}
