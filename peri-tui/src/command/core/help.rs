use crate::{
    app::{App, MessageViewModel},
    command::Command,
};

pub struct HelpCommand;

impl Command for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-help-description")
    }

    fn execute(&self, app: &mut App, _args: &str) {
        // 使用启动时预计算的列表（command_registry 在 dispatch 时已被 std::mem::take 取出）
        let mut lines = vec!["可用命令：".to_string()];
        for (name, desc, aliases) in &app.session_mgr.current_mut().commands.command_help_list {
            let alias_str = if aliases.is_empty() {
                String::new()
            } else {
                format!(" (别名: /{})", aliases.join(", /"))
            };
            lines.push(format!("  /{:<10} {}{}", name, desc, alias_str));
        }

        // Skills 说明
        let skills_count = app.session_mgr.current_mut().commands.skills.len();
        if skills_count > 0 {
            lines.push("".to_string());
            lines.push(format!(
                "Skills（{} 个可用）: 输入 # 前缀查看",
                skills_count
            ));
        } else {
            lines.push("".to_string());
            lines.push("Skills: 将 .md 文件放入 .claude/skills/ 目录即可添加".to_string());
        }

        // 全局快捷键提示
        lines.push("".to_string());
        lines.push(
            format!(
                "快捷键：Shift+Tab 切换权限模式 │ {} 切换模型 │ Shift+Enter 换行 │ Esc 退出 │ Ctrl+C 中断",
                crate::event::keyboard::cycle_model_label()
            ),
        );

        let vm = MessageViewModel::system(lines.join("\n"));
        app.session_mgr
            .current_mut()
            .messages
            .view_messages
            .push(vm);
        app.render_rebuild();
    }
}
