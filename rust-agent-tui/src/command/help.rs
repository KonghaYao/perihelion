use crate::app::{App, MessageViewModel};
use crate::command::Command;

pub struct HelpCommand;

impl Command for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }

    fn description(&self) -> &str {
        "列出所有可用命令"
    }

    fn execute(&self, app: &mut App, _args: &str) {
        // 使用启动时预计算的列表（command_registry 在 dispatch 时已被 std::mem::take 取出）
        let mut lines = vec!["可用命令：".to_string()];
        for (name, desc) in &app.core.command_help_list {
            lines.push(format!("  /{:<10} {}", name, desc));
        }

        // Skills 说明
        let skills_count = app.core.skills.len();
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
        lines.push("快捷键：Shift+Tab 切换权限模式 │ Esc 退出 │ Ctrl+C 中断".to_string());

        let vm = MessageViewModel::system(lines.join("\n"));
        app.core.view_messages.push(vm.clone());
        let _ = app
            .core
            .render_tx
            .send(crate::ui::render_thread::RenderEvent::AddMessage(vm));
    }
}
