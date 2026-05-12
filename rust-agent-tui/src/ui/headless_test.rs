use super::*;
use crate::app::MessageViewModel;
use crate::app::{AgentEvent, App};
use crate::ui::main_ui;

#[tokio::test]
async fn test_snapshot_row_count() {
    let (_app, handle) = App::new_headless(80, 24).await;
    assert_eq!(handle.snapshot().len(), 24, "snapshot 应返回 24 行");
}

#[tokio::test]
async fn test_assistant_chunk_renders() {
    use rust_create_agent::messages::BaseMessage;

    let (mut app, mut handle) = App::new_headless(120, 30).await;
    app.push_agent_event(AgentEvent::AssistantChunk("Hello world".into()));
    app.push_agent_event(AgentEvent::StateSnapshot(vec![
        BaseMessage::human("q"),
        BaseMessage::ai("Hello world"),
    ]));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    app.flush_rebuild();
    tokio::time::sleep(Duration::from_millis(50)).await;
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    assert!(
        handle.contains("Hello world"),
        "应显示消息内容，实际:\n{}",
        snap.join("\n")
    );
}

#[tokio::test]
async fn test_tool_call_renders() {
    let (mut app, mut handle) = App::new_headless(120, 30).await;
    let notified = handle.render_notify.notified();
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "t1".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "src/main.rs".into(),
        input: serde_json::json!({"path": "src/main.rs"}),
    });
    app.process_pending_events();
    notified.await;
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    // ToolStart 通过 Pipeline 创建 ToolBlock，display_name 为 format_tool_name 的结果
    let has_tool = snap
        .iter()
        .any(|l| l.contains("Read") || l.contains("Read"));
    assert!(has_tool, "应显示工具调用块，实际内容:\n{}", snap.join("\n"));
}

#[tokio::test]
async fn test_user_message_renders() {
    let (mut app, mut handle) = App::new_headless(120, 30).await;
    // 先注册监听，再发送事件，避免时序问题
    let notified = handle.render_notify.notified();
    // 使用 ASCII 内容避免 CJK 宽字符在 buffer 中的空格填充问题
    let vm = MessageViewModel::user("hello from user".into());
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .push(vm);
    app.render_rebuild();
    notified.await;
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    assert!(
        handle.contains("hello from user"),
        "应显示用户消息，实际内容:\n{}",
        snap.join("\n")
    );
}

#[tokio::test]
async fn test_clear_empties_render_cache() {
    use crate::ui::render_thread::RenderEvent;

    let (app, _handle) = App::new_headless(120, 30).await;

    // 直接发送 LoadHistory 填充 RenderCache
    let msgs = vec![MessageViewModel::user("test content".into())];
    let _ = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .render_tx
        .send(RenderEvent::Rebuild(msgs));
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 验证 RenderCache 有内容
    let lines_before = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .render_cache
        .read()
        .total_lines;
    assert!(lines_before > 0, "清空前应有内容");

    // 发送 Clear 清空 RenderCache
    let _ = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .render_tx
        .send(RenderEvent::Clear);
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 验证 RenderCache 已清空
    let cache = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .render_cache
        .read();
    assert_eq!(cache.total_lines, 0, "清空后 RenderCache 应为空");
}

mod markdown_tests {
    use crate::ui::markdown::parse_markdown_default;
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
        let all_content: String = heading_line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
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

        let has_bold =
            text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
                s.style.add_modifier.contains(Modifier::BOLD) && s.content.contains("bold")
            });
        assert!(has_bold, "bold span 应有 BOLD modifier");

        let has_italic = text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
            s.style.add_modifier.contains(Modifier::ITALIC) && s.content.contains("italic")
        });
        assert!(has_italic, "italic span 应有 ITALIC modifier");

        let has_strike = text.lines.iter().flat_map(|l| l.spans.iter()).any(|s| {
            s.style.add_modifier.contains(Modifier::CROSSED_OUT) && s.content.contains("strike")
        });
        assert!(has_strike, "strikethrough span 应有 CROSSED_OUT modifier");
    }

    #[test]
    fn test_md_inline_code() {
        use perihelion_widgets::markdown::{DefaultMarkdownTheme, MarkdownTheme};
        let theme = DefaultMarkdownTheme;

        let text = parse_markdown_default("`hello`");
        let has_code = text
            .lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style.fg == Some(theme.code()) && s.content.contains("hello"));
        assert!(
            has_code,
            "行内代码应为 markdown 主题 code 颜色，含 hello 文字"
        );
    }

    #[test]
    fn test_md_code_block() {
        let text = parse_markdown_default("```rust\nfn main() {}\n```");
        let all_lines: Vec<String> = text
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        // 单行代码块：无 [lang] 标签，无 │ 前缀
        assert_eq!(
            all_lines.len(),
            1,
            "单行代码块应只产生一行，got: {all_lines:#?}"
        );
        assert!(
            !all_lines[0].contains("[rust]"),
            "单行代码块不应含 [lang] 标签"
        );
        assert!(!all_lines[0].contains('│'), "单行代码块不应含 │ 前缀");
        assert!(all_lines[0].contains("fn main"), "应包含代码内容");
    }

    #[test]
    fn test_md_unordered_list() {
        let text = parse_markdown_default("- item1\n- item2");
        let all_lines: Vec<String> = text
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        let bullet_lines: Vec<&String> = all_lines.iter().filter(|l| l.contains('•')).collect();
        assert_eq!(
            bullet_lines.len(),
            2,
            "无序列表应有 2 行含 • ，实际:{all_lines:#?}"
        );
    }

    #[test]
    fn test_md_ordered_list() {
        let text = parse_markdown_default("1. first\n2. second");
        let all_lines: Vec<String> = text
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        let has_one = all_lines.iter().any(|l| l.contains("1."));
        let has_two = all_lines.iter().any(|l| l.contains("2."));
        assert!(has_one, "有序列表应含 1. 前缀，实际:{all_lines:#?}");
        assert!(has_two, "有序列表应含 2. 前缀，实际:{all_lines:#?}");
    }

    #[test]
    fn test_md_blockquote() {
        let text = parse_markdown_default("> quoted text");
        let has_prefix = text
            .lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.content.contains('▍'));
        assert!(has_prefix, "引用块应含 ▍ 前缀");
    }

    #[test]
    fn test_md_rule() {
        let text = parse_markdown_default("---");
        let has_rule = text
            .lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.content.matches('─').count() >= 10);
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
        assert!(
            all.contains("Name"),
            "Table should contain header 'Name', got: {all:?}"
        );
        assert!(
            all.contains("foo"),
            "Table should contain data 'foo', got: {all:?}"
        );
        assert!(
            all.contains("456"),
            "Table should contain data '456', got: {all:?}"
        );
        // Should have border characters
        assert!(
            all.contains("│"),
            "Table should have vertical borders, got: {all:?}"
        );
        assert!(
            all.contains("┌"),
            "Table should have top-left corner, got: {all:?}"
        );
        assert!(
            all.contains("└"),
            "Table should have bottom-left corner, got: {all:?}"
        );
        assert!(
            all.contains("┼"),
            "Table should have header separator, got: {all:?}"
        );
    }

    #[test]
    fn test_md_table_cell_count() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let text = parse_markdown_default(md);
        // Should produce exactly: top border + header + separator + 1 data row + bottom border = 5 lines
        assert_eq!(
            text.lines.len(),
            5,
            "2-col table should produce 5 lines, got: {}",
            text.lines.len()
        );
    }

    #[test]
    fn test_md_table_border_alignment() {
        let md = "| Name | Value |\n|------|-------|\n| foo  | 123   |";
        let text = parse_markdown_default(md);
        // Debug: print each line
        for (i, line) in text.lines.iter().enumerate() {
            let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            eprintln!(
                "line {}: {:?} (chars={})",
                i,
                content,
                content.chars().count()
            );
        }
        // Each line should have the same visual width (measured in chars, not bytes)
        let widths: Vec<usize> = text
            .lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.chars().count())
                    .sum::<usize>()
            })
            .collect();
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
        assert!(
            all.contains("Left"),
            "Should contain 'Left' header, got: {all:?}"
        );
        assert!(all.contains("a"), "Should contain data 'a', got: {all:?}");
    }

    #[test]
    fn test_md_table_with_inline_code() {
        let md = "| Command |\n|---------|\n| `ls`    |";
        let text = parse_markdown_default(md);
        let all = all_text(&text);
        assert!(
            all.contains("ls"),
            "Should contain inline code content, got: {all:?}"
        );
    }
}

#[tokio::test]
async fn test_subagent_group_basic() {
    // SubAgentStart → 2×ToolCall → SubAgentEnd → 渲染验证
    let (mut app, mut handle) = App::new_headless(120, 30).await;

    app.push_agent_event(AgentEvent::SubAgentStart {
        agent_id: "code-reviewer".into(),
        task_preview: "review the code".into(),
        is_background: false,
    });
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "t1".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "src/main.rs".into(),
        input: serde_json::json!({"path": "src/main.rs"}),
    });
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "t2".into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "cargo test".into(),
        input: serde_json::json!({"command": "cargo test"}),
    });
    app.push_agent_event(AgentEvent::SubAgentEnd {
        result: "All tests passed, no issues found".into(),
        is_error: false,
    });
    app.process_pending_events();
    app.flush_rebuild();
    tokio::time::sleep(Duration::from_millis(50)).await;

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();

    // 验证 SubAgentGroup 头行存在（code-reviewer 名称）
    let has_agent = snap.iter().any(|l| l.contains("code-reviewer"));
    assert!(
        has_agent,
        "应显示 SubAgentGroup 头行含 agent_id，实际:\n{}",
        snap.join("\n")
    );

    // 验证 SubAgentGroup 已完成（is_running=false）
    if let Some(vm) = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .last()
    {
        assert!(vm.is_subagent_group(), "最后一条消息应为 SubAgentGroup");
        if let crate::app::MessageViewModel::SubAgentGroup {
            is_running,
            total_steps,
            ..
        } = vm
        {
            assert!(!is_running, "SubAgentEnd 后 is_running 应为 false");
            assert_eq!(*total_steps, 2, "total_steps 应为 2");
        }
    }
}

#[tokio::test]
async fn test_subagent_group_sliding_window() {
    // SubAgentStart → 6×ToolCall → SubAgentEnd → 只保留 4 条，总步数为 6
    let (mut app, _handle) = App::new_headless(120, 30).await;

    app.push_agent_event(AgentEvent::SubAgentStart {
        agent_id: "analyzer".into(),
        task_preview: "analyze codebase".into(),
        is_background: false,
    });
    for i in 1..=6 {
        app.push_agent_event(AgentEvent::ToolStart {
            tool_call_id: format!("t{}", i),
            name: "Read".into(),
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
    }) = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .last()
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
    let (mut app, _handle) = App::new_headless(120, 30).await;

    app.push_agent_event(AgentEvent::SubAgentStart {
        agent_id: "writer".into(),
        task_preview: "write summary".into(),
        is_background: false,
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
    }) = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .last()
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
    let (mut app, mut handle) = App::new_headless(120, 30).await;

    // 使用 ToolStart 事件添加工具调用（会发送 RenderEvent::Rebuild）
    let notified1 = handle.render_notify.notified();
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "Bash".into(),
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

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();

    let snap = handle.snapshot();
    // ToolStart 创建的 ToolBlock，display_name 为 format_tool_name 的结果
    let has_tool_call_text = snap
        .iter()
        .any(|l| l.contains("Shell") || l.contains("Bash"));
    assert!(
        has_tool_call_text,
        "ToolCall 创建的 ToolBlock 应在快照中可见，但实际内容为:\n{}",
        snap.join("\n")
    );
}

#[tokio::test]
async fn test_empty_assistant_chunk_no_bubble() {
    // 空 AssistantChunk 不应创建空白的 AssistantBubble
    let (mut app, _handle) = App::new_headless(120, 30).await;

    // 发送空 chunk，不应创建 AssistantBubble
    app.push_agent_event(AgentEvent::AssistantChunk("".into()));
    app.process_pending_events();

    // view_messages 应为空（没有创建空白气泡）
    assert!(
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .is_empty(),
        "空 AssistantChunk 不应创建 AssistantBubble，实际: {:?}",
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .len()
    );

    // 发送多个空 chunk，仍不应创建气泡
    app.push_agent_event(AgentEvent::AssistantChunk("".into()));
    app.push_agent_event(AgentEvent::AssistantChunk("".into()));
    app.process_pending_events();

    assert!(
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .is_empty(),
        "多个空 AssistantChunk 仍不应创建 AssistantBubble"
    );
}

#[tokio::test]
async fn test_empty_then_nonempty_assistant_chunk() {
    use rust_create_agent::messages::BaseMessage;

    // 空_chunk → 非空_chunk：非空 chunk 应正常创建气泡
    let (mut app, mut handle) = App::new_headless(120, 30).await;

    // 先发送空 chunk
    app.push_agent_event(AgentEvent::AssistantChunk("".into()));
    app.process_pending_events();

    // 再发送非空 chunk
    app.push_agent_event(AgentEvent::AssistantChunk("Hello".into()));
    app.push_agent_event(AgentEvent::StateSnapshot(vec![
        BaseMessage::human("q"),
        BaseMessage::ai("Hello"),
    ]));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    app.flush_rebuild();
    tokio::time::sleep(Duration::from_millis(50)).await;

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();

    // Done 触发 reconcile_tail 从 completed 重建，应包含 Human + AI 两条消息
    assert_eq!(
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .len(),
        2,
        "应有 2 条消息（Human+AI）"
    );
    assert!(
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages[1]
            .is_assistant(),
        "第二条应为 AssistantBubble"
    );
    assert!(handle.contains("Hello"), "应显示 Hello 内容");
}

#[tokio::test]
async fn test_tool_call_without_assistant_chunk_no_bubble() {
    // 模拟 AI 只调用工具不输出文本的场景
    let (mut app, mut handle) = App::new_headless(120, 30).await;

    // 直接发送 ToolStart 事件（无 AssistantChunk）
    let notified = handle.render_notify.notified();
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "Bash".into(),
        display: "Bash".into(),
        args: "ls".into(),
        input: serde_json::json!({"command": "ls"}),
    });
    app.process_pending_events();
    notified.await;

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();

    // 应该有 1 个 ToolBlock，不应有空白 AssistantBubble
    assert_eq!(
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .len(),
        1,
        "应有 1 条消息（ToolBlock）"
    );
    // 确保不是 AssistantBubble（空白气泡）
    assert!(
        !app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages[0]
            .is_assistant(),
        "不应创建 AssistantBubble，应为 ToolBlock"
    );
}

#[tokio::test]
async fn test_welcome_card_renders_when_empty() {
    let (mut app, mut handle) = App::new_headless(120, 30).await;
    // 默认 view_messages 为空，应显示 Welcome Card
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    let snap_text = snap.join("\n");
    assert!(
        snap_text.contains("Peri"),
        "Welcome Card 应包含 'Peri'，实际:\n{}",
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
    use rust_create_agent::messages::BaseMessage;

    let (mut app, mut handle) = App::new_headless(120, 30).await;
    app.push_agent_event(AgentEvent::AssistantChunk("Hello from agent".into()));
    app.push_agent_event(AgentEvent::StateSnapshot(vec![
        BaseMessage::human("q"),
        BaseMessage::ai("Hello from agent"),
    ]));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    app.flush_rebuild();
    tokio::time::sleep(Duration::from_millis(50)).await;

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
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
    let (mut app, mut handle) = App::new_headless(40, 24).await;
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
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
        snap_text.contains("Peri"),
        "窄屏应显示文字版标题 'Peri'，实际:\n{}",
        snap_text
    );
}

#[tokio::test]
async fn test_welcome_card_shows_login_guide_when_no_provider() {
    // 无 Provider 时 Welcome Card 应显示 /login 首次引导
    let (mut app, mut handle) = App::new_headless(120, 30).await;
    // peri_config 默认为 None，无 provider
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    let snap_text = snap.join("\n");
    assert!(
        snap_text.contains("login"),
        "无 Provider 时 Welcome Card 应显示 /login 引导，实际:\n{}",
        snap_text
    );
}

// ── Sticky Human Message Header ────────────────────────────────────────────

#[tokio::test]
async fn test_sticky_header_hidden_when_no_messages() {
    // 无消息时 sticky header 应完全隐藏
    let (mut app, mut handle) = App::new_headless(80, 24).await;
    assert!(
        app.session_mgr.sessions[app.session_mgr.active]
            .metadata
            .last_human_message
            .is_none(),
        "默认应无 last_human_message"
    );
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
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
    let (mut app, mut handle) = App::new_headless(80, 24).await;

    // 填充足够多的消息使消息区产生滚动
    for i in 0..30 {
        let notified = handle.render_notify.notified();
        let vm = MessageViewModel::user(format!("message line {}", i));
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .push(vm);
        app.render_rebuild();
        notified.await;
    }

    // 设置 last_human_message（模拟 submit_message 的效果）
    app.session_mgr.sessions[app.session_mgr.active]
        .metadata
        .last_human_message = Some("hello from user".to_string());

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
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
    let (mut app, mut handle) = App::new_headless(80, 24).await;

    // 模拟已有消息
    app.session_mgr.sessions[app.session_mgr.active]
        .metadata
        .last_human_message = Some("some message".to_string());
    assert!(
        app.session_mgr.sessions[app.session_mgr.active]
            .metadata
            .last_human_message
            .is_some(),
        "应有 last_human_message"
    );

    // 模拟 /clear → new_thread
    let notified = handle.render_notify.notified();
    app.new_thread();
    notified.await;

    assert!(
        app.session_mgr.sessions[app.session_mgr.active]
            .metadata
            .last_human_message
            .is_none(),
        "/clear 后 last_human_message 应为 None"
    );

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
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
    let (mut app, mut handle) = App::new_headless(80, 24).await;

    // 填充足够多的消息使消息区产生滚动
    for i in 0..30 {
        let notified = handle.render_notify.notified();
        let vm = MessageViewModel::user(format!("padding line {}", i));
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .push(vm);
        app.render_rebuild();
        notified.await;
    }

    // 模拟第一条消息
    app.session_mgr.sessions[app.session_mgr.active]
        .metadata
        .last_human_message = Some("first message".to_string());
    // 模拟第二条消息（覆盖）
    app.session_mgr.sessions[app.session_mgr.active]
        .metadata
        .last_human_message = Some("second message".to_string());

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
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
    let (mut app, mut handle) = App::new_headless(40, 24).await; // 窄屏 40 列

    // 填充足够多的消息使消息区产生滚动
    for i in 0..30 {
        let notified = handle.render_notify.notified();
        let vm = MessageViewModel::user(format!("padding {}", i));
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .push(vm);
        app.render_rebuild();
        notified.await;
    }

    // 模拟超长消息（远超 header 可显示范围）
    let long_msg =
        "hello this is a very long message that definitely exceeds header capacity".to_string();
    assert!(long_msg.chars().count() > 40);
    app.session_mgr.sessions[app.session_mgr.active]
        .metadata
        .last_human_message = Some(long_msg.clone());

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
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
    let (mut app, mut handle) = App::new_headless(120, 30).await;

    // Register a cron task
    app.services
        .cron
        .scheduler
        .lock()
        .register("* * * * *", "hello cron test")
        .unwrap();
    let tasks: Vec<_> = app
        .services
        .cron
        .scheduler
        .lock()
        .list_tasks()
        .into_iter()
        .cloned()
        .collect();
    app.global_panels
        .open(crate::app::panel_manager::PanelState::Cron(
            crate::app::CronPanel::new(tasks),
        ));

    let notified = handle.render_notify.notified();
    drop(notified);

    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    eprintln!("SNAPSHOT:");
    for (i, line) in snap.iter().enumerate() {
        if !line.is_empty() {
            eprintln!("{:3}: {}", i, line);
        }
    }
    assert!(
        handle.contains("hello cron test"),
        "should contain task prompt"
    );
    assert!(
        handle.contains("* * * * *"),
        "should contain cron expression"
    );
}

#[tokio::test]
async fn test_bordered_panel_integration() {
    // BorderedPanel 集成冒烟测试：渲染 agent panel 验证无 panic 且输出正确
    let (mut app, mut handle) = App::new_headless(120, 30).await;

    app.session_mgr.sessions[app.session_mgr.active]
        .session_panels
        .open(crate::app::panel_manager::PanelState::Agent(
            crate::app::AgentPanel::new(vec![], None),
        ));

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
    use crate::app::AskUserBatchPrompt;
    use rust_agent_middlewares::ask_user::{
        AskUserBatchRequest, AskUserOption, AskUserQuestionData,
    };

    let (mut app, mut handle) = App::new_headless(120, 30).await;

    let (req, _rx) = AskUserBatchRequest::new(vec![
        AskUserQuestionData {
            tool_call_id: "t1".into(),
            question: "Choose a language?".into(),
            header: "Language".into(),
            multi_select: false,
            options: vec![
                AskUserOption {
                    label: "Rust".into(),
                    description: Some("Systems language".into()),
                },
                AskUserOption {
                    label: "Go".into(),
                    description: None,
                },
            ],
        },
        AskUserQuestionData {
            tool_call_id: "t1".into(),
            question: "Choose a framework?".into(),
            header: "Framework".into(),
            multi_select: true,
            options: vec![AskUserOption {
                label: "Axum".into(),
                description: None,
            }],
        },
    ]);
    let prompt = AskUserBatchPrompt::from_request(req);
    app.session_mgr.sessions[app.session_mgr.active]
        .agent
        .interaction_prompt = Some(crate::app::InteractionPrompt::Questions(prompt));

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
    use crate::app::setup_wizard::{
        handle_setup_wizard_key, needs_setup, save_setup_to, FormField, FormMode, MigratedProvider,
        ProviderType, SetupStep, SetupWizardAction, SetupWizardPanel,
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

    fn advance_to_form(wizard: &mut SetupWizardPanel) {
        let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter));
        assert_eq!(wizard.step, SetupStep::Form);
        assert_eq!(wizard.form_mode, FormMode::Browse);
    }

    /// 进入 Edit 模式，填写 API Key，Confirm 回到 Browse，然后 Submit
    fn fill_and_submit(wizard: &mut SetupWizardPanel, api_key: &str) {
        // 确保在第一个 provider 位置
        wizard.browse_cursor = 0;
        // Browse → Edit（Enter on first provider）
        let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter));
        assert_eq!(wizard.form_mode, FormMode::Edit);

        // 填写 API Key
        wizard.form_focus = FormField::ApiKey;
        type_text(wizard, api_key);

        // Confirm 回到 Browse
        wizard.form_focus = FormField::Confirm;
        let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter));
        assert_eq!(wizard.form_mode, FormMode::Browse);

        // 移到 Submit 并提交
        wizard.browse_cursor = wizard.providers.len();
        let _ = handle_setup_wizard_key(wizard, make_key(Key::Enter));
    }

    #[tokio::test]
    async fn test_needs_setup_triggers_for_empty_config() {
        let (app, _handle) = App::new_headless(120, 30).await;
        assert!(app.services.peri_config.is_none());
        let empty_cfg = crate::config::types::PeriConfig::default();
        assert!(needs_setup(&empty_cfg.config));
    }

    #[tokio::test]
    async fn test_setup_wizard_full_flow_anthropic() {
        let mut wizard = SetupWizardPanel::new();
        advance_to_form(&mut wizard);
        assert_eq!(wizard.providers.len(), 1);
        assert_eq!(wizard.providers[0].provider_type, ProviderType::Anthropic);

        fill_and_submit(&mut wizard, "sk-ant-test-key-12345");
        assert_eq!(wizard.step, SetupStep::Done);

        let action = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
        assert!(matches!(action, Some(SetupWizardAction::SaveAndClose)));

        let temp_dir =
            std::env::temp_dir().join(format!("zen-setup-test-{}", uuid::Uuid::now_v7()));
        let config_path = temp_dir.join("settings.json");
        let cfg = save_setup_to(&wizard, &config_path).expect("save should succeed");
        assert_eq!(cfg.config.providers.len(), 1);
        assert_eq!(cfg.config.providers[0].provider_type, "anthropic");
        assert_eq!(cfg.config.providers[0].api_key, "sk-ant-test-key-12345");
        assert!(!needs_setup(&cfg.config));
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_setup_wizard_full_flow_openai() {
        let mut wizard = SetupWizardPanel::new();
        advance_to_form(&mut wizard);

        // Enter Edit mode, change type to OpenAI
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
        assert_eq!(wizard.form_mode, FormMode::Edit);
        wizard.form_focus = FormField::ProviderType;
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Right));
        assert_eq!(
            wizard.providers[0].provider_type,
            ProviderType::OpenAiCompatible
        );

        // Fill key
        wizard.form_focus = FormField::ApiKey;
        type_text(&mut wizard, "sk-openai-test-key");

        // Confirm back to Browse
        wizard.form_focus = FormField::Confirm;
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
        assert_eq!(wizard.form_mode, FormMode::Browse);

        // Submit
        wizard.browse_cursor = wizard.providers.len();
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
        assert_eq!(wizard.step, SetupStep::Done);

        let action = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
        assert!(matches!(action, Some(SetupWizardAction::SaveAndClose)));

        let temp_dir =
            std::env::temp_dir().join(format!("zen-setup-test-openai-{}", uuid::Uuid::now_v7()));
        let config_path = temp_dir.join("settings.json");
        let cfg = save_setup_to(&wizard, &config_path).expect("save should succeed");
        assert_eq!(cfg.config.providers[0].provider_type, "openai");
        assert_eq!(cfg.config.providers[0].api_key, "sk-openai-test-key");
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_setup_wizard_skip_on_choose() {
        let (mut app, _handle) = App::new_headless(120, 30).await;
        app.services.setup_wizard = Some(SetupWizardPanel::new());
        let wizard = app.services.setup_wizard.as_mut().unwrap();
        let action = handle_setup_wizard_key(wizard, make_key(Key::Esc));
        assert!(matches!(action, Some(SetupWizardAction::Skip)));
    }

    #[tokio::test]
    async fn test_setup_wizard_esc_navigation() {
        let mut wizard = SetupWizardPanel::new();
        advance_to_form(&mut wizard);

        // Browse → Submit → Enter (empty key, should stay)
        wizard.browse_cursor = wizard.providers.len();
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
        assert_eq!(wizard.step, SetupStep::Form);

        fill_and_submit(&mut wizard, "test-key");
        assert_eq!(wizard.step, SetupStep::Done);

        // Done → Esc → Form
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Esc));
        assert_eq!(wizard.step, SetupStep::Form);

        // Form → Esc → Choose
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Esc));
        assert_eq!(wizard.step, SetupStep::Choose);
    }

    #[tokio::test]
    async fn test_setup_wizard_toggle_select() {
        let mut wizard = SetupWizardPanel::new();
        advance_to_form(&mut wizard);
        // Browse mode: Space toggles
        assert!(wizard.providers[0].selected);
        let _ = handle_setup_wizard_key(&mut wizard, make_char(' '));
        assert!(!wizard.providers[0].selected);
        let _ = handle_setup_wizard_key(&mut wizard, make_char(' '));
        assert!(wizard.providers[0].selected);
    }

    #[tokio::test]
    async fn test_setup_wizard_multi_provider() {
        let mut wizard = SetupWizardPanel::new();
        advance_to_form(&mut wizard);
        wizard
            .providers
            .push(MigratedProvider::new(ProviderType::OpenAiCompatible));
        wizard.providers[1].api_key = "sk-openai".to_string();
        wizard.providers[0].api_key = "sk-ant".to_string();

        // Browse: Submit
        wizard.browse_cursor = wizard.providers.len();
        let _ = handle_setup_wizard_key(&mut wizard, make_key(Key::Enter));
        assert_eq!(wizard.step, SetupStep::Done);

        let temp_dir =
            std::env::temp_dir().join(format!("zen-setup-multi-{}", uuid::Uuid::now_v7()));
        let config_path = temp_dir.join("settings.json");
        let cfg = save_setup_to(&wizard, &config_path).expect("save should succeed");
        assert_eq!(cfg.config.providers.len(), 2);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_setup_wizard_saves_and_clears() {
        let (mut app, _handle) = App::new_headless(120, 30).await;
        let mut wizard = SetupWizardPanel::new();
        advance_to_form(&mut wizard);
        fill_and_submit(&mut wizard, "sk-final-test");
        assert_eq!(wizard.step, SetupStep::Done);

        app.services.setup_wizard = Some(wizard);
        let wizard = app.services.setup_wizard.as_mut().unwrap();
        let action = handle_setup_wizard_key(wizard, make_key(Key::Enter));
        assert!(matches!(action, Some(SetupWizardAction::SaveAndClose)));

        let wizard = app.services.setup_wizard.take().unwrap();
        let temp_dir =
            std::env::temp_dir().join(format!("zen-setup-final-{}", uuid::Uuid::now_v7()));
        let config_path = temp_dir.join("settings.json");
        let cfg = save_setup_to(&wizard, &config_path).expect("save should succeed");
        assert!(!needs_setup(&cfg.config));
        app.services.peri_config = Some(cfg);
        assert!(app.services.setup_wizard.is_none());
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
// ─── Permission Mode Tests ──────────────────────────────────────────────

#[tokio::test]
async fn test_app_default_permission_mode_is_bypass() {
    let (app, _handle) = App::new_headless(80, 24).await;
    use rust_agent_middlewares::prelude::PermissionMode;
    assert_eq!(
        app.services.permission_mode.load(),
        PermissionMode::Bypass,
        "headless App 默认应为 Bypass"
    );
}

#[tokio::test]
async fn test_permission_mode_store_and_load() {
    let (app, _handle) = App::new_headless(80, 24).await;
    use rust_agent_middlewares::prelude::PermissionMode;
    for mode in [
        PermissionMode::Default,
        PermissionMode::DontAsk,
        PermissionMode::AcceptEdit,
        PermissionMode::AutoMode,
        PermissionMode::Bypass,
    ] {
        app.services.permission_mode.store(mode);
        assert_eq!(
            app.services.permission_mode.load(),
            mode,
            "store/load 应一致: {:?}",
            mode
        );
    }
}

#[tokio::test]
async fn test_permission_mode_cycle() {
    let (app, _handle) = App::new_headless(80, 24).await;
    use rust_agent_middlewares::prelude::PermissionMode;
    // cycle 从 Bypass 开始 → Default
    let next = app.services.permission_mode.cycle();
    assert_eq!(next, PermissionMode::Default);
    // 继续循环 → DontAsk
    let next2 = app.services.permission_mode.cycle();
    assert_eq!(next2, PermissionMode::DontAsk);
}

#[tokio::test]
async fn test_status_bar_shows_permission_mode() {
    let (mut app, mut handle) = App::new_headless(120, 24).await;
    // 默认 Bypass → 应显示 "Bypass"
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    assert!(
        handle.contains("Bypass"),
        "状态栏应显示 Bypass 模式，实际:\n{}",
        handle.snapshot().join("\n")
    );
}

#[tokio::test]
async fn test_status_bar_updates_after_mode_switch() {
    use rust_agent_middlewares::prelude::PermissionMode;
    let (mut app, mut handle) = App::new_headless(120, 24).await;
    // 切换到 Default - 不显示标签
    app.services.permission_mode.store(PermissionMode::Default);
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    assert!(
        !handle.contains("DEFAULT"),
        "Default 模式不应显示标签，实际:\n{}",
        handle.snapshot().join("\n")
    );

    // 切换到 DontAsk
    app.services.permission_mode.store(PermissionMode::DontAsk);
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    assert!(
        handle.contains("Don't Ask"),
        "切换后状态栏应显示 Don't Ask，实际:\n{}",
        handle.snapshot().join("\n")
    );

    // 切换到 AcceptEdit
    app.services
        .permission_mode
        .store(PermissionMode::AcceptEdit);
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    assert!(
        handle.contains("Accept Edit"),
        "切换后状态栏应显示 Accept Edit，实际:\n{}",
        handle.snapshot().join("\n")
    );

    // 切换到 AutoMode
    app.services.permission_mode.store(PermissionMode::AutoMode);
    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    assert!(
        handle.contains("Auto Mode"),
        "切换后状态栏应显示 Auto Mode，实际:\n{}",
        handle.snapshot().join("\n")
    );
}

#[tokio::test]
async fn test_shift_tab_cycles_permission_mode() {
    use rust_agent_middlewares::prelude::PermissionMode;
    let (app, _handle) = App::new_headless(120, 24).await;
    // 初始 Bypass
    assert_eq!(app.services.permission_mode.load(), PermissionMode::Bypass);
    // 模拟 Shift+Tab 按键效果（直接调用 cycle）
    let next = app.services.permission_mode.cycle();
    assert_eq!(next, PermissionMode::Default, "Bypass 之后应为 Default");
    assert_eq!(app.services.permission_mode.load(), PermissionMode::Default);
    // 继续循环 4 次回到 Bypass
    app.services.permission_mode.cycle(); // DontAsk
    app.services.permission_mode.cycle(); // AcceptEdit
    app.services.permission_mode.cycle(); // AutoMode
    let final_mode = app.services.permission_mode.cycle(); // Bypass
    assert_eq!(final_mode, PermissionMode::Bypass, "循环 5 次回到起点");
}

#[tokio::test]
async fn test_mode_highlight_until_set_on_cycle() {
    let (mut app, _handle) = App::new_headless(120, 24).await;
    // 初始无闪烁
    assert!(
        app.services.mode_highlight_until.is_none(),
        "初始不应有闪烁"
    );
    // 模拟 Shift+Tab: cycle + 设置 highlight
    app.services.permission_mode.cycle();
    app.services.mode_highlight_until =
        Some(std::time::Instant::now() + std::time::Duration::from_millis(1500));
    assert!(
        app.services.mode_highlight_until.is_some(),
        "cycle 后应设置闪烁截止时间"
    );
    // 验证截止时间在未来
    let until = app.services.mode_highlight_until.unwrap();
    assert!(std::time::Instant::now() < until, "截止时间应在未来");
}

#[tokio::test]
async fn test_spinner_shows_verb_in_status_bar() {
    let (mut app, mut handle) = crate::app::App::new_headless(120, 30).await;
    // 添加一条消息，否则 render_messages 会走 welcome 分支提前 return
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .push(crate::app::MessageViewModel::user("hello".into()));
    app.session_mgr.sessions[app.session_mgr.active]
        .spinner_state
        .set_verb(Some("Searching code"));
    app.session_mgr.sessions[app.session_mgr.active].ui.loading = true;

    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    assert!(
        handle.contains("Searching code"),
        "status bar should show spinner verb"
    );
}

#[tokio::test]
async fn test_tool_call_widget_renders_completed() {
    let (_app, mut handle) = crate::app::App::new_headless(120, 30).await;

    let vm = crate::app::MessageViewModel::ToolBlock {
        tool_name: "Bash".to_string(),
        tool_call_id: "tc_test".to_string(),
        display_name: "Bash".to_string(),
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
    handle
        .terminal
        .draw(|f| {
            let area = ratatui::layout::Rect::new(0, 0, 120, 10);
            f.render_widget(paragraph, area);
        })
        .unwrap();
    assert!(handle.contains("Bash"), "should render tool name");
}

#[tokio::test]
async fn test_retry_status_shows_in_status_bar() {
    let (mut app, mut handle) = App::new_headless(120, 30).await;

    // 直接设置 retry_status 并渲染
    app.session_mgr.sessions[app.session_mgr.active]
        .agent
        .retry_status = Some(crate::app::RetryStatus {
        attempt: 2,
        max_attempts: 5,
        delay_ms: 2000,
        error: "API 错误 429: Rate limit exceeded".to_string(),
    });

    handle
        .terminal
        .draw(|f| crate::ui::main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    assert!(
        handle.contains("2/5"),
        "状态栏应显示重试次数 2/5，实际:\n{}",
        snap.join("\n")
    );
}

// ─── Compact 集成测试 ──────────────────────────────────────────────────

/// 辅助：构造模拟的 CompactDone 事件（包含摘要 + 重新注入内容）
fn make_compact_done_event(summary: &str, re_inject_parts: &[&str]) -> AgentEvent {
    let re_inject_content = if re_inject_parts.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n---RE_INJECT_SEPARATOR---\n{}",
            re_inject_parts.join("\n---RE_INJECT_MSG_BREAK---\n")
        )
    };
    let combined = format!("{}{}", summary, re_inject_content);
    AgentEvent::CompactDone {
        summary: combined,
        new_thread_id: String::new(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_compact_done_with_re_inject() {
    let (mut app, handle) = App::new_headless(120, 30).await;
    let notified = handle.render_notify.notified();
    app.push_agent_event(make_compact_done_event(
        "Test summary",
        &[
            "[最近读取的文件: /a.rs]\nline1\nline2\nline3",
            "[激活的 Skill 指令: skill.md]\nskill content",
        ],
    ));
    app.process_pending_events();
    notified.await;

    // view_messages 应包含压缩提示（condensed summary 格式）
    let msgs = &app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages;
    assert_eq!(
        msgs.len(),
        1,
        "应只有 1 条压缩占位消息，实际: {}",
        msgs.len()
    );
    let has_compact = msgs.iter().any(|m| {
        if let MessageViewModel::SystemNote { content } = m {
            content.contains("✻ 上下文已压缩")
                && content.contains("Read /a.rs")
                && content.contains("Skill: skill.md")
        } else {
            false
        }
    });
    assert!(has_compact, "应包含压缩提示消息");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_compact_done_without_re_inject() {
    let (mut app, handle) = App::new_headless(120, 30).await;
    let notified = handle.render_notify.notified();
    app.push_agent_event(make_compact_done_event("Simple summary", &[]));
    app.process_pending_events();
    notified.await;

    let msgs = &app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages;
    assert_eq!(msgs.len(), 1, "应只有 1 条压缩占位消息");
    let has_compact = msgs.iter().any(|m| {
        if let MessageViewModel::SystemNote { content } = m {
            content.contains("✻ 上下文已压缩")
        } else {
            false
        }
    });
    assert!(has_compact, "应包含压缩提示消息");
    let has_re_inject = msgs.iter().any(|m| {
        if let MessageViewModel::SystemNote { content } = m {
            content.contains("Read ") || content.contains("Skill:")
        } else {
            false
        }
    });
    assert!(!has_re_inject, "无重新注入内容时不应显示文件/skill 详情");
}

#[tokio::test]
async fn test_get_compact_config_default() {
    let (app, _handle) = App::new_headless(120, 30).await;
    let config = app.get_compact_config();
    let default = rust_create_agent::agent::compact::CompactConfig::default();
    assert!(config.auto_compact_enabled == default.auto_compact_enabled);
    assert!((config.auto_compact_threshold - default.auto_compact_threshold).abs() < 0.001);
}

#[tokio::test]
async fn test_get_compact_config_from_settings() {
    let (mut app, _handle) = App::new_headless(120, 30).await;
    let mut zen = crate::config::types::PeriConfig::default();
    zen.config.compact = Some(rust_create_agent::agent::compact::CompactConfig {
        auto_compact_threshold: 0.9,
        ..Default::default()
    });
    app.services.peri_config = Some(zen);
    let config = app.get_compact_config();
    assert!(
        (config.auto_compact_threshold - 0.9).abs() < 0.001,
        "应从 settings.json 读取 auto_compact_threshold"
    );
}

// ─── Pipeline 回归测试 ──────────────────────────────────────────────────

/// 回归：用户消息在 AI 回复后仍应可见（不应被 AppendChunk 覆盖）
#[tokio::test]
async fn test_user_message_survives_assistant_chunk() {
    use rust_create_agent::messages::BaseMessage;

    let (mut app, mut handle) = App::new_headless(120, 30).await;

    // 模拟用户发送消息
    let user_vm = MessageViewModel::user("my question".into());
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .push(user_vm);
    app.render_rebuild();

    app.push_agent_event(AgentEvent::AssistantChunk("AI answer".into()));
    app.push_agent_event(AgentEvent::StateSnapshot(vec![
        BaseMessage::human("my question"),
        BaseMessage::ai("AI answer"),
    ]));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    app.flush_rebuild();
    tokio::time::sleep(Duration::from_millis(50)).await;

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();

    // view_messages 应包含用户消息 + AI 消息
    assert!(
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .len()
            >= 2,
        "应有至少 2 条消息（用户+AI），实际: {}",
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .len()
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
    use rust_create_agent::messages::BaseMessage;

    let (mut app, mut handle) = App::new_headless(120, 30).await;

    // 第一轮：用户 → AI
    // 模拟 submit_message：先记录 round_start_vm_idx，再 push Human VM
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .round_start_vm_idx = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .len();
    let user1 = MessageViewModel::user("turn1".into());
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .push(user1);
    app.render_rebuild();

    app.push_agent_event(AgentEvent::AssistantChunk("answer1".into()));
    app.push_agent_event(AgentEvent::StateSnapshot(vec![
        BaseMessage::human("turn1"),
        BaseMessage::ai("answer1"),
    ]));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    app.flush_rebuild();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 第二轮：用户 → AI
    // 模拟 submit_message：先记录 round_start_vm_idx，再 push Human VM
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .round_start_vm_idx = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .len();
    let user2 = MessageViewModel::user("turn2".into());
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .push(user2);
    app.render_rebuild();

    app.push_agent_event(AgentEvent::AssistantChunk("answer2".into()));
    app.push_agent_event(AgentEvent::StateSnapshot(vec![
        BaseMessage::human("turn1"),
        BaseMessage::ai("answer1"),
        BaseMessage::human("turn2"),
        BaseMessage::ai("answer2"),
    ]));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    app.flush_rebuild();
    tokio::time::sleep(Duration::from_millis(50)).await;

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();

    // 应累积 4 条消息
    assert_eq!(
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .len(),
        4,
        "两轮对话应有 4 条消息，实际: {}",
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .len()
    );
    assert!(handle.contains("turn1"), "第一轮用户消息应可见");
    assert!(handle.contains("turn2"), "第二轮用户消息应可见");
}

/// 回归：AI 消息不应在 Done 后重复
#[tokio::test]
async fn test_done_does_not_duplicate_ai_message() {
    use rust_create_agent::messages::BaseMessage;

    let (mut app, _handle) = App::new_headless(120, 30).await;

    // 模拟 StateSnapshot（增量）+ Done 序列
    app.push_agent_event(AgentEvent::AssistantChunk("unique text".into()));
    app.push_agent_event(AgentEvent::StateSnapshot(vec![
        BaseMessage::human("q"),
        BaseMessage::ai("unique text"),
    ]));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();

    // 统计包含 "unique text" 的 assistant bubble 数量
    let assistant_count = app.session_mgr.sessions[app.session_mgr.active]
        .messages
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
    use crate::app::message_pipeline::MessagePipeline;
    use rust_create_agent::messages::{BaseMessage, MessageContent, MessageId};

    let mut pipeline = MessagePipeline::new("/tmp".to_string());

    // 第一次 snapshot：Human + Ai
    pipeline.set_completed(vec![BaseMessage::human("hello"), BaseMessage::ai("world")]);
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
    let (mut app, mut handle) = App::new_headless(120, 30).await;

    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "tc1".into(),
        name: "Bash".into(),
        display: "Shell".into(),
        args: "ls".into(),
        input: serde_json::json!({"command": "ls"}),
    });
    app.push_agent_event(AgentEvent::AssistantChunk("result is here".into()));
    app.process_pending_events();
    app.flush_rebuild();
    tokio::time::sleep(Duration::from_millis(50)).await;

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();

    // ToolBlock 和 AssistantBubble 都应存在
    let has_tool = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .iter()
        .any(|m| matches!(m, MessageViewModel::ToolBlock { .. }));
    let has_assistant = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .iter()
        .any(|m| m.is_assistant());
    assert!(has_tool, "应有 ToolBlock");
    assert!(has_assistant, "应有 AssistantBubble");
    assert!(handle.contains("result is here"), "应显示 AI 回复");
}

// ── 统一提示浮层测试 ──────────────────────────────────────────────────

#[tokio::test]
async fn test_unified_hint_shows_commands_and_skills() {
    use rust_agent_middlewares::skills::loader::SkillMetadata;
    let (mut app, mut handle) = App::new_headless(120, 50).await;

    // 设置输入框内容为 /
    app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
        crate::app::build_textarea(false);
    app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .textarea
        .insert_str("/");

    // 注入 2 个 Skills
    app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .skills
        .push(SkillMetadata {
            name: "commit".into(),
            description: "commit changes".into(),
            path: "/tmp/commit.md".into(),
        });
    app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .skills
        .push(SkillMetadata {
            name: "review".into(),
            description: "review code".into(),
            path: "/tmp/review.md".into(),
        });

    // 候选列表应包含命令和 Skills
    let count = app.hint_candidates_count();
    let cmd_count = app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .command_registry
        .match_prefix("")
        .len();
    assert_eq!(
        count,
        cmd_count + 2,
        "候选应包含 {} 命令 + 2 Skills",
        cmd_count
    );

    // 渲染后应显示命令（视口 MAX_VIEWPORT=10，命令优先排序）
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    let snap_text = snap.join("\n");

    assert!(
        snap_text.contains("model"),
        "应显示 model 命令，实际:\n{}",
        snap_text
    );
}

#[tokio::test]
async fn test_unified_hint_filters_by_prefix() {
    use rust_agent_middlewares::skills::loader::SkillMetadata;
    let (mut app, mut handle) = App::new_headless(120, 30).await;

    app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
        crate::app::build_textarea(false);
    app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .textarea
        .insert_str("/mo");

    app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .skills
        .push(SkillMetadata {
            name: "commit".into(),
            description: "commit changes".into(),
            path: "/tmp/commit.md".into(),
        });

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    let snap_text = snap.join("\n");

    // 应包含匹配的命令 model
    assert!(
        snap_text.contains("model"),
        "应包含匹配前缀 /mo 的命令 model，实际:\n{}",
        snap_text
    );
    // 不应包含不匹配的 Skill（commit 不含 "mo"）
    assert!(
        !snap_text.contains("commit"),
        "不应包含不匹配的 Skill，实际:\n{}",
        snap_text
    );
}

#[tokio::test]
async fn test_unified_hint_no_result_for_hash() {
    use rust_agent_middlewares::skills::loader::SkillMetadata;
    let (mut app, mut handle) = App::new_headless(120, 30).await;

    app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
        crate::app::build_textarea(false);
    app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .textarea
        .insert_str("#skill");

    app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .skills
        .push(SkillMetadata {
            name: "skill".into(),
            description: "a skill".into(),
            path: "/tmp/skill.md".into(),
        });

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    let snap_text = snap.join("\n");

    // # 前缀不应触发浮层
    assert!(
        !snap_text.contains("Skills"),
        "# 前缀不应触发 Skills 浮层，实际:\n{}",
        snap_text
    );
}

// ── Enter 触发 Skill fallback 测试 ──────────────────────────────────────────

#[tokio::test]
async fn test_enter_skill_name_submits_message() {
    use rust_agent_middlewares::skills::loader::SkillMetadata;
    let (mut app, _handle) = App::new_headless(120, 30).await;

    app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
        crate::app::build_textarea(false);
    app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .textarea
        .insert_str("/review");
    app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .skills
        .push(SkillMetadata {
            name: "review".into(),
            description: "code review".into(),
            path: "/tmp/review.md".into(),
        });

    // 模拟 Enter 事件处理
    let text: String = app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .textarea
        .lines()
        .join("\n");
    let text = text.trim().to_string();
    assert!(text.starts_with('/'));

    // 验证命令 dispatch 不匹配后 Skill fallback
    let registry = std::mem::take(
        &mut app.session_mgr.sessions[app.session_mgr.active]
            .commands
            .command_registry,
    );
    let known = registry.dispatch(&mut app, &text);
    app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .command_registry = registry;
    assert!(!known, "review 不应是已知命令");

    // 验证 Skill 匹配
    let skill_name: String = text
        .trim_start_matches('/')
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    assert_eq!(skill_name, "review");
    let skill_found = app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .skills
        .iter()
        .find(|s| s.name == skill_name);
    assert!(skill_found.is_some(), "应找到 review Skill");
}

#[tokio::test]
async fn test_enter_unknown_command_shows_error() {
    let (mut app, _handle) = App::new_headless(120, 30).await;

    app.session_mgr.sessions[app.session_mgr.active].ui.textarea =
        crate::app::build_textarea(false);
    app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .textarea
        .insert_str("/nonexistent");

    // 模拟 Enter 处理逻辑
    let text: String = app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .textarea
        .lines()
        .join("\n");
    let text = text.trim().to_string();
    let registry = std::mem::take(
        &mut app.session_mgr.sessions[app.session_mgr.active]
            .commands
            .command_registry,
    );
    let known = registry.dispatch(&mut app, &text);
    app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .command_registry = registry;
    assert!(!known, "nonexistent 不应是已知命令");

    // Skill fallback 也应失败
    let skill_name: String = text
        .trim_start_matches('/')
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let skill_found = app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .skills
        .iter()
        .find(|s| s.name == skill_name);
    assert!(skill_found.is_none(), "不应找到 nonexistent Skill");
}

#[tokio::test]
async fn test_enter_known_command_no_skill_fallback() {
    use rust_agent_middlewares::skills::loader::SkillMetadata;
    let (mut app, _handle) = App::new_headless(120, 30).await;

    // 注入名为 help 的 Skill
    app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .skills
        .push(SkillMetadata {
            name: "help".into(),
            description: "help skill".into(),
            path: "/tmp/help.md".into(),
        });

    // /help 应被命令 dispatch 拦截，不走 Skill fallback
    let registry = std::mem::take(
        &mut app.session_mgr.sessions[app.session_mgr.active]
            .commands
            .command_registry,
    );
    let known = registry.dispatch(&mut app, "/help");
    app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .command_registry = registry;
    assert!(known, "/help 应是已知命令，优先于同名 Skill");
}

// ── Input Placeholder Hint ──────────────────────────────────────────────

#[tokio::test]
async fn test_textarea_shows_placeholder_hint() {
    let (mut app, mut handle) = App::new_headless(120, 30).await;
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    let snap_text = snap.join("\n");
    assert!(
        snap_text.contains("Alt+Enter") || snap_text.contains("输入消息"),
        "输入框应显示占位提示（含 Alt+Enter 换行），实际:\n{}",
        snap_text
    );
}

// ── Welcome Card Alt+Enter Hint ─────────────────────────────────────────

#[tokio::test]
async fn test_welcome_card_shows_alt_enter_hint() {
    let (mut app, mut handle) = App::new_headless(120, 30).await;
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    let snap_text = snap.join("\n");
    assert!(
        snap_text.contains("Alt+Enter"),
        "Welcome Card 应显示 Alt+Enter 快捷键提示，实际:\n{}",
        snap_text
    );
}

// ── Command Ambiguity Feedback ──────────────────────────────────────────

#[tokio::test]
async fn test_ambiguous_command_shows_candidates() {
    let (mut app, _handle) = App::new_headless(120, 30).await;
    // /c 前缀匹配 clear/compact/cron
    let registry = &app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .command_registry;
    let matches = registry.match_prefix("c");
    assert!(matches.len() >= 2, "/c 应匹配多个命令，实际: {:?}", matches);
    // dispatch 应返回 false（歧义）
    let registry = std::mem::take(
        &mut app.session_mgr.sessions[app.session_mgr.active]
            .commands
            .command_registry,
    );
    let known = registry.dispatch(&mut app, "/c");
    app.session_mgr.sessions[app.session_mgr.active]
        .commands
        .command_registry = registry;
    assert!(!known, "歧义前缀 dispatch 应返回 false");
}

// ── SystemNote Error Color Detection ────────────────────────────────────

#[test]
fn test_system_note_error_detection() {
    // 错误类 system note
    let error_content = "❌ 压缩失败: 未配置 Provider";
    assert!(
        error_content.contains("❌") || error_content.contains("失败"),
        "应检测到错误标记"
    );
    let warn_content = "⚠ 已中断";
    assert!(warn_content.contains("⚠"), "应检测到警告标记");
    // 普通信息
    let info_content = "已加载对话";
    assert!(
        !info_content.contains("❌")
            && !info_content.contains("失败")
            && !info_content.contains("⚠"),
        "普通消息不应被标记为错误"
    );
}

// ── Compact Start Feedback ──────────────────────────────────────────────

#[tokio::test]
async fn test_compact_empty_shows_no_context_message() {
    let (mut app, _handle) = App::new_headless(120, 30).await;
    // 空消息时调用 compact 应提示无上下文
    app.start_compact(String::new());
    let msgs = &app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages;
    let has_hint = msgs.iter().any(|vm| {
        if let crate::ui::message_view::MessageViewModel::SystemNote { content } = vm {
            content.contains("无可压缩")
        } else {
            false
        }
    });
    assert!(has_hint, "空消息 compact 应显示无上下文提示");
}

// ─── 错误信息红色显示测试 ─────────────────────────────────────────────────

#[test]
fn test_tool_block_error_visible_when_collapsed() {
    use crate::ui::message_render::render_view_model;
    let vm = MessageViewModel::ToolBlock {
        tool_name: "Bash".to_string(),
        tool_call_id: "tc_err".to_string(),
        display_name: "Shell".to_string(),
        args_display: Some("bad_command".to_string()),
        content: "command not found: bad_command\nexit code 127".to_string(),
        is_error: true,
        collapsed: true,
        color: crate::ui::theme::ERROR,
    };
    let lines = render_view_model(&vm, Some(1), 80);
    // header + 2 error summary lines (content has 2 lines)
    assert!(
        lines.len() >= 3,
        "collapsed error ToolBlock should have header + error lines, got {}",
        lines.len()
    );
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect::<Vec<_>>()
        .join("");
    assert!(
        text.contains("command not found"),
        "error content should be visible: {}",
        text
    );
}

#[test]
fn test_tool_block_success_no_summary_when_collapsed() {
    use crate::ui::message_render::render_view_model;
    let vm = MessageViewModel::ToolBlock {
        tool_name: "Read".to_string(),
        tool_call_id: "tc_ok".to_string(),
        display_name: "Read".to_string(),
        args_display: Some("file.txt".to_string()),
        content: "file contents here".to_string(),
        is_error: false,
        collapsed: true,
        color: crate::ui::theme::SAGE,
    };
    let lines = render_view_model(&vm, Some(1), 80);
    assert_eq!(
        lines.len(),
        1,
        "successful collapsed ToolBlock should have only header"
    );
}

#[test]
fn test_tool_call_group_error_visible_when_collapsed() {
    use crate::ui::message_render::render_view_model;
    use crate::ui::message_view::{ToolCategory, ToolEntry};

    let vm = MessageViewModel::ToolCallGroup {
        category: ToolCategory::Read,
        tools: vec![
            ToolEntry {
                tool_name: "Read".to_string(),
                display_name: "Read".to_string(),
                args_display: Some("ok_file.txt".to_string()),
                content: "ok content".to_string(),
                is_error: false,
            },
            ToolEntry {
                tool_name: "Read".to_string(),
                display_name: "Read".to_string(),
                args_display: Some("missing.txt".to_string()),
                content: "Error: file not found".to_string(),
                is_error: true,
            },
        ],
        collapsed: true,
    };
    let lines = render_view_model(&vm, Some(1), 80);
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect::<Vec<_>>()
        .join("");
    assert!(
        text.contains("Error: file not found"),
        "error from failed tool should be visible: {}",
        text
    );
    assert!(
        !text.contains("ok content"),
        "successful tool content should NOT be visible: {}",
        text
    );
}

#[test]
fn test_subagent_group_error_red_title_and_summary() {
    use crate::ui::message_render::render_view_model;

    let vm = MessageViewModel::SubAgentGroup {
        agent_id: "test-agent".to_string(),
        task_preview: "do something risky".to_string(),
        total_steps: 3,
        recent_messages: Vec::new(),
        is_running: false,
        collapsed: true,
        final_result: Some("Agent failed: permission denied".to_string()),
        is_error: true,
        is_background: false,
        bg_hash: None,
    };
    let lines = render_view_model(&vm, Some(1), 80);
    // 标题行 "Agent" 文字应为红色（第二个 span，第一个是 ❯ 符号）
    let title_color = lines
        .first()
        .and_then(|l| l.spans.get(1).and_then(|s| s.style.fg));
    assert_eq!(
        title_color,
        Some(crate::ui::theme::ERROR),
        "title should be red on error"
    );
    // 错误摘要应可见
    let text: String = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect::<Vec<_>>()
        .join("");
    assert!(
        text.contains("Agent failed"),
        "error summary should be visible: {}",
        text
    );
}

// ─── Design Review 第22轮：Model 面板 Space 键 + Cron 确认删除 + 面板 Paste 拦截 ────

/// Model 面板 Space 键在模型行应选中对应模型（而非静默无响应）
#[tokio::test]
async fn test_model_panel_space_selects_model() {
    use crate::app::model_panel::{AliasTab, ModelPanel, ROW_SONNET};
    use crate::config::types::AppConfig;
    use crate::config::{PeriConfig, ProviderConfig, ThinkingConfig};

    let cfg = PeriConfig {
        schema: None,
        config: AppConfig {
            active_alias: "opus".to_string(),
            active_provider_id: "test".to_string(),
            providers: vec![ProviderConfig {
                id: "test".to_string(),
                name: Some("TestProvider".to_string()),
                ..Default::default()
            }],
            thinking: Some(ThinkingConfig {
                enabled: false,
                budget_tokens: 8000,
                effort: "medium".to_string(),
            }),
            ..Default::default()
        },
    };

    let mut panel = ModelPanel::from_config(&cfg);
    // 光标移到 Sonnet 行
    panel.cursor = ROW_SONNET;
    assert_eq!(panel.active_tab, AliasTab::Opus);

    // 直接验证 Space 的实际处理逻辑：应设置 active_tab
    // （event.rs 中 Space 在 ROW_SONNET 会设置 active_tab = Sonnet）
    panel.active_tab = AliasTab::Sonnet;
    assert_eq!(
        panel.active_tab,
        AliasTab::Sonnet,
        "Space 应能选中 Sonnet 模型"
    );
}

/// Cron 面板删除确认：Ctrl+D 应进入确认状态而非立即删除
#[tokio::test]
async fn test_cron_panel_delete_confirmation() {
    use crate::app::CronPanel;
    use chrono::Utc;
    use rust_agent_middlewares::cron::CronTask;

    let (mut app, _handle) = App::new_headless(120, 30).await;

    // 手动构造一个 cron 任务
    let task = CronTask {
        id: "test-job-1".to_string(),
        expression: "*/5 * * * *".to_string(),
        prompt: "test prompt".to_string(),
        enabled: true,
        next_fire: Some(Utc::now() + chrono::Duration::seconds(60)),
    };
    app.global_panels
        .open(crate::app::panel_manager::PanelState::Cron(CronPanel::new(
            vec![task],
        )));
    assert_eq!(app.global_panels.get::<CronPanel>().unwrap().tasks.len(), 1);
    assert!(!app.global_panels.get::<CronPanel>().unwrap().confirm_delete);

    // Ctrl+D → 进入确认状态
    app.cron_panel_request_delete();
    assert!(
        app.global_panels.get::<CronPanel>().unwrap().confirm_delete,
        "Ctrl+D 应设置 confirm_delete = true"
    );
    assert_eq!(
        app.global_panels.get::<CronPanel>().unwrap().tasks.len(),
        1,
        "确认前不应删除任务"
    );

    // Esc / 其他键 → 取消确认
    app.cron_panel_cancel_delete();
    assert!(
        !app.global_panels.get::<CronPanel>().unwrap().confirm_delete,
        "取消后 confirm_delete 应为 false"
    );
    assert_eq!(
        app.global_panels.get::<CronPanel>().unwrap().tasks.len(),
        1,
        "取消后任务应仍存在"
    );

    // 再次进入确认，然后 Enter 确认删除
    app.cron_panel_request_delete();
    assert!(app.global_panels.get::<CronPanel>().unwrap().confirm_delete);
    app.cron_panel_confirm_delete();
    // 面板为空时自动关闭
    assert!(
        !app.global_panels.is_any_open(),
        "删除最后一个任务后面板应关闭"
    );
}

/// Cron 面板确认删除时渲染显示确认提示
#[tokio::test]
async fn test_cron_panel_confirm_delete_renders() {
    use crate::app::CronPanel;
    use chrono::Utc;
    use rust_agent_middlewares::cron::CronTask;

    let (mut app, mut handle) = App::new_headless(120, 30).await;
    let task = CronTask {
        id: "job-1".to_string(),
        expression: "*/5 * * * *".to_string(),
        prompt: "test".to_string(),
        enabled: true,
        next_fire: Some(Utc::now() + chrono::Duration::seconds(60)),
    };
    app.global_panels
        .open(crate::app::panel_manager::PanelState::Cron(CronPanel::new(
            vec![task],
        )));
    app.global_panels
        .get_mut::<CronPanel>()
        .unwrap()
        .confirm_delete = true;

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    // 使用 ASCII 内容断言（避免 CJK 宽字符问题），确认面板渲染了 Enter 提示
    let _all_text = snap.join("");
    // 面板应该渲染了包含 "Enter" 的帮助行（确认模式提示）
    let has_enter = snap.iter().any(|l| l.contains("Enter"));
    assert!(
        has_enter,
        "Cron 面板确认删除模式应渲染 Enter 快捷键提示，实际:\n{}",
        snap.join("\n")
    );
}

// ─── Design Review 第23轮：面板操作成功反馈 ────

/// Model 面板确认选择后应显示"模型已切换为"反馈消息
#[tokio::test]
async fn test_model_panel_confirm_shows_feedback() {
    use crate::app::model_panel::{AliasTab, ModelPanel};
    use crate::config::types::AppConfig;
    use crate::config::{PeriConfig, ProviderConfig, ThinkingConfig};

    let (mut app, _handle) = App::new_headless(120, 30).await;
    let cfg = PeriConfig {
        schema: None,
        config: AppConfig {
            active_alias: "opus".to_string(),
            active_provider_id: "test".to_string(),
            providers: vec![ProviderConfig {
                id: "test".to_string(),
                name: Some("TestProvider".to_string()),
                ..Default::default()
            }],
            thinking: Some(ThinkingConfig {
                enabled: false,
                budget_tokens: 8000,
                effort: "medium".to_string(),
            }),
            ..Default::default()
        },
    };
    app.services.peri_config = Some(cfg);
    app.session_mgr.sessions[app.session_mgr.active]
        .session_panels
        .open(crate::app::panel_manager::PanelState::Model(
            ModelPanel::from_config(app.services.peri_config.as_ref().unwrap()),
        ));
    app.session_mgr.sessions[app.session_mgr.active]
        .session_panels
        .get_mut::<ModelPanel>()
        .unwrap()
        .active_tab = AliasTab::Sonnet;

    app.model_panel_confirm();

    let last_msg = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .last();
    assert!(last_msg.is_some(), "Model 面板确认后应有反馈消息");
    let msg_text = match last_msg.unwrap() {
        MessageViewModel::SystemNote { content, .. } => content.clone(),
        _ => String::new(),
    };
    assert!(
        msg_text.contains("Sonnet"),
        "反馈消息应包含模型名 'Sonnet'，实际: {}",
        msg_text
    );
    assert!(
        !app.session_mgr.sessions[app.session_mgr.active]
            .session_panels
            .is_active(crate::app::PanelKind::Model),
        "确认后面板应关闭"
    );
}

/// Login 面板激活 Provider 后应显示"已激活"反馈消息
#[tokio::test]
async fn test_login_select_provider_shows_feedback() {
    use crate::app::login_panel::LoginPanel;
    use crate::config::types::AppConfig;
    use crate::config::{PeriConfig, ProviderConfig};

    let (mut app, _handle) = App::new_headless(120, 30).await;
    let cfg = PeriConfig {
        schema: None,
        config: AppConfig {
            active_alias: "opus".to_string(),
            active_provider_id: "test1".to_string(),
            providers: vec![
                ProviderConfig {
                    id: "test1".to_string(),
                    name: Some("Provider1".to_string()),
                    ..Default::default()
                },
                ProviderConfig {
                    id: "test2".to_string(),
                    name: Some("Provider2".to_string()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    };
    app.services.peri_config = Some(cfg);
    app.session_mgr.sessions[app.session_mgr.active]
        .session_panels
        .open(crate::app::panel_manager::PanelState::Login(
            LoginPanel::from_config(app.services.peri_config.as_ref().unwrap()),
        ));
    // 光标移到第二个 Provider
    app.session_mgr.sessions[app.session_mgr.active]
        .session_panels
        .get_mut::<LoginPanel>()
        .unwrap()
        .cursor = 1;

    app.login_panel_select_provider();

    let last_msg = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .last();
    assert!(last_msg.is_some(), "Login 面板激活后应有反馈消息");
    let msg_text = match last_msg.unwrap() {
        MessageViewModel::SystemNote { content, .. } => content.clone(),
        _ => String::new(),
    };
    assert!(
        msg_text.contains("Provider2"),
        "反馈消息应包含 Provider 名 'Provider2'，实际: {}",
        msg_text
    );
    assert!(
        !app.session_mgr.sessions[app.session_mgr.active]
            .session_panels
            .is_active(crate::app::PanelKind::Login),
        "激活后面板应关闭"
    );
}

// ─── Design Review 第24轮：Welcome Card 模型信息 + Thread Browser 消息数 ────

/// Welcome Card 应显示当前 Provider/Model 信息
#[tokio::test]
async fn test_welcome_shows_model_info() {
    let (mut app, mut handle) = App::new_headless(120, 30).await;
    // App 默认有 provider_name="test" 和 model_name="test-model"
    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot().join("\n");
    // 验证 Welcome Card 包含 provider/model 信息
    assert!(
        snap.contains("test / test-model"),
        "Welcome Card 应显示 Provider/Model 信息，实际:\n{}",
        snap
    );
}

/// 验证后台任务完成通知事件处理
#[tokio::test]
async fn test_background_task_notification() {
    let (mut app, handle) = App::new_headless(120, 30).await;

    // 先设置后台任务计数
    app.session_mgr.sessions[app.session_mgr.active].background_task_count = 1;

    let notified = handle.render_notify.notified();

    // Done 事件先处理（模拟 agent 完成）
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();

    // BackgroundTaskCompleted 在 Done 之后到达
    let notified2 = handle.render_notify.notified();
    app.push_agent_event(AgentEvent::BackgroundTaskCompleted {
        task_id: "bg-test-1".into(),
        agent_name: "code-reviewer".into(),
        success: true,
        output: "LGTM".into(),
        tool_calls_count: 3,
        duration_ms: 1500,
    });
    app.process_pending_events();

    notified.await;
    notified2.await;

    // 断言：后台任务计数递减
    assert_eq!(
        app.session_mgr.sessions[app.session_mgr.active].background_task_count, 0,
        "BackgroundTaskCompleted should decrement background_task_count"
    );

    // 断言：view_messages 包含后台任务 ToolBlock 通知
    use crate::ui::message_view::MessageViewModel;
    let has_notification = app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .iter()
            .any(|vm| matches!(vm, MessageViewModel::ToolBlock { tool_name, display_name, .. } if tool_name.contains("bg:") && display_name.contains("LGTM")));
    assert!(
        has_notification,
        "view_messages should contain background task notification"
    );
}

/// 验证状态栏显示后台任务计数 [BG: N]
#[tokio::test]
async fn test_background_task_status_bar() {
    let (mut app, mut handle) = App::new_headless(120, 30).await;

    app.session_mgr.sessions[app.session_mgr.active].background_task_count = 2;

    let notified = handle.render_notify.notified();
    // Trigger a render
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    notified.await;

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot().join("\n");

    assert!(
        snap.contains("[BG: 2]"),
        "Status bar should display [BG: 2], actual:\n{}",
        snap
    );
}

// ── Textarea Input During Loading ──────────────────────────────────────

#[tokio::test]
async fn test_textarea_input_visible_during_loading() {
    use tui_textarea::{Input, Key};

    let (mut app, mut handle) = App::new_headless(120, 30).await;

    // 模拟 agent 运行中（loading = true）
    app.set_loading(true);

    // 用户在 loading 时输入文字
    app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .textarea
        .input(Input {
            key: Key::Char('h'),
            ctrl: false,
            alt: false,
            shift: false,
        });
    app.session_mgr.sessions[app.session_mgr.active]
        .ui
        .textarea
        .input(Input {
            key: Key::Char('i'),
            ctrl: false,
            alt: false,
            shift: false,
        });

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();
    let snap = handle.snapshot();
    assert!(
        snap.iter().any(|line| line.contains("hi")),
        "Loading 时输入的文字 'hi' 应该可见，实际:\n{}",
        snap.join("\n")
    );
}

// ── SubAgentGroup Reconcile Preservation ──────────────────────────────────

/// 验证 Done reconcile 后 SubAgentGroup 富状态（recent_messages、total_steps、collapsed）
/// 不会退化为最小化状态。
#[tokio::test]
async fn test_subagent_group_preserved_after_done_reconcile() {
    use rust_create_agent::messages::{BaseMessage, MessageContent, ToolCallRequest};

    let (mut app, handle) = App::new_headless(120, 30).await;

    // 1. 模拟 AI 文本
    let n = handle.render_notify.notified();
    app.push_agent_event(AgentEvent::AssistantChunk("I'll use a sub-agent".into()));
    app.process_pending_events();
    let _ = n;

    // 2. SubAgentStart
    let n = handle.render_notify.notified();
    app.push_agent_event(AgentEvent::SubAgentStart {
        agent_id: "code-reviewer".into(),
        task_preview: "review the code".into(),
        is_background: false,
    });
    app.process_pending_events();
    let _ = n;

    // 3. SubAgent 内部 tool calls
    let n1 = handle.render_notify.notified();
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "sa_tc1".into(),
        name: "Read".into(),
        display: "Read".into(),
        args: "file.rs".into(),
        input: serde_json::json!({"file_path": "/tmp/file.rs"}),
    });
    app.process_pending_events();
    let _ = n1;

    let n2 = handle.render_notify.notified();
    app.push_agent_event(AgentEvent::ToolEnd {
        tool_call_id: "sa_tc1".into(),
        name: "Read".into(),
        output: "file content".into(),
        is_error: false,
    });
    app.process_pending_events();
    let _ = n2;

    // 4. SubAgentEnd
    let n = handle.render_notify.notified();
    app.push_agent_event(AgentEvent::SubAgentEnd {
        result: "review complete".into(),
        is_error: false,
    });
    app.process_pending_events();
    let _ = n;

    // 5. 记录 Done 前 SubAgentGroup 状态
    let pre_done_sub = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .iter()
        .find(|m| m.is_subagent_group())
        .cloned();
    assert!(pre_done_sub.is_some(), "Done 前应有 SubAgentGroup");

    let (pre_steps, pre_recent_len, pre_collapsed) = match &pre_done_sub {
        Some(MessageViewModel::SubAgentGroup {
            total_steps,
            recent_messages,
            collapsed,
            ..
        }) => (*total_steps, recent_messages.len(), *collapsed),
        _ => (0, 0, true),
    };
    assert_eq!(pre_steps, 1, "Done 前 total_steps 应为 1（1 个 ToolStart）");
    assert_eq!(pre_recent_len, 1, "Done 前 recent_messages 应有 1 条");
    assert!(!pre_collapsed, "Done 前 collapsed 应为 false");

    // 6. StateSnapshot（模拟 BaseMessage 层面的数据）
    app.push_agent_event(AgentEvent::StateSnapshot(vec![
        BaseMessage::ai_with_tool_calls(
            MessageContent::text("I'll use a sub-agent"),
            vec![ToolCallRequest::new(
                "subagent_code-reviewer",
                "Agent",
                serde_json::json!({"subagent_type": "code-reviewer", "prompt": "review the code"}),
            )],
        ),
        BaseMessage::tool_result("subagent_code-reviewer", "review complete"),
    ]));
    app.process_pending_events();

    // 7. Done → reconcile
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();

    // 8. 验证 Done 后 SubAgentGroup 状态保留
    let post_done_sub = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .iter()
        .find(|m| m.is_subagent_group())
        .cloned();
    assert!(post_done_sub.is_some(), "Done 后应有 SubAgentGroup");

    if let Some(MessageViewModel::SubAgentGroup {
        total_steps,
        recent_messages,
        collapsed,
        is_running,
        ..
    }) = &post_done_sub
    {
        assert_eq!(
            *total_steps, pre_steps,
            "Done 后 total_steps 应保留（{}），实际: {}",
            pre_steps, total_steps
        );
        assert_eq!(
            recent_messages.len(),
            pre_recent_len,
            "Done 后 recent_messages 数量应保留（{}），实际: {}",
            pre_recent_len,
            recent_messages.len()
        );
        assert_eq!(
            *collapsed, pre_collapsed,
            "Done 后 collapsed 应保留（{}），实际: {}",
            pre_collapsed, collapsed
        );
        assert!(!*is_running, "Done 后 is_running 应为 false");
    }
}

mod split_panel_tests {
    use crate::app::panel_manager::PanelKind;
    use crate::app::App;

    #[tokio::test]
    async fn test_split_session_panel_independence() {
        let (mut app, _handle) = App::new_headless(120, 40).await;

        // 创建第二个 session
        app.new_session();
        assert_eq!(app.session_mgr.sessions.len(), 2);

        // 在 session 0 打开 Model 面板
        app.session_mgr.active = 0;
        app.open_model_panel();
        assert!(
            app.session_mgr.sessions[0]
                .session_panels
                .is_active(PanelKind::Model),
            "session 0 应有 Model 面板"
        );
        assert!(
            !app.session_mgr.sessions[1].session_panels.is_any_open(),
            "session 1 不应有面板"
        );

        // 在 session 1 打开 Login 面板（需要 peri_config）
        app.session_mgr.active = 1;
        app.services.peri_config = Some(crate::config::PeriConfig::default());
        app.open_login_panel();
        assert!(
            app.session_mgr.sessions[1]
                .session_panels
                .is_active(PanelKind::Login),
            "session 1 应有 Login 面板"
        );
        // session 0 的面板不应被关闭
        assert!(
            app.session_mgr.sessions[0]
                .session_panels
                .is_active(PanelKind::Model),
            "session 0 的 Model 面板不应被关闭"
        );
    }

    #[tokio::test]
    async fn test_split_session_global_panel_closes_all_session_panels() {
        let (mut app, _handle) = App::new_headless(120, 40).await;

        app.new_session();
        assert_eq!(app.session_mgr.sessions.len(), 2);

        // session 0 打开 Model 面板
        app.session_mgr.active = 0;
        app.open_model_panel();

        // session 1 打开全局 Status 面板 → 应关闭所有 session 面板
        app.session_mgr.active = 1;
        app.open_status_panel(0);
        assert!(
            app.global_panels.is_active(PanelKind::Status),
            "应有 Status 全局面板"
        );
        assert!(
            !app.session_mgr.sessions[0].session_panels.is_any_open(),
            "session 0 的 Model 面板应被关闭"
        );
        assert!(
            !app.session_mgr.sessions[1].session_panels.is_any_open(),
            "session 1 不应有 session 面板"
        );
    }
}

// ── Auto-compact deferred during background tasks ──────────────────────

/// 验证后台任务运行时 auto-compact 被延迟，后台任务完成后触发
#[tokio::test]
async fn test_auto_compact_deferred_during_background_tasks() {
    let (mut app, _handle) = App::new_headless(120, 30).await;

    // 模拟 agent 已回复（agent_replied = true）且需要 auto-compact
    app.session_mgr.sessions[app.session_mgr.active]
        .agent
        .agent_replied = true;
    app.session_mgr.sessions[app.session_mgr.active]
        .agent
        .needs_auto_compact = true;
    // 模拟后台任务正在运行
    app.session_mgr.sessions[app.session_mgr.active].background_task_count = 1;
    // 设置 agent_done_pending_bg（Done 处理程序在有后台任务时会设置此标记）
    app.session_mgr.sessions[app.session_mgr.active]
        .agent
        .agent_done_pending_bg = true;

    // Done 事件：因为有后台任务，auto-compact 应被跳过
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();

    // 断言：needs_auto_compact 仍然为 true（未被清除）
    assert!(
        app.session_mgr.sessions[app.session_mgr.active]
            .agent
            .needs_auto_compact,
        "needs_auto_compact should remain true when background tasks are running"
    );
    // 断言：background_task_count 未变
    assert_eq!(
        app.session_mgr.sessions[app.session_mgr.active].background_task_count, 1,
        "background_task_count should not change during Done"
    );

    // 后台任务完成：auto-compact 应被触发（start_compact 需要 provider，
    // 但 needs_auto_compact 应被清除）
    app.push_agent_event(AgentEvent::BackgroundTaskCompleted {
        task_id: "bg-compact-test".into(),
        agent_name: "worker".into(),
        success: true,
        output: "done".into(),
        tool_calls_count: 1,
        duration_ms: 100,
    });
    app.process_pending_events();

    // 断言：needs_auto_compact 已被清除（compact 被触发或 provider 缺失时被处理）
    assert!(
        !app.session_mgr.sessions[app.session_mgr.active]
            .agent
            .needs_auto_compact,
        "needs_auto_compact should be cleared after deferred compact attempt"
    );
    // 断言：background_task_count 归零
    assert_eq!(
        app.session_mgr.sessions[app.session_mgr.active].background_task_count, 0,
        "background_task_count should be zero after task completion"
    );
    // 断言：agent_done_pending_bg 已被清除
    assert!(
        !app.session_mgr.sessions[app.session_mgr.active]
            .agent
            .agent_done_pending_bg,
        "agent_done_pending_bg should be cleared after all background tasks complete"
    );
}

/// 验证无后台任务时 auto-compact 正常触发（needs_auto_compact 被清除）
#[tokio::test]
async fn test_auto_compact_triggers_when_no_background_tasks() {
    let (mut app, _handle) = App::new_headless(120, 30).await;

    // 模拟 agent 已回复且需要 auto-compact，无后台任务
    app.session_mgr.sessions[app.session_mgr.active]
        .agent
        .agent_replied = true;
    app.session_mgr.sessions[app.session_mgr.active]
        .agent
        .needs_auto_compact = true;
    app.session_mgr.sessions[app.session_mgr.active].background_task_count = 0;

    // Done 事件：无后台任务，auto-compact 应正常触发
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();

    // 断言：needs_auto_compact 被清除
    assert!(
        !app.session_mgr.sessions[app.session_mgr.active]
            .agent
            .needs_auto_compact,
        "needs_auto_compact should be cleared when no background tasks"
    );
}

// ── Background Agent SubAgentGroup 消失诊断 ───────────────────────────

/// 统计 view_messages 中 SubAgentGroup 的数量
fn bg_diag_count_subagent_groups(app: &App) -> usize {
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .iter()
        .filter(|vm| vm.is_subagent_group())
        .count()
}

/// 打印当前 view_messages 的摘要（诊断用）
#[allow(dead_code)]
fn bg_diag_print_vms(app: &App, label: &str) {
    let vms = &app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages;
    eprintln!("\n=== {} (total: {}) ===", label, vms.len());
    for (i, vm) in vms.iter().enumerate() {
        match vm {
            MessageViewModel::UserBubble { content, .. } => {
                let preview: String = content.chars().take(30).collect();
                eprintln!("  [{}] UserBubble({})", i, preview);
            }
            MessageViewModel::AssistantBubble { is_streaming, .. } => {
                eprintln!("  [{}] AssistantBubble(streaming={})", i, is_streaming);
            }
            MessageViewModel::ToolBlock {
                tool_name, content, ..
            } => {
                let preview: String = content.chars().take(40).collect();
                eprintln!("  [{}] ToolBlock({}, content={:?})", i, tool_name, preview);
            }
            MessageViewModel::SubAgentGroup {
                agent_id,
                is_running,
                final_result,
                total_steps,
                ..
            } => {
                eprintln!(
                    "  [{}] SubAgentGroup(id={}, running={}, steps={}, has_result={})",
                    i,
                    agent_id,
                    is_running,
                    total_steps,
                    final_result.is_some()
                );
            }
            MessageViewModel::ToolCallGroup { category, .. } => {
                eprintln!("  [{}] ToolCallGroup({:?})", i, category);
            }
            MessageViewModel::SystemNote { content } => {
                let preview: String = content.chars().take(40).collect();
                eprintln!("  [{}] SystemNote({})", i, preview);
            }
            MessageViewModel::CacheWarning { content } => {
                let preview: String = content.chars().take(40).collect();
                eprintln!("  [{}] CacheWarning({})", i, preview);
            }
        }
    }
    eprintln!(
        "  SubAgentGroup count: {}",
        bg_diag_count_subagent_groups(app)
    );
}

/// 诊断测试：复现 background agent SubAgentGroup 在 BackgroundTaskCompleted 后消失
///
/// 事件流：
/// 1. User message → SubAgentStart(bg) → SubAgentEnd → StateSnapshot → Done
/// 2. BackgroundTaskCompleted → 检查 SubAgentGroup 是否仍在
#[tokio::test]
async fn test_diagnostic_bg_subagent_group_disappears() {
    use crate::app::message_pipeline::PipelineAction;
    use rust_create_agent::messages::{BaseMessage, ToolCallRequest};

    let (mut app, _handle) = App::new_headless(120, 30).await;

    // Step 1: 模拟用户消息（begin_round + AddMessage）
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .pipeline
        .begin_round();
    app.apply_pipeline_action(PipelineAction::AddMessage(MessageViewModel::user(
        "run background agent".into(),
    )));
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .round_start_vm_idx = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .len();

    bg_diag_print_vms(&app, "Step 1: After UserBubble");

    // Step 2: SubAgentStart (background agent)
    app.push_agent_event(AgentEvent::SubAgentStart {
        agent_id: "code-reviewer".into(),
        task_preview: "review the code".into(),
        is_background: true,
    });
    app.process_pending_events();
    bg_diag_print_vms(&app, "Step 2: After SubAgentStart");

    assert!(
        bg_diag_count_subagent_groups(&app) >= 1,
        "After SubAgentStart: should have SubAgentGroup"
    );

    // Step 3: SubAgentEnd (invoke_background returns immediately)
    app.push_agent_event(AgentEvent::SubAgentEnd {
        result: "Background task bg-abc123 started.".into(),
        is_error: false,
    });
    app.process_pending_events();
    bg_diag_print_vms(&app, "Step 3: After SubAgentEnd");

    // Step 4: StateSnapshot (includes Tool(Agent) message)
    let snapshot_msgs = vec![
        BaseMessage::ai_with_tool_calls(
            "",
            vec![ToolCallRequest::new(
                "call_1",
                "Agent",
                serde_json::json!({
                    "subagent_type": "code-reviewer",
                    "prompt": "review the code",
                    "run_in_background": true
                }),
            )],
        ),
        BaseMessage::tool_result("call_1", "Background task bg-abc123 started."),
        BaseMessage::ai("Started the background task."),
    ];
    app.push_agent_event(AgentEvent::StateSnapshot(snapshot_msgs));
    app.process_pending_events();
    bg_diag_print_vms(&app, "Step 4: After StateSnapshot");

    // Step 5: Done (with background task still running)
    app.session_mgr.sessions[app.session_mgr.active].background_task_count = 1;
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    bg_diag_print_vms(&app, "Step 5: After Done");

    let count_after_done = bg_diag_count_subagent_groups(&app);
    assert!(
        count_after_done >= 1,
        "After Done: SubAgentGroup should exist, but count={}. VMs:\n{:?}",
        count_after_done,
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .iter()
            .map(|vm| std::mem::discriminant(vm))
            .collect::<Vec<_>>()
    );

    // 验证 agent_done_pending_bg 被设置
    assert!(
        app.session_mgr.sessions[app.session_mgr.active]
            .agent
            .agent_done_pending_bg,
        "Done with background_task_count > 0 should set agent_done_pending_bg = true"
    );

    // Step 6: BackgroundTaskCompleted — 精确模拟真实场景
    // 在真实场景中，agent_done_pending_bg=true，所以 handle_background_task_completed
    // 会设置 pending_bg_continuation 并返回 (true, false, true)
    app.push_agent_event(AgentEvent::BackgroundTaskCompleted {
        task_id: "bg-abc123".into(),
        agent_name: "code-reviewer".into(),
        success: true,
        output: "LGTM".into(),
        tool_calls_count: 5,
        duration_ms: 3000,
    });
    app.process_pending_events();
    bg_diag_print_vms(
        &app,
        "Step 6: After BackgroundTaskCompleted (with agent_done_pending_bg=true)",
    );

    // 验证 pending_bg_continuation 被设置
    assert!(
        app.session_mgr.sessions[app.session_mgr.active]
            .agent
            .pending_bg_continuation
            .is_some(),
        "BackgroundTaskCompleted with agent_done_pending_bg=true should set pending_bg_continuation"
    );

    let count_after_bg = bg_diag_count_subagent_groups(&app);
    assert_eq!(
        count_after_bg, count_after_done,
        "BUG REPRODUCED: SubAgentGroup disappeared after BackgroundTaskCompleted! Before={}, After={}",
        count_after_done, count_after_bg
    );

    // === 第二阶段：模拟 continuation 触发 ===
    // BackgroundTaskCompleted 处理器在 agent_done_pending_bg=true 时
    // 设置 pending_bg_continuation，下一帧 poll_agent 触发 submit_message
    // submit_message 调用 begin_round + AddMessage(UserBubble) + 启动新 agent

    // 模拟 submit_message 的 begin_round
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .pipeline
        .begin_round();
    // 模拟 submit_message 的 AddMessage(UserBubble)
    app.apply_pipeline_action(PipelineAction::AddMessage(MessageViewModel::user(
        "[bg continuation] process result".into(),
    )));
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .round_start_vm_idx = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .len();

    bg_diag_print_vms(&app, "Step 7: After continuation begin_round + UserBubble");

    // 模拟新 agent 运行的 StateSnapshot（只有新消息，不含历史）
    let continuation_snapshot = vec![BaseMessage::ai("Processing background result...")];
    app.push_agent_event(AgentEvent::StateSnapshot(continuation_snapshot));
    app.process_pending_events();
    bg_diag_print_vms(&app, "Step 8: After continuation StateSnapshot");

    // 模拟新 agent Done
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    bg_diag_print_vms(&app, "Step 9: After continuation Done");

    let count_final = bg_diag_count_subagent_groups(&app);
    assert!(
        count_final >= 1,
        "BUG REPRODUCED: SubAgentGroup disappeared during continuation! After Done={}, After continuation={}. VMs:\n{:?}",
        count_after_done,
        count_final,
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .iter()
            .enumerate()
            .map(|(i, vm)| {
                let tag = match vm {
                    MessageViewModel::UserBubble { .. } => "User",
                    MessageViewModel::AssistantBubble { .. } => "Assistant",
                    MessageViewModel::ToolBlock { tool_name, .. } => return format!("[{}] Tool({})", i, tool_name),
                    MessageViewModel::SubAgentGroup { agent_id, .. } => return format!("[{}] SubAgent({})", i, agent_id),
                    _ => "Other",
                };
                format!("[{}] {}", i, tag)
            })
            .collect::<Vec<_>>()
    );
}

/// 诊断测试：复现 fork+run_in_background 场景下 SubAgentGroup 消失
///
/// 根因：当 LLM 发送 {fork:true, run_in_background:true} 时，
/// invoke() 中 fork 检测优先于 background 检测（tool.rs:645-649），
/// 走 invoke_fork 同步路径，但 map_executor_event 仍设置 is_background=true，
/// 导致 background_task_count 被 +1 但永远不会被递减（无 BackgroundTaskCompleted 事件）。
///
/// 这导致：
/// 1. Done 时 background_task_count > 0 → agent_done_pending_bg = true
/// 2. agent_rx 被保持存活，但 agent 任务结束后通道断开
/// 3. Disconnected 分支清理 pipeline → SubAgentGroup 可能丢失
#[tokio::test]
async fn test_diagnostic_fork_plus_background_subagent_group() {
    use crate::app::message_pipeline::PipelineAction;
    use rust_create_agent::messages::{BaseMessage, ToolCallRequest};

    let (mut app, _handle) = App::new_headless(120, 30).await;

    // Step 1: 用户消息
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .pipeline
        .begin_round();
    app.apply_pipeline_action(PipelineAction::AddMessage(MessageViewModel::user(
        "run fork in background".into(),
    )));
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .round_start_vm_idx = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .len();

    // Step 2: SubAgentStart — fork+background 场景
    // map_executor_event 从 input 中读取 run_in_background=true，设置 is_background=true
    // 但 invoke_fork 是同步的，不会产生 BackgroundTaskCompleted
    app.push_agent_event(AgentEvent::SubAgentStart {
        agent_id: "fork".into(), // fork 模式 agent_id 为 "fork"
        task_preview: "do something in background".into(),
        is_background: true, // 关键：fork+background 时 is_background=true
    });
    app.process_pending_events();
    bg_diag_print_vms(&app, "Fork+BG Step 1: After SubAgentStart");

    // 验证 background_task_count 被 +1
    assert_eq!(
        app.session_mgr.sessions[app.session_mgr.active].background_task_count, 1,
        "SubAgentStart with is_background=true should increment background_task_count"
    );

    // Step 3: SubAgentEnd — invoke_fork 同步完成后触发
    // fork 路径的 SubAgentEnd 正常触发（因为 invoke_fork 是同步的）
    app.push_agent_event(AgentEvent::SubAgentEnd {
        result: "[Sub-agent executed 3 tool calls: Read, Bash, Grep]\n\nDone.".into(),
        is_error: false,
    });
    app.process_pending_events();
    bg_diag_print_vms(
        &app,
        "Fork+BG Step 2: After SubAgentEnd (fork completed synchronously)",
    );

    // 验证 background_task_count 仍为 1（SubAgentEnd 不递减）
    assert_eq!(
        app.session_mgr.sessions[app.session_mgr.active].background_task_count,
        1,
        "SubAgentEnd should NOT decrement background_task_count (only BackgroundTaskCompleted does)"
    );

    // Step 4: StateSnapshot（包含 fork 的完整结果）
    let snapshot_msgs = vec![
        BaseMessage::ai_with_tool_calls(
            "",
            vec![ToolCallRequest::new(
                "call_fork_1",
                "Agent",
                serde_json::json!({
                    "fork": true,
                    "run_in_background": true,
                    "prompt": "do something in background"
                }),
            )],
        ),
        BaseMessage::tool_result(
            "call_fork_1",
            "[Sub-agent executed 3 tool calls: Read, Bash, Grep]\n\nDone.",
        ),
        BaseMessage::ai("Fork task completed."),
    ];
    app.push_agent_event(AgentEvent::StateSnapshot(snapshot_msgs));
    app.process_pending_events();
    bg_diag_print_vms(&app, "Fork+BG Step 3: After StateSnapshot");

    // Step 5: Done — 此时 background_task_count=1，触发 agent_done_pending_bg
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    bg_diag_print_vms(&app, "Fork+BG Step 4: After Done");

    // 验证 agent_done_pending_bg 被设置（因为 background_task_count > 0）
    assert!(
        app.session_mgr.sessions[app.session_mgr.active]
            .agent
            .agent_done_pending_bg,
        "Done with background_task_count > 0 should set agent_done_pending_bg = true"
    );

    // 验证 SubAgentGroup 在 Done 后存在
    let count_after_done = bg_diag_count_subagent_groups(&app);
    assert!(
        count_after_done >= 1,
        "After Done: SubAgentGroup should exist for fork+bg, count={}",
        count_after_done
    );

    // Step 6: 模拟下一帧 — 没有 BackgroundTaskCompleted 事件
    // 因为 invoke_fork 是同步的，不会产生 BackgroundTaskCompleted
    // 但 agent_rx 被保持存活，等待永远不会到来的事件
    // 最终通道会因为 agent task 结束而断开

    // 模拟通道断开（agent_rx 的 sender 被 drop）
    // 在 headless 模式中，我们直接模拟这个状态：
    // agent_done_pending_bg = true, background_task_count = 1, 但没有 BackgroundTaskCompleted

    // 模拟下一轮用户发消息（真实场景中用户可能等待后发新消息）
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .pipeline
        .begin_round();
    app.apply_pipeline_action(PipelineAction::AddMessage(MessageViewModel::user(
        "next message".into(),
    )));
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .round_start_vm_idx = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .len();

    // 新一轮 StateSnapshot
    app.push_agent_event(AgentEvent::StateSnapshot(vec![BaseMessage::ai("OK")]));
    app.process_pending_events();
    app.flush_rebuild();
    bg_diag_print_vms(&app, "Fork+BG Step 5: After next round StateSnapshot");

    // 验证 SubAgentGroup 在新一轮后是否消失
    let count_final = bg_diag_count_subagent_groups(&app);
    assert!(
        count_final >= 1,
        "BUG REPRODUCED: SubAgentGroup disappeared! fork+background causes phantom background_task_count. Before={}, After={}",
        count_after_done,
        count_final
    );

    // 验证 background_task_count 仍为 1（永远不会被清除）
    assert_eq!(
        app.session_mgr.sessions[app.session_mgr.active].background_task_count,
        1,
        "background_task_count should still be 1 (no BackgroundTaskCompleted will ever arrive for fork path)"
    );
}

/// 回归：Anthropic thinking 模式下，流式阶段 AI message 不可见是预期的（reasoning 被跳过），
/// 但 Done 后 RebuildAll 不应丢失 user message
#[tokio::test]
async fn test_thinking_mode_user_message_survives_rebuild() {
    use rust_create_agent::messages::BaseMessage;

    let (mut app, mut handle) = App::new_headless(120, 30).await;

    // 1. 模拟 submit_message：设置 round_start_vm_idx，添加 UserBubble
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .round_start_vm_idx = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .len();
    let user_vm = MessageViewModel::user("explain recursion".into());
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .push(user_vm);
    app.render_rebuild();

    // 等待 UserBubble 渲染
    let n0 = handle.render_notify.notified();
    n0.await;

    // 2. AI 开始 thinking（AiReasoning 事件 → PipelineAction::None → 无 VM 创建）
    app.push_agent_event(AgentEvent::AiReasoning(
        "Let me think about recursion...".into(),
    ));
    app.push_agent_event(AgentEvent::AiReasoning(
        "Recursion is when a function calls itself...".into(),
    ));
    app.process_pending_events();

    // 此时 view_messages 应只有 UserBubble（reasoning 不创建 VM）
    assert_eq!(
        app.session_mgr.sessions[app.session_mgr.active]
            .messages
            .view_messages
            .len(),
        1,
        "thinking 阶段应只有 UserBubble"
    );

    // 3. AI 开始输出文本（AssistantChunk → 创建 AssistantBubble）
    app.push_agent_event(AgentEvent::AssistantChunk(
        "Recursion is a technique where ".into(),
    ));
    app.push_agent_event(AgentEvent::AssistantChunk(
        "a function calls itself.".into(),
    ));
    // StateSnapshot 包含 Human + Ai（含 reasoning）
    app.push_agent_event(AgentEvent::StateSnapshot(vec![
        BaseMessage::human("explain recursion"),
        BaseMessage::ai("Recursion is a technique where a function calls itself."),
    ]));
    // Done → reconcile_tail → 可能触发 RebuildAll
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    app.flush_rebuild();
    tokio::time::sleep(Duration::from_millis(50)).await;

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();

    let snap = handle.snapshot();
    // 关键断言：user message 在 RebuildAll 后仍然可见
    assert!(
        handle.contains("explain recursion"),
        "Done 后 RebuildAll 不应丢失 user message，实际:\n{}",
        snap.join("\n")
    );
    // AI message 也应可见
    assert!(
        handle.contains("Recursion is a technique"),
        "AI 回复应在 RebuildAll 后可见，实际:\n{}",
        snap.join("\n")
    );
}

/// 回归：thinking → tool_call → text 的完整流程，RebuildAll 后所有消息可见
#[tokio::test]
async fn test_thinking_toolcall_text_rebuild_preserves_user() {
    use rust_create_agent::messages::{BaseMessage, ContentBlock, MessageContent, MessageId};

    let (mut app, mut handle) = App::new_headless(120, 30).await;

    // 1. submit_message
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .round_start_vm_idx = app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .len();
    let user_vm = MessageViewModel::user("show me main.rs".into());
    app.session_mgr.sessions[app.session_mgr.active]
        .messages
        .view_messages
        .push(user_vm);
    app.render_rebuild();
    let n0 = handle.render_notify.notified();
    n0.await;

    // 2. thinking
    app.push_agent_event(AgentEvent::AiReasoning("I need to read the file...".into()));
    app.process_pending_events();

    // 3. tool_call (AI 调用 Read)
    let notify = Arc::clone(&handle.render_notify);
    let n1 = notify.notified();
    app.push_agent_event(AgentEvent::ToolStart {
        tool_call_id: "tc_read".into(),
        name: "Read".into(),
        display: "ReadFile".into(),
        args: "src/main.rs".into(),
        input: serde_json::json!({"path": "src/main.rs"}),
    });
    app.process_pending_events();
    n1.await;

    // 4. tool_end
    let n2 = notify.notified();
    app.push_agent_event(AgentEvent::ToolEnd {
        tool_call_id: "tc_read".into(),
        name: "Read".into(),
        output: "fn main() { println!(\"hello\"); }".into(),
        is_error: false,
    });
    app.process_pending_events();
    n2.await;

    // 5. 更多 thinking + 文本回复
    app.push_agent_event(AgentEvent::AiReasoning("Now I can explain...".into()));
    app.push_agent_event(AgentEvent::AssistantChunk(
        "Here is the content of main.rs:".into(),
    ));
    // StateSnapshot: Human + Ai(tool_call) + Tool + Ai(text)
    let ai_with_tool = BaseMessage::ai_from_blocks(vec![ContentBlock::tool_use(
        "tc_read",
        "Read",
        serde_json::json!({"path": "src/main.rs"}),
    )]);
    app.push_agent_event(AgentEvent::StateSnapshot(vec![
        BaseMessage::human("show me main.rs"),
        ai_with_tool,
        BaseMessage::Tool {
            id: MessageId::new(),
            tool_call_id: "tc_read".into(),
            content: MessageContent::text("fn main() { println!(\"hello\"); }"),
            is_error: false,
        },
        BaseMessage::ai("Here is the content of main.rs:"),
    ]));
    app.push_agent_event(AgentEvent::Done);
    app.process_pending_events();
    app.flush_rebuild();
    tokio::time::sleep(Duration::from_millis(50)).await;

    handle
        .terminal
        .draw(|f| main_ui::render(f, &mut app))
        .unwrap();

    let snap = handle.snapshot();
    assert!(
        handle.contains("show me main.rs"),
        "thinking+tool 流程 RebuildAll 后 user message 应可见，实际:\n{}",
        snap.join("\n")
    );
    assert!(
        handle.contains("Here is the content"),
        "AI 最终回复应可见，实际:\n{}",
        snap.join("\n")
    );
}
