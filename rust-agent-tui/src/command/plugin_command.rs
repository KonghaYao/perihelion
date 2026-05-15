use super::{App, Command};
use rust_agent_middlewares::plugin::{CommandEntry, CommandSource};

/// 将插件的 CommandEntry 适配为 TUI Command trait
pub struct PluginCommandAdapter {
    entry: CommandEntry,
}

impl PluginCommandAdapter {
    pub fn new(entry: CommandEntry) -> Self {
        Self { entry }
    }
}

impl Command for PluginCommandAdapter {
    fn name(&self) -> &str {
        &self.entry.name
    }
    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        self.entry.description.clone()
    }
    fn execute(&self, app: &mut App, _args: &str) {
        match &self.entry.source {
            CommandSource::Plugin { path } => {
                if let Ok(content) = std::fs::read_to_string(path) {
                    app.active_mut().ui.textarea.insert_str(&content);
                } else {
                    tracing::warn!(path = %path.display(), "读取插件命令文件失败");
                }
            }
            CommandSource::Builtin => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::LcRegistry;
    use rust_agent_middlewares::plugin::CommandSource;

    fn make_lc() -> LcRegistry {
        LcRegistry::new(None)
    }

    #[test]
    fn test_adapter_name_returns_entry_name() {
        let entry = CommandEntry {
            name: "test:cmd".into(),
            description: "desc".into(),
            source: CommandSource::Builtin,
        };
        let adapter = PluginCommandAdapter::new(entry);
        assert_eq!(adapter.name(), "test:cmd");
    }

    #[test]
    fn test_adapter_description_returns_entry_description() {
        let entry = CommandEntry {
            name: "test:cmd".into(),
            description: "my description".into(),
            source: CommandSource::Builtin,
        };
        let adapter = PluginCommandAdapter::new(entry);
        let lc = make_lc();
        assert_eq!(adapter.description(&lc), "my description");
    }
}
