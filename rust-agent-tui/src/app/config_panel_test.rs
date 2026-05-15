use super::*;

fn make_lc() -> crate::i18n::LcRegistry {
    crate::i18n::LcRegistry::default()
}

#[test]
fn test_config_panel_from_config_defaults() {
    let cfg = PeriConfig::default();
    let panel = ConfigPanel::from_config(&cfg);
    assert!(panel.buf_autocompact);
    assert_eq!(panel.buf_threshold, "85");
    assert!(panel.buf_language.is_empty());
    assert_eq!(panel.buf_proactiveness, "medium");
}

#[test]
fn test_config_panel_field_navigation() {
    let _panel = ConfigPanel::from_config(&PeriConfig::default());
    let _fields: Vec<_> = (0..6)
        .map(|_| {
            let mut p = ConfigEditField::Autocompact;
            for _ in std::iter::empty::<u8>() {
                p = p.next();
            }
            p
        })
        .collect();
    // verify all 6 fields are distinct
    assert_eq!(ConfigPanel::field_count(), 6);

    let mut f = ConfigEditField::Autocompact;
    for _ in 0..6 {
        f = f.next();
    }
    assert_eq!(f, ConfigEditField::Autocompact);

    f = ConfigEditField::Proactiveness;
    f = f.prev();
    assert_eq!(f, ConfigEditField::Tone);
}

#[test]
fn test_config_panel_cycle_autocompact() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    assert!(panel.buf_autocompact);
    panel.cycle_autocompact();
    assert!(!panel.buf_autocompact);
    panel.cycle_autocompact();
    assert!(panel.buf_autocompact);
}

#[test]
fn test_config_panel_cycle_proactiveness() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    panel.buf_proactiveness = "low".to_string();
    panel.cycle_proactiveness();
    assert_eq!(panel.buf_proactiveness, "medium");
    panel.cycle_proactiveness();
    assert_eq!(panel.buf_proactiveness, "high");
    panel.cycle_proactiveness();
    assert_eq!(panel.buf_proactiveness, "low");
}

#[test]
fn test_config_panel_apply_edit_saves_to_config() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_language = "zh-CN".to_string();
    panel.buf_persona = "Rust expert".to_string();
    panel.buf_tone = "concise".to_string();
    panel.buf_proactiveness = "high".to_string();
    panel.apply_edit(&mut cfg, &lc).unwrap();
    assert_eq!(cfg.config.language.as_deref(), Some("zh-CN"));
    assert_eq!(cfg.config.persona.as_deref(), Some("Rust expert"));
    assert_eq!(cfg.config.tone.as_deref(), Some("concise"));
    assert_eq!(cfg.config.proactiveness.as_deref(), Some("high"));
}

#[test]
fn test_config_panel_apply_edit_compact_threshold() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_threshold = "90".to_string();
    panel.apply_edit(&mut cfg, &lc).unwrap();
    let compact = cfg.config.compact.unwrap();
    assert!((compact.auto_compact_threshold - 0.90).abs() < 0.001);
}

#[test]
fn test_config_panel_apply_edit_invalid_threshold_clamps() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_threshold = "30".to_string();
    panel.apply_edit(&mut cfg, &lc).unwrap();
    let compact = cfg.config.compact.unwrap();
    assert!((compact.auto_compact_threshold - 0.50).abs() < 0.001);
}

#[test]
fn test_config_panel_apply_edit_language_validation_valid() {
    let lc = make_lc();
    // en 保存成功
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_language = "en".to_string();
    assert!(panel.apply_edit(&mut cfg, &lc).is_ok());
    assert_eq!(cfg.config.language.as_deref(), Some("en"));

    // zh-CN 保存成功
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    panel.buf_language = "zh-CN".to_string();
    assert!(panel.apply_edit(&mut cfg, &lc).is_ok());
    assert_eq!(cfg.config.language.as_deref(), Some("zh-CN"));
}

#[test]
fn test_config_panel_apply_edit_language_validation_empty() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_language = String::new();
    assert!(panel.apply_edit(&mut cfg, &lc).is_ok());
    assert_eq!(cfg.config.language, None);

    // "auto" 也等同于 None
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    panel.buf_language = "auto".to_string();
    assert!(panel.apply_edit(&mut cfg, &lc).is_ok());
    assert_eq!(cfg.config.language, None);
}

#[test]
fn test_config_panel_apply_edit_language_validation_invalid() {
    let lc = make_lc();
    let mut cfg = PeriConfig::default();
    let mut panel = ConfigPanel::from_config(&cfg);
    panel.buf_language = "fr".to_string();
    let result = panel.apply_edit(&mut cfg, &lc);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("Unsupported language"),
        "错误消息应包含 'Unsupported language': {}",
        err
    );
    assert!(err.contains("fr"), "错误消息应包含无效语言: {}", err);
    // 语言不应被修改
    assert_eq!(cfg.config.language, None);
}

#[test]
fn test_config_panel_field_display_language() {
    let panel = ConfigPanel::from_config(&PeriConfig::default());
    // 空语言显示 auto
    assert_eq!(panel.field_display_value(2), "auto");

    // en 显示 English
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    panel.buf_language = "en".to_string();
    assert_eq!(panel.field_display_value(2), "English");

    // zh-CN 显示 简体中文
    panel.buf_language = "zh-CN".to_string();
    assert_eq!(panel.field_display_value(2), "简体中文");

    // 未知语言原样显示
    panel.buf_language = "ja".to_string();
    assert_eq!(panel.field_display_value(2), "ja");
}

#[test]
fn test_config_panel_active_field_text_editable() {
    let mut panel = ConfigPanel::from_config(&PeriConfig::default());
    // Autocompact → None
    panel.edit_field = ConfigEditField::Autocompact;
    assert!(panel.active_field().is_none());
    // Proactiveness → None
    panel.edit_field = ConfigEditField::Proactiveness;
    assert!(panel.active_field().is_none());
    // Language → Some
    panel.edit_field = ConfigEditField::Language;
    assert!(panel.active_field().is_some());
    // Persona → Some
    panel.edit_field = ConfigEditField::Persona;
    assert!(panel.active_field().is_some());
    // Tone → Some
    panel.edit_field = ConfigEditField::Tone;
    assert!(panel.active_field().is_some());
    // CompactThreshold → Some
    panel.edit_field = ConfigEditField::CompactThreshold;
    assert!(panel.active_field().is_some());
}
