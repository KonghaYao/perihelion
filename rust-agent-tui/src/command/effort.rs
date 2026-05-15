use super::Command;
use crate::app::App;
use crate::config::ThinkingConfig;
use crate::ui::message_view::MessageViewModel;

pub struct EffortCommand;

impl Command for EffortCommand {
    fn name(&self) -> &str {
        "effort"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-effort-description")
    }

    fn execute(&self, app: &mut App, args: &str) {
        let arg = args.trim().to_lowercase();
        match arg.as_str() {
            "low" | "medium" | "high" | "xhigh" | "max" => {
                let cfg = app
                    .services
                    .peri_config
                    .get_or_insert_with(Default::default);
                cfg.config.thinking = Some(ThinkingConfig {
                    enabled: cfg.config.thinking.as_ref().is_none_or(|t| t.enabled),
                    budget_tokens: cfg
                        .config
                        .thinking
                        .as_ref()
                        .map_or(8000, |t| t.budget_tokens),
                    effort: arg.clone(),
                });
                if let Err(e) = App::save_config(cfg, app.services.config_path_override.as_deref())
                {
                    let vm = MessageViewModel::system(format!("配置保存失败: {}", e));
                    app.session_mgr
                        .current_mut()
                        .messages
                        .view_messages
                        .push(vm);
                    return;
                }
                let vm = MessageViewModel::system(format!("推理力度已设为 {}", arg));
                app.session_mgr
                    .current_mut()
                    .messages
                    .view_messages
                    .push(vm);
                app.render_rebuild();
            }
            _ => {
                let current = app
                    .services
                    .peri_config
                    .as_ref()
                    .and_then(|c| c.config.thinking.as_ref())
                    .map(|t| t.effort.as_str())
                    .unwrap_or("high");
                let vm = MessageViewModel::system(format!(
                    "当前推理力度: {}\n用法: /effort low|medium|high|xhigh|max",
                    current
                ));
                app.session_mgr
                    .current_mut()
                    .messages
                    .view_messages
                    .push(vm);
                app.render_rebuild();
            }
        }
    }
}
