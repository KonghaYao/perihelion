use std::sync::Arc;

use parking_lot::RwLock;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use tokio::sync::{mpsc, Notify};

use super::markdown::ensure_rendered;
use super::message_render::render_view_model;
use super::message_view::MessageViewModel;

/// 单个逻辑行的换行映射信息
#[derive(Debug, Clone)]
pub struct WrappedLineInfo {
    /// 该行在 cache.lines 中的索引
    pub line_idx: usize,
    /// 该逻辑行渲染后的起始视觉行号（基于 0）
    pub visual_row_start: u16,
    /// 该逻辑行渲染后的结束视觉行号（不含）
    pub visual_row_end: u16,
    /// 该逻辑行的纯文本内容（去样式，用于复制）
    pub plain_text: String,
    /// 每个字符的显示宽度序列（ASCII=1, CJK=2）
    pub char_widths: Vec<u8>,
}

/// 渲染缓存，由渲染线程写入、UI 线程读取
pub struct RenderCache {
    /// 所有消息渲染后的行
    pub lines: Vec<Line<'static>>,
    /// 每条消息在 lines 中的起始行索引（用于定位）
    pub message_offsets: Vec<usize>,
    /// 总行数（已考虑 wrap 换行后的真实视觉行数）
    pub total_lines: usize,
    /// 版本号，UI 线程比较是否有变化以决定是否重绘
    pub version: u64,
    pub wrap_map: Vec<WrappedLineInfo>,
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            message_offsets: Vec::new(),
            total_lines: 0,
            version: 0,
            wrap_map: Vec::new(),
        }
    }

    /// 计算给定 lines 在指定宽度下 wrap 后的真实视觉行数。
    /// 使用 ratatui 的 Paragraph::line_count 与 Wrap{trim:false} 确保与实际渲染一致。
    fn compute_wrapped_height(lines: &[Line<'static>], width: u16) -> usize {
        if width == 0 || lines.is_empty() {
            return 0;
        }
        let text = ratatui::text::Text::from(lines.to_vec());
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .line_count(width)
    }
}

/// 渲染线程接收的事件
pub enum RenderEvent {
    /// 新增一条完整消息（用户消息/工具结果等）
    AddMessage(MessageViewModel),
    /// 追加流式 chunk 到最后一条 assistant 消息
    AppendChunk(String),
    /// 流式输出结束，清除最后一条 assistant 消息的 is_streaming 标志
    StreamingDone,
    /// 终端宽度变化，需要全量重新计算行包装
    Resize(u16),
    /// 清空所有消息
    Clear,
    /// 加载历史消息（批量）
    LoadHistory(Vec<MessageViewModel>),
    /// 切换工具调用消息的显示状态
    ToggleToolMessages(bool),
    /// 替换最后一条消息并重新渲染（SubAgentGroup 更新专用）
    UpdateLastMessage(MessageViewModel),
    /// 移除最后一条消息（用于隐藏空的 AssistantBubble）
    RemoveLastMessage,
}

/// 渲染线程，持有消息数据的私有副本，在后台执行渲染计算
struct RenderTask {
    messages: Vec<MessageViewModel>,
    cache: Arc<RwLock<RenderCache>>,
    notify: Arc<Notify>,
    width: u16,
    show_tool_messages: bool,
}

impl RenderTask {
    /// 根据 cache.lines 和当前宽度计算 wrap_map
    fn build_wrap_map(lines: &[Line<'static>], width: u16) -> Vec<WrappedLineInfo> {
        let usable_width = width.saturating_sub(1) as usize; // 右侧留 1 列给滚动条
        if usable_width == 0 || lines.is_empty() {
            return Vec::new();
        }
        let mut wrap_map = Vec::with_capacity(lines.len());
        let mut visual_row: u16 = 0;

        for (idx, line) in lines.iter().enumerate() {
            // 1. 提取纯文本
            let plain_text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            // 2. 计算每个字符的显示宽度
            let char_widths: Vec<u8> = plain_text
                .chars()
                .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0) as u8)
                .collect();
            // 3. 模拟 word-wrap 计算该行占几个视觉行
            let line_display_width: usize = char_widths.iter().map(|&w| w as usize).sum();
            let visual_count = if line_display_width == 0 {
                1 // 空行占 1 个视觉行
            } else {
                (line_display_width + usable_width - 1) / usable_width // 向上取整
            };
            let visual_count = visual_count.max(1) as u16;

            wrap_map.push(WrappedLineInfo {
                line_idx: idx,
                visual_row_start: visual_row,
                visual_row_end: visual_row + visual_count,
                plain_text,
                char_widths,
            });
            visual_row += visual_count;
        }
        wrap_map
    }

    /// 渲染单条消息为 lines（含前后空行分隔）
    fn render_one(vm: &mut MessageViewModel, index: usize, width: usize) -> Vec<Line<'static>> {
        // 处理 dirty blocks
        if let MessageViewModel::AssistantBubble { blocks, .. } = vm {
            for block in blocks.iter_mut() {
                ensure_rendered(block, width);
            }
        }

        let mut lines = render_view_model(vm, Some(index), width);
        // 空内容消息不添加分隔行（如只有思考内容被隐藏的 AssistantBubble）
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines
    }

    /// 全量重新渲染所有消息，写入缓存
    fn rebuild_all(&mut self) {
        let width = self.width.saturating_sub(1) as usize;
        let mut all_lines: Vec<Line<'static>> = Vec::new();
        let mut offsets: Vec<usize> = Vec::new();

        for (i, vm) in self.messages.iter_mut().enumerate() {
            offsets.push(all_lines.len());
            all_lines.extend(Self::render_one(vm, i + 1, width));
        }

        let render_width = self.width.saturating_sub(1);
        let mut cache = self.cache.write();
        cache.lines = all_lines;
        cache.message_offsets = offsets;
        cache.total_lines = RenderCache::compute_wrapped_height(&cache.lines, render_width);
        cache.wrap_map = Self::build_wrap_map(&cache.lines, self.width);
        cache.version += 1;
    }

    /// 主事件循环
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<RenderEvent>) {
        while let Some(event) = rx.recv().await {
            match event {
                RenderEvent::AddMessage(vm) => {
                    self.messages.push(vm);
                    let width = self.width.saturating_sub(1) as usize;
                    let idx = self.messages.len() - 1;
                    let lines = Self::render_one(&mut self.messages[idx], idx + 1, width);

                    let render_width = self.width.saturating_sub(1);
                    let mut cache = self.cache.write();
                    let offset = cache.lines.len();
                    cache.message_offsets.push(offset);
                    cache.lines.extend(lines);
                    cache.total_lines =
                        RenderCache::compute_wrapped_height(&cache.lines, render_width);
                    cache.wrap_map = Self::build_wrap_map(&cache.lines, self.width);
                    cache.version += 1;
                }
                RenderEvent::AppendChunk(chunk) => {
                    // 找到最后一条 assistant 消息并追加 chunk
                    let appended = if let Some(last) = self.messages.last_mut() {
                        if last.is_assistant() {
                            last.append_chunk(&chunk);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if !appended {
                        // 没有 assistant 消息，创建一个新的
                        let mut vm = MessageViewModel::assistant();
                        vm.append_chunk(&chunk);
                        self.messages.push(vm);
                    }

                    // 重新渲染最后一条消息，替换缓存中对应区间
                    let width = self.width.saturating_sub(1) as usize;
                    let last_idx = self.messages.len() - 1;
                    let new_lines =
                        Self::render_one(&mut self.messages[last_idx], last_idx + 1, width);

                    let render_width = self.width.saturating_sub(1);
                    let mut cache = self.cache.write();
                    // 获取最后一条消息的起始偏移
                    let start = if let Some(&offset) = cache.message_offsets.last() {
                        offset
                    } else {
                        // 新消息，还没有 offset
                        let offset = cache.lines.len();
                        cache.message_offsets.push(offset);
                        offset
                    };
                    // 替换从 start 开始到末尾的所有行
                    cache.lines.truncate(start);
                    cache.lines.extend(new_lines);
                    cache.total_lines =
                        RenderCache::compute_wrapped_height(&cache.lines, render_width);
                    cache.wrap_map = Self::build_wrap_map(&cache.lines, self.width);
                    cache.version += 1;
                }
                RenderEvent::StreamingDone => {
                    // 将最后一条 assistant 消息的 is_streaming 设为 false，重新渲染
                    if let Some(MessageViewModel::AssistantBubble { is_streaming, .. }) =
                        self.messages.last_mut()
                    {
                        *is_streaming = false;
                    }
                    // 重新渲染最后一条消息
                    let width = self.width.saturating_sub(1) as usize;
                    if !self.messages.is_empty() {
                        let last_idx = self.messages.len() - 1;
                        let new_lines =
                            Self::render_one(&mut self.messages[last_idx], last_idx + 1, width);
                        let render_width = self.width.saturating_sub(1);
                        let mut cache = self.cache.write();
                        if let Some(&start) = cache.message_offsets.last() {
                            cache.lines.truncate(start);
                            cache.lines.extend(new_lines);
                            cache.total_lines =
                                RenderCache::compute_wrapped_height(&cache.lines, render_width);
                            cache.wrap_map = Self::build_wrap_map(&cache.lines, self.width);
                            cache.version += 1;
                        }
                    }
                }
                RenderEvent::Resize(new_width) => {
                    self.width = new_width;
                    self.rebuild_all();
                }
                RenderEvent::Clear => {
                    self.messages.clear();
                    let mut cache = self.cache.write();
                    cache.lines.clear();
                    cache.message_offsets.clear();
                    cache.total_lines = 0;
                    cache.wrap_map = Vec::new();
                    cache.version += 1;
                }
                RenderEvent::LoadHistory(vms) => {
                    self.messages = vms;
                    self.rebuild_all();
                }
                RenderEvent::ToggleToolMessages(show) => {
                    self.show_tool_messages = show;
                    self.rebuild_all();
                }
                RenderEvent::UpdateLastMessage(vm) => {
                    // 替换最后一条消息（SubAgentGroup 更新专用）
                    if let Some(last) = self.messages.last_mut() {
                        *last = vm;
                    } else {
                        self.messages.push(vm);
                    }
                    // 重新渲染最后一条消息，替换缓存中对应区间的行
                    let width = self.width.saturating_sub(1) as usize;
                    if !self.messages.is_empty() {
                        let last_idx = self.messages.len() - 1;
                        let new_lines =
                            Self::render_one(&mut self.messages[last_idx], last_idx + 1, width);
                        let render_width = self.width.saturating_sub(1);
                        let mut cache = self.cache.write();
                        if let Some(&start) = cache.message_offsets.last() {
                            cache.lines.truncate(start);
                            cache.lines.extend(new_lines);
                            cache.total_lines =
                                RenderCache::compute_wrapped_height(&cache.lines, render_width);
                            cache.wrap_map = Self::build_wrap_map(&cache.lines, self.width);
                            cache.version += 1;
                        }
                    }
                }
                RenderEvent::RemoveLastMessage => {
                    // 移除最后一条消息及其对应的渲染缓存
                    if !self.messages.is_empty() {
                        self.messages.pop();
                        let render_width = self.width.saturating_sub(1);
                        let mut cache = self.cache.write();
                        // 移除最后一条消息的 offset
                        cache.message_offsets.pop();
                        if let Some(&start) = cache.message_offsets.last() {
                            cache.lines.truncate(start);
                        } else {
                            cache.lines.clear();
                        }
                        cache.total_lines =
                            RenderCache::compute_wrapped_height(&cache.lines, render_width);
                        cache.wrap_map = Self::build_wrap_map(&cache.lines, self.width);
                        cache.version += 1;
                    }
                }
            }

            self.notify.notify_one();
        }
    }
}

/// 启动渲染线程，返回事件发送端、共享缓存和通知
///
/// 使用无界 channel：渲染事件处理耗时微秒级，不会积压；
/// 有界 channel 的 try_send 静默丢弃会导致渲染线程与 App 状态分叉。
pub fn spawn_render_thread(
    width: u16,
) -> (
    mpsc::UnboundedSender<RenderEvent>,
    Arc<RwLock<RenderCache>>,
    Arc<Notify>,
) {
    let (tx, rx) = mpsc::unbounded_channel();
    let cache = Arc::new(RwLock::new(RenderCache::new()));
    let notify = Arc::new(Notify::new());

    let task = RenderTask {
        messages: Vec::new(),
        cache: Arc::clone(&cache),
        notify: Arc::clone(&notify),
        width,
        show_tool_messages: false,
    };

    tokio::spawn(task.run(rx));

    (tx, cache, notify)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_message_increments_version() {
        let (tx, cache, _notify) = spawn_render_thread(80);

        // 初始 version 为 0
        assert_eq!(cache.read().version, 0);

        // 发送一条用户消息（UnboundedSender::send 是同步的）
        tx.send(RenderEvent::AddMessage(MessageViewModel::user(
            "Hello".to_string(),
        )))
        .unwrap();

        // 等待渲染线程处理
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let c = cache.read();
        assert!(c.version > 0, "version should increment after AddMessage");
        assert!(
            !c.lines.is_empty(),
            "lines should not be empty after AddMessage"
        );
    }

    #[tokio::test]
    async fn test_append_chunk_updates_last_message() {
        let (tx, cache, _notify) = spawn_render_thread(80);

        // 先添加一条 assistant 消息
        let mut vm = MessageViewModel::assistant();
        vm.append_chunk("Hello ");
        tx.send(RenderEvent::AddMessage(vm)).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let v1 = cache.read().version;
        let lines_before = cache.read().lines.len();

        // AppendChunk
        tx.send(RenderEvent::AppendChunk("World".to_string()))
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let c = cache.read();
        assert!(c.version > v1, "version should increment after AppendChunk");
        // offset 不应增加（仍是同一条消息）
        assert_eq!(
            c.message_offsets.len(),
            1,
            "should still have 1 message offset"
        );
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
        // usable_width = 40 - 1 = 39; 200 / 39 = 5.13 → 6 visual rows
        assert_eq!(result[0].visual_row_start, 0);
        assert_eq!(result[0].visual_row_end, 6);
    }

    #[test]
    fn test_build_wrap_map_cjk_char_width() {
        let lines = vec![Line::from("你好世界")];
        let result = RenderTask::build_wrap_map(&lines, 80);
        assert_eq!(result[0].char_widths, vec![2, 2, 2, 2]);
        // line_display_width = 8, usable_width = 79, fits in 1 row
        assert_eq!(result[0].visual_row_end - result[0].visual_row_start, 1);
    }

    #[test]
    fn test_build_wrap_map_multi_line_visual_rows() {
        // First line: 80 chars, width=41 → usable=40, 80/40=2 visual rows
        let first_line: String = "A".repeat(80);
        let second_line = Line::from("short");
        let lines: Vec<Line<'static>> = vec![Line::from(first_line), second_line];
        let result = RenderTask::build_wrap_map(&lines, 41);
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
}
