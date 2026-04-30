use super::Command;
use crate::app::App;
use crate::ui::message_view::MessageViewModel;
use crate::ui::render_thread::RenderEvent;

pub struct LoopCommand;

impl Command for LoopCommand {
    fn name(&self) -> &str {
        "loop"
    }

    fn description(&self) -> &str {
        "注册定时循环任务（自然语言描述，如 /loop 每隔5分钟提醒我喝水）"
    }

    fn execute(&self, app: &mut App, args: &str) {
        let args = args.trim();
        if args.is_empty() {
            let vm = MessageViewModel::system(
                "用法: /loop <自然语言时间描述> <提示词>\n例如: /loop 每隔5分钟提醒我喝水"
                    .to_string(),
            );
            app.core.view_messages.push(vm.clone());
            let _ = app.core.render_tx.send(RenderEvent::AddMessage(vm));
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

    fn headless_app() -> App {
        App::new_headless(80, 24).0
    }

    #[tokio::test]
    async fn test_loop_cmd_empty_args_shows_usage() {
        let mut app = headless_app();
        let cmd = LoopCommand;
        cmd.execute(&mut app, "");
        assert_eq!(app.core.view_messages.len(), 1);
        let text = format!("{:?}", app.core.view_messages[0]);
        assert!(
            text.contains("用法"),
            "空参数应显示用法提示，实际: {}",
            text
        );
    }

    #[tokio::test]
    async fn test_loop_cmd_empty_whitespace_shows_usage() {
        let mut app = headless_app();
        let cmd = LoopCommand;
        cmd.execute(&mut app, "   ");
        assert_eq!(app.core.view_messages.len(), 1);
        let text = format!("{:?}", app.core.view_messages[0]);
        assert!(text.contains("用法"), "纯空格参数应显示用法提示");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_loop_cmd_valid_args_submits_message() {
        let mut app = headless_app();
        let initial_len = app.core.view_messages.len();
        let cmd = LoopCommand;
        cmd.execute(&mut app, "每隔5分钟提醒我喝水");
        // submit_message 会添加一条 user 消息到 view_messages
        assert!(
            app.core.view_messages.len() > initial_len,
            "有参数时应提交消息给 Agent"
        );
        // 检查提交的消息包含 cron_register 指令
        let text = format!("{:?}", app.core.view_messages);
        assert!(
            text.contains("cron_register"),
            "提交的消息应包含 cron_register 指令，实际: {}",
            text
        );
    }

    #[test]
    fn test_loop_cmd_name() {
        let cmd = LoopCommand;
        assert_eq!(cmd.name(), "loop");
    }

    #[test]
    fn test_loop_cmd_description_not_empty() {
        let cmd = LoopCommand;
        assert!(!cmd.description().is_empty());
    }
}
