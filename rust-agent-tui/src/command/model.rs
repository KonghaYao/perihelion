use super::Command;
use crate::app::{agent, App, MessageViewModel};

pub struct ModelCommand;

impl Command for ModelCommand {
    fn name(&self) -> &str {
        "model"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-model-description")
    }

    fn execute(&self, app: &mut App, args: &str) {
        let alias = args.trim().to_lowercase();
        match alias.as_str() {
            "opus" | "sonnet" | "haiku" => {
                let cfg = app
                    .services
                    .peri_config
                    .get_or_insert_with(Default::default);
                cfg.config.active_alias = alias.clone();
                if let Err(e) = App::save_config(cfg, app.services.config_path_override.as_deref())
                {
                    app.session_mgr.sessions[app.session_mgr.active]
                        .messages
                        .view_messages
                        .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
                }
                if let Some(p) = agent::LlmProvider::from_config(cfg) {
                    app.services.provider_name = p.display_name().to_string();
                    app.services.model_name = p.model_name().to_string();
                }
            }
            _ => {
                app.open_model_panel();
            }
        }
    }
}
