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
        // 只在 input_tokens > 0 时更新 last_usage，
        // 防止异常 API 响应（input_tokens=0）覆盖正常的上下文估算
        if usage.input_tokens > 0 {
            self.last_usage = Some(usage.clone());
        }
        self.llm_call_count += 1;
    }

    pub fn estimated_context_tokens(&self) -> Option<u64> {
        // input_tokens 已在 adapter 层规范化为总输入（含缓存 token），
        // 即当前 prompt 的实际大小，直接反映上下文窗口占用。
        // 不加 output_tokens：output 会在下一轮 API 调用中包含进 input_tokens，
        // 相加会导致双重计算，使显示用量约为实际的 2 倍。
        self.last_usage.as_ref().map(|u| u.input_tokens as u64)
    }

    pub fn context_usage_percent(&self, context_window: u32) -> Option<f64> {
        self.estimated_context_tokens()
            .map(|used| (used as f64 / context_window as f64) * 100.0)
    }

    /// 会话累计缓存命中率
    ///
    /// 统一公式：`cache_read / total_input_tokens`
    /// 适用于所有 provider（adapter 层已将 input_tokens 规范化为含缓存的总输入）。
    ///
    /// 返回 `None` 表示无缓存命中数据。
    pub fn cache_hit_rate(&self) -> Option<f64> {
        if self.total_cache_read_tokens == 0 {
            return None;
        }
        if self.total_input_tokens == 0 {
            return None;
        }
        Some(self.total_cache_read_tokens as f64 / self.total_input_tokens as f64)
    }

    /// 当次调用的缓存命中率（基于 last_usage）
    ///
    /// 与 `cache_hit_rate()`（累计）不同，此方法反映最近一次 LLM 调用的缓存效率。
    pub fn last_cache_hit_rate(&self) -> Option<f64> {
        self.last_usage.as_ref().and_then(|u| {
            let cache_read = u.cache_read_input_tokens.unwrap_or(0);
            if cache_read == 0 || u.input_tokens == 0 {
                return None;
            }
            Some(cache_read as f64 / u.input_tokens as f64)
        })
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
    fn test_accumulate_zero_input_tokens_does_not_overwrite_last_usage() {
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(50000, 2000, None, None));
        assert_eq!(tracker.estimated_context_tokens(), Some(50000));

        // 异常 API 响应 input_tokens=0，不应覆盖 last_usage
        tracker.accumulate(&make_usage(0, 100, None, None));
        assert_eq!(tracker.total_input_tokens, 50000, "total 仍累积");
        assert_eq!(tracker.total_output_tokens, 2100, "total 仍累积");
        assert_eq!(tracker.llm_call_count, 2);
        assert_eq!(
            tracker.estimated_context_tokens(),
            Some(50000),
            "last_usage 不应被 input_tokens=0 覆盖"
        );
    }

    #[test]
    fn test_estimated_context_tokens_some() {
        let mut tracker = TokenTracker::default();
        // input 已在 adapter 层规范化：raw(1000) + cache_creation(200) + cache_read(300) = 1500
        tracker.accumulate(&make_usage(1500, 500, Some(200), Some(300)));
        // estimated_context_tokens 只返回 input_tokens
        assert_eq!(tracker.estimated_context_tokens(), Some(1500));
    }

    #[test]
    fn test_estimated_context_tokens_no_cache() {
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(1000, 500, None, None));
        // estimated_context_tokens 只返回 input_tokens
        assert_eq!(tracker.estimated_context_tokens(), Some(1000));
    }

    #[test]
    fn test_estimated_context_tokens_openai_with_cached_tokens() {
        // OpenAI API: prompt_tokens 已包含 cached_tokens，adapter 层无需额外处理
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(150_000, 10_000, None, Some(120_000)));
        // estimated_context_tokens 只返回 input_tokens = 150K
        assert_eq!(tracker.estimated_context_tokens(), Some(150_000),);
        let pct = tracker.context_usage_percent(200_000).unwrap();
        assert!((pct - 75.0).abs() < 0.01, "应为 75%，实际 {}%", pct);
    }

    #[test]
    fn test_context_usage_percent() {
        let mut tracker = TokenTracker::default();
        // input 已规范化：raw(50000) + cache(12500) + cache(12500) = 75000
        tracker.accumulate(&make_usage(75000, 25000, Some(12500), Some(12500)));
        // estimated_context_tokens 只返回 input_tokens = 75000 → 37.5%
        let pct = tracker.context_usage_percent(200_000).unwrap();
        assert!((pct - 37.5).abs() < 0.01);
    }

    #[test]
    fn test_context_budget_should_auto_compact() {
        let budget = ContextBudget::new(200_000);
        let mut tracker = TokenTracker::default();
        // input=170K → 170K/200K = 85% → 达到 auto-compact 阈值
        tracker.accumulate(&make_usage(170000, 40000, None, None));
        assert!(budget.should_auto_compact(&tracker));
        // input=150K → 150K/200K = 75% < 85%
        let mut tracker2 = TokenTracker::default();
        tracker2.accumulate(&make_usage(150000, 40000, None, None));
        assert!(!budget.should_auto_compact(&tracker2));
    }

    #[test]
    fn test_context_budget_should_warn() {
        let budget = ContextBudget::new(200_000);
        let mut tracker = TokenTracker::default();
        // input=140K → 140K/200K = 70% → 达到警告阈值
        tracker.accumulate(&make_usage(140000, 60000, None, None));
        assert!(budget.should_warn(&tracker));
        // input=110K → 110K/200K = 55% < 70%
        let mut tracker2 = TokenTracker::default();
        tracker2.accumulate(&make_usage(110000, 40000, None, None));
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
        // input 已规范化：raw(85000) + cache(21250) + cache(21250) = 127500 → 127500 + 42500 = 170K (85%)
        // 90% threshold → 170K/200K = 85% < 90% → should NOT auto-compact
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(127500, 42500, Some(21250), Some(21250)));
        assert!(
            !budget.should_auto_compact(&tracker),
            "85% should not trigger at 90% threshold"
        );
    }

    #[test]
    fn test_context_budget_with_warning_threshold() {
        let budget = ContextBudget::new(200_000).with_warning_threshold(0.5);
        // input 已规范化：raw(60000) + cache(13750) + cache(13750) = 87500 → 87500 + 40000 = 127500 (63.75%)
        // 但用原始 input(60000) 模拟 OpenAI（无 cache_creation）：60000 + 40000 = 100K (50%)
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(100000, 0, None, None));
        assert!(
            budget.should_warn(&tracker),
            "50% should trigger warning at 50% threshold"
        );
    }

    #[test]
    fn test_token_tracker_reset() {
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(51500, 2000, Some(1000), Some(500)));
        assert!(tracker.llm_call_count > 0);
        tracker.reset();
        assert_eq!(tracker.total_input_tokens, 0);
        assert_eq!(tracker.total_output_tokens, 0);
        assert_eq!(tracker.total_cache_creation_tokens, 0);
        assert_eq!(tracker.total_cache_read_tokens, 0);
        assert!(tracker.last_usage.is_none());
        assert_eq!(tracker.llm_call_count, 0);
    }

    #[test]
    fn test_context_budget_zero_context_window() {
        let budget = ContextBudget::new(0);
        let tracker = TokenTracker::default();
        assert!(!budget.should_warn(&tracker));
        assert!(!budget.should_auto_compact(&tracker));
    }

    #[test]
    fn test_cache_hit_rate_none_when_no_cache_data() {
        let tracker = TokenTracker::default();
        assert!(tracker.cache_hit_rate().is_none());

        // OpenAI 兼容 API：cache 字段为 None
        let mut tracker2 = TokenTracker::default();
        tracker2.accumulate(&make_usage(1000, 500, None, None));
        assert!(tracker2.cache_hit_rate().is_none());
    }

    #[test]
    fn test_cache_hit_rate_zero_on_first_creation() {
        // 首次调用仅有 cache_creation，cache_read=0 → 返回 None（无命中数据）
        // input 已规范化：raw(1000) + cache_creation(5000) + cache_read(0) = 6000
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(6000, 500, Some(5000), Some(0)));
        assert!(
            tracker.cache_hit_rate().is_none(),
            "无 cache hit 应返回 None"
        );
    }

    #[test]
    fn test_cache_hit_rate_cumulative_after_multiple_calls() {
        let mut tracker = TokenTracker::default();

        // Call 1: cache_creation=5000, cache_read=0 → None
        tracker.accumulate(&make_usage(6000, 500, Some(5000), Some(0)));
        assert!(tracker.cache_hit_rate().is_none());

        // Call 2: cache_creation=0, cache_read=5000
        // total_input=6000+5100=11100, total_cache_read=5000 → 5000/11100 ≈ 45.05%
        tracker.accumulate(&make_usage(5100, 50, Some(0), Some(5000)));
        let rate = tracker.cache_hit_rate().unwrap();
        assert!(
            (rate - 5000.0 / 11100.0).abs() < 1e-9,
            "expected ≈45.05%, got {rate}"
        );

        // Call 3: cache_creation=0, cache_read=5000
        // total_input=11100+5100=16200, total_cache_read=10000 → 10000/16200 ≈ 61.73%
        tracker.accumulate(&make_usage(5100, 50, Some(0), Some(5000)));
        let rate = tracker.cache_hit_rate().unwrap();
        assert!(
            (rate - 10000.0 / 16200.0).abs() < 1e-9,
            "expected ≈61.73%, got {rate}"
        );
    }

    #[test]
    fn test_cache_hit_rate_anthropic_pattern() {
        // Anthropic prompt caching 典型模式：
        // 首次请求写入缓存，后续请求全部命中缓存
        // input 已在 adapter 层规范化（含缓存 token）
        let mut tracker = TokenTracker::default();

        // 首次：创建缓存。input=500+8000+0=8500, cache_read=0 → None
        tracker.accumulate(&make_usage(8500, 200, Some(8000), Some(0)));
        assert!(
            tracker.cache_hit_rate().is_none(),
            "首次创建缓存，无 cache hit 应返回 None"
        );

        // 后续：全部命中。total_input=17000, total_cache_read=8000 → 8000/17000 ≈ 47.06%
        tracker.accumulate(&make_usage(8500, 200, Some(0), Some(8000)));
        let rate = tracker.cache_hit_rate().unwrap();
        assert!(
            (rate - 8000.0 / 17000.0).abs() < 1e-9,
            "8000 cache_read / 17000 total_input ≈ 47.06%, got {rate}"
        );

        // 第三次命中。total_input=25500, total_cache_read=16000 → 16000/25500 ≈ 62.75%
        tracker.accumulate(&make_usage(8500, 200, Some(0), Some(8000)));
        let rate = tracker.cache_hit_rate().unwrap();
        assert!(
            (rate - 16000.0 / 25500.0).abs() < 1e-9,
            "16000 cache_read / 25500 total_input ≈ 62.75%, got {rate}"
        );
    }

    #[test]
    fn test_cache_hit_rate_after_reset() {
        let mut tracker = TokenTracker::default();
        // input 已规范化：raw(1000) + cache_creation(5000) + cache_read(5000) = 11000
        tracker.accumulate(&make_usage(11000, 500, Some(5000), Some(5000)));
        let rate = tracker.cache_hit_rate().unwrap();
        assert!((rate - 5000.0 / 11000.0).abs() < 1e-9);

        tracker.reset();
        assert!(tracker.cache_hit_rate().is_none(), "reset 后应无缓存数据");
    }

    #[test]
    fn test_cache_hit_rate_openai_pattern() {
        // OpenAI 风格：cache_creation 始终 None，
        // prompt_tokens 已含 cached_tokens，input 已规范化
        let mut tracker = TokenTracker::default();

        // 首次调用：prompt_tokens=10000, cached_tokens=0 → None
        tracker.accumulate(&make_usage(10000, 500, None, Some(0)));
        assert!(
            tracker.cache_hit_rate().is_none(),
            "cache_read=0 应返回 None"
        );

        // 第二次调用：prompt_tokens=10000, cached_tokens=8000 → 8000/20000 = 40%
        tracker.accumulate(&make_usage(10000, 500, None, Some(8000)));
        let rate = tracker.cache_hit_rate().unwrap();
        assert!(
            (rate - 0.4).abs() < 1e-9,
            "8000 cached / 20000 total input = 40%, got {rate}"
        );

        // 第三次调用：prompt_tokens=10000, cached_tokens=9500 → 17500/30000 ≈ 58.3%
        tracker.accumulate(&make_usage(10000, 500, None, Some(9500)));
        let rate = tracker.cache_hit_rate().unwrap();
        assert!(
            (rate - 17500.0 / 30000.0).abs() < 1e-9,
            "17500 cached / 30000 total input ≈ 58.3%, got {rate}"
        );
    }

    #[test]
    fn test_cache_hit_rate_openai_no_cached_field() {
        // OpenAI 某些模型不返回 cached_tokens → cache_read=None → 永远 None
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(10000, 500, None, None));
        tracker.accumulate(&make_usage(10000, 500, None, None));
        assert!(
            tracker.cache_hit_rate().is_none(),
            "无 cached_tokens 字段应返回 None"
        );
    }

    #[test]
    fn test_last_cache_hit_rate_none_when_empty() {
        let tracker = TokenTracker::default();
        assert!(tracker.last_cache_hit_rate().is_none());
    }

    #[test]
    fn test_last_cache_hit_rate_reflects_latest_call() {
        let mut tracker = TokenTracker::default();
        // 首次调用：无缓存
        tracker.accumulate(&make_usage(10000, 500, None, Some(0)));
        assert!(tracker.last_cache_hit_rate().is_none());

        // 第二次调用：高缓存命中 34230/34820 ≈ 98.3%
        tracker.accumulate(&make_usage(34820, 423, None, Some(34230)));
        let rate = tracker.last_cache_hit_rate().unwrap();
        assert!(
            (rate - 34230.0 / 34820.0).abs() < 1e-9,
            "expected ≈98.3%, got {rate}"
        );

        // 第三次调用：低缓存命中
        tracker.accumulate(&make_usage(20000, 1000, None, Some(5000)));
        let rate = tracker.last_cache_hit_rate().unwrap();
        assert!(
            (rate - 5000.0 / 20000.0).abs() < 1e-9,
            "expected 25%, got {rate}"
        );
    }

    #[test]
    fn test_last_cache_hit_rate_none_when_no_cache_field() {
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(10000, 500, None, None));
        assert!(tracker.last_cache_hit_rate().is_none());
    }

    #[test]
    fn test_last_vs_cumulative_cache_hit_rate() {
        let mut tracker = TokenTracker::default();
        // Call 1: input=10000, cache_read=0
        tracker.accumulate(&make_usage(10000, 500, None, Some(0)));
        // Call 2: input=34820, cache_read=34230
        tracker.accumulate(&make_usage(34820, 423, None, Some(34230)));

        // 累计：34230 / (10000+34820) = 34230/44820 ≈ 76.4%
        let cumulative = tracker.cache_hit_rate().unwrap();
        assert!((cumulative - 34230.0 / 44820.0).abs() < 1e-9);

        // 当次：34230 / 34820 ≈ 98.3%
        let last = tracker.last_cache_hit_rate().unwrap();
        assert!((last - 34230.0 / 34820.0).abs() < 1e-9);

        assert!(last > cumulative, "当次命中率应高于累计命中率");
    }

    #[test]
    fn test_context_usage_percent_zero_window() {
        let mut tracker = TokenTracker::default();
        tracker.accumulate(&make_usage(100, 50, None, None));
        let pct = tracker.context_usage_percent(0);
        // Division by zero → should return Some(infinity) or handle gracefully
        // The actual behavior is: 150.0 / 0.0 * 100.0 = inf
        assert!(
            pct.is_some(),
            "should return Some even with 0 context window"
        );
    }
}
