    async fn render_headless_login_browse() -> (App, crate::ui::headless::HeadlessHandle) {
        let (mut app, mut handle) = App::new_headless(120, 30).await;
        let mut panel = LoginPanel {
            providers: vec![ProviderConfig {
                id: "test".to_string(),
                provider_type: "openai".to_string(),
                base_url: "http://localhost".to_string(),
                api_key: "sk-test".to_string(),
                models: crate::config::ProviderModels {
                    opus: "opus-model".to_string(),
                    sonnet: "sonnet-model".to_string(),
                    haiku: "haiku-model".to_string(),
                },
                ..Default::default()
            }],
            mode: LoginPanelMode::Browse,
            browse_list: crate::app::panel_list::PanelList::new(),
            edit_field: LoginEditField::Name,
            buf_name: String::new(),
            buf_type: String::new(),
            buf_base_url: String::new(),
            buf_api_key: String::new(),
            buf_opus_model: String::new(),
            buf_sonnet_model: String::new(),
            buf_haiku_model: String::new(),
            cur_name: 0,
            cur_base_url: 0,
            cur_api_key: 0,
            cur_opus_model: 0,
            cur_sonnet_model: 0,
            cur_haiku_model: 0,
        };
        panel.browse_list.set_items(vec![(); 1]);
        app.session_mgr.current_mut()
            .session_panels
            .open(crate::app::panel_manager::PanelState::Login(panel.clone()));
        app.session_mgr.current_mut()
            .session_panels
            .open(crate::app::panel_manager::PanelState::Login(panel));
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        (app, handle)
    }

    #[tokio::test]
    async fn test_login_browse_no_single_letter_hints() {
        let (_, handle) = render_headless_login_browse().await;
        let snap = handle.snapshot().join("\n");
        assert!(
            snap.contains("Ctrl+N"),
            "新建应显示 Ctrl+N 而非单字母 n，实际:\n{}",
            snap
        );
        assert!(
            snap.contains("Ctrl+D"),
            "删除应显示 Ctrl+D 而非单字母 d，实际:\n{}",
            snap
        );
    }

    async fn render_headless_login_edit() -> (App, crate::ui::headless::HeadlessHandle) {
        let (mut app, mut handle) = App::new_headless(120, 30).await;
        let panel = LoginPanel {
            providers: vec![],
            mode: LoginPanelMode::New,
            browse_list: crate::app::panel_list::PanelList::new(),
            edit_field: LoginEditField::Name,
            buf_name: String::new(),
            buf_type: "openai".to_string(),
            buf_base_url: String::new(),
            buf_api_key: String::new(),
            buf_opus_model: String::new(),
            buf_sonnet_model: String::new(),
            buf_haiku_model: String::new(),
            cur_name: 0,
            cur_base_url: 0,
            cur_api_key: 0,
            cur_opus_model: 0,
            cur_sonnet_model: 0,
            cur_haiku_model: 0,
        };
        app.session_mgr.current_mut()
            .session_panels
            .open(crate::app::panel_manager::PanelState::Login(panel.clone()));
        app.session_mgr.current_mut()
            .session_panels
            .open(crate::app::panel_manager::PanelState::Login(panel));
        handle
            .terminal
            .draw(|f| crate::ui::main_ui::render(f, &mut app))
            .unwrap();
        (app, handle)
    }

    #[tokio::test]
    async fn test_login_edit_has_paste_hint() {
        let (_, handle) = render_headless_login_edit().await;
        let snap = handle.snapshot().join("\n");
        assert!(
            snap.contains("Ctrl+V"),
            "编辑模式应显示 Ctrl+V 粘贴提示，实际:\n{}",
            snap
        );
    }
