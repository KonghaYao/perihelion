use crate::llm::types::TokenUsage;

/// 会话级 token 用量追踪器
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TokenTracker {
    /// 累计输入 token（含 cache_read + cache_creation）
    pub total_input_tokens: u64,
    /// 累计输出 token
    pub total_output_tokens: u64,
    /// 累计 cache_creation token
    pub total_cache_creation_tokens: u64,
    /// 累计 cache_read token
    pub total_cache_read_tokens: u64,
    /// 最近一次 LLM 响应的 usage（用于估算当前上下文大小）
    pub last_usage: Option<TokenUsage>,
    /// 已完成的 LLM 调用次数
    pub llm_call_count: u32,
}

impl TokenTracker {
    pub fn accumulate(&mut self, usage: &TokenUsage) {
        self.total_input_tokens += usage.input_tokens as u64;
        self.total_output_tokens += usage.output_tokens as u64;
        if let Some(v) = usage.cache_creation_input_tokens {
            self.total_cache_creation_tokens += v as u64;
        }
        if let Some(v) = usage.cache_read_input_tokens {
            self.total_cache_read_tokens += v as u64;
        }
        self.last_usage = Some(usage.clone());
        self.llm_call_count += 1;
    }

    pub fn estimated_context_tokens(&self) -> Option<u64> {
        self.last_usage.as_ref().map(|u| {
            u.input_tokens as u64
                + u.output_tokens as u64
                + u.cache_creation_input_tokens.unwrap_or(0) as u64
                + u.cache_read_input_tokens.unwrap_or(0) as u64
        })
    }

    pub fn context_usage_percent(&self, context_window: u32) -> Option<f64> {
        self.estimated_context_tokens()
            .map(|used| (used as f64 / context_window as f64) * 100.0)
    }

    /// 重置追踪器（compact 后调用）
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// 上下文窗口预算配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContextBudget {
    /// 模型的上下文窗口大小（token 数）
    pub context_window: u32,
    /// auto-compact 触发阈值（百分比，0.0-1.0）
    pub auto_compact_threshold: f64,
    /// 警告阈值（百分比，0.0-1.0）
    pub warning_threshold: f64,
}

impl ContextBudget {
    pub const DEFAULT_CONTEXT_WINDOW: u32 = 200_000;
    pub const DEFAULT_AUTO_COMPACT_THRESHOLD: f64 = 0.85;
    pub const DEFAULT_WARNING_THRESHOLD: f64 = 0.70;

    pub fn new(context_window: u32) -> Self {
        Self {
            context_window,
            auto_compact_threshold: Self::DEFAULT_AUTO_COMPACT_THRESHOLD,
            warning_threshold: Self::DEFAULT_WARNING_THRESHOLD,
        }
    }

    pub fn should_auto_compact(&self, tracker: &TokenTracker) -> bool {
        match tracker.context_usage_percent(self.context_window) {
            Some(pct) => pct / 100.0 >= self.auto_compact_threshold,
            None => false,
        }
    }

    pub fn should_warn(&self, tracker: &TokenTracker) -> bool {
        match tracker.context_usage_percent(self.context_window) {
            Some(pct) => pct / 100.0 >= self.warning_threshold,
            None => false,
        }
    }

    pub fn with_auto_compact_threshold(mut self, threshold: f64) -> Self {
        self.auto_compact_threshold = threshold;
        self
    }

    pub fn with_warning_threshold(mut self, threshold: f64) -> Self {
        self.warning_threshold = threshold;
        self
    }
}

#[deprecated(
    since = "0.2.0",
    note = "使用 `crate::agent::compact::micro_compact_enhanced` 代替，支持白名单过滤、时间衰减、图片清除和工具对保护"
)]
/// 轻量级压缩：清除旧工具结果中的大段内容
/// 保留最近 `keep_recent` 条消息的工具结果完整内容
/// 仅清除 cutoff 之前且文本长度 > 500 字符的工具结果
pub fn micro_compact(messages: &mut [crate::messages::BaseMessage], keep_recent: usize) -> usize {
    let total = messages.len();
    let cutoff = total.saturating_sub(keep_recent);
    let mut cleared = 0;
    for msg in messages.iter_mut().take(cutoff) {
        if let crate::messages::BaseMessage::Tool { content, .. } = msg {
            let text = content.text_content();
            if text.len() > 500 {
                *content = crate::messages::MessageContent::text("[旧工具结果已清除]");
                cleared += 1;
            }
        }
    }
    cleared
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_usage(
        input: u32,
        output: u32,
        cache_creation: Option<u32>,
        cache_read: Option<u32>,
    ) -> TokenUsage {
        TokenUsage {
            input_tokens: input,
            output_tokens: output,
            cache_creation_input_tokens: cache_creation,
            cache_read_input_tokens: cache_read,
        }
    }

    #[test]
    fn test_accumulate_sums_tokens() {
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(100, 50, Some(30), Some(20)));
        tracker.accumulate(&make_usage(200, 80, Some(10), Some(40)));
        assert_eq!(tracker.total_input_tokens, 300);
        assert_eq!(tracker.total_output_tokens, 130);
        assert_eq!(tracker.total_cache_creation_tokens, 40);
        assert_eq!(tracker.total_cache_read_tokens, 60);
        assert_eq!(tracker.llm_call_count, 2);
    }

    #[test]
    fn test_accumulate_with_none_cache() {
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(100, 50, None, None));
        assert_eq!(tracker.total_input_tokens, 100);
        assert_eq!(tracker.total_output_tokens, 50);
        assert_eq!(tracker.total_cache_creation_tokens, 0);
        assert_eq!(tracker.total_cache_read_tokens, 0);
        assert_eq!(tracker.llm_call_count, 1);
    }

    #[test]
    fn test_estimated_context_tokens_none() {
        let tracker = TokenTracker::default();
        assert!(tracker.estimated_context_tokens().is_none());
    }

    #[test]
    fn test_estimated_context_tokens_some() {
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(1000, 500, Some(200), Some(300)));
        // 1000 + 500 + 200 + 300 = 2000
        assert_eq!(tracker.estimated_context_tokens(), Some(2000));
    }

    #[test]
    fn test_context_usage_percent() {
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(50000, 25000, Some(12500), Some(12500)));
        // 50000 + 25000 + 12500 + 12500 = 100000
        let pct = tracker.context_usage_percent(200_000).unwrap();
        assert!((pct - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_context_budget_should_auto_compact() {
        let budget = ContextBudget::new(200_000);
        let mut tracker = TokenTracker::default();
        // 85% of 200K = 170K
        tracker.accumulate(&make_usage(85000, 42500, Some(21250), Some(21250)));
        assert!(budget.should_auto_compact(&tracker));
        // 80% = 160K
        let mut tracker2 = TokenTracker::default();
        tracker2.accumulate(&make_usage(80000, 40000, Some(20000), Some(20000)));
        assert!(!budget.should_auto_compact(&tracker2));
    }

    #[test]
    fn test_context_budget_should_warn() {
        let budget = ContextBudget::new(200_000);
        let mut tracker = TokenTracker::default();
        // 70% of 200K = 140K
        tracker.accumulate(&make_usage(70000, 35000, Some(17500), Some(17500)));
        assert!(budget.should_warn(&tracker));
        // 60% = 120K
        let mut tracker2 = TokenTracker::default();
        tracker2.accumulate(&make_usage(60000, 30000, Some(15000), Some(15000)));
        assert!(!budget.should_warn(&tracker2));
    }

    #[test]
    fn test_context_budget_new_uses_defaults() {
        let budget = ContextBudget::new(128_000);
        assert_eq!(budget.context_window, 128_000);
        assert!((budget.auto_compact_threshold - 0.85).abs() < 0.001);
        assert!((budget.warning_threshold - 0.70).abs() < 0.001);
    }

    #[test]
    fn test_micro_compact_clears_old() {
        use crate::messages::{BaseMessage, MessageContent};

        let long_text = "x".repeat(600);
        let short_text = "y".repeat(100);
        let mut messages: Vec<BaseMessage> = vec![
            BaseMessage::Tool {
                id: Default::default(),
                tool_call_id: "1".into(),
                content: MessageContent::text(&long_text),
                is_error: false,
            },
            BaseMessage::Tool {
                id: Default::default(),
                tool_call_id: "2".into(),
                content: MessageContent::text(&long_text),
                is_error: false,
            },
            BaseMessage::Tool {
                id: Default::default(),
                tool_call_id: "3".into(),
                content: MessageContent::text(&short_text),
                is_error: false,
            },
            BaseMessage::Tool {
                id: Default::default(),
                tool_call_id: "4".into(),
                content: MessageContent::text(&long_text),
                is_error: false,
            },
            BaseMessage::Tool {
                id: Default::default(),
                tool_call_id: "5".into(),
                content: MessageContent::text(&long_text),
                is_error: false,
            },
            BaseMessage::Tool {
                id: Default::default(),
                tool_call_id: "6".into(),
                content: MessageContent::text(&short_text),
                is_error: false,
            },
            BaseMessage::Tool {
                id: Default::default(),
                tool_call_id: "7".into(),
                content: MessageContent::text(&long_text),
                is_error: false,
            },
            BaseMessage::Human { id: Default::default(), content: MessageContent::text("hello") },
            BaseMessage::Ai { id: Default::default(), content: MessageContent::text("hi"), tool_calls: vec![] },
            BaseMessage::Human { id: Default::default(), content: MessageContent::text("bye") },
        ];
        let cleared = micro_compact(&mut messages, 3);
        // Total 10, keep_recent 3, cutoff = 7
        // Among first 7: indices 0,1,3,4,6 have long text (5 cleared), index 2,5 have short text
        assert_eq!(cleared, 5);
        // Check first long one was replaced
        if let BaseMessage::Tool { content, .. } = &messages[0] {
            assert_eq!(content.text_content(), "[旧工具结果已清除]");
        }
        // Check short one was NOT replaced
        if let BaseMessage::Tool { content, .. } = &messages[2] {
            assert_eq!(content.text_content(), short_text);
        }
        // Last 3 should be untouched
        assert!(matches!(&messages[7], BaseMessage::Human { .. }));
    }

    #[test]
    fn test_micro_compact_short_content_untouched() {
        use crate::messages::{BaseMessage, MessageContent};

        let mut messages: Vec<BaseMessage> = vec![
            BaseMessage::Tool {
                id: Default::default(),
                tool_call_id: "1".into(),
                content: MessageContent::text("short"),
                is_error: false,
            },
            BaseMessage::Tool {
                id: Default::default(),
                tool_call_id: "2".into(),
                content: MessageContent::text("also short"),
                is_error: false,
            },
        ];
        let cleared = micro_compact(&mut messages, 1);
        assert_eq!(cleared, 0);
    }

    #[test]
    fn test_micro_compact_keep_recent() {
        use crate::messages::{BaseMessage, MessageContent};

        let long_text = "x".repeat(600);
        let mut messages: Vec<BaseMessage> = vec![
            BaseMessage::Tool {
                id: Default::default(),
                tool_call_id: "1".into(),
                content: MessageContent::text(&long_text),
                is_error: false,
            },
            BaseMessage::Tool {
                id: Default::default(),
                tool_call_id: "2".into(),
                content: MessageContent::text(&long_text),
                is_error: false,
            },
            BaseMessage::Tool {
                id: Default::default(),
                tool_call_id: "3".into(),
                content: MessageContent::text(&long_text),
                is_error: false,
            },
        ];
        let cleared = micro_compact(&mut messages, 2);
        // Total 3, keep_recent 2, cutoff = 1, only first is cleared
        assert_eq!(cleared, 1);
        if let BaseMessage::Tool { content, .. } = &messages[1] {
            assert_eq!(content.text_content(), long_text);
        }
    }

    #[test]
    fn test_micro_compact_empty() {
        let mut messages: Vec<crate::messages::BaseMessage> = vec![];
        let cleared = micro_compact(&mut messages, 3);
        assert_eq!(cleared, 0);
    }

    // ── ContextBudget builder 方法测试 ─────────────────────────────────────────

    #[test]
    fn test_context_budget_with_auto_compact_threshold() {
        let budget = ContextBudget::new(200_000).with_auto_compact_threshold(0.9);
        // 85% of 200K = 170K, 90% threshold = 180K → should NOT auto-compact
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(85000, 42500, Some(21250), Some(21250)));
        assert!(
            !budget.should_auto_compact(&tracker),
            "85% should not trigger at 90% threshold"
        );
    }

    #[test]
    fn test_context_budget_with_warning_threshold() {
        let budget = ContextBudget::new(200_000).with_warning_threshold(0.5);
        // 55% of 200K = 110K, 50% threshold = 100K → should warn
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(55000, 27500, Some(13750), Some(13750)));
        assert!(
            budget.should_warn(&tracker),
            "55% should trigger warning at 50% threshold"
        );
    }
}
