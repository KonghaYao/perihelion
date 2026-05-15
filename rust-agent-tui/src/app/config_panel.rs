use std::any::Any;

use ratatui::layout::Rect;
use ratatui::Frame;
use tui_textarea::Input;

use crate::config::PeriConfig;

use super::panel_component::PanelComponent;
use super::panel_list::PanelList;
use super::panel_manager::{EventResult, PanelContext, PanelKind};
use super::App;

// ─── 枚举 ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigPanelMode {
    Browse,
    Edit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigEditField {
    Autocompact,
    CompactThreshold,
    Language,
    Persona,
    Tone,
    Proactiveness,
}

impl ConfigEditField {
    pub fn next(&self) -> Self {
        match self {
            Self::Autocompact => Self::CompactThreshold,
            Self::CompactThreshold => Self::Language,
            Self::Language => Self::Persona,
            Self::Persona => Self::Tone,
            Self::Tone => Self::Proactiveness,
            Self::Proactiveness => Self::Autocompact,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Autocompact => Self::Proactiveness,
            Self::CompactThreshold => Self::Autocompact,
            Self::Language => Self::CompactThreshold,
            Self::Persona => Self::Language,
            Self::Tone => Self::Persona,
            Self::Proactiveness => Self::Tone,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Autocompact => "Autocompact",
            Self::CompactThreshold => "Compact 阈值",
            Self::Language => "语言",
            Self::Persona => "Persona",
            Self::Tone => "Tone",
            Self::Proactiveness => "Proactiveness",
        }
    }
}

// ─── ConfigPanel ─────────────────────────────────────────────────────────────

const FIELD_COUNT: usize = 6;

#[derive(Clone)]
pub struct ConfigPanel {
    pub mode: ConfigPanelMode,
    /// Browse 模式光标管理
    browse_list: PanelList<ConfigEditField>,
    pub edit_field: ConfigEditField,
    // 编辑缓冲区
    pub buf_autocompact: bool,
    pub buf_threshold: String,
    pub cur_threshold: usize,
    pub buf_language: String,
    pub cur_language: usize,
    pub buf_persona: String,
    pub cur_persona: usize,
    pub buf_tone: String,
    pub cur_tone: usize,
    pub buf_proactiveness: String, // "low" / "medium" / "high"
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

        let mut browse_list = PanelList::new();
        browse_list.set_items(vec![
            ConfigEditField::Autocompact,
            ConfigEditField::CompactThreshold,
            ConfigEditField::Language,
            ConfigEditField::Persona,
            ConfigEditField::Tone,
            ConfigEditField::Proactiveness,
        ]);

        Self {
            mode: ConfigPanelMode::Browse,
            browse_list,
            edit_field: ConfigEditField::Autocompact,
            buf_autocompact: autocompact,
            buf_threshold: threshold,
            cur_threshold: 0,
            buf_language: cfg.config.language.clone().unwrap_or_default(),
            cur_language: 0,
            buf_persona: cfg.config.persona.clone().unwrap_or_default(),
            cur_persona: 0,
            buf_tone: cfg.config.tone.clone().unwrap_or_default(),
            cur_tone: 0,
            buf_proactiveness: proactiveness,
        }
    }

    pub fn enter_edit(&mut self) {
        self.mode = ConfigPanelMode::Edit;
        self.edit_field = self
            .browse_list
            .selected()
            .cloned()
            .unwrap_or(ConfigEditField::Autocompact);
        // 设置光标到末尾
        self.cur_threshold = self.buf_threshold.chars().count();
        self.cur_language = self.buf_language.chars().count();
        self.cur_persona = self.buf_persona.chars().count();
        self.cur_tone = self.buf_tone.chars().count();
    }

    pub fn field_next(&mut self) {
        self.edit_field = self.edit_field.next();
    }

    pub fn field_prev(&mut self) {
        self.edit_field = self.edit_field.prev();
    }

    /// 返回当前可编辑字段的 (buf, cursor) 可变引用
    /// Autocompact 和 Proactiveness 返回 None（用 Space 切换）
    pub fn active_field(&mut self) -> Option<(&mut String, &mut usize)> {
        match self.edit_field {
            ConfigEditField::Autocompact | ConfigEditField::Proactiveness => None,
            ConfigEditField::CompactThreshold => {
                Some((&mut self.buf_threshold, &mut self.cur_threshold))
            }
            ConfigEditField::Language => Some((&mut self.buf_language, &mut self.cur_language)),
            ConfigEditField::Persona => Some((&mut self.buf_persona, &mut self.cur_persona)),
            ConfigEditField::Tone => Some((&mut self.buf_tone, &mut self.cur_tone)),
        }
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

    pub fn paste_text(&mut self, text: &str) {
        let text: String = text.chars().filter(|&c| c != '\n' && c != '\r').collect();
        if let Some((buf, cursor)) = self.active_field() {
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
    }

    pub fn apply_edit(
        &mut self,
        cfg: &mut PeriConfig,
        lc: &crate::i18n::LcRegistry,
    ) -> Result<(), String> {
        // autocompact + threshold
        let compact = cfg
            .config
            .compact
            .get_or_insert_with(rust_create_agent::agent::CompactConfig::default);
        compact.auto_compact_enabled = self.buf_autocompact;
        let threshold_val: u8 = self.buf_threshold.parse().unwrap_or(85).clamp(50, 99);
        compact.auto_compact_threshold = threshold_val as f64 / 100.0;

        // language: validate against available langs
        let lang_val = if self.buf_language.is_empty() || self.buf_language == "auto" {
            None
        } else {
            let lang_str = self.buf_language.trim().to_string();
            if lc.available_langs().contains(&lang_str.as_str()) {
                Some(lang_str)
            } else {
                return Err(format!(
                    "Unsupported language: '{}'. Available: {}",
                    self.buf_language,
                    lc.available_langs().join(", ")
                ));
            }
        };
        cfg.config.language = lang_val;

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

        Ok(())
    }

    pub fn field_count() -> usize {
        FIELD_COUNT
    }

    pub fn cursor(&self) -> usize {
        self.browse_list.cursor()
    }

    pub fn field_label(index: usize) -> &'static str {
        match index {
            0 => "Autocompact",
            1 => "Compact 阈值",
            2 => "语言",
            3 => "Persona",
            4 => "Tone",
            5 => "Proactiveness",
            _ => "???",
        }
    }

    pub fn field_display_value(&self, index: usize) -> String {
        match index {
            0 => {
                if self.buf_autocompact {
                    "ON".to_string()
                } else {
                    "OFF".to_string()
                }
            }
            1 => format!("{}%", self.buf_threshold),
            2 => {
                if self.buf_language.is_empty() {
                    "auto".to_string()
                } else {
                    match self.buf_language.as_str() {
                        "en" => "English".to_string(),
                        "zh-CN" => "简体中文".to_string(),
                        other => other.to_string(),
                    }
                }
            }
            3 => {
                if self.buf_persona.is_empty() {
                    "-".to_string()
                } else {
                    self.buf_persona.clone()
                }
            }
            4 => {
                if self.buf_tone.is_empty() {
                    "-".to_string()
                } else {
                    self.buf_tone.clone()
                }
            }
            5 => self.buf_proactiveness.clone(),
            _ => String::new(),
        }
    }
}

impl PanelComponent for ConfigPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Config
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;
        match self.mode {
            ConfigPanelMode::Browse => match input {
                Input { key: Key::Up, .. } => {
                    self.browse_list.move_cursor(-1);
                    EventResult::Consumed
                }
                Input { key: Key::Down, .. } => {
                    self.browse_list.move_cursor(1);
                    EventResult::Consumed
                }
                Input {
                    key: Key::Enter, ..
                } => {
                    self.enter_edit();
                    EventResult::Consumed
                }
                Input { key: Key::Esc, .. } => EventResult::ClosePanel,
                _ => EventResult::Consumed,
            },
            ConfigPanelMode::Edit => {
                match input {
                    Input { key: Key::Esc, .. } => {
                        self.mode = ConfigPanelMode::Browse;
                        EventResult::Consumed
                    }
                    Input {
                        key: Key::Enter, ..
                    } => {
                        // apply_config and close
                        let Some(cfg) = ctx.services.peri_config.as_mut() else {
                            return EventResult::Consumed;
                        };
                        match self.apply_edit(cfg, &ctx.services.lc) {
                            Ok(()) => {
                                if let Some(ref lang) = cfg.config.language {
                                    let _ = ctx.services.lc.switch(lang);
                                }
                                use super::App;
                                if let Err(e) = App::save_config(
                                    cfg,
                                    ctx.services.config_path_override.as_deref(),
                                ) {
                                    ctx.session_mgr.sessions[ctx.session_mgr.active]
                                        .messages
                                        .push_system_note(ctx.services.lc.tr_args(
                                            "app-config-save-failed",
                                            &[("error".into(), e.to_string().into())],
                                        ));
                                } else {
                                    ctx.session_mgr.sessions[ctx.session_mgr.active]
                                        .messages
                                        .push_system_note(ctx.services.lc.tr("app-config-saved"));
                                }
                                EventResult::ClosePanel
                            }
                            Err(err_msg) => {
                                ctx.session_mgr.sessions[ctx.session_mgr.active]
                                    .messages
                                    .push_system_note(err_msg);
                                EventResult::Consumed
                            }
                        }
                    }
                    Input { key: Key::Up, .. } => {
                        self.field_prev();
                        EventResult::Consumed
                    }
                    Input { key: Key::Down, .. } => {
                        self.field_next();
                        EventResult::Consumed
                    }
                    Input {
                        key: Key::Char(' '),
                        ctrl: false,
                        ..
                    } => {
                        match self.edit_field {
                            ConfigEditField::Autocompact => self.cycle_autocompact(),
                            ConfigEditField::Proactiveness => self.cycle_proactiveness(),
                            _ => {
                                if let Some((buf, cursor)) = self.active_field() {
                                    super::handle_edit_key(
                                        buf,
                                        cursor,
                                        Input {
                                            key: Key::Char(' '),
                                            ctrl: false,
                                            alt: false,
                                            shift: false,
                                        },
                                    );
                                }
                            }
                        }
                        EventResult::Consumed
                    }
                    Input {
                        key: Key::Left,
                        ctrl: false,
                        ..
                    }
                    | Input {
                        key: Key::Right,
                        ctrl: false,
                        ..
                    } => {
                        match self.edit_field {
                            ConfigEditField::Autocompact => self.cycle_autocompact(),
                            ConfigEditField::Proactiveness => self.cycle_proactiveness(),
                            _ => {
                                if let Some((buf, cursor)) = self.active_field() {
                                    super::handle_edit_key(buf, cursor, input);
                                }
                            }
                        }
                        EventResult::Consumed
                    }
                    _ => {
                        if let Some((buf, cursor)) = self.active_field() {
                            super::handle_edit_key(buf, cursor, input);
                        }
                        EventResult::Consumed
                    }
                }
            }
        }
    }

    fn handle_paste(&mut self, text: &str, _ctx: &mut PanelContext<'_>) -> EventResult {
        self.paste_text(text);
        EventResult::Consumed
    }

    fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        if matches!(self.mode, ConfigPanelMode::Browse) {
            self.browse_list.handle_scroll(lines, 10);
            EventResult::Consumed
        } else {
            EventResult::NotConsumed
        }
    }

    fn handle_mouse(
        &mut self,
        mouse: ratatui::crossterm::event::MouseEvent,
        area: Rect,
        _ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        use ratatui::crossterm::event::{MouseButton, MouseEventKind};
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left)
                if matches!(self.mode, ConfigPanelMode::Browse) =>
            {
                if self
                    .browse_list
                    .handle_mouse_click(mouse.row, mouse.column, area, 1)
                {
                    self.enter_edit();
                    return EventResult::Consumed;
                }
                EventResult::NotConsumed
            }
            _ => EventResult::NotConsumed,
        }
    }

    fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 {
        match self.mode {
            ConfigPanelMode::Browse => 12,
            ConfigPanelMode::Edit => 14,
        }
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

    fn status_bar_hints(&self, _lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        match self.mode {
            ConfigPanelMode::Browse => vec![
                (
                    "\u{2191}\u{2193}".to_string(),
                    "\u{5bfc}\u{822a}".to_string(),
                ),
                ("Enter".to_string(), "\u{7f16}\u{8f91}".to_string()),
                ("Esc".to_string(), "\u{5173}\u{95ed}".to_string()),
            ],
            ConfigPanelMode::Edit => vec![
                (
                    "\u{2191}\u{2193}".to_string(),
                    "\u{5b57}\u{6bb5}".to_string(),
                ),
                ("Enter".to_string(), "\u{4fdd}\u{5b58}".to_string()),
                ("Space".to_string(), "\u{5207}\u{6362}".to_string()),
                ("Esc".to_string(), "\u{53d6}\u{6d88}".to_string()),
            ],
        }
    }
}

#[cfg(test)]
#[path = "config_panel_test.rs"]
mod tests;
