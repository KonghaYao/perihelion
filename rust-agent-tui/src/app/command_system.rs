use rust_agent_middlewares::prelude::SkillMetadata;

use crate::command::CommandRegistry;

/// 命令系统：命令注册表、帮助列表、Skills 元数据。
pub struct CommandSystem {
    pub command_registry: CommandRegistry,
    pub command_help_list: Vec<(String, String, Vec<String>)>,
    pub skills: Vec<SkillMetadata>,
}

impl CommandSystem {
    pub fn new(
        command_registry: CommandRegistry,
        skills: Vec<SkillMetadata>,
        lc: &crate::i18n::LcRegistry,
    ) -> Self {
        let command_help_list = command_registry.list(lc);
        Self {
            command_registry,
            command_help_list,
            skills,
        }
    }
}
