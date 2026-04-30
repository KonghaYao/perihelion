use super::Command;
use crate::app::{agent, App, MessageViewModel};

pub struct ModelCommand;

impl Command for ModelCommand {
    fn name(&self) -> &str {
        "model"
    }

    fn description(&self) -> &str {
        "打开模型选择面板（Provider + 级别 + Thinking）；带参数时直接切换别名（opus/sonnet/haiku）"
    }

    fn execute(&self, app: &mut App, args: &str) {
        let alias = args.trim().to_lowercase();
        match alias.as_str() {
            "opus" | "sonnet" | "haiku" => {
                let cfg = app.zen_config.get_or_insert_with(Default::default);
                cfg.config.active_alias = alias.clone();
                if let Err(e) = crate::config::save(cfg) {
                    app.core
                        .view_messages
                        .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
                }
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
