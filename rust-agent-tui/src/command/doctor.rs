use super::Command;
use crate::app::App;
use crate::ui::message_view::MessageViewModel;

pub struct DoctorCommand;

impl Command for DoctorCommand {
    fn name(&self) -> &str {
        "doctor"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-doctor-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        let mut lines = vec!["Doctor 检查结果：".to_string(), "".to_string()];

        // 1. Settings 文件
        let settings_path = dirs_next::home_dir().map(|h| h.join(".peri").join("settings.json"));
        let settings_status = match &settings_path {
            Some(p) if p.is_file() => format!("OK  {}", p.display()),
            Some(p) => format!("Missing  {}", p.display()),
            None => "Missing  无法获取 home 目录".to_string(),
        };
        lines.push("| 检查项 | 状态 | 详情 |".to_string());
        lines.push("|--------|------|------|".to_string());
        lines.push(format!("| Settings | {} |", settings_status));

        // 2. API Key
        let has_anthropic = std::env::var("ANTHROPIC_API_KEY")
            .map(|k| !k.is_empty())
            .unwrap_or(false);
        let has_openai = std::env::var("OPENAI_API_KEY")
            .map(|k| !k.is_empty())
            .unwrap_or(false);
        let api_status = if has_anthropic || has_openai {
            let keys: Vec<&str> = [
                has_anthropic.then_some("ANTHROPIC_API_KEY"),
                has_openai.then_some("OPENAI_API_KEY"),
            ]
            .into_iter()
            .flatten()
            .collect();
            format!("OK  {}", keys.join(" + "))
        } else {
            "Missing  未设置 ANTHROPIC_API_KEY 或 OPENAI_API_KEY".to_string()
        };
        lines.push(format!("| API Key | {} |", api_status));

        // 3. Provider 配置
        let provider_status = match &app.services.peri_config {
            Some(cfg) if !cfg.config.providers.is_empty() => {
                let active = &cfg.config.active_provider_id;
                let provider = cfg.config.providers.iter().find(|p| p.id == *active);
                match provider {
                    Some(p) => format!(
                        "OK  {} ({})",
                        p.display_name(),
                        p.models
                            .get_model(&cfg.config.active_alias)
                            .unwrap_or("default")
                    ),
                    None => format!("No Provider  active_provider_id '{}' 未找到", active),
                }
            }
            _ => "No Provider  未配置任何 Provider".to_string(),
        };
        lines.push(format!("| Provider | {} |", provider_status));

        // 4. MCP 配置
        let mcp_status = if app.services.mcp_pool.is_some() {
            "OK  MCP 连接池已初始化".to_string()
        } else {
            let mcp_project = std::path::Path::new(&app.services.cwd).join(".mcp.json");
            if mcp_project.is_file() {
                "None  .mcp.json 存在但 MCP 未初始化".to_string()
            } else {
                "None  未配置 MCP 服务器".to_string()
            }
        };
        lines.push(format!("| MCP | {} |", mcp_status));

        // 5. Model Alias
        let alias_status = match &app.services.peri_config {
            Some(cfg) => {
                let p = cfg
                    .config
                    .providers
                    .iter()
                    .find(|p| p.id == cfg.config.active_provider_id);
                match p {
                    Some(p) => {
                        let aliases: Vec<String> = ["opus", "sonnet", "haiku"]
                            .iter()
                            .filter(|a| !p.models.get_model(a).unwrap_or("").is_empty())
                            .map(|a| a.to_string())
                            .collect();
                        if aliases.is_empty() {
                            "No Alias  未配置任何模型别名".to_string()
                        } else {
                            format!("OK  {}", aliases.join("/"))
                        }
                    }
                    None => "No Alias  无活跃 Provider".to_string(),
                }
            }
            _ => "No Alias  未配置".to_string(),
        };
        lines.push(format!("| Model Alias | {} |", alias_status));

        let vm = MessageViewModel::system(lines.join("\n"));
        app.session_mgr
            .current_mut()
            .messages
            .view_messages
            .push(vm);
        app.render_rebuild();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    async fn headless_app() -> App {
        App::new_headless(80, 24).await.0
    }

    #[tokio::test]
    async fn test_doctor_no_config() {
        let mut app = headless_app().await;
        let cmd = DoctorCommand;
        cmd.execute(&mut app, "");
        let msgs = &app.session_mgr.current_mut().messages.view_messages;
        assert_eq!(msgs.len(), 1);
        let text = format!("{:?}", msgs[0]);
        assert!(
            text.contains("No Provider") || text.contains("Missing"),
            "无配置时应显示 No Provider 或 Missing: {}",
            text
        );
    }
}
