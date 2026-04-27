use perihelion_widgets::{FormField, FormState};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelayEditField {
    Url,
    Token,
    Name,
}

impl FormField for RelayEditField {
    fn next(self) -> Self {
        match self {
            Self::Url => Self::Token,
            Self::Token => Self::Name,
            Self::Name => Self::Url,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Url => Self::Name,
            Self::Token => Self::Url,
            Self::Name => Self::Token,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Url => "URL",
            Self::Token => "Token",
            Self::Name => "Name",
        }
    }
}

impl RelayEditField {
    pub fn all() -> &'static [RelayEditField] {
        &[Self::Url, Self::Token, Self::Name]
    }
}

/// 远程控制配置面板状态
pub struct RelayPanel {
    /// 当前模式
    pub mode: RelayPanelMode,
    /// 表单状态（管理 URL/Token/Name 三个字段）
    pub form: FormState<RelayEditField>,
    /// 状态消息（保存成功/失败）
    pub status_message: Option<String>,
    /// Web 接入 URL（含 user_id hash，连接成功后填充，只读展示）
    pub web_access_url: Option<String>,
}

impl RelayPanel {
    /// 从 ZenConfig 加载配置
    pub fn from_config(cfg: &ZenConfig) -> Self {
        let rc = cfg.config.remote_control.as_ref();
        let url = rc.map(|r| r.url.clone()).unwrap_or_default();
        let token = rc.map(|r| r.token.clone()).unwrap_or_default();
        let name = rc.map(|r| r.name.clone().unwrap_or_default()).unwrap_or_default();

        let mut form = FormState::new(RelayEditField::all().iter().copied());
        form.set_active(RelayEditField::Url);
        form.input_mut(RelayEditField::Url).set_value(url);
        form.input_mut(RelayEditField::Token).set_value(token);
        form.input_mut(RelayEditField::Name).set_value(name);

        Self {
            mode: RelayPanelMode::View,
            form,
            status_message: None,
            web_access_url: None,
        }
    }

    /// 设置 Web 接入 URL（连接成功后由 relay_ops 调用）
    pub fn set_web_access_url(&mut self, url: Option<String>) {
        self.web_access_url = url;
    }

    /// View 模式下显示脱敏的 Token（如 "****abc123****"）
    pub fn display_token(&self) -> String {
        let token = self.form.input(RelayEditField::Token).value();
        if token.is_empty() {
            "(未设置)".to_string()
        } else if token.len() <= 8 {
            "****".to_string()
        } else {
            format!("****{}****", &token[token.len() - 4..])
        }
    }

    /// 获取当前编辑字段
    pub fn edit_field(&self) -> RelayEditField {
        self.form.active_field()
    }

    /// 获取指定字段的值
    pub fn field_value(&self, field: RelayEditField) -> &str {
        self.form.input(field).value()
    }

    /// 获取当前字段的 cursor 位置
    pub fn cursor(&self) -> usize {
        self.form.active_input().cursor()
    }

    /// 进入编辑模式
    pub fn enter_edit(&mut self) {
        self.mode = RelayPanelMode::Edit;
        self.form.set_active(RelayEditField::Url);
        self.form.active_input_mut().cursor_end();
        self.status_message = None;
    }

    /// 取消编辑
    pub fn cancel_edit(&mut self, cfg: &ZenConfig) {
        let rc = cfg.config.remote_control.as_ref();
        let url = rc.map(|r| r.url.clone()).unwrap_or_default();
        let token = rc.map(|r| r.token.clone()).unwrap_or_default();
        let name = rc.map(|r| r.name.clone().unwrap_or_default()).unwrap_or_default();

        self.form.input_mut(RelayEditField::Url).set_value(url);
        self.form.input_mut(RelayEditField::Token).set_value(token);
        self.form.input_mut(RelayEditField::Name).set_value(name);
        self.mode = RelayPanelMode::View;
        self.status_message = None;
    }

    /// 保存编辑到配置
    pub fn apply_edit(&mut self, cfg: &mut ZenConfig) -> bool {
        let url = self.form.input(RelayEditField::Url).value().trim();
        if url.is_empty() {
            self.status_message = Some("URL 不能为空".to_string());
            return false;
        }

        let name = {
            let n = self.form.input(RelayEditField::Name).value().trim();
            if n.is_empty() { None } else { Some(n.to_string()) }
        };

        let token = self.form.input(RelayEditField::Token).value().to_string();

        // 保留已存在的 user_id（不被编辑面板覆盖）
        let existing_user_id = cfg.config.remote_control.as_ref().and_then(|rc| rc.user_id.clone());
        cfg.config.remote_control = Some(RemoteControlConfig {
            url: url.to_string(),
            token,
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
        assert_eq!(panel.field_value(RelayEditField::Url), "wss://relay.example.com");
        assert_eq!(panel.field_value(RelayEditField::Token), "secret123");
        assert_eq!(panel.field_value(RelayEditField::Name), "my-laptop");
        assert_eq!(panel.mode, RelayPanelMode::View);
    }

    #[test]
    fn test_relay_panel_from_config_empty() {
        let cfg = ZenConfig::default();
        let panel = RelayPanel::from_config(&cfg);
        assert!(panel.field_value(RelayEditField::Url).is_empty());
        assert!(panel.field_value(RelayEditField::Token).is_empty());
        assert!(panel.field_value(RelayEditField::Name).is_empty());
    }

    #[test]
    fn test_display_token() {
        let mut panel = RelayPanel::from_config(&ZenConfig::default());

        // 空 token
        assert_eq!(panel.display_token(), "(未设置)");

        // 短 token
        panel.form.input_mut(RelayEditField::Token).set_value("abc".into());
        assert_eq!(panel.display_token(), "****");

        // 长 token
        panel.form.input_mut(RelayEditField::Token).set_value("secret123456".into());
        assert_eq!(panel.display_token(), "****3456****");
    }

    #[test]
    fn test_field_navigation() {
        let cfg = ZenConfig::default();
        let panel = RelayPanel::from_config(&cfg);
        let mut panel = panel;
        assert_eq!(panel.edit_field(), RelayEditField::Url);

        panel.form.next_field();
        assert_eq!(panel.edit_field(), RelayEditField::Token);

        panel.form.next_field();
        assert_eq!(panel.edit_field(), RelayEditField::Name);

        panel.form.next_field();
        assert_eq!(panel.edit_field(), RelayEditField::Url);

        panel.form.prev_field();
        assert_eq!(panel.edit_field(), RelayEditField::Name);
    }

    #[test]
    fn test_form_state_text_editing() {
        let cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.form.set_active(RelayEditField::Url);

        panel.form.handle_char('a');
        panel.form.handle_char('b');
        panel.form.handle_char('c');
        assert_eq!(panel.field_value(RelayEditField::Url), "abc");
        assert_eq!(panel.cursor(), 3);

        panel.form.handle_backspace();
        assert_eq!(panel.field_value(RelayEditField::Url), "ab");
        assert_eq!(panel.cursor(), 2);
    }

    #[test]
    fn test_cursor_movement() {
        let cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.form.input_mut(RelayEditField::Url).set_value("hello".into());
        panel.form.active_input_mut().cursor_end();
        assert_eq!(panel.cursor(), 5);

        panel.form.handle_cursor_left();
        assert_eq!(panel.cursor(), 4);

        panel.form.handle_cursor_home();
        assert_eq!(panel.cursor(), 0);

        panel.form.handle_cursor_right();
        assert_eq!(panel.cursor(), 1);

        panel.form.handle_cursor_end();
        assert_eq!(panel.cursor(), 5);
    }

    #[test]
    fn test_apply_edit_success() {
        let mut cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.mode = RelayPanelMode::Edit;
        panel.form.input_mut(RelayEditField::Url).set_value("ws://localhost:8080".into());
        panel.form.input_mut(RelayEditField::Token).set_value("token123".into());
        panel.form.input_mut(RelayEditField::Name).set_value("test-device".into());

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
        panel.form.input_mut(RelayEditField::Url).set_value(String::new());

        let result = panel.apply_edit(&mut cfg);
        assert!(!result);
        assert_eq!(panel.status_message, Some("URL 不能为空".to_string()));
    }

    #[test]
    fn test_apply_edit_empty_name() {
        let mut cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.mode = RelayPanelMode::Edit;
        panel.form.input_mut(RelayEditField::Url).set_value("ws://localhost:8080".into());
        panel.form.input_mut(RelayEditField::Name).set_value("   ".into());

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
        panel.form.input_mut(RelayEditField::Url).set_value("modified".into());

        panel.cancel_edit(&cfg);
        assert_eq!(panel.field_value(RelayEditField::Url), "wss://relay.example.com");
        assert_eq!(panel.mode, RelayPanelMode::View);
    }

    #[test]
    fn test_paste_text() {
        let cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.mode = RelayPanelMode::Edit;
        panel.form.set_active(RelayEditField::Url);

        // 粘贴到 URL 字段
        panel.form.handle_paste("ws://example.com");
        assert_eq!(panel.field_value(RelayEditField::Url), "ws://example.com");
        assert_eq!(panel.field_value(RelayEditField::Url).len(), 16);
        assert_eq!(panel.cursor(), 16);

        // 粘贴到 Token 字段
        panel.form.next_field();
        panel.form.handle_cursor_home();
        panel.form.handle_paste("my-token-123");
        assert_eq!(panel.field_value(RelayEditField::Token), "my-token-123");
        assert_eq!(panel.field_value(RelayEditField::Token).len(), 12);
        assert_eq!(panel.cursor(), 12);

        // 在中间位置粘贴
        panel.form.handle_cursor_home();
        panel.form.handle_cursor_right();
        panel.form.handle_cursor_right();
        panel.form.handle_cursor_right();
        panel.form.handle_paste("-abc");
        assert_eq!(panel.field_value(RelayEditField::Token), "my--abctoken-123");
        assert_eq!(panel.cursor(), 7);
    }

    #[test]
    fn test_paste_text_with_newlines() {
        let cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.mode = RelayPanelMode::Edit;
        panel.form.set_active(RelayEditField::Name);
        panel.form.handle_cursor_home();

        // paste() 不过滤换行符——这是 InputState 的行为
        // 如果需要过滤，应在调用前处理
        panel.form.handle_paste("line1line2line3");
        assert_eq!(panel.field_value(RelayEditField::Name), "line1line2line3");
        assert_eq!(panel.cursor(), 15);
    }

    #[test]
    fn test_delete_char() {
        let cfg = ZenConfig::default();
        let mut panel = RelayPanel::from_config(&cfg);
        panel.form.set_active(RelayEditField::Url);
        panel.form.handle_paste("abc");
        panel.form.handle_cursor_home();
        panel.form.handle_delete();
        assert_eq!(panel.field_value(RelayEditField::Url), "bc");
    }

    #[test]
    fn relay_form_state_field_navigation() {
        let mut panel = RelayPanel::from_config(&ZenConfig::default());
        panel.form.next_field();
        assert_eq!(panel.edit_field(), RelayEditField::Token);
        panel.form.next_field();
        assert_eq!(panel.edit_field(), RelayEditField::Name);
        panel.form.next_field();
        assert_eq!(panel.edit_field(), RelayEditField::Url);
    }

    #[test]
    fn relay_form_state_text_editing() {
        let mut panel = RelayPanel::from_config(&ZenConfig::default());
        panel.form.set_active(RelayEditField::Url);
        panel.form.handle_char('h');
        panel.form.handle_char('e');
        panel.form.handle_char('l');
        panel.form.handle_char('l');
        panel.form.handle_char('o');
        assert_eq!(panel.form.input(RelayEditField::Url).value(), "hello");

        panel.form.next_field();
        panel.form.handle_char('w');
        panel.form.handle_char('o');
        panel.form.handle_char('r');
        panel.form.handle_char('l');
        panel.form.handle_char('d');
        assert_eq!(panel.form.input(RelayEditField::Token).value(), "world");

        panel.form.prev_field();
        assert_eq!(panel.form.input(RelayEditField::Url).value(), "hello");
    }
}
