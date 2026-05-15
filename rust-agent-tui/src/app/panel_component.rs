#![allow(dead_code)]

use std::any::Any;

use ratatui::crossterm::event::MouseEvent;
use ratatui::layout::Rect;
use ratatui::Frame;
use tui_textarea::Input;

use super::panel_manager::{EventResult, PanelContext, PanelKind};
use super::App;

/// 面板组件统一行为接口
pub trait PanelComponent: Any {
    /// 获取面板类型
    fn kind(&self) -> PanelKind;

    /// 处理按键事件
    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult;

    /// 处理粘贴事件
    fn handle_paste(&mut self, _text: &str, _ctx: &mut PanelContext<'_>) -> EventResult {
        EventResult::Consumed
    }

    /// 处理滚动事件
    fn handle_scroll(&mut self, _lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        EventResult::NotConsumed
    }

    /// 处理鼠标事件（点击、悬停移动等）
    ///
    /// 默认不消费。面板按需覆写以支持鼠标点击选择等交互。
    /// 鼠标滚轮事件通过 `handle_scroll` 分发，不经过此方法。
    fn handle_mouse(
        &mut self,
        _mouse: MouseEvent,
        _area: Rect,
        _ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        EventResult::NotConsumed
    }

    /// 期望的面板高度
    fn desired_height(&self, screen_height: u16, screen_width: u16) -> u16;

    /// 渲染面板
    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect);

    /// Any downcast（不可变引用）
    fn as_any_ref(&self) -> &dyn Any;

    /// Any downcast（可变引用）
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// 快捷键提示
    fn status_bar_hints(&self, _lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        Vec::new()
    }
}
