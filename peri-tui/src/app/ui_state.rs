use tui_textarea::TextArea;

use crate::app::text_selection::{PanelTextSelection, TextSelection};

/// UI 交互状态：会话级的输入、滚动、选区、历史等。
pub struct UiState {
    pub textarea: TextArea<'static>,
    pub loading: bool,
    pub scroll_offset: u16,
    pub scroll_follow: bool,
    pub show_tool_messages: bool,
    pub hint_cursor: Option<usize>,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub draft_input: Option<String>,
    pub text_selection: TextSelection,
    pub messages_area: Option<ratatui::layout::Rect>,
    pub textarea_area: Option<ratatui::layout::Rect>,
    pub copy_message_until: Option<std::time::Instant>,
    pub copy_char_count: usize,
    pub panel_selection: PanelTextSelection,
    pub panel_area: Option<ratatui::layout::Rect>,
    pub panel_plain_lines: Vec<String>,
    pub panel_scroll_offset: u16,
    /// 用户是否正在拖拽消息区域右侧滚动条
    pub scrollbar_dragging: bool,
    /// 消息区域滚动条的最大偏移量（内容高度 - 可见高度）
    pub scrollbar_max_offset: u16,
}

impl UiState {
    pub fn new(textarea: TextArea<'static>) -> Self {
        Self {
            textarea,
            loading: false,
            scroll_offset: u16::MAX,
            scroll_follow: true,
            show_tool_messages: false,
            hint_cursor: None,
            input_history: Vec::new(),
            history_index: None,
            draft_input: None,
            text_selection: TextSelection::new(),
            messages_area: None,
            textarea_area: None,
            copy_message_until: None,
            copy_char_count: 0,
            panel_selection: PanelTextSelection::new(),
            panel_area: None,
            panel_plain_lines: Vec::new(),
            panel_scroll_offset: 0,
            scrollbar_dragging: false,
            scrollbar_max_offset: 0,
        }
    }
}
