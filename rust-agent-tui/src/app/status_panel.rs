use perihelion_widgets::tab_bar::TabState;

/// Status 面板 Tab 索引
pub const STATUS_TAB_COST: usize = 0;
pub const STATUS_TAB_CONTEXT: usize = 1;

/// /cost & /context 共用的只读状态面板
pub struct StatusPanel {
    pub tab: TabState,
    pub scroll_offset: u16,
}

impl StatusPanel {
    /// 创建面板并激活指定 Tab
    pub fn new(active_tab: usize) -> Self {
        let mut tab = TabState::new(vec!["Cost".to_string(), "Context".to_string()]);
        tab.set_active(active_tab);
        Self {
            tab,
            scroll_offset: 0,
        }
    }
}
