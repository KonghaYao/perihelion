use super::*;

/// 等待 RenderThread 处理完事件：yield 让出执行权给后台 task
async fn wait_render() {
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
}

#[tokio::test]
async fn test_rebuild_increments_version() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    assert_eq!(cache.read().version, 0);

    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "Hello".to_string(),
    )]))
    .unwrap();

    wait_render().await;

    let c = cache.read();
    assert!(c.version > 0, "version should increment after Rebuild");
    assert!(
        !c.lines.is_empty(),
        "lines should not be empty after Rebuild"
    );
}

#[tokio::test]
async fn test_rebuild_hash_diff_skips_unchanged() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    // 第一次 Rebuild：渲染两条消息
    let user1 = MessageViewModel::user("First".to_string());
    let user2 = MessageViewModel::user("Second".to_string());
    tx.send(RenderEvent::Rebuild(vec![user1.clone(), user2.clone()]))
        .unwrap();
    wait_render().await;

    let v1 = cache.read().version;
    let lines_v1 = cache.read().lines.len();

    // 第二次 Rebuild：相同内容，hash diff 应跳过渲染
    tx.send(RenderEvent::Rebuild(vec![user1, user2])).unwrap();
    wait_render().await;

    let c = cache.read();
    // version 仍应递增（即使内容不变）
    assert!(c.version > v1, "version should still increment");
    // 行数不变
    assert_eq!(c.lines.len(), lines_v1, "lines count should be the same");
}

#[tokio::test]
async fn test_rebuild_no_trailing_blank() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "Hello".to_string(),
    )]))
    .unwrap();
    wait_render().await;

    let c = cache.read();
    let last_is_empty = c.lines.last().is_some_and(|l| {
        l.spans.is_empty() || (l.spans.len() == 1 && l.spans[0].content.is_empty())
    });
    assert!(!last_is_empty, "should not have trailing blank line");
}

#[tokio::test]
async fn test_rebuild_multiple_messages_have_gaps() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    tx.send(RenderEvent::Rebuild(vec![
        MessageViewModel::user("First message".to_string()),
        MessageViewModel::user("Second message".to_string()),
    ]))
    .unwrap();
    wait_render().await;

    let c = cache.read();
    // 找 "Second message" 的行，检查前一行是否为空行
    let mut second_msg_idx = None;
    for (i, line) in c.lines.iter().enumerate() {
        for span in &line.spans {
            if span.content.contains("Second message") {
                second_msg_idx = Some(i);
                break;
            }
        }
        if second_msg_idx.is_some() {
            break;
        }
    }
    let idx = second_msg_idx.expect("should find second user message");
    assert!(idx > 0, "second message should not be the first line");
    let prev_is_empty = c.lines[idx - 1].spans.is_empty()
        || (c.lines[idx - 1].spans.len() == 1 && c.lines[idx - 1].spans[0].content.is_empty());
    assert!(
        prev_is_empty,
        "should have blank line before second user message, but line {} is: {:?}",
        idx - 1,
        c.lines[idx - 1]
    );
}

#[tokio::test]
async fn test_rebuild_with_anchor_sets_scroll_anchor() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    tx.send(RenderEvent::RebuildWithAnchor {
        messages: vec![
            MessageViewModel::user("First".to_string()),
            MessageViewModel::user("Second".to_string()),
        ],
        anchor_message_idx: 1,
    })
    .unwrap();
    wait_render().await;

    let c = cache.read();
    assert!(c.scroll_anchor.is_some(), "scroll_anchor should be set");
}

#[tokio::test]
async fn test_clear_resets_cache() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    tx.send(RenderEvent::Rebuild(vec![MessageViewModel::user(
        "Hello".to_string(),
    )]))
    .unwrap();
    wait_render().await;

    tx.send(RenderEvent::Clear).unwrap();
    wait_render().await;

    let c = cache.read();
    assert!(c.lines.is_empty(), "lines should be empty after Clear");
    assert_eq!(c.total_lines, 0);
}

#[tokio::test]
async fn test_resize_rebuilds_with_new_width() {
    let (tx, cache, _notify) = spawn_render_thread(80);

    let user = MessageViewModel::user("Hello world".to_string());
    tx.send(RenderEvent::Rebuild(vec![user.clone()])).unwrap();
    wait_render().await;

    let v1 = cache.read().version;
    let total_v1 = cache.read().total_lines;

    // Resize
    tx.send(RenderEvent::Resize(40)).unwrap();
    wait_render().await;

    let c = cache.read();
    assert!(c.version > v1, "version should increment after Resize");
    // 窄宽度可能导致更多 wrap 行
    assert!(c.total_lines >= total_v1);
}

#[test]
fn test_build_wrap_map_empty() {
    let result = RenderTask::build_wrap_map(&[], 80);
    assert!(result.is_empty());
}

#[test]
fn test_build_wrap_map_single_short_line() {
    let lines = vec![Line::from("Hello")];
    let result = RenderTask::build_wrap_map(&lines, 80);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].visual_row_start, 0);
    assert_eq!(result[0].visual_row_end, 1);
    assert_eq!(result[0].plain_text, "Hello");
}

#[test]
fn test_build_wrap_map_single_long_line_wraps() {
    let long_text: String = "A".repeat(200);
    let lines: Vec<Line<'static>> = vec![Line::from(long_text)];
    let result = RenderTask::build_wrap_map(&lines, 40);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].visual_row_start, 0);
    assert_eq!(result[0].visual_row_end, 5);
}

#[test]
fn test_build_wrap_map_cjk_char_width() {
    let lines = vec![Line::from("你好世界")];
    let result = RenderTask::build_wrap_map(&lines, 80);
    assert_eq!(result[0].char_widths, vec![2, 2, 2, 2]);
    assert_eq!(result[0].visual_row_end - result[0].visual_row_start, 1);
}

#[test]
fn test_build_wrap_map_multi_line_visual_rows() {
    let first_line: String = "A".repeat(80);
    let second_line = Line::from("short");
    let lines: Vec<Line<'static>> = vec![Line::from(first_line), second_line];
    let result = RenderTask::build_wrap_map(&lines, 40);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].visual_row_start, 0);
    assert_eq!(result[0].visual_row_end, 2);
    assert_eq!(result[1].visual_row_start, 2);
    assert_eq!(result[1].visual_row_end, 3);
}

#[test]
fn test_build_wrap_map_empty_line() {
    let lines = vec![Line::from("")];
    let result = RenderTask::build_wrap_map(&lines, 80);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].visual_row_end - result[0].visual_row_start, 1);
}
