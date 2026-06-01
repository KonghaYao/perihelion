use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarAction {
    Checkout,
    CreateTag,
    CreateBranch,
    Merge,
    CherryPick,
    Reset,
    DeleteBranch,
    StashPop,
    StashDrop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalAction {
    RemoteFetch,
    RemotePull,
    RemotePush,
    ToggleBranches,
    ToggleTags,
    ToggleStash,
}

pub struct ToolbarButton {
    pub emoji: &'static str,
    pub label: &'static str,
    pub action: ToolbarAction,
    pub dangerous: bool,
}

#[allow(dead_code)]
pub struct GlobalToolbarButton {
    pub emoji: &'static str,
    pub label: &'static str,
    pub shortcut: char,
    pub action: GlobalAction,
}

pub fn global_buttons() -> Vec<GlobalToolbarButton> {
    vec![
        GlobalToolbarButton {
            emoji: "⬇",
            label: "fetch",
            shortcut: 'f',
            action: GlobalAction::RemoteFetch,
        },
        GlobalToolbarButton {
            emoji: "⬆",
            label: "push",
            shortcut: 'p',
            action: GlobalAction::RemotePush,
        },
        GlobalToolbarButton {
            emoji: "⬇⬆",
            label: "pull",
            shortcut: 'P',
            action: GlobalAction::RemotePull,
        },
        GlobalToolbarButton {
            emoji: "🌿",
            label: "branches",
            shortcut: 'b',
            action: GlobalAction::ToggleBranches,
        },
        GlobalToolbarButton {
            emoji: "🏷",
            label: "tags",
            shortcut: 't',
            action: GlobalAction::ToggleTags,
        },
        GlobalToolbarButton {
            emoji: "📦",
            label: "stash",
            shortcut: 's',
            action: GlobalAction::ToggleStash,
        },
    ]
}

/// 获取基于当前选中 commit 的操作按钮
pub fn commit_buttons(app: &App) -> Vec<ToolbarButton> {
    let mut buttons = vec![
        ToolbarButton {
            emoji: "⎇",
            label: "checkout",
            action: ToolbarAction::Checkout,
            dangerous: false,
        },
        ToolbarButton {
            emoji: "🏷",
            label: "tag",
            action: ToolbarAction::CreateTag,
            dangerous: false,
        },
        ToolbarButton {
            emoji: "⑂",
            label: "branch",
            action: ToolbarAction::CreateBranch,
            dangerous: false,
        },
        ToolbarButton {
            emoji: "🔀",
            label: "merge",
            action: ToolbarAction::Merge,
            dangerous: false,
        },
        ToolbarButton {
            emoji: "🍒",
            label: "pick",
            action: ToolbarAction::CherryPick,
            dangerous: false,
        },
        ToolbarButton {
            emoji: "⏪",
            label: "reset",
            action: ToolbarAction::Reset,
            dangerous: true,
        },
    ];

    if let Some(detail) = &app.selected_detail {
        if !detail.branches.is_empty() {
            buttons.push(ToolbarButton {
                emoji: "❌",
                label: "del",
                action: ToolbarAction::DeleteBranch,
                dangerous: true,
            });
        }
    }

    if let Some(oid) = app.selected_oid {
        if app.stash_map.contains_key(&oid) {
            buttons.push(ToolbarButton {
                emoji: "📤",
                label: "pop",
                action: ToolbarAction::StashPop,
                dangerous: false,
            });
            buttons.push(ToolbarButton {
                emoji: "🗑",
                label: "drop",
                action: ToolbarAction::StashDrop,
                dangerous: true,
            });
        }
    }

    buttons
}

/// 追踪按钮位置用于点击检测
pub struct ToolbarState {
    pub button_starts: Vec<u16>,
    pub button_widths: Vec<u16>,
    pub y: u16,
    pub width_per_button: u16,
}

impl ToolbarState {
    pub fn new() -> Self {
        Self {
            button_starts: Vec::new(),
            button_widths: Vec::new(),
            y: 0,
            width_per_button: 10,
        }
    }

    pub fn hit_test(&self, col: u16, row: u16) -> Option<usize> {
        if row != self.y {
            return None;
        }
        for (i, &start) in self.button_starts.iter().enumerate() {
            let width = self
                .button_widths
                .get(i)
                .copied()
                .unwrap_or(self.width_per_button);
            if col >= start && col < start + width {
                return Some(i);
            }
        }
        None
    }
}

pub fn draw_toolbar(
    f: &mut Frame,
    area: Rect,
    buttons: &[ToolbarButton],
    state: &mut ToolbarState,
) {
    state.button_starts.clear();
    state.button_widths.clear();
    state.y = area.y;

    let mut spans: Vec<Span> = Vec::new();
    let mut x = area.x;
    for (i, btn) in buttons.iter().enumerate() {
        let color = if btn.dangerous {
            Color::Rgb(255, 100, 100)
        } else {
            Color::White
        };
        let text = format!(" {}{} ", btn.emoji, btn.label);
        let text_width = UnicodeWidthStr::width(text.as_str()) as u16;
        state.button_starts.push(x);
        state.button_widths.push(text_width);
        spans.push(Span::styled(
            text.clone(),
            Style::default().fg(color).bg(Color::Rgb(35, 35, 45)),
        ));
        x += text_width;
        if i < buttons.len() - 1 {
            spans.push(Span::raw(" "));
            x += 1;
        }
    }

    let para = Paragraph::new(Line::from(spans));
    f.render_widget(para, area);
}

/// 全局工具栏状态追踪
pub struct GlobalToolbarState {
    pub button_starts: Vec<u16>,
    pub y: u16,
}

impl GlobalToolbarState {
    pub fn new() -> Self {
        Self {
            button_starts: Vec::new(),
            y: 0,
        }
    }

    pub fn hit_test(&self, col: u16, row: u16) -> Option<usize> {
        if row != self.y {
            return None;
        }
        let mut prev_end = 0u16;
        for (i, &start) in self.button_starts.iter().enumerate() {
            let end = if i + 1 < self.button_starts.len() {
                self.button_starts[i + 1]
            } else {
                start + 200
            };
            if col >= start && col < end {
                return Some(i);
            }
            prev_end = end;
        }
        let _ = prev_end;
        None
    }
}

pub fn draw_global_toolbar(f: &mut Frame, area: Rect, app: &mut App) {
    let buttons = global_buttons();
    app.global_toolbar_state.button_starts.clear();
    app.global_toolbar_state.y = area.y;

    let mut spans: Vec<Span> = Vec::new();
    let mut x = area.x;

    // 左侧：分支名 + ahead/behind + dirty 标记
    if let Some(branch) = app.repo.head_branch() {
        spans.push(Span::styled(
            format!(" {} ", branch),
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(80, 50, 130))
                .add_modifier(Modifier::BOLD),
        ));
        x += UnicodeWidthStr::width(format!(" {} ", branch).as_str()) as u16;

        // ahead/behind 标记
        if let Some((ahead, behind)) = app.ahead_behind {
            if ahead > 0 || behind > 0 {
                let mut ab_text = String::new();
                if behind > 0 {
                    ab_text = format!("↓{}", behind);
                }
                if ahead > 0 {
                    if !ab_text.is_empty() {
                        ab_text.push(' ');
                    }
                    ab_text.push_str(&format!("↑{}", ahead));
                }
                let styled = format!(" {} ", ab_text);
                spans.push(Span::styled(
                    styled.clone(),
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Rgb(40, 70, 110)),
                ));
                x += UnicodeWidthStr::width(styled.as_str()) as u16;
            }
        }

        // dirty 标记：工作区不干净时显示 *
        if !app.git_status.is_empty() {
            spans.push(Span::styled(
                " * ".to_string(),
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(140, 100, 20)),
            ));
            x += 3;
        }

        spans.push(Span::raw("  "));
        x += 2;
    }

    for (i, btn) in buttons.iter().enumerate() {
        let text = format!(" {}{} ", btn.emoji, btn.label);
        let text_width = UnicodeWidthStr::width(text.as_str()) as u16;
        app.global_toolbar_state.button_starts.push(x);
        spans.push(Span::styled(
            text.clone(),
            Style::default().fg(Color::White).bg(Color::Rgb(35, 35, 45)),
        ));
        x += text_width;
        if i < buttons.len() - 1 {
            spans.push(Span::raw(" "));
            x += 1;
        }
    }

    let para = Paragraph::new(Line::from(spans));
    f.render_widget(para, area);
}
