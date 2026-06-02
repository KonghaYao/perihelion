    fn make_skill(name: &str) -> SkillMetadata {
        SkillMetadata {
            name: name.to_string(),
            description: format!("{} skill", name),
            path: std::path::PathBuf::from(format!("/tmp/{}.md", name)),
        }
    }

    #[tokio::test]
    async fn test_candidates_count_slash_prefix_returns_cmd_plus_skills() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.session_mgr.current_mut().ui.textarea = build_textarea(false);
        app.session_mgr.current_mut()
            .ui
            .textarea
            .insert_str("/");
        app.session_mgr.current_mut()
            .commands
            .skills
            .push(make_skill("aaa-skill"));
        app.session_mgr.current_mut()
            .commands
            .skills
            .push(make_skill("zzz-skill"));

        let count = app.hint_candidates_count();
        // 命令数 + 2 技能，但最多 8 项
        let cmd_count = app.session_mgr.current_mut()
            .commands
            .command_registry
            .match_prefix("", &app.services.lc)
            .len();
        let expected = cmd_count + 2;
        assert_eq!(count, expected, "/ 前缀应返回命令数 + Skills 数");
    }

    #[tokio::test]
    async fn test_candidates_count_slash_prefix_filters_both() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.session_mgr.current_mut().ui.textarea = build_textarea(false);
        app.session_mgr.current_mut()
            .ui
            .textarea
            .insert_str("/mo");
        app.session_mgr.current_mut()
            .commands
            .skills
            .push(make_skill("commit"));
        app.session_mgr.current_mut()
            .commands
            .skills
            .push(make_skill("model-skill"));

        let count = app.hint_candidates_count();
        assert!(
            count >= 2,
            "/mo 前缀应至少返回 model 命令 + model-skill 技能"
        );
    }

    #[tokio::test]
    async fn test_candidates_count_no_prefix_returns_zero() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.session_mgr.current_mut().ui.textarea = build_textarea(false);
        app.session_mgr.current_mut()
            .ui
            .textarea
            .insert_str("hello");

        let count = app.hint_candidates_count();
        assert_eq!(count, 0, "无前缀应返回 0");
    }

    #[tokio::test]
    async fn test_hint_complete_command_at_cursor_0() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.session_mgr.current_mut().ui.textarea = build_textarea(false);
        app.session_mgr.current_mut()
            .ui
            .textarea
            .insert_str("/m");
        app.session_mgr.current_mut()
            .ui
            .hint_cursor = Some(0);

        app.hint_complete();
        let text: String = app.session_mgr.current_mut()
            .ui
            .textarea
            .lines()
            .iter()
            .map(|s| s.as_str())
            .collect();
        // 字母排序后第一个匹配 /m 的项
        let _items = app.build_hint_items();
        // hint_complete 已经清空了 hint_cursor 并修改了 textarea，这里直接验证
        assert!(text.starts_with("/"), "补全后应以 / 开头，实际: {}", text);
        assert!(
            app.session_mgr.current_mut()
                .ui
                .hint_cursor
                .is_none(),
            "补全后 hint_cursor 应为 None"
        );
    }

    #[tokio::test]
    async fn test_hint_complete_clears_hint_cursor() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.session_mgr.current_mut().ui.textarea = build_textarea(false);
        app.session_mgr.current_mut()
            .ui
            .textarea
            .insert_str("/m");
        app.session_mgr.current_mut()
            .ui
            .hint_cursor = Some(0);

        app.hint_complete();
        assert_eq!(
            app.session_mgr.current_mut()
                .ui
                .hint_cursor,
            None,
            "补全后 hint_cursor 应为 None"
        );
    }

    #[tokio::test]
    async fn test_hint_complete_skill_item() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24).await;
        app.session_mgr.current_mut().ui.textarea = build_textarea(false);
        app.session_mgr.current_mut()
            .ui
            .textarea
            .insert_str("/aaa");
        app.session_mgr.current_mut()
            .commands
            .skills
            .push(make_skill("aaa-skill"));

        // 找到 aaa-skill 在排序后的索引
        let items = app.build_hint_items();
        let idx = items
            .iter()
            .position(|it| it.name() == "aaa-skill")
            .expect("应有 aaa-skill 候选");
        app.session_mgr.current_mut()
            .ui
            .hint_cursor = Some(idx);

        app.hint_complete();
        let text: String = app.session_mgr.current_mut()
            .ui
            .textarea
            .lines()
            .iter()
            .map(|s| s.as_str())
            .collect();
        assert!(
            text.starts_with("/aaa-skill "),
            "应补全 Skill aaa-skill，实际: {}",
            text
        );
    }
