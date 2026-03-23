use crate::app::{agent, App};
use super::Command;

pub struct ModelCommand;

impl Command for ModelCommand {
    fn name(&self) -> &str {
        "model"
    }

    fn description(&self) -> &str {
        "打开 Provider / Model 配置面板；带参数时直接切换别名（opus/sonnet/haiku）"
    }

    fn execute(&self, app: &mut App, args: &str) {
        let alias = args.trim().to_lowercase();
        match alias.as_str() {
            "opus" | "sonnet" | "haiku" => {
                let cfg = app.zen_config.get_or_insert_with(Default::default);
                cfg.config.active_alias = alias.clone();
                let _ = crate::config::save(cfg);
                if let Some(p) = agent::LlmProvider::from_config(cfg) {
                    app.provider_name = p.display_name().to_string();
                    app.model_name = p.model_name().to_string();
                }
            }
            _ => {
                app.open_model_panel();
            }
        }
    }
}
