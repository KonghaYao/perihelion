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
        // input_tokens 已包含 cache_creation 和 cache_read 部分，
        // 直接相加会导致重复计算，使 auto-compact 过早触发。
        // 实际上下文 ≈ input_tokens + output_tokens
        self.last_usage.as_ref().map(|u| {
            u.input_tokens as u64 + u.output_tokens as u64
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
        // input_tokens(1000) + output_tokens(500) = 1500（不含 cache 重复计算）
        assert_eq!(tracker.estimated_context_tokens(), Some(1500));
    }

    #[test]
    fn test_context_usage_percent() {
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(50000, 25000, Some(12500), Some(12500)));
        // 50000 + 25000 = 75000（不含 cache 重复计算）
        let pct = tracker.context_usage_percent(200_000).unwrap();
        assert!((pct - 37.5).abs() < 0.01);
    }

    #[test]
    fn test_context_budget_should_auto_compact() {
        let budget = ContextBudget::new(200_000);
        let mut tracker = TokenTracker::default();
        // 85% of 200K = 170K → input 100K + output 70K = 170K
        tracker.accumulate(&make_usage(100000, 70000, Some(21250), Some(21250)));
        assert!(budget.should_auto_compact(&tracker));
        // 80% = 160K → input 90K + output 70K = 160K
        let mut tracker2 = TokenTracker::default();
        tracker2.accumulate(&make_usage(90000, 70000, Some(20000), Some(20000)));
        assert!(!budget.should_auto_compact(&tracker2));
    }

    #[test]
    fn test_context_budget_should_warn() {
        let budget = ContextBudget::new(200_000);
        let mut tracker = TokenTracker::default();
        // 70% of 200K = 140K → input 80K + output 60K = 140K
        tracker.accumulate(&make_usage(80000, 60000, Some(17500), Some(17500)));
        assert!(budget.should_warn(&tracker));
        // 60% = 120K → input 70K + output 50K = 120K
        let mut tracker2 = TokenTracker::default();
        tracker2.accumulate(&make_usage(70000, 50000, Some(15000), Some(15000)));
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
        // 50% of 200K = 100K → input 60K + output 40K = 100K → should warn
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(60000, 40000, Some(13750), Some(13750)));
        assert!(
            budget.should_warn(&tracker),
            "50% should trigger warning at 50% threshold"
        );
    }
}
