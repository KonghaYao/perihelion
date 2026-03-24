//! Headless 测试支持模块
//!
//! 提供 [`HeadlessHandle`]，允许在无真实终端的情况下对 TUI 渲染管道进行端到端集成测试。
//! 渲染路径（`main_ui::render`）与生产代码完全一致。
//!
//! 使用方式：
//! ```rust,ignore
//! let (mut app, mut handle) = App::new_headless(120, 30);
//! app.push_agent_event(AgentEvent::AssistantChunk("Hello".into()));
//! app.process_pending_events();
//! handle.wait_for_render().await;
//! handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
//! assert!(handle.contains("Hello"));
//! ```

use std::sync::Arc;

use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::Notify;

/// Headless 测试句柄，包含 TestBackend Terminal 和渲染通知
pub struct HeadlessHandle {
    pub terminal: Terminal<TestBackend>,
    pub render_notify: Arc<Notify>,
}

impl HeadlessHandle {
    /// 截取当前 buffer 为纯文本行列表（去除每行尾部空格，跳过宽字符填充 cell）
    pub fn snapshot(&self) -> Vec<String> {
        let buffer = self.terminal.backend().buffer();
        let width = buffer.area.width as usize;
        buffer
            .content
            .chunks(width)
            .map(|row| {
                // skip=true 的 cell 是宽字符的占位填充，直接跳过
                let line: String = row.iter()
                    .filter_map(|cell| {
                        if cell.skip { None } else { Some(cell.symbol()) }
                    })
                    .collect();
                line.trim_end().to_string()
            })
            .collect()
    }

    /// 检查任意行是否包含指定文本
    pub fn contains(&self, text: &str) -> bool {
        self.snapshot().iter().any(|line| line.contains(text))
    }

        /// 等待渲染线程完成一次渲染（内部 notify.notified().await，无 sleep）
    pub async fn wait_for_render(&self) {
        self.render_notify.notified().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AgentEvent, App};
    use crate::ui::main_ui;
    use crate::ui::render_thread::RenderEvent;
    use crate::app::MessageViewModel;

    #[tokio::test]
    async fn test_snapshot_row_count() {
        let (_app, handle) = App::new_headless(80, 24);
        assert_eq!(handle.snapshot().len(), 24, "snapshot 应返回 24 行");
    }

    #[tokio::test]
    async fn test_assistant_chunk_renders() {
        let (mut app, mut handle) = App::new_headless(120, 30);
        // 先注册监听，再发送事件，确保不错过通知
        let notified = handle.render_notify.notified();
        app.push_agent_event(AgentEvent::AssistantChunk("Hello world".into()));
        app.push_agent_event(AgentEvent::Done);
        app.process_pending_events();
        notified.await;
        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        assert!(handle.contains("Agent"), "应显示 Agent 标头，实际:\n{}", snap.join("\n"));
        assert!(handle.contains("Hello world"), "应显示消息内容，实际:\n{}", snap.join("\n"));
    }

    #[tokio::test]
    async fn test_tool_call_renders() {
        let (mut app, mut handle) = App::new_headless(120, 30);
        let notified = handle.render_notify.notified();
        app.push_agent_event(AgentEvent::ToolCall {
            tool_call_id: "t1".into(),
            name: "read_file".into(),
            display: "ReadFile".into(),
            args: Some("src/main.rs".into()),
            is_error: false,
        });
        app.process_pending_events();
        notified.await;
        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        // ToolBlock 显示 display 字段或 name 字段
        let has_tool = snap.iter().any(|l| l.contains("read_file") || l.contains("ReadFile"));
        assert!(has_tool, "应显示工具调用块，实际内容:\n{}", snap.join("\n"));
    }

    #[tokio::test]
    async fn test_user_message_renders() {
        let (mut app, mut handle) = App::new_headless(120, 30);
        // 先注册监听，再发送事件，避免时序问题
        let notified = handle.render_notify.notified();
        // 使用 ASCII 内容避免 CJK 宽字符在 buffer 中的空格填充问题
        let vm = MessageViewModel::user("hello from user".into());
        app.view_messages.push(vm.clone());
        let _ = app.render_tx.send(RenderEvent::AddMessage(vm));
        notified.await;
        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        assert!(handle.contains("hello from user"), "应显示用户消息，实际内容:\n{}", snap.join("\n"));
    }

    #[tokio::test]
    async fn test_clear_empties_render_cache() {
        let (mut app, mut handle) = App::new_headless(120, 30);

        // 先发内容，等待所有渲染事件（AddMessage + AppendChunk = 2 次通知）处理完
        for _ in 0..2 {
            let notified = handle.render_notify.notified();
            app.push_agent_event(AgentEvent::AssistantChunk("SomeUniqueContent".into()));
            app.process_pending_events();
            notified.await;
        }

        // 验证 RenderCache 有内容
        let lines_before = app.render_cache.read().total_lines;
        assert!(lines_before > 0, "清空前应有内容");

        // 注册监听后发送 Clear，确保不错过通知
        let notified_clear = handle.render_notify.notified();
        app.view_messages.clear();
        let _ = app.render_tx.send(RenderEvent::Clear);
        notified_clear.await;

        // 验证 RenderCache 已清空
        let cache = app.render_cache.read();
        assert_eq!(cache.total_lines, 0, "清空后 RenderCache 应为空");
    }

    #[tokio::test]
    async fn test_tool_call_message_collapsed_by_default() {
        let (mut app, mut handle) = App::new_headless(120, 30);

        // 创建一个带工具调用的 AI 消息
        let tool_calls = vec![rust_create_agent::messages::ToolCallRequest {
            id: "tc1".into(),
            name: "bash".into(),
            arguments: serde_json::json!({"command": "ls"}),
        }];

        let ai_msg = rust_create_agent::messages::BaseMessage::ai_with_tool_calls(
            "I'll run ls for you",
            tool_calls,
        );

        // 监听渲染事件
        let notified = handle.render_notify.notified();

        // 发送带工具调用的消息
        app.push_agent_event(AgentEvent::MessageAdded(ai_msg));
        app.process_pending_events();

        // 等待渲染
        notified.await;
        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();

        let snap = handle.snapshot();
        // 默认情况下，工具调用消息应该是隐藏的（collapsed=true）
        let has_tool_call_text = snap.iter().any(|l| l.contains("I'll run ls for you") || l.contains("bash"));
        assert!(!has_tool_call_text, "工具调用消息默认应该是隐藏的，但实际显示为:\n{}", snap.join("\n"));
    }

    mod markdown_tests {
        use crate::ui::markdown::parse_markdown;
        use ratatui::style::{Color, Modifier};

        fn all_text(text: &ratatui::text::Text) -> String {
            text.lines
                .iter()
                .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
                .collect::<Vec<_>>()
                .join("")
        }

        #[test]
        fn test_md_heading() {
            let text = parse_markdown("# Hello World");
            // H1 首行应有 ━━ 前缀
            let first_line = &text.lines[0];
            let all_content: String =
                first_line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(
                all_content.contains("━━"),
                "H1 首行应含 ━━ 前缀，实际: {all_content:?}"
            );
            assert!(
                all_content.contains("Hello World"),
                "H1 首行应含标题文字，实际: {all_content:?}"
            );
            // 检查颜色为 Cyan
            let has_cyan = first_line
                .spans
                .iter()
                .any(|s| s.style.fg == Some(Color::Cyan));
            assert!(has_cyan, "H1 应为 Cyan 颜色");
        }

        #[test]
        fn test_md_inline_styles() {
            let text = parse_markdown("**bold** *italic* ~~strike~~");
            let all = all_text(&text);
            assert!(all.contains("bold"), "应含 bold 文字");
            assert!(all.contains("italic"), "应含 italic 文字");
            assert!(all.contains("strike"), "应含 strike 文字");

            let has_bold = text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
                s.style.add_modifier.contains(Modifier::BOLD)
                    && s.content.contains("bold")
            });
            assert!(has_bold, "bold span 应有 BOLD modifier");

            let has_italic = text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
                s.style.add_modifier.contains(Modifier::ITALIC)
                    && s.content.contains("italic")
            });
            assert!(has_italic, "italic span 应有 ITALIC modifier");

            let has_strike = text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
                s.style.add_modifier.contains(Modifier::CROSSED_OUT)
                    && s.content.contains("strike")
            });
            assert!(has_strike, "strikethrough span 应有 CROSSED_OUT modifier");
        }

        #[test]
        fn test_md_inline_code() {
            let text = parse_markdown("`hello`");
            let has_code = text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
                s.style.fg == Some(Color::Yellow) && s.content.contains("hello")
            });
            assert!(has_code, "行内代码应为 Yellow 颜色，含 hello 文字");
        }

        #[test]
        fn test_md_code_block() {
            let text = parse_markdown("```rust\nfn main() {}\n```");
            let all_lines: Vec<String> = text
                .lines
                .iter()
                .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
                .collect();
            let has_lang_tag = all_lines.iter().any(|l| l.contains("[rust]"));
            assert!(has_lang_tag, "代码块首行应含 [rust] 标签，实际行:\n{all_lines:#?}");
            let has_prefix = all_lines.iter().any(|l| l.contains("│ "));
            assert!(has_prefix, "代码块应含 │ 前缀，实际行:\n{all_lines:#?}");
        }

        #[test]
        fn test_md_unordered_list() {
            let text = parse_markdown("- item1\n- item2");
            let all_lines: Vec<String> = text
                .lines
                .iter()
                .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
                .collect();
            let bullet_lines: Vec<&String> =
                all_lines.iter().filter(|l| l.contains('•')).collect();
            assert_eq!(bullet_lines.len(), 2, "无序列表应有 2 行含 • ，实际:{all_lines:#?}");
        }

        #[test]
        fn test_md_ordered_list() {
            let text = parse_markdown("1. first\n2. second");
            let all_lines: Vec<String> = text
                .lines
                .iter()
                .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
                .collect();
            let has_one = all_lines.iter().any(|l| l.contains("1."));
            let has_two = all_lines.iter().any(|l| l.contains("2."));
            assert!(has_one, "有序列表应含 1. 前缀，实际:{all_lines:#?}");
            assert!(has_two, "有序列表应含 2. 前缀，实际:{all_lines:#?}");
        }

        #[test]
        fn test_md_blockquote() {
            let text = parse_markdown("> quoted text");
            let has_prefix = text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
                s.content.contains('▍')
            });
            assert!(has_prefix, "引用块应含 ▍ 前缀");
        }

        #[test]
        fn test_md_rule() {
            let text = parse_markdown("---");
            let has_rule = text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
                s.content.matches('─').count() >= 10
            });
            assert!(has_rule, "水平线应含多个 ─ 字符");
        }

        #[test]
        fn test_md_incomplete_does_not_panic() {
            // 不完整 Markdown 不应 panic，应降级为纯文本
            let text = parse_markdown("**unclosed bold");
            let all = all_text(&text);
            assert!(
                all.contains("unclosed bold"),
                "不完整 Markdown 应降级为纯文本，实际: {all:?}"
            );
        }
    }

    #[tokio::test]
    async fn test_tool_call_message_visible_when_toggled() {
        let (mut app, mut handle) = App::new_headless(120, 30);

        // 创建一个带工具调用的 AI 消息
        let tool_calls = vec![rust_create_agent::messages::ToolCallRequest {
            id: "tc1".into(),
            name: "bash".into(),
            arguments: serde_json::json!({"command": "ls"}),
        }];

        let ai_msg = rust_create_agent::messages::BaseMessage::ai_with_tool_calls(
            "I'll run ls for you",
            tool_calls,
        );

        // 发送带工具调用的消息
        let notified1 = handle.render_notify.notified();
        app.push_agent_event(AgentEvent::MessageAdded(ai_msg));
        app.process_pending_events();
        notified1.await;

        // 切换显示状态
        let notified2 = handle.render_notify.notified();
        app.toggle_collapsed_messages();
        notified2.await;

        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();

        let snap = handle.snapshot();
        // 切换后，工具调用消息应该可见
        let has_tool_call_text = snap.iter().any(|l| l.contains("I'll run ls for you") || l.contains("bash"));
        assert!(has_tool_call_text, "切换后工具调用消息应该可见，但实际内容为:\n{}", snap.join("\n"));
    }
}
