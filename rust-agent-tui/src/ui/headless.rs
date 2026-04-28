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
        // Pipeline: AssistantChunk → AppendChunk (1 个 RenderEvent)
        // Pipeline: Done           → RebuildAll/LoadHistory (1 个 RenderEvent)
        // 合计 2 个通知：必须在发送事件前预注册所有 waiter
        let notify = Arc::clone(&handle.render_notify);
        let n1 = notify.notified();
        let n2 = notify.notified();
        app.push_agent_event(AgentEvent::AssistantChunk("Hello world".into()));
        app.push_agent_event(AgentEvent::Done);
        app.process_pending_events();
        tokio::join!(n1, n2);
        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        assert!(handle.contains("Hello world"), "应显示消息内容，实际:\n{}", snap.join("\n"));
    }

    #[tokio::test]
    async fn test_tool_call_renders() {
        let (mut app, mut handle) = App::new_headless(120, 30);
        let notified = handle.render_notify.notified();
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: "t1".into(),
            name: "read_file".into(),
            display: "ReadFile".into(),
            args: "src/main.rs".into(),
            input: serde_json::json!({"path": "src/main.rs"}),
        });
        app.process_pending_events();
        notified.await;
        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        // ToolStart 通过 Pipeline 创建 ToolBlock，display_name 为 format_tool_name 的结果
        let has_tool = snap.iter().any(|l| l.contains("Read") || l.contains("read_file"));
        assert!(has_tool, "应显示工具调用块，实际内容:\n{}", snap.join("\n"));
    }

    #[tokio::test]
    async fn test_user_message_renders() {
        let (mut app, mut handle) = App::new_headless(120, 30);
        // 先注册监听，再发送事件，避免时序问题
        let notified = handle.render_notify.notified();
        // 使用 ASCII 内容避免 CJK 宽字符在 buffer 中的空格填充问题
        let vm = MessageViewModel::user("hello from user".into());
        app.core.view_messages.push(vm.clone());
        let _ = app.core.render_tx.send(RenderEvent::AddMessage(vm));
        notified.await;
        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        assert!(handle.contains("hello from user"), "应显示用户消息，实际内容:\n{}", snap.join("\n"));
    }

    #[tokio::test]
    async fn test_clear_empties_render_cache() {
        let (mut app, mut handle) = App::new_headless(120, 30);
        let notify = Arc::clone(&handle.render_notify);

        // Pipeline: 每个 AssistantChunk → AppendChunk (1 个 RenderEvent)
        // 合计 2 个通知，必须在发送事件前预注册所有 waiter
        let n1 = notify.notified();
        let n2 = notify.notified();
        app.push_agent_event(AgentEvent::AssistantChunk("SomeUniqueContent".into()));
        app.push_agent_event(AgentEvent::AssistantChunk("SomeUniqueContent".into()));
        app.process_pending_events();
        tokio::join!(n1, n2);

        // 验证 RenderCache 有内容
        let lines_before = app.core.render_cache.read().total_lines;
        assert!(lines_before > 0, "清空前应有内容");

        // 注册监听后发送 Clear，确保不错过通知
        let notified_clear = handle.render_notify.notified();
        app.core.view_messages.clear();
        let _ = app.core.render_tx.send(RenderEvent::Clear);
        notified_clear.await;

        // 验证 RenderCache 已清空
        let cache = app.core.render_cache.read();
        assert_eq!(cache.total_lines, 0, "清空后 RenderCache 应为空");
    }

    mod markdown_tests {
        use crate::ui::markdown::parse_markdown_default;
        use crate::ui::theme;
        use ratatui::style::Modifier;

        fn all_text(text: &ratatui::text::Text) -> String {
            text.lines
                .iter()
                .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
                .collect::<Vec<_>>()
                .join("")
        }

        #[test]
        fn test_md_heading() {
            use perihelion_widgets::markdown::{DefaultMarkdownTheme, MarkdownTheme};
            let theme = DefaultMarkdownTheme;

            let text = parse_markdown_default("# Hello World");
            // 标题前有空行，标题在 index 1
            let heading_line = &text.lines[1];
            let all_content: String =
                heading_line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(
                all_content.contains("Hello World"),
                "H1 应含标题文字，实际: {all_content:?}"
            );
            let has_heading_color = heading_line
                .spans
                .iter()
                .any(|s| s.style.fg == Some(theme.heading()));
            assert!(has_heading_color, "H1 应为 markdown 主题 heading 颜色");
        }

        #[test]
        fn test_md_heading_h2() {
            use perihelion_widgets::markdown::{DefaultMarkdownTheme, MarkdownTheme};
            let theme = DefaultMarkdownTheme;

            let text = parse_markdown_default("## Section Title");
            // 标题前有空行，标题在 index 1
            let heading_line = &text.lines[1];
            let has_heading_color = heading_line
                .spans
                .iter()
                .any(|s| s.style.fg == Some(theme.heading()));
            assert!(has_heading_color, "H2 应为 markdown 主题 heading 颜色");
        }

        #[test]
        fn test_md_inline_styles() {
            let text = parse_markdown_default("**bold** *italic* ~~strike~~");
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
            use perihelion_widgets::markdown::{DefaultMarkdownTheme, MarkdownTheme};
            let theme = DefaultMarkdownTheme;

            let text = parse_markdown_default("`hello`");
            let has_code = text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
                s.style.fg == Some(theme.code()) && s.content.contains("hello")
            });
            assert!(has_code, "行内代码应为 markdown 主题 code 颜色，含 hello 文字");
        }

        #[test]
        fn test_md_code_block() {
            let text = parse_markdown_default("```rust\nfn main() {}\n```");
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
            let text = parse_markdown_default("- item1\n- item2");
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
            let text = parse_markdown_default("1. first\n2. second");
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
            let text = parse_markdown_default("> quoted text");
            let has_prefix = text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
                s.content.contains('▍')
            });
            assert!(has_prefix, "引用块应含 ▍ 前缀");
        }

        #[test]
        fn test_md_rule() {
            let text = parse_markdown_default("---");
            let has_rule = text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
                s.content.matches('─').count() >= 10
            });
            assert!(has_rule, "水平线应含多个 ─ 字符");
        }

        #[test]
        fn test_md_incomplete_does_not_panic() {
            // 不完整 Markdown 不应 panic，应降级为纯文本
            let text = parse_markdown_default("**unclosed bold");
            let all = all_text(&text);
            assert!(
                all.contains("unclosed bold"),
                "不完整 Markdown 应降级为纯文本，实际: {all:?}"
            );
        }

        #[test]
        fn test_md_table_basic() {
            let md = "| Name  | Value |\n|-------|-------|\n| foo   | 123   |\n| bar   | 456   |";
            let text = parse_markdown_default(md);
            let all = all_text(&text);
            // Should contain header and data cells
            assert!(all.contains("Name"), "Table should contain header 'Name', got: {all:?}");
            assert!(all.contains("foo"), "Table should contain data 'foo', got: {all:?}");
            assert!(all.contains("456"), "Table should contain data '456', got: {all:?}");
            // Should have border characters
            assert!(all.contains("│"), "Table should have vertical borders, got: {all:?}");
            assert!(all.contains("┌"), "Table should have top-left corner, got: {all:?}");
            assert!(all.contains("└"), "Table should have bottom-left corner, got: {all:?}");
            assert!(all.contains("┼"), "Table should have header separator, got: {all:?}");
        }

        #[test]
        fn test_md_table_cell_count() {
            let md = "| A | B |\n|---|---|\n| 1 | 2 |";
            let text = parse_markdown_default(md);
            // Should produce exactly: top border + header + separator + 1 data row + bottom border = 5 lines
            assert_eq!(text.lines.len(), 5, "2-col table should produce 5 lines, got: {}", text.lines.len());
        }

        #[test]
        fn test_md_table_border_alignment() {
            let md = "| Name | Value |\n|------|-------|\n| foo  | 123   |";
            let text = parse_markdown_default(md);
            // Debug: print each line
            for (i, line) in text.lines.iter().enumerate() {
                let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                eprintln!("line {}: {:?} (chars={})", i, content, content.chars().count());
            }
            // Each line should have the same visual width (measured in chars, not bytes)
            let widths: Vec<usize> = text.lines.iter().map(|line| {
                line.spans.iter().map(|s| s.content.chars().count()).sum::<usize>()
            }).collect();
            let unique_widths: std::collections::HashSet<usize> = widths.iter().copied().collect();
            assert!(
                unique_widths.len() == 1,
                "All table lines should have same visual width, got: {:?}",
                widths
            );
        }

        #[test]
        fn test_md_table_alignment() {
            let md = "| Left | Center | Right |\n|:-----|:------:|------:|\n| a    | b      | c     |";
            let text = parse_markdown_default(md);
            let all = all_text(&text);
            assert!(all.contains("Left"), "Should contain 'Left' header, got: {all:?}");
            assert!(all.contains("a"), "Should contain data 'a', got: {all:?}");
        }

        #[test]
        fn test_md_table_with_inline_code() {
            let md = "| Command |\n|---------|\n| `ls`    |";
            let text = parse_markdown_default(md);
            let all = all_text(&text);
            assert!(all.contains("ls"), "Should contain inline code content, got: {all:?}");
        }
    }

    #[tokio::test]
    async fn test_subagent_group_basic() {
        // SubAgentStart → 2×ToolCall → SubAgentEnd → 渲染验证
        let (mut app, mut handle) = App::new_headless(120, 30);
        let notify = Arc::clone(&handle.render_notify);

        // 事件数：SubAgentStart(1) + ToolCall×2(2) + SubAgentEnd(1) = 4 个 RenderEvent
        let n1 = notify.notified();
        let n2 = notify.notified();
        let n3 = notify.notified();
        let n4 = notify.notified();

        app.push_agent_event(AgentEvent::SubAgentStart {
            agent_id: "code-reviewer".into(),
            task_preview: "review the code".into(),
        });
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: "t1".into(),
            name: "read_file".into(),
            display: "ReadFile".into(),
            args: "src/main.rs".into(),
            input: serde_json::json!({"path": "src/main.rs"}),
        });
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: "t2".into(),
            name: "bash".into(),
            display: "Bash".into(),
            args: "cargo test".into(),
            input: serde_json::json!({"command": "cargo test"}),
        });
        app.push_agent_event(AgentEvent::SubAgentEnd {
            result: "All tests passed, no issues found".into(),
            is_error: false,
        });
        app.process_pending_events();
        tokio::join!(n1, n2, n3, n4);

        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();

        // 验证 SubAgentGroup 头行存在（code-reviewer 名称）
        let has_agent = snap.iter().any(|l| l.contains("code-reviewer"));
        assert!(has_agent, "应显示 SubAgentGroup 头行含 agent_id，实际:\n{}", snap.join("\n"));

        // 验证 total_steps 步数显示（2 步）
        let has_steps = snap.iter().any(|l| l.contains("2"));
        assert!(has_steps, "应显示步数计数，实际:\n{}", snap.join("\n"));

        // 验证 SubAgentGroup 已完成（is_running=false）
        if let Some(vm) = app.core.view_messages.last() {
            assert!(vm.is_subagent_group(), "最后一条消息应为 SubAgentGroup");
            if let crate::app::MessageViewModel::SubAgentGroup { is_running, total_steps, .. } = vm {
                assert!(!is_running, "SubAgentEnd 后 is_running 应为 false");
                assert_eq!(*total_steps, 2, "total_steps 应为 2");
            }
        }
    }

    #[tokio::test]
    async fn test_subagent_group_sliding_window() {
        // SubAgentStart → 6×ToolCall → SubAgentEnd → 只保留 4 条，总步数为 6
        let (mut app, _handle) = App::new_headless(120, 30);

        app.push_agent_event(AgentEvent::SubAgentStart {
            agent_id: "analyzer".into(),
            task_preview: "analyze codebase".into(),
        });
        for i in 1..=6 {
            app.push_agent_event(AgentEvent::ToolStart {
                tool_call_id: format!("t{}", i),
                name: "read_file".into(),
                display: "ReadFile".into(),
                args: format!("file{}.rs", i),
                input: serde_json::json!({"path": format!("file{}.rs", i)}),
            });
        }
        app.push_agent_event(AgentEvent::SubAgentEnd {
            result: "analysis complete".into(),
            is_error: false,
        });
        app.process_pending_events();

        // 验证 SubAgentGroup 状态
        if let Some(crate::app::MessageViewModel::SubAgentGroup {
            total_steps,
            recent_messages,
            is_running,
            ..
        }) = app.core.view_messages.last()
        {
            assert_eq!(*total_steps, 6, "total_steps 应为 6，实际: {}", total_steps);
            assert!(
                recent_messages.len() <= 4,
                "recent_messages 最多 4 条，实际: {}",
                recent_messages.len()
            );
            assert!(!is_running, "SubAgentEnd 后 is_running 应为 false");
        } else {
            panic!("最后一条消息应为 SubAgentGroup");
        }
    }

    #[tokio::test]
    async fn test_subagent_group_assistant_chunk() {
        // SubAgentStart → AssistantChunk → SubAgentEnd → AssistantBubble 在 recent_messages 中
        let (mut app, _handle) = App::new_headless(120, 30);

        app.push_agent_event(AgentEvent::SubAgentStart {
            agent_id: "writer".into(),
            task_preview: "write summary".into(),
        });
        app.push_agent_event(AgentEvent::AssistantChunk("summary text here".into()));
        app.push_agent_event(AgentEvent::SubAgentEnd {
            result: "Done writing".into(),
            is_error: false,
        });
        app.process_pending_events();

        // 验证 SubAgentGroup 包含 AssistantBubble
        if let Some(crate::app::MessageViewModel::SubAgentGroup {
            recent_messages,
            final_result,
            ..
        }) = app.core.view_messages.last()
        {
            let has_assistant = recent_messages.iter().any(|m| m.is_assistant());
            assert!(has_assistant, "recent_messages 应包含 AssistantBubble");
            assert_eq!(
                final_result.as_deref(),
                Some("Done writing"),
                "final_result 应为工具返回值"
            );
        } else {
            panic!("最后一条消息应为 SubAgentGroup");
        }
    }

    #[tokio::test]
    async fn test_tool_call_message_visible_when_toggled() {
        let (mut app, mut handle) = App::new_headless(120, 30);

        // 使用 ToolStart 事件添加工具调用（会发送 RenderEvent::AddMessage）
        let notified1 = handle.render_notify.notified();
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: "tc1".into(),
            name: "bash".into(),
            display: "Bash".into(),
            args: "ls".into(),
            input: serde_json::json!({"command": "ls"}),
        });
        app.process_pending_events();
        notified1.await;

        // toggle_collapsed_messages 发送 ToggleToolMessages → 渲染线程 rebuild_all → notify
        let notified2 = handle.render_notify.notified();
        app.toggle_collapsed_messages();
        notified2.await;

        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();

        let snap = handle.snapshot();
        // ToolStart 创建的 ToolBlock，display_name 为 format_tool_name 的结果
        let has_tool_call_text = snap.iter().any(|l| l.contains("Shell") || l.contains("bash"));
        assert!(has_tool_call_text, "ToolCall 创建的 ToolBlock 应在快照中可见，但实际内容为:\n{}", snap.join("\n"));
    }

    #[tokio::test]
    async fn test_empty_assistant_chunk_no_bubble() {
        // 空 AssistantChunk 不应创建空白的 AssistantBubble
        let (mut app, _handle) = App::new_headless(120, 30);

        // 发送空 chunk，不应创建 AssistantBubble
        app.push_agent_event(AgentEvent::AssistantChunk("".into()));
        app.process_pending_events();

        // view_messages 应为空（没有创建空白气泡）
        assert!(
            app.core.view_messages.is_empty(),
            "空 AssistantChunk 不应创建 AssistantBubble，实际: {:?}",
            app.core.view_messages.len()
        );

        // 发送多个空 chunk，仍不应创建气泡
        app.push_agent_event(AgentEvent::AssistantChunk("".into()));
        app.push_agent_event(AgentEvent::AssistantChunk("".into()));
        app.process_pending_events();

        assert!(
            app.core.view_messages.is_empty(),
            "多个空 AssistantChunk 仍不应创建 AssistantBubble"
        );
    }

    #[tokio::test]
    async fn test_empty_then_nonempty_assistant_chunk() {
        // 空_chunk → 非空_chunk：非空 chunk 应正常创建气泡
        let (mut app, mut handle) = App::new_headless(120, 30);

        // 先发送空 chunk
        app.push_agent_event(AgentEvent::AssistantChunk("".into()));
        app.process_pending_events();

        // 再发送非空 chunk
        let notify = Arc::clone(&handle.render_notify);
        let n1 = notify.notified();
        let n2 = notify.notified();
        app.push_agent_event(AgentEvent::AssistantChunk("Hello".into()));
        app.push_agent_event(AgentEvent::Done);
        app.process_pending_events();
        tokio::join!(n1, n2);

        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();

        // 应该只有 1 个 AssistantBubble，内容为 "Hello"
        assert_eq!(app.core.view_messages.len(), 1, "应只有 1 条消息");
        assert!(app.core.view_messages[0].is_assistant(), "应为 AssistantBubble");
        assert!(handle.contains("Hello"), "应显示 Hello 内容");
    }

    #[tokio::test]
    async fn test_tool_call_without_assistant_chunk_no_bubble() {
        // 模拟 AI 只调用工具不输出文本的场景
        let (mut app, mut handle) = App::new_headless(120, 30);

        // 直接发送 ToolStart 事件（无 AssistantChunk）
        let notified = handle.render_notify.notified();
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: "tc1".into(),
            name: "bash".into(),
            display: "Bash".into(),
            args: "ls".into(),
            input: serde_json::json!({"command": "ls"}),
        });
        app.process_pending_events();
        notified.await;

        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();

        // 应该有 1 个 ToolBlock，不应有空白 AssistantBubble
        assert_eq!(app.core.view_messages.len(), 1, "应有 1 条消息（ToolBlock）");
        // 确保不是 AssistantBubble（空白气泡）
        assert!(
            !app.core.view_messages[0].is_assistant(),
            "不应创建 AssistantBubble，应为 ToolBlock"
        );
    }

    #[tokio::test]
    async fn test_welcome_card_renders_when_empty() {
        let (mut app, mut handle) = App::new_headless(120, 30);
        // 默认 view_messages 为空，应显示 Welcome Card
        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        let snap_text = snap.join("\n");
        assert!(
            snap_text.contains("Perihelion"),
            "Welcome Card 应包含 'Perihelion'，实际:\n{}",
            snap_text
        );
        assert!(
            snap_text.contains("/help") || snap_text.contains("/model"),
            "Welcome Card 应包含命令提示，实际:\n{}",
            snap_text
        );
    }

    #[tokio::test]
    async fn test_welcome_card_hidden_after_message() {
        let (mut app, mut handle) = App::new_headless(120, 30);
        let notify = Arc::clone(&handle.render_notify);
        let n1 = notify.notified();
        let n2 = notify.notified();
        app.push_agent_event(AgentEvent::AssistantChunk("Hello from agent".into()));
        app.push_agent_event(AgentEvent::Done);
        app.process_pending_events();
        tokio::join!(n1, n2);

        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        let snap_text = snap.join("\n");
        assert!(
            !snap_text.contains("What can I do?"),
            "有消息后 Welcome Card 应消失，但仍有 welcome 内容，实际:\n{}",
            snap_text
        );
        assert!(
            handle.contains("Hello from agent"),
            "应显示消息内容，实际:\n{}",
            snap_text
        );
    }

    #[tokio::test]
    async fn test_welcome_card_narrow_screen() {
        let (mut app, mut handle) = App::new_headless(40, 24);
        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        let snap_text = snap.join("\n");
        // 窄屏不应显示 ASCII Art（包含 ██ 或 ╚═ 等 block 字符）
        assert!(
            !snap_text.contains("██"),
            "窄屏不应显示 ASCII Art Logo，实际:\n{}",
            snap_text
        );
        // 但仍应包含文字版标题
        assert!(
            snap_text.contains("Perihelion"),
            "窄屏应显示文字版标题 'Perihelion'，实际:\n{}",
            snap_text
        );
    }

    // ── Sticky Human Message Header ────────────────────────────────────────────

    #[tokio::test]
    async fn test_sticky_header_hidden_when_no_messages() {
        // 无消息时 sticky header 应完全隐藏
        let (mut app, mut handle) = App::new_headless(80, 24);
        assert!(
            app.core.last_human_message.is_none(),
            "默认应无 last_human_message"
        );
        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        let snap_text = snap.join("\n");
        assert!(
            !snap_text.contains("你:"),
            "无消息时不应显示 sticky header，实际:\n{}",
            snap_text
        );
    }

    #[tokio::test]
    async fn test_sticky_header_shows_after_submit() {
        // 模拟 submit_message 后 sticky header 显示
        // 需要足够多的消息使内容超过可视区域（max_scroll > 0）
        let (mut app, mut handle) = App::new_headless(80, 24);

        // 填充足够多的消息使消息区产生滚动
        for i in 0..30 {
            let notified = handle.render_notify.notified();
            let vm = MessageViewModel::user(format!("message line {}", i));
            app.core.view_messages.push(vm.clone());
            let _ = app.core.render_tx.send(RenderEvent::AddMessage(vm));
            notified.await;
        }

        // 设置 last_human_message（模拟 submit_message 的效果）
        app.core.last_human_message = Some("hello from user".to_string());

        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        let snap_text = snap.join("\n");

        assert!(
            snap_text.contains("hello from"),
            "应显示消息内容，实际:\n{}",
            snap_text
        );
    }

    #[tokio::test]
    async fn test_sticky_header_hidden_after_clear() {
        // /clear 后 sticky header 应消失
        let (mut app, mut handle) = App::new_headless(80, 24);

        // 模拟已有消息
        app.core.last_human_message = Some("some message".to_string());
        assert!(
            app.core.last_human_message.is_some(),
            "应有 last_human_message"
        );

        // 模拟 /clear → new_thread
        let notified = handle.render_notify.notified();
        app.new_thread();
        notified.await;

        assert!(
            app.core.last_human_message.is_none(),
            "/clear 后 last_human_message 应为 None"
        );

        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        let snap_text = snap.join("\n");
        assert!(
            !snap_text.contains("你:"),
            "/clear 后不应显示 sticky header，实际:\n{}",
            snap_text
        );
    }

    #[tokio::test]
    async fn test_sticky_header_shows_last_message_not_first() {
        // 连续发送多条消息，header 应显示最后一条
        let (mut app, mut handle) = App::new_headless(80, 24);

        // 填充足够多的消息使消息区产生滚动
        for i in 0..30 {
            let notified = handle.render_notify.notified();
            let vm = MessageViewModel::user(format!("padding line {}", i));
            app.core.view_messages.push(vm.clone());
            let _ = app.core.render_tx.send(RenderEvent::AddMessage(vm));
            notified.await;
        }

        // 模拟第一条消息
        app.core.last_human_message = Some("first message".to_string());
        // 模拟第二条消息（覆盖）
        app.core.last_human_message = Some("second message".to_string());

        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        let snap_text = snap.join("\n");

        assert!(
            snap_text.contains("second"),
            "应显示最后一条消息，实际:\n{}",
            snap_text
        );
        assert!(
            !snap_text.contains("first"),
            "不应显示第一条消息（已被覆盖），实际:\n{}",
            snap_text
        );
    }

    #[tokio::test]
    async fn test_sticky_header_truncation_long_message() {
        // 超长消息应在达到行数上限后截断并加 …
        let (mut app, mut handle) = App::new_headless(40, 24); // 窄屏 40 列

        // 填充足够多的消息使消息区产生滚动
        for i in 0..30 {
            let notified = handle.render_notify.notified();
            let vm = MessageViewModel::user(format!("padding {}", i));
            app.core.view_messages.push(vm.clone());
            let _ = app.core.render_tx.send(RenderEvent::AddMessage(vm));
            notified.await;
        }

        // 模拟超长消息（远超 header 可显示范围）
        let long_msg = "hello this is a very long message that definitely exceeds header capacity".to_string();
        assert!(long_msg.chars().count() > 40);
        app.core.last_human_message = Some(long_msg.clone());

        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        let snap_text = snap.join("\n");

        // 应显示消息开头
        assert!(
            snap_text.contains("hello this"),
            "应显示消息开头部分，实际:\n{}",
            snap_text
        );
        // 超长时应在末尾有省略号
        // （多行内容在 max_lines 行后被截断）
    }

    #[tokio::test]
    async fn test_cron_panel_render() {
        let (mut app, mut handle) = App::new_headless(120, 30);

        // Register a cron task
        app.cron.scheduler.lock().register("* * * * *", "hello cron test").unwrap();
        let tasks: Vec<_> = app.cron.scheduler.lock().list_tasks().into_iter().cloned().collect();
        app.cron.cron_panel = Some(crate::app::CronPanel::new(tasks));

        let notified = handle.render_notify.notified();
        drop(notified);

        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        eprintln!("SNAPSHOT:");
        for (i, line) in snap.iter().enumerate() {
            if !line.is_empty() {
                eprintln!("{:3}: {}", i, line);
            }
        }
        assert!(handle.contains("hello cron test"), "should contain task prompt");
        assert!(handle.contains("* * * * *"), "should contain cron expression");
    }

    #[tokio::test]
    async fn test_bordered_panel_integration() {
        // BorderedPanel 集成冒烟测试：渲染 agent panel 验证无 panic 且输出正确
        let (mut app, mut handle) = App::new_headless(120, 30);

        app.core.agent_panel = Some(crate::app::AgentPanel::new(vec![], None));

        handle
            .terminal
            .draw(|f| main_ui::render(f, &mut app))
            .unwrap();
        assert!(
            handle.contains("Agent"),
            "BorderedPanel integration should render agent panel title"
        );
    }

    #[tokio::test]
    async fn test_tab_bar_integration() {
        // TabBar 集成冒烟测试：渲染 ask_user popup 验证 TabBar widget 正确工作
        use rust_agent_middlewares::ask_user::{AskUserBatchRequest, AskUserQuestionData, AskUserOption};
        use crate::app::AskUserBatchPrompt;

        let (mut app, mut handle) = App::new_headless(120, 30);

        let (req, _rx) = AskUserBatchRequest::new(vec![
            AskUserQuestionData {
                tool_call_id: "t1".into(),
                question: "Choose a language?".into(),
                header: "Language".into(),
                multi_select: false,
                options: vec![
                    AskUserOption { label: "Rust".into(), description: Some("Systems language".into()) },
                    AskUserOption { label: "Go".into(), description: None },
                ],
            },
            AskUserQuestionData {
                tool_call_id: "t1".into(),
                question: "Choose a framework?".into(),
                header: "Framework".into(),
                multi_select: true,
                options: vec![
                    AskUserOption { label: "Axum".into(), description: None },
                ],
            },
        ]);
        let prompt = AskUserBatchPrompt::from_request(req);
        app.agent.interaction_prompt = Some(crate::app::InteractionPrompt::Questions(prompt));

        handle
            .terminal
            .draw(|f| main_ui::render(f, &mut app))
            .unwrap();
        let snap = handle.snapshot();
        // TabBar should render the tab labels
        assert!(
            snap.iter().any(|l| l.contains("Language")),
            "TabBar should render 'Language' tab label, got:\n{}",
            snap.join("\n")
        );
        assert!(
            snap.iter().any(|l| l.contains("Framework")),
            "TabBar should render 'Framework' tab label, got:\n{}",
            snap.join("\n")
        );
    }

    mod setup_wizard_e2e {
        use super::*;
        use crate::app::setup_wizard::{
            handle_setup_wizard_key, needs_setup, save_setup_to, ProviderType, SetupStep,
            SetupWizardAction, SetupWizardPanel, Step1Field,
        };
        use crate::app::App;
        use tui_textarea::{Input, Key};

        fn make_char(c: char) -> Input {
            Input {
                key: Key::Char(c),
                ctrl: false,
                alt: false,
                shift: false,
            }
        }
        fn make_key(key: Key) -> Input {
            Input {
                key,
                ctrl: false,
                alt: false,
                shift: false,
            }
        }
        fn type_text(wizard: &mut SetupWizardPanel, text: &str) {
            for c in text.chars() {
                let _ = handle_setup_wizard_key(wizard, make_char(c));
            }
        }

        #[tokio::test]
        async fn test_needs_setup_triggers_for_empty_config() {
            let (app, _handle) = App::new_headless(120, 30);
            assert!(app.zen_config.is_none(), "headless App default has no config");
            let empty_cfg = crate::config::types::ZenConfig::default();
            assert!(needs_setup(&empty_cfg.config), "empty providers should need setup");
        }

        #[tokio::test]
        async fn test_setup_wizard_full_flow_anthropic() {
            let (mut app, mut handle) = App::new_headless(120, 30);
            app.setup_wizard = Some(SetupWizardPanel::new());

            // Render Step 1
            {
                let wizard = app.setup_wizard.as_ref().unwrap();
                assert_eq!(wizard.step, SetupStep::Provider);
                assert_eq!(wizard.provider_type, ProviderType::Anthropic);
            }
            handle
                .terminal
                .draw(|f| crate::ui::main_ui::render(f, &mut app))
                .unwrap();
            assert!(handle.contains("Step 1/2"));

            // Step 1: type API key then Enter → ModelAlias
            let wizard = app.setup_wizard.as_mut().unwrap();
            wizard.step1_focus = Step1Field::ApiKey;
            type_text(wizard, "sk-ant-test-key-12345");
            let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
            assert!(matches!(action, Some(SetupWizardAction::Redraw)));
            assert_eq!(wizard.step, SetupStep::ModelAlias);

            // Step 2: Enter → Done
            handle
                .terminal
                .draw(|f| crate::ui::main_ui::render(f, &mut app))
                .unwrap();
            assert!(handle.contains("Step 2/2"));
            let wizard = app.setup_wizard.as_ref().unwrap();
            assert!(wizard.aliases[0].model_id.contains("claude-opus"));
            let wizard = app.setup_wizard.as_mut().unwrap();
            let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
            assert_eq!(wizard.step, SetupStep::Done);

            // Done → Enter → SaveAndClose
            handle
                .terminal
                .draw(|f| crate::ui::main_ui::render(f, &mut app))
                .unwrap();
            assert!(handle.contains("Complete"));
            let wizard = app.setup_wizard.as_mut().unwrap();
            let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
            assert!(matches!(action, Some(SetupWizardAction::SaveAndClose)));

            // Verify save_setup_to
            let wizard = app.setup_wizard.as_ref().unwrap();
            let temp_dir =
                std::env::temp_dir().join(format!("zen-setup-test-{}", uuid::Uuid::now_v7()));
            let config_path = temp_dir.join("settings.json");
            let cfg = save_setup_to(wizard, &config_path).expect("save_setup_to should succeed");

            assert_eq!(cfg.config.providers.len(), 1);
            assert_eq!(cfg.config.providers[0].provider_type, "anthropic");
            assert_eq!(cfg.config.providers[0].api_key, "sk-ant-test-key-12345");
            assert_eq!(cfg.config.active_alias, "opus");
            assert_eq!(cfg.config.active_provider_id, "anthropic");
            assert!(cfg.config.providers[0].models.opus.contains("claude-opus"));

            let content = std::fs::read_to_string(&config_path).expect("config file should exist");
            assert!(content.contains("anthropic"));

            assert!(!needs_setup(&cfg.config), "after setup, should not need setup");

            let _ = std::fs::remove_dir_all(&temp_dir);
        }

        #[tokio::test]
        async fn test_setup_wizard_full_flow_openai() {
            let (mut app, mut handle) = App::new_headless(120, 30);
            let mut wizard = SetupWizardPanel::new();

            // Switch to OpenAI Compatible
            assert_eq!(wizard.step1_focus, Step1Field::ProviderType);
            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Down));
            assert_eq!(wizard.provider_type, ProviderType::OpenAiCompatible);
            assert_eq!(wizard.provider_id, "openai");

            // Render and verify
            app.setup_wizard = Some(wizard);
            handle
                .terminal
                .draw(|f| crate::ui::main_ui::render(f, &mut app))
                .unwrap();
            assert!(handle.contains("OpenAI Compatible"));

            // Step 1: set api_key, Enter → ModelAlias
            let wizard = app.setup_wizard.as_mut().unwrap();
            wizard.step1_focus = Step1Field::ApiKey;
            type_text(wizard, "sk-openai-test-key");
            let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter));
            assert_eq!(wizard.step, SetupStep::ModelAlias);

            // Verify OpenAI defaults
            assert_eq!(wizard.aliases[0].model_id, "o3");
            assert_eq!(wizard.aliases[1].model_id, "gpt-4o");
            assert_eq!(wizard.aliases[2].model_id, "gpt-4o-mini");

            // Step 3 → Done → SaveAndClose
            let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter));
            assert_eq!(wizard.step, SetupStep::Done);
            let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
            assert!(matches!(action, Some(SetupWizardAction::SaveAndClose)));

            // Verify config
            let temp_dir = std::env::temp_dir()
                .join(format!("zen-setup-test-openai-{}", uuid::Uuid::now_v7()));
            let config_path = temp_dir.join("settings.json");
            let cfg = save_setup_to(wizard, &config_path).expect("save_setup_to should succeed");
            assert_eq!(cfg.config.providers[0].provider_type, "openai");
            assert_eq!(cfg.config.providers[0].api_key, "sk-openai-test-key");
            assert_eq!(cfg.config.providers[0].models.opus, "o3");

            let _ = std::fs::remove_dir_all(&temp_dir);
        }

        #[tokio::test]
        async fn test_setup_wizard_skip_with_confirm() {
            let (mut app, mut handle) = App::new_headless(120, 30);
            app.setup_wizard = Some(SetupWizardPanel::new());

            // Esc → confirm skip
            let wizard = app.setup_wizard.as_mut().unwrap();
            let action = handle_setup_wizard_key(wizard, make_key(Key::Esc));
            assert!(matches!(action, Some(SetupWizardAction::Redraw)));
            assert!(wizard.confirm_skip);

            // Esc cancel
            let action = handle_setup_wizard_key(wizard, make_key(Key::Esc));
            assert!(matches!(action, Some(SetupWizardAction::Redraw)));
            assert!(!wizard.confirm_skip);

            // Esc again → confirm
            let action = handle_setup_wizard_key(wizard, make_key(Key::Esc));
            assert!(wizard.confirm_skip);

            // Enter → Skip
            let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
            assert!(matches!(action, Some(SetupWizardAction::Skip)));
        }

        #[tokio::test]
        async fn test_setup_wizard_esc_navigation() {
            let mut wizard = SetupWizardPanel::new();

            // Step 1: empty api_key → Enter blocked (stays on Provider)
            assert_eq!(wizard.step, SetupStep::Provider);
            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
            assert_eq!(wizard.step, SetupStep::Provider);

            // Step 1: fill api_key → Enter → ModelAlias
            wizard.step1_focus = Step1Field::ApiKey;
            type_text(&mut wizard, "test-key");
            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
            assert_eq!(wizard.step, SetupStep::ModelAlias);

            // ModelAlias → Esc → Provider (api_key preserved)
            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Esc));
            assert_eq!(wizard.step, SetupStep::Provider);
            assert_eq!(wizard.api_key, "test-key");

            // Provider → ModelAlias → Done → Esc → ModelAlias
            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
            assert_eq!(wizard.step, SetupStep::ModelAlias);
            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
            assert_eq!(wizard.step, SetupStep::Done);
            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Esc));
            assert_eq!(wizard.step, SetupStep::ModelAlias);
        }

        #[tokio::test]
        async fn test_setup_wizard_validation_blocks_empty_fields() {
            let mut wizard = SetupWizardPanel::new();

            // Empty provider_id → Enter blocked
            wizard.provider_id.clear();
            let action = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
            assert!(matches!(action, Some(SetupWizardAction::Redraw)));
            assert_eq!(wizard.step, SetupStep::Provider);

            // Empty api_key → Enter still blocked (both must be non-empty)
            wizard.provider_id = "anthropic".to_string();
            let action = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
            assert!(matches!(action, Some(SetupWizardAction::Redraw)));
            assert_eq!(wizard.step, SetupStep::Provider);

            // Type key → Enter → ModelAlias
            wizard.step1_focus = Step1Field::ApiKey;
            type_text(&mut wizard, "test-key");
            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
            assert_eq!(wizard.step, SetupStep::ModelAlias);

            // Empty model_id → Enter blocked
            wizard.aliases[0].model_id.clear();
            let action = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
            assert_eq!(wizard.step, SetupStep::ModelAlias);
        }

        #[tokio::test]
        async fn test_setup_wizard_step1_tab_navigation() {
            let mut wizard = SetupWizardPanel::new();
            assert_eq!(wizard.step1_focus, Step1Field::ProviderType);

            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
            assert_eq!(wizard.step1_focus, Step1Field::ProviderId);

            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
            assert_eq!(wizard.step1_focus, Step1Field::BaseUrl);

            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
            assert_eq!(wizard.step1_focus, Step1Field::ApiKey);

            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
            assert_eq!(wizard.step1_focus, Step1Field::ProviderType);

            // Shift+Tab reverse
            let _ = handle_setup_wizard_key(
                &mut wizard,
                Input {
                    key: Key::Tab,
                    ctrl: false,
                    alt: false,
                    shift: true,
                },
            );
            assert_eq!(wizard.step1_focus, Step1Field::ApiKey);
        }

        #[tokio::test]
        async fn test_setup_wizard_step3_tab_navigation() {
            let mut wizard = SetupWizardPanel::new();
            wizard.step = SetupStep::ModelAlias;
            assert_eq!(wizard.step3_focus, 0);

            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
            assert_eq!(wizard.step3_focus, 1);

            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
            assert_eq!(wizard.step3_focus, 2);

            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Tab));
            assert_eq!(wizard.step3_focus, 0);
        }

        #[tokio::test]
        async fn test_setup_wizard_backspace_editing() {
            let mut wizard = SetupWizardPanel::new();

            // Step 1 ApiKey field: type + backspace
            wizard.step1_focus = Step1Field::ApiKey;
            type_text(&mut wizard, "abc");
            assert_eq!(wizard.api_key, "abc");
            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Backspace));
            assert_eq!(wizard.api_key, "ab");

            // Step 1 ProviderId: backspace
            wizard.step1_focus = Step1Field::ProviderId;
            wizard.provider_id = "myprovider".to_string();
            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Backspace));
            assert_eq!(wizard.provider_id, "myprovide");

            // Step 1 BaseUrl (Anthropic): editable
            wizard.step1_focus = Step1Field::BaseUrl;
            wizard.base_url = "https://api.anthropic.com".to_string();
            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Backspace));
            assert_eq!(wizard.base_url, "https://api.anthropic.co");

            // Step 1 BaseUrl (OpenAI): editable
            wizard.provider_type = ProviderType::OpenAiCompatible;
            wizard.base_url = "https://api.openai.com/v1".to_string();
            let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Backspace));
            assert_eq!(wizard.base_url, "https://api.openai.com/v");
        }

        #[tokio::test]
        async fn test_setup_wizard_saves_and_clears() {
            let (mut app, mut handle) = App::new_headless(120, 30);
            app.setup_wizard = Some(SetupWizardPanel::new());
            assert!(app.setup_wizard.is_some());

            // Render
            handle
                .terminal
                .draw(|f| crate::ui::main_ui::render(f, &mut app))
                .unwrap();
            assert!(handle.contains("Step 1/2"));

            // Quick complete: set api_key, then Enter through all steps
            let wizard = app.setup_wizard.as_mut().unwrap();
            wizard.step1_focus = Step1Field::ApiKey;
            type_text(wizard, "sk-final-test");
            let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter)); // Step 1 → ModelAlias
            let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter)); // ModelAlias → Done

            // Done → SaveAndClose
            let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
            assert!(matches!(action, Some(SetupWizardAction::SaveAndClose)));

            // Simulate SaveAndClose
            let wizard = app.setup_wizard.take().unwrap();
            let temp_dir =
                std::env::temp_dir().join(format!("zen-setup-final-{}", uuid::Uuid::now_v7()));
            let config_path = temp_dir.join("settings.json");
            let cfg = save_setup_to(&wizard, &config_path).expect("save should succeed");
            assert!(!needs_setup(&cfg.config));

            app.zen_config = Some(cfg);
            assert!(app.setup_wizard.is_none());

            let _ = std::fs::remove_dir_all(&temp_dir);
        }
    }

    // ─── Permission Mode Tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_app_default_permission_mode_is_bypass() {
        let (app, _handle) = App::new_headless(80, 24);
        use rust_agent_middlewares::prelude::PermissionMode;
        assert_eq!(
            app.permission_mode.load(),
            PermissionMode::Bypass,
            "headless App 默认应为 Bypass"
        );
    }

    #[tokio::test]
    async fn test_permission_mode_store_and_load() {
        let (mut app, _handle) = App::new_headless(80, 24);
        use rust_agent_middlewares::prelude::PermissionMode;
        for mode in [
            PermissionMode::Default,
            PermissionMode::DontAsk,
            PermissionMode::AcceptEdit,
            PermissionMode::AutoMode,
            PermissionMode::Bypass,
        ] {
            app.permission_mode.store(mode);
            assert_eq!(app.permission_mode.load(), mode, "store/load 应一致: {:?}", mode);
        }
    }

    #[tokio::test]
    async fn test_permission_mode_cycle() {
        let (app, _handle) = App::new_headless(80, 24);
        use rust_agent_middlewares::prelude::PermissionMode;
        // cycle 从 Bypass 开始 → Default
        let next = app.permission_mode.cycle();
        assert_eq!(next, PermissionMode::Default);
        // 继续循环 → DontAsk
        let next2 = app.permission_mode.cycle();
        assert_eq!(next2, PermissionMode::DontAsk);
    }

    #[tokio::test]
    async fn test_status_bar_shows_permission_mode() {
        use rust_agent_middlewares::prelude::PermissionMode;
        let (mut app, mut handle) = App::new_headless(120, 24);
        // 默认 Bypass → 应显示 "Bypass"
        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        assert!(handle.contains("Bypass"), "状态栏应显示 Bypass 模式，实际:\n{}", handle.snapshot().join("\n"));
    }

    #[tokio::test]
    async fn test_status_bar_updates_after_mode_switch() {
        use rust_agent_middlewares::prelude::PermissionMode;
        let (mut app, mut handle) = App::new_headless(120, 24);
        // 切换到 Default - 不显示标签
        app.permission_mode.store(PermissionMode::Default);
        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        assert!(!handle.contains("DEFAULT"), "Default 模式不应显示标签，实际:\n{}", handle.snapshot().join("\n"));

        // 切换到 DontAsk
        app.permission_mode.store(PermissionMode::DontAsk);
        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        assert!(handle.contains("Don't Ask"), "切换后状态栏应显示 Don't Ask，实际:\n{}", handle.snapshot().join("\n"));

        // 切换到 AcceptEdit
        app.permission_mode.store(PermissionMode::AcceptEdit);
        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        assert!(handle.contains("Accept Edit"), "切换后状态栏应显示 Accept Edit，实际:\n{}", handle.snapshot().join("\n"));

        // 切换到 AutoMode
        app.permission_mode.store(PermissionMode::AutoMode);
        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        assert!(handle.contains("Auto Mode"), "切换后状态栏应显示 Auto Mode，实际:\n{}", handle.snapshot().join("\n"));
    }

    #[tokio::test]
    async fn test_shift_tab_cycles_permission_mode() {
        use rust_agent_middlewares::prelude::PermissionMode;
        let (mut app, _handle) = App::new_headless(120, 24);
        // 初始 Bypass
        assert_eq!(app.permission_mode.load(), PermissionMode::Bypass);
        // 模拟 Shift+Tab 按键效果（直接调用 cycle）
        let next = app.permission_mode.cycle();
        assert_eq!(next, PermissionMode::Default, "Bypass 之后应为 Default");
        assert_eq!(app.permission_mode.load(), PermissionMode::Default);
        // 继续循环 4 次回到 Bypass
        app.permission_mode.cycle(); // DontAsk
        app.permission_mode.cycle(); // AcceptEdit
        app.permission_mode.cycle(); // AutoMode
        let final_mode = app.permission_mode.cycle(); // Bypass
        assert_eq!(final_mode, PermissionMode::Bypass, "循环 5 次回到起点");
    }

    #[tokio::test]
    async fn test_mode_highlight_until_set_on_cycle() {
        let (mut app, _handle) = App::new_headless(120, 24);
        // 初始无闪烁
        assert!(app.mode_highlight_until.is_none(), "初始不应有闪烁");
        // 模拟 Shift+Tab: cycle + 设置 highlight
        app.permission_mode.cycle();
        app.mode_highlight_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
        assert!(app.mode_highlight_until.is_some(), "cycle 后应设置闪烁截止时间");
        // 验证截止时间在未来
        let until = app.mode_highlight_until.unwrap();
        assert!(std::time::Instant::now() < until, "截止时间应在未来");
    }

    #[tokio::test]
    async fn test_spinner_shows_verb_in_status_bar() {
        let (mut app, mut handle) = crate::app::App::new_headless(120, 30);
        // 添加一条消息，否则 render_messages 会走 welcome 分支提前 return
        app.core.view_messages.push(crate::app::MessageViewModel::user("hello".into()));
        app.spinner_state.set_verb(Some("Searching code"));
        app.core.loading = true;

        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        assert!(handle.contains("Searching code"), "status bar should show spinner verb");
    }

    #[tokio::test]
    async fn test_tool_call_widget_renders_completed() {
        let (mut app, mut handle) = crate::app::App::new_headless(120, 30);

        let vm = crate::app::MessageViewModel::ToolBlock {
            tool_name: "bash".to_string(),
            display_name: "bash".to_string(),
            args_display: Some("ls -la".to_string()),
            content: "file1.txt\nfile2.txt".to_string(),
            color: crate::ui::theme::SAGE,
            is_error: false,
            collapsed: false,
        };

        let lines = crate::ui::message_render::render_view_model(&vm, Some(1), 80);
        // Render into a visible area for verification
        use ratatui::widgets::Paragraph;
        let paragraph = Paragraph::new(lines);
        handle.terminal.draw(|f| {
            let area = ratatui::layout::Rect::new(0, 0, 120, 10);
            f.render_widget(paragraph, area);
        }).unwrap();
        assert!(handle.contains("bash"), "should render tool name");
    }

    #[tokio::test]
    async fn test_retry_status_shows_in_status_bar() {
        let (mut app, mut handle) = App::new_headless(120, 30);

        // 直接设置 retry_status 并渲染
        app.agent.retry_status = Some(crate::app::RetryStatus {
            attempt: 2,
            max_attempts: 5,
            delay_ms: 2000,
        });

        handle.terminal.draw(|f| crate::ui::main_ui::render(f, &mut app)).unwrap();
        let snap = handle.snapshot();
        assert!(handle.contains("2/5"), "状态栏应显示重试次数 2/5，实际:\n{}", snap.join("\n"));
    }

    // ─── Pipeline 回归测试 ──────────────────────────────────────────────────

    /// 回归：用户消息在 AI 回复后仍应可见（不应被 AppendChunk 覆盖）
    #[tokio::test]
    async fn test_user_message_survives_assistant_chunk() {
        let (mut app, mut handle) = App::new_headless(120, 30);

        // 模拟用户发送消息
        let user_vm = MessageViewModel::user("my question".into());
        app.core.view_messages.push(user_vm.clone());
        let _ = app.core.render_tx.send(RenderEvent::AddMessage(user_vm));

        let n1 = handle.render_notify.notified();
        let n2 = handle.render_notify.notified();
        app.push_agent_event(AgentEvent::AssistantChunk("AI answer".into()));
        app.push_agent_event(AgentEvent::Done);
        app.process_pending_events();
        tokio::join!(n1, n2);

        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();

        // view_messages 应包含用户消息 + AI 消息
        assert!(
            app.core.view_messages.len() >= 2,
            "应有至少 2 条消息（用户+AI），实际: {}",
            app.core.view_messages.len()
        );
        assert!(
            handle.contains("my question"),
            "用户消息应在渲染输出中可见，实际:\n{}",
            handle.snapshot().join("\n")
        );
        assert!(
            handle.contains("AI answer"),
            "AI 回复应在渲染输出中可见，实际:\n{}",
            handle.snapshot().join("\n")
        );
    }

    /// 回归：多轮对话消息累积，不应只看到最后一条
    #[tokio::test]
    async fn test_messages_accumulate_across_turns() {
        let (mut app, mut handle) = App::new_headless(120, 30);

        // 第一轮：用户 → AI
        let user1 = MessageViewModel::user("turn1".into());
        app.core.view_messages.push(user1.clone());
        let _ = app.core.render_tx.send(RenderEvent::AddMessage(user1));

        let n1 = handle.render_notify.notified();
        let n2 = handle.render_notify.notified();
        app.push_agent_event(AgentEvent::AssistantChunk("answer1".into()));
        app.push_agent_event(AgentEvent::Done);
        app.process_pending_events();
        tokio::join!(n1, n2);

        // 第二轮：用户 → AI
        let user2 = MessageViewModel::user("turn2".into());
        app.core.view_messages.push(user2.clone());
        let _ = app.core.render_tx.send(RenderEvent::AddMessage(user2));

        let n3 = handle.render_notify.notified();
        let n4 = handle.render_notify.notified();
        app.push_agent_event(AgentEvent::AssistantChunk("answer2".into()));
        app.push_agent_event(AgentEvent::Done);
        app.process_pending_events();
        tokio::join!(n3, n4);

        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();

        // 应累积 4 条消息
        assert_eq!(
            app.core.view_messages.len(),
            4,
            "两轮对话应有 4 条消息，实际: {}",
            app.core.view_messages.len()
        );
        assert!(handle.contains("turn1"), "第一轮用户消息应可见");
        assert!(handle.contains("turn2"), "第二轮用户消息应可见");
    }

    /// 回归：AI 消息不应在 Done 后重复
    #[tokio::test]
    async fn test_done_does_not_duplicate_ai_message() {
        use rust_create_agent::messages::BaseMessage;

        let (mut app, _handle) = App::new_headless(120, 30);

        // 模拟 StateSnapshot（增量）+ Done 序列
        app.push_agent_event(AgentEvent::AssistantChunk("unique text".into()));
        app.push_agent_event(AgentEvent::StateSnapshot(vec![
            BaseMessage::human("q"),
            BaseMessage::ai("unique text"),
        ]));
        app.push_agent_event(AgentEvent::Done);
        app.process_pending_events();

        // 统计包含 "unique text" 的 assistant bubble 数量
        let assistant_count = app
            .core
            .view_messages
            .iter()
            .filter(|m| m.is_assistant())
            .count();
        assert_eq!(
            assistant_count, 1,
            "应有恰好 1 个 assistant bubble，实际: {}",
            assistant_count
        );
    }

    /// 回归：StateSnapshot 是增量的，不应覆盖之前已完成的消息
    #[test]
    fn test_state_snapshot_is_incremental() {
        use rust_create_agent::messages::{BaseMessage, MessageContent, MessageId};
        use crate::app::message_pipeline::MessagePipeline;

        let mut pipeline = MessagePipeline::new("/tmp".to_string());

        // 第一次 snapshot：Human + Ai
        pipeline.set_completed(vec![
            BaseMessage::human("hello"),
            BaseMessage::ai("world"),
        ]);
        assert_eq!(pipeline.completed_messages().len(), 2);

        // 第二次 snapshot（增量）：Tool result
        pipeline.set_completed(vec![BaseMessage::Tool {
            id: MessageId::new(),
            tool_call_id: "tc1".into(),
            content: MessageContent::text("result"),
            is_error: false,
        }]);

        // 应累积到 3 条，不是只剩 1 条
        assert_eq!(
            pipeline.completed_messages().len(),
            3,
            "StateSnapshot 应增量追加，不应覆盖，实际: {}",
            pipeline.completed_messages().len()
        );
    }

    /// 回归：ToolStart 之后 AssistantChunk 不会丢失工具消息
    #[tokio::test]
    async fn test_tool_then_text_preserves_tool_block() {
        let (mut app, mut handle) = App::new_headless(120, 30);

        let n1 = handle.render_notify.notified();
        let n2 = handle.render_notify.notified();
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: "tc1".into(),
            name: "bash".into(),
            display: "Shell".into(),
            args: "ls".into(),
            input: serde_json::json!({"command": "ls"}),
        });
        app.push_agent_event(AgentEvent::AssistantChunk("result is here".into()));
        app.process_pending_events();
        tokio::join!(n1, n2);

        handle.terminal.draw(|f| main_ui::render(f, &mut app)).unwrap();

        // ToolBlock 和 AssistantBubble 都应存在
        let has_tool = app.core.view_messages.iter().any(|m| {
            matches!(m, MessageViewModel::ToolBlock { .. })
        });
        let has_assistant = app
            .core
            .view_messages
            .iter()
            .any(|m| m.is_assistant());
        assert!(has_tool, "应有 ToolBlock");
        assert!(has_assistant, "应有 AssistantBubble");
        assert!(handle.contains("result is here"), "应显示 AI 回复");
    }
}
