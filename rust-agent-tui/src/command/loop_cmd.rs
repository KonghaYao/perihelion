use super::Command;
use crate::app::App;
use crate::ui::message_view::MessageViewModel;

pub struct LoopCommand;

impl Command for LoopCommand {
    fn name(&self) -> &str {
        "loop"
    }

    fn description(&self, _lc: &crate::i18n::LcRegistry) -> String {
        _lc.tr("command-loop-description")
    }

    fn execute(&self, app: &mut App, args: &str) {
        let args = args.trim();
        if args.is_empty() {
            let vm = MessageViewModel::system(
                "用法: /loop <自然语言时间描述> <提示词>\n例如: /loop 每隔5分钟提醒我喝水"
                    .to_string(),
            );
            app.session_mgr.sessions[app.session_mgr.active]
                .messages
                .view_messages
                .push(vm);
            app.render_rebuild();
            return;
        }

        // 将用户输入包装为指令提交给 Agent，由 LLM 解析时间并调用 cron_register 工具
        let prompt = format!(
            "请根据以下要求注册一个定时循环任务。\
            你需要解析用户描述的时间间隔，转换为标准 5 段 cron 表达式，\
            然后调用 cron_register 工具完成注册。\n\n\
            用户要求: {}\n\n\
            注意：直接调用 cron_register 工具，不需要额外确认。",
            args
        );

        app.submit_message(prompt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    include!("loop_cmd_test.rs");
}
