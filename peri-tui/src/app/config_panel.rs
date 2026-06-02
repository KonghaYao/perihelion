use std::any::Any;

use ratatui::{layout::Rect, Frame};
use tui_textarea::Input;

use crate::config::PeriConfig;

use super::{
    panel_component::PanelComponent,
    panel_manager::{EventResult, PanelContext, PanelKind},
    App,
};

// ─── 行索引常量 ─────────────────────────────────────────────────────────────────

pub const ROW_GENERAL_HEADER: usize = 0;
pub const ROW_AUTOCOMPACT: usize = 1;
pub const ROW_THRESHOLD: usize = 2;
pub const ROW_LANGUAGE: usize = 3;
pub const ROW_DIFF: usize = 4;
pub const ROW_PROACTIVENESS: usize = 5;
pub const ROW_SEPARATOR: usize = 6;
pub const ROW_OVERRIDES_HEADER: usize = 7;
pub const ROW_PERSONA: usize = 8;
pub const ROW_TONE: usize = 9;
pub const ROW_COUNT: usize = 10;

fn next_editable_row(current: usize, reverse: bool) -> usize {
    let editable: &[usize] = &[
        ROW_AUTOCOMPACT,
        ROW_THRESHOLD,
        ROW_LANGUAGE,
        ROW_DIFF,
        ROW_PROACTIVENESS,
        ROW_PERSONA,
        ROW_TONE,
    ];
    if reverse {
        editable
            .iter()
            .rev()
            .find(|&&r| r < current)
            .copied()
            .unwrap_or(editable[editable.len() - 1])
    } else {
        editable
            .iter()
            .find(|&&r| r > current)
            .copied()
            .unwrap_or(editable[0])
    }
}

// ─── ConfigPanel ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct ConfigPanel {
    pub cursor: usize,
    // 编辑缓冲区
    pub buf_autocompact: bool,
    pub buf_threshold: String,
    pub cur_threshold: usize,
    pub buf_language: String, // "" = auto, "en", "zh-CN"
    pub buf_persona: String,
    pub cur_persona: usize,
    pub buf_tone: String,
    pub cur_tone: usize,
    pub buf_proactiveness: String, // "low" / "medium" / "high"
    pub buf_diff: bool,
}

impl ConfigPanel {
    pub fn from_config(cfg: &PeriConfig) -> Self {
        let compact_config = cfg.config.compact.as_ref();
        let autocompact = compact_config
            .map(|c| c.auto_compact_enabled)
            .unwrap_or(true);
        let threshold = compact_config
            .map(|c| format!("{}", (c.auto_compact_threshold * 100.0) as u8))
            .unwrap_or_else(|| "85".to_string());
        let proactiveness = cfg
            .config
            .proactiveness
            .clone()
            .unwrap_or_else(|| "medium".to_string());
        let diff_enabled = cfg.config.diff_enabled;

        Self {
            cursor: ROW_AUTOCOMPACT,
            buf_autocompact: autocompact,
            buf_threshold: threshold,
            cur_threshold: 0,
            buf_language: cfg.config.language.clone().unwrap_or_default(),
            buf_persona: cfg.config.persona.clone().unwrap_or_default(),
            cur_persona: 0,
            buf_tone: cfg.config.tone.clone().unwrap_or_default(),
            cur_tone: 0,
            buf_proactiveness: proactiveness,
            buf_diff: diff_enabled,
        }
    }

    pub fn cursor_down(&mut self) {
        self.cursor = next_editable_row(self.cursor, false);
    }

    pub fn cursor_up(&mut self) {
        self.cursor = next_editable_row(self.cursor, true);
    }

    pub fn cycle_autocompact(&mut self) {
        self.buf_autocompact = !self.buf_autocompact;
    }

    pub fn cycle_proactiveness(&mut self) {
        self.buf_proactiveness = match self.buf_proactiveness.as_str() {
            "low" => "medium".to_string(),
            "medium" => "high".to_string(),
            _ => "low".to_string(),
        };
    }

    pub fn cycle_diff(&mut self) {
        self.buf_diff = !self.buf_diff;
    }

    /// 可选语言列表："" (auto) → "en" → "zh-CN" → ""
    const LANGUAGE_OPTIONS: &[&str] = &["en", "zh-CN"];

    pub fn cycle_language(&mut self, reverse: bool) {
        let current = self.buf_language.as_str();
        let next = match Self::LANGUAGE_OPTIONS.iter().position(|&o| o == current) {
            Some(p) => {
                if reverse {
                    if p == 0 {
                        Self::LANGUAGE_OPTIONS.len() - 1
                    } else {
                        p - 1
                    }
                } else {
                    (p + 1) % Self::LANGUAGE_OPTIONS.len()
                }
            }
            // 未匹配时：forward → 第一个，reverse → 最后一个
            None => {
                if reverse {
                    Self::LANGUAGE_OPTIONS.len() - 1
                } else {
                    0
                }
            }
        };
        self.buf_language = Self::LANGUAGE_OPTIONS[next].to_string();
    }

    pub fn paste_text(&mut self, text: &str) {
        let text: String = text.chars().filter(|&c| c != '\n' && c != '\r').collect();
        match self.cursor {
            ROW_THRESHOLD => {
                let buf = &mut self.buf_threshold;
                let cursor = &mut self.cur_threshold;
                let char_count = buf.chars().count();
                if *cursor > char_count {
                    *cursor = char_count;
                }
                let byte_pos = buf
                    .char_indices()
                    .nth(*cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(buf.len());
                buf.insert_str(byte_pos, &text);
                *cursor += text.chars().count();
            }
            ROW_PERSONA => {
                let buf = &mut self.buf_persona;
                let cursor = &mut self.cur_persona;
                let char_count = buf.chars().count();
                if *cursor > char_count {
                    *cursor = char_count;
                }
                let byte_pos = buf
                    .char_indices()
                    .nth(*cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(buf.len());
                buf.insert_str(byte_pos, &text);
                *cursor += text.chars().count();
            }
            ROW_TONE => {
                let buf = &mut self.buf_tone;
                let cursor = &mut self.cur_tone;
                let char_count = buf.chars().count();
                if *cursor > char_count {
                    *cursor = char_count;
                }
                let byte_pos = buf
                    .char_indices()
                    .nth(*cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(buf.len());
                buf.insert_str(byte_pos, &text);
                *cursor += text.chars().count();
            }
            _ => {}
        }
    }

    pub fn apply_edit(
        &mut self,
        cfg: &mut PeriConfig,
        _lc: &crate::i18n::LcRegistry,
    ) -> Result<(), String> {
        // autocompact + threshold
        let compact = cfg
            .config
            .compact
            .get_or_insert_with(peri_agent::agent::CompactConfig::default);
        compact.auto_compact_enabled = self.buf_autocompact;
        let threshold_val: u8 = self.buf_threshold.parse().unwrap_or(85).clamp(50, 99);
        compact.auto_compact_threshold = threshold_val as f64 / 100.0;

        // language: value is always valid (selected from LANGUAGE_OPTIONS)
        cfg.config.language = if self.buf_language.is_empty() {
            None
        } else {
            Some(self.buf_language.clone())
        };

        // persona
        cfg.config.persona = if self.buf_persona.is_empty() {
            None
        } else {
            Some(self.buf_persona.clone())
        };

        // tone
        cfg.config.tone = if self.buf_tone.is_empty() {
            None
        } else {
            Some(self.buf_tone.clone())
        };

        // proactiveness
        cfg.config.proactiveness = if self.buf_proactiveness == "medium" {
            None
        } else {
            Some(self.buf_proactiveness.clone())
        };

        // diff
        cfg.config.diff_enabled = self.buf_diff;

        Ok(())
    }

    fn input_char(&mut self, c: char) {
        match self.cursor {
            ROW_THRESHOLD => {
                super::handle_edit_key(
                    &mut self.buf_threshold,
                    &mut self.cur_threshold,
                    Input {
                        key: tui_textarea::Key::Char(c),
                        ctrl: false,
                        alt: false,
                        shift: false,
                    },
                );
            }
            ROW_PERSONA => {
                super::handle_edit_key(
                    &mut self.buf_persona,
                    &mut self.cur_persona,
                    Input {
                        key: tui_textarea::Key::Char(c),
                        ctrl: false,
                        alt: false,
                        shift: false,
                    },
                );
            }
            ROW_TONE => {
                super::handle_edit_key(
                    &mut self.buf_tone,
                    &mut self.cur_tone,
                    Input {
                        key: tui_textarea::Key::Char(c),
                        ctrl: false,
                        alt: false,
                        shift: false,
                    },
                );
            }
            _ => {}
        }
    }

    fn handle_text_key(&mut self, input: Input) {
        match self.cursor {
            ROW_THRESHOLD => {
                super::handle_edit_key(&mut self.buf_threshold, &mut self.cur_threshold, input);
            }
            ROW_PERSONA => {
                super::handle_edit_key(&mut self.buf_persona, &mut self.cur_persona, input);
            }
            ROW_TONE => {
                super::handle_edit_key(&mut self.buf_tone, &mut self.cur_tone, input);
            }
            _ => {}
        }
    }
}

impl PanelComponent for ConfigPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Config
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;
        match input {
            Input { key: Key::Esc, .. } => EventResult::ClosePanel,
            Input { key: Key::Up, .. } => {
                self.cursor_up();
                EventResult::Consumed
            }
            Input { key: Key::Down, .. } => {
                self.cursor_down();
                EventResult::Consumed
            }
            Input {
                key: Key::Enter, ..
            } => {
                let Some(cfg) = ctx.services.peri_config.as_mut() else {
                    return EventResult::Consumed;
                };
                match self.apply_edit(cfg, &ctx.services.lc) {
                    Ok(()) => {
                        if let Some(ref lang) = cfg.config.language {
                            let _ = ctx.services.lc.switch(lang);
                        }
                        if let Err(e) =
                            App::save_config(cfg, ctx.services.config_path_override.as_deref())
                        {
                            ctx.session_mgr.current_mut().messages.push_system_note(
                                ctx.services.lc.tr_args(
                                    "app-config-save-failed",
                                    &[("error".into(), e.to_string().into())],
                                ),
                            );
                        } else {
                            ctx.session_mgr
                                .current_mut()
                                .messages
                                .push_system_note(ctx.services.lc.tr("app-config-saved"));
                        }
                        EventResult::ClosePanel
                    }
                    Err(err_msg) => {
                        ctx.session_mgr
                            .current_mut()
                            .messages
                            .push_system_note(err_msg);
                        EventResult::Consumed
                    }
                }
            }
            Input {
                key: Key::Char(' '),
                ctrl: false,
                ..
            } => {
                match self.cursor {
                    ROW_AUTOCOMPACT => self.cycle_autocompact(),
                    ROW_LANGUAGE => self.cycle_language(false),
                    ROW_PROACTIVENESS => self.cycle_proactiveness(),
                    ROW_DIFF => self.cycle_diff(),
                    _ => self.input_char(' '),
                }
                EventResult::Consumed
            }
            Input {
                key: Key::Left,
                ctrl: false,
                ..
            } => {
                match self.cursor {
                    ROW_AUTOCOMPACT => self.cycle_autocompact(),
                    ROW_LANGUAGE => self.cycle_language(true),
                    ROW_PROACTIVENESS => self.cycle_proactiveness(),
                    ROW_DIFF => self.cycle_diff(),
                    _ => {
                        self.handle_text_key(input);
                    }
                }
                EventResult::Consumed
            }
            Input {
                key: Key::Right,
                ctrl: false,
                ..
            } => {
                match self.cursor {
                    ROW_AUTOCOMPACT => self.cycle_autocompact(),
                    ROW_LANGUAGE => self.cycle_language(false),
                    ROW_PROACTIVENESS => self.cycle_proactiveness(),
                    ROW_DIFF => self.cycle_diff(),
                    _ => {
                        self.handle_text_key(input);
                    }
                }
                EventResult::Consumed
            }
            _ => {
                self.handle_text_key(input);
                EventResult::Consumed
            }
        }
    }

    fn handle_paste(&mut self, text: &str, _ctx: &mut PanelContext<'_>) -> EventResult {
        self.paste_text(text);
        EventResult::Consumed
    }

    fn handle_scroll(&mut self, _lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        EventResult::NotConsumed
    }

    fn set_scroll_offset(&mut self, _offset: u16) {}

    fn handle_mouse(
        &mut self,
        mouse: ratatui::crossterm::event::MouseEvent,
        area: Rect,
        _ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            let relative_y = mouse.row.saturating_sub(area.y);
            if relative_y >= 1 {
                let clicked = (relative_y - 1) as usize;
                if matches!(
                    clicked,
                    ROW_AUTOCOMPACT
                        | ROW_THRESHOLD
                        | ROW_LANGUAGE
                        | ROW_DIFF
                        | ROW_PROACTIVENESS
                        | ROW_PERSONA
                        | ROW_TONE
                ) {
                    self.cursor = clicked;
                    return EventResult::Consumed;
                }
            }
        }
        EventResult::NotConsumed
    }

    fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 {
        16
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::config::render_config_panel(f, self, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self, lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        vec![
            ("↑↓".to_string(), lc.tr("hint-config-field")),
            ("Space".to_string(), lc.tr("hint-config-toggle")),
            ("Enter".to_string(), lc.tr("hint-config-save")),
            ("Esc".to_string(), lc.tr("key-close")),
        ]
    }
}

#[cfg(test)]
#[path = "config_panel_test.rs"]
mod tests;
