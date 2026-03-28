use crate::config::{RemoteControlConfig, ZenConfig};

/// RelayPanel 模式
#[derive(Debug, Clone, PartialEq)]
pub enum RelayPanelMode {
    /// 浏览模式：显示当前配置（Token 脱敏）
    View,
    /// 编辑模式：修改配置
    Edit,
}

/// 编辑字段
#[derive(Debug, Clone, PartialEq)]
pub enum RelayEditField {
    Url,
    Token,
    Name,
}

impl RelayEditField {
    pub fn next(&self) -> Self {
        match self {
            Self::Url => Self::Token,
            Self::Token => Self::Name,
            Self::Name => Self::Url,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::Url => Self::Name,
            Self::Token => Self::Url,
            Self::Name => Self::Token,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Url => "URL   ",
            Self::Token => "Token ",
            Self::Name => "Name  ",
        }
    }
}

/// 远程控制配置面板状态
pub struct RelayPanel {
    /// 当前模式
    pub mode: RelayPanelMode,
    /// 当前编辑字段
    pub edit_field: RelayEditField,
    /// URL 缓冲
    pub buf_url: String,
    /// Token 缓冲
    pub buf_token: String,
    /// Name 缓冲
    pub buf_name: String,
    /// 状态消息（保存成功/失败）
    pub status_message: Option<String>,
    /// 编辑光标位置
    pub cursor: usize,
    /// Web 接入 URL（含 user_id hash，连接成功后填充，只读展示）
    pub web_access_url: Option<String>,
}

impl RelayPanel {
    /// 从 ZenConfig 加载配置
    pub fn from_config(cfg: &ZenConfig) -> Self {
        let rc = cfg.config.remote_control.as_ref();
        Self {
            mode: RelayPanelMode::View,
            edit_field: RelayEditField::Url,
            buf_url: rc.map(|r| r.url.clone()).unwrap_or_default(),
            buf_token: rc.map(|r| r.token.clone()).unwrap_or_default(),
            buf_name: rc.map(|r| r.name.clone().unwrap_or_default()).unwrap_or_default(),
            status_message: None,
            cursor: 0,
            web_access_url: None,
        }
    }

    /// 设置 Web 接入 URL（连接成功后由 relay_ops 调用）
    pub fn set_web_access_url(&mut self, url: Option<String>) {
        self.web_access_url = url;
    }

    /// View 模式下显示脱敏的 Token（如 "****abc123****"）
    pub fn display_token(&self) -> String {
        if self.buf_token.is_empty() {
            "(未设置)".to_string()
        } else if self.buf_token.len() <= 8 {
            "****".to_string()
        } else {
            format!("****{}****", &self.buf_token[self.buf_token.len() - 4..])
        }
    }

    /// 切换到下一个编辑字段
    pub fn field_next(&mut self) {
        self.edit_field = self.edit_field.next();
        self.cursor = self.current_buf().len();
    }

    /// 切换到上一个编辑字段
    pub fn field_prev(&mut self) {
        self.edit_field = self.edit_field.prev();
        self.cursor = self.current_buf().len();
    }

    /// 获取当前字段的缓冲区引用
    fn current_buf(&mut self) -> &mut String {
        match self.edit_field {
            RelayEditField::Url => &mut self.buf_url,
            RelayEditField::Token => &mut self.buf_token,
            RelayEditField::Name => &mut self.buf_name,
        }
    }

    /// 输入字符
    pub fn push_char(&mut self, c: char) {
        let cursor = self.cursor;
        match self.edit_field {
            RelayEditField::Url => {
                self.buf_url.insert(cursor, c);
            }
            RelayEditField::Token => {
                self.buf_token.insert(cursor, c);
            }
            RelayEditField::Name => {
                self.buf_name.insert(cursor, c);
            }
        }
        self.cursor += c.len_utf8();
    }

    /// 删除字符（Backspace）
    pub fn pop_char(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let buf = match self.edit_field {
            RelayEditField::Url => &mut self.buf_url,
            RelayEditField::Token => &mut self.buf_token,
            RelayEditField::Name => &mut self.buf_name,
        };
        // 找到前一个字符的起始位置
        let prev_cursor = buf[..self.cursor]
            .char_indices()
            .rev()
            .next()
            .map(|(i, _)| i)
            .unwrap_or(0);
        buf.remove(prev_cursor);
        self.cursor = prev_cursor;
    }

    /// 删除字符（Delete，删除光标后的字符）
    pub fn delete_char(&mut self) {
        let buf = match self.edit_field {
            RelayEditField::Url => &mut self.buf_url,
            RelayEditField::Token => &mut self.buf_token,
            RelayEditField::Name => &mut self.buf_name,
        };
        if self.cursor < buf.len() {
            buf.remove(self.cursor);
        }
    }

    /// 移动光标左移
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            let buf = match self.edit_field {
                RelayEditField::Url => &self.buf_url,
                RelayEditField::Token => &self.buf_token,
                RelayEditField::Name => &self.buf_name,
            };
            self.cursor = buf[..self.cursor]
                .char_indices()
                .rev()
                .next()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// 移动光标右移
    pub fn cursor_right(&mut self) {
        let buf = match self.edit_field {
            RelayEditField::Url => &self.buf_url,
            RelayEditField::Token => &self.buf_token,
            RelayEditField::Name => &self.buf_name,
        };
        if self.cursor < buf.len() {
            let next_pos = buf[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(buf.len());
            self.cursor = next_pos;
        }
    }

    /// 移动光标到行首
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// 移动光标到行尾
    pub fn cursor_end(&mut self) {
        self.cursor = self.current_buf().len();
    }

    /// 粘贴文本到当前字段
    pub fn paste_text(&mut self, text: &str) {
        // 过滤掉换行符和回车符，只保留单行文本
        let text = text.replace(['\n', '\r'], "");
        let cursor = self.cursor;
        match self.edit_field {
            RelayEditField::Url => {
                self.buf_url.insert_str(cursor, &text);
            }
            RelayEditField::Token => {
                self.buf_token.insert_str(cursor, &text);
            }
            RelayEditField::Name => {
                self.buf_name.insert_str(cursor, &text);
            }
        }
        self.cursor += text.len();
    }

    /// 进入编辑模式
    pub fn enter_edit(&mut self) {
        self.mode = RelayPanelMode::Edit;
        self.edit_field = RelayEditField::Url;
        self.cursor = self.buf_url.len();
        self.status_message = None;
    }

    /// 取消编辑
    pub fn cancel_edit(&mut self, cfg: &ZenConfig) {
        // 恢复原始值
        let rc = cfg.config.remote_control.as_ref();
        self.buf_url = rc.map(|r| r.url.clone()).unwrap_or_default();
        self.buf_token = rc.map(|r| r.token.clone()).unwrap_or_default();
        self.buf_name = rc.map(|r| r.name.clone().unwrap_or_default()).unwrap_or_default();
        self.mode = RelayPanelMode::View;
        self.status_message = None;
    }

    /// 保存编辑到配置
    pub fn apply_edit(&mut self, cfg: &mut ZenConfig) -> bool {
        // URL 必填
        if self.buf_url.trim().is_empty() {
            self.status_message = Some("URL 不能为空".to_string());
            return false;
        }

        let name = if self.buf_name.trim().is_empty() {
            None
        } else {
            Some(self.buf_name.trim().to_string())
        };

        // 保留已存在的 user_id（不被编辑面板覆盖）
        let existing_user_id = cfg.config.remote_control.as_ref().and_then(|rc| rc.user_id.clone());
        cfg.config.remote_control = Some(RemoteControlConfig {
            url: self.buf_url.trim().to_string(),
            token: self.buf_token.clone(),
            name,
            user_id: existing_user_id,
        });

        self.mode = RelayPanelMode::View;
        self.status_message = Some("配置已保存".to_string());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cfg_with_remote() -> ZenConfig {
        let mut cfg = ZenConfig::default();
        cfg.config.remote_control = Some(RemoteControlConfig {
            url: "wss://relay.example.com".to_string(),
            token: "secret123".to_string(),
            name: Some("my-laptop".to_string()),
            user_id: None,
        });
        cfg
    }

    #[test]
    fn test_relay_panel_from_config() {
        let cfg = make_cfg_with_remote();
        let panel = RelayPanel::from_config(&cfg);
        assert_eq!(panel.buf_url, "wss://relay.example.com");
        assert_eq!(panel.buf_token, "secret123");
        assert_eq!(panel.buf_name, "my-laptop");
        assert_eq!(panel.mode, RelayPanelMode::View);
    }

    #[test]
    fn test_relay_panel_from_config_empty() {
        let cfg = ZenConfig::default();
        let panel = RelayPanel::from_config(&cfg);
        assert!(panel.buf_url.is_empty());
        assert!(panel.buf_token.is_empty());
        assert!(panel.buf_name.is_empty());
    }

    #[test]
    fn test_display_token() {
        let mut panel = RelayPanel::from_config(&ZenConfig::default());

        // 空 token
        assert_eq!(panel.display_token(), "(未设置)");

        // 短 token
        panel.buf_token = "abc".to_string();
        assert_eq!(panel.display_token(), "****");

        // 长 token
        panel.buf_token = "secret123456".to_string();
        assert_eq!(panel.display_token(), "****3456****");
    }

    #[test]
    fn test_field_navigation() {
        let cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        assert_eq!(panel.edit_field, RelayEditField::Url);

        panel.field_next();
        assert_eq!(panel.edit_field, RelayEditField::Token);

        panel.field_next();
        assert_eq!(panel.edit_field, RelayEditField::Name);

        panel.field_next();
        assert_eq!(panel.edit_field, RelayEditField::Url);

        panel.field_prev();
        assert_eq!(panel.edit_field, RelayEditField::Name);
    }

    #[test]
    fn test_push_pop_char() {
        let cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.edit_field = RelayEditField::Url;

        panel.push_char('a');
        panel.push_char('b');
        panel.push_char('c');
        assert_eq!(panel.buf_url, "abc");
        assert_eq!(panel.cursor, 3);

        panel.pop_char();
        assert_eq!(panel.buf_url, "ab");
        assert_eq!(panel.cursor, 2);
    }

    #[test]
    fn test_cursor_movement() {
        let cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.buf_url = "hello".to_string();
        panel.cursor = 5;

        panel.cursor_left();
        assert_eq!(panel.cursor, 4);

        panel.cursor_home();
        assert_eq!(panel.cursor, 0);

        panel.cursor_right();
        assert_eq!(panel.cursor, 1);

        panel.cursor_end();
        assert_eq!(panel.cursor, 5);
    }

    #[test]
    fn test_apply_edit_success() {
        let mut cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.mode = RelayPanelMode::Edit;
        panel.buf_url = "ws://localhost:8080".to_string();
        panel.buf_token = "token123".to_string();
        panel.buf_name = "test-device".to_string();

        let result = panel.apply_edit(&mut cfg);
        assert!(result);

        let rc = cfg.config.remote_control.as_ref().unwrap();
        assert_eq!(rc.url, "ws://localhost:8080");
        assert_eq!(rc.token, "token123");
        assert_eq!(rc.name, Some("test-device".to_string()));
        assert_eq!(panel.mode, RelayPanelMode::View);
    }

    #[test]
    fn test_apply_edit_empty_url() {
        let mut cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.mode = RelayPanelMode::Edit;
        panel.buf_url = "".to_string();

        let result = panel.apply_edit(&mut cfg);
        assert!(!result);
        assert_eq!(panel.status_message, Some("URL 不能为空".to_string()));
    }

    #[test]
    fn test_apply_edit_empty_name() {
        let mut cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.mode = RelayPanelMode::Edit;
        panel.buf_url = "ws://localhost:8080".to_string();
        panel.buf_name = "   ".to_string(); // 空白

        let result = panel.apply_edit(&mut cfg);
        assert!(result);

        let rc = cfg.config.remote_control.as_ref().unwrap();
        assert_eq!(rc.name, None);
    }

    #[test]
    fn test_cancel_edit() {
        let cfg = make_cfg_with_remote();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.mode = RelayPanelMode::Edit;
        panel.buf_url = "modified".to_string();

        panel.cancel_edit(&cfg);
        assert_eq!(panel.buf_url, "wss://relay.example.com");
        assert_eq!(panel.mode, RelayPanelMode::View);
    }

    #[test]
    fn test_paste_text() {
        let cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.mode = RelayPanelMode::Edit;
        panel.cursor = 0;

        // 粘贴到 URL 字段
        panel.edit_field = RelayEditField::Url;
        panel.paste_text("ws://example.com");
        assert_eq!(panel.buf_url, "ws://example.com");
        assert_eq!(panel.buf_url.len(), 16); // "ws://example.com" is 16 chars
        assert_eq!(panel.cursor, 16);

        // 粘贴到 Token 字段
        panel.field_next();
        panel.cursor = 0;
        panel.paste_text("my-token-123");
        assert_eq!(panel.buf_token, "my-token-123");
        assert_eq!(panel.buf_token.len(), 12); // "my-token-123" is 12 chars
        assert_eq!(panel.cursor, 12);

        // 在中间位置粘贴
        panel.cursor = 3;
        panel.paste_text("-abc");
        assert_eq!(panel.buf_token, "my--abctoken-123"); // "my-" + "-abc" + "token-123"
        assert_eq!(panel.cursor, 7); // 3 + 4 = 7
    }

    #[test]
    fn test_paste_text_with_newlines() {
        let cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.mode = RelayPanelMode::Edit;
        panel.cursor = 0;

        // 粘贴包含换行符的文本（应该被过滤）
        panel.edit_field = RelayEditField::Name;
        panel.paste_text("line1\nline2\rline3");
        assert_eq!(panel.buf_name, "line1line2line3");
        assert_eq!(panel.cursor, 15); // 5 + 5 + 5 = 15
    }
}
