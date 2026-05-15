    fn make_lc() -> crate::i18n::LcRegistry {
        crate::i18n::LcRegistry::default()
    }

    #[test]
    fn test_tips_contains_slash_command_hint() {
        let lc = make_lc();
        let tip = lc.tr("tip-0");
        assert!(tip.contains("/"), "tip-0 应包含 '/' 提示: {}", tip);
    }

    #[test]
    fn test_tips_tab_hint() {
        let lc = make_lc();
        let tip = lc.tr("tip-0");
        assert!(tip.contains("Tab"), "tip-0 应包含 'Tab': {}", tip);
    }

    #[test]
    fn test_pick_tip_returns_non_empty() {
        let lc = make_lc();
        for i in 0..18 {
            let tip = super::pick_tip(i as u64 * 180, &lc);
            assert!(!tip.is_empty(), "tip-{} 不应为空", i);
            assert_ne!(tip, format!("tip-{}", i), "tip-{} 应有翻译内容", i);
        }
    }
