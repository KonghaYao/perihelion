use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rand::Rng;

use crate::agent::events::{AgentEvent, AgentEventHandler};
use crate::agent::react::{ReactLLM, Reasoning};
use crate::error::AgentResult;
use crate::messages::BaseMessage;
use crate::tools::BaseTool;

/// 重试配置
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            base_delay_ms: 500,
            max_delay_ms: 32_000,
        }
    }
}

impl RetryConfig {
    pub fn with_max_retries(mut self, n: usize) -> Self { self.max_retries = n; self }
    pub fn with_base_delay_ms(mut self, ms: u64) -> Self { self.base_delay_ms = ms; self }
    pub fn with_max_delay_ms(mut self, ms: u64) -> Self { self.max_delay_ms = ms; self }

    /// 指数退避 + 25% 随机抖动
    ///
    /// attempt 从 0 开始，但首次重试（attempt=0）使用 base_delay * 2
    /// 以确保对 429 限流有足够等待时间。
    pub fn exponential_delay(&self, attempt: usize) -> u64 {
        let effective = attempt + 1;
        let base = (self.base_delay_ms as f64 * 2f64.powi(effective as i32))
            .min(self.max_delay_ms as f64);
        let mut rng = rand::thread_rng();
        let jitter = rng.gen_range(0.0..0.25) * base;
        (base + jitter) as u64
    }
}

/// ReactLLM 装饰器：在调用失败时自动重试
pub struct RetryableLLM<L: ReactLLM> {
    inner: L,
    config: RetryConfig,
    event_handler: Option<Arc<dyn AgentEventHandler>>,
}

impl<L: ReactLLM> RetryableLLM<L> {
    pub fn new(inner: L, config: RetryConfig) -> Self {
        Self { inner, config, event_handler: None }
    }

    pub fn with_event_handler(mut self, handler: Arc<dyn AgentEventHandler>) -> Self {
        self.event_handler = Some(handler);
        self
    }

    fn emit(&self, event: AgentEvent) {
        if let Some(h) = &self.event_handler {
            h.on_event(event);
        }
    }
}

#[async_trait]
impl<L: ReactLLM> ReactLLM for RetryableLLM<L> {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
    ) -> AgentResult<Reasoning> {
        // 重试循环：attempt 0..max_retries，每次失败若可重试则延迟后继续
        for attempt in 0..self.config.max_retries {
            match self.inner.generate_reasoning(messages, tools).await {
                Ok(r) => return Ok(r),
                Err(e) if e.is_retryable() => {
                    let delay = self.config.exponential_delay(attempt);
                    tracing::warn!(
                        attempt = attempt + 1,
                        max_retries = self.config.max_retries,
                        delay_ms = delay,
                        error = %e,
                        "LLM 调用失败，准备重试"
                    );
                    self.emit(AgentEvent::LlmRetrying {
                        attempt: attempt + 1,
                        max_attempts: self.config.max_retries,
                        delay_ms: delay,
                        error: e.to_string(),
                    });
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
                Err(e) => return Err(e),
            }
        }
        // 最终尝试（不重试），直接返回结果或错误
        self.inner.generate_reasoning(messages, tools).await
    }

    fn model_name(&self) -> String {
        self.inner.model_name()
    }

    fn context_window(&self) -> u32 {
        self.inner.context_window()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AgentError;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Mock LLM：按脚本返回成功或失败
    struct MockLLM {
        results: Arc<Vec<AgentResult<Reasoning>>>,
        call_count: AtomicUsize,
    }

    impl MockLLM {
        fn new(results: Vec<AgentResult<Reasoning>>) -> Self {
            Self {
                results: Arc::new(results),
                call_count: AtomicUsize::new(0),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl ReactLLM for MockLLM {
        async fn generate_reasoning(
            &self,
            _messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> AgentResult<Reasoning> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            if idx < self.results.len() {
                match &self.results[idx] {
                    Ok(r) => Ok(r.clone()),
                    Err(e) => Err(clone_error(e)),
                }
            } else {
                Err(AgentError::LlmError("unexpected call".into()))
            }
        }

        fn model_name(&self) -> String {
            "mock".to_string()
        }

        fn context_window(&self) -> u32 {
            200_000
        }
    }

    fn clone_error(e: &AgentError) -> AgentError {
        match e {
            AgentError::LlmError(msg) => AgentError::LlmError(msg.clone()),
            AgentError::LlmHttpError { status, message } => AgentError::LlmHttpError {
                status: *status,
                message: message.clone(),
            },
            AgentError::ToolNotFound(name) => AgentError::ToolNotFound(name.clone()),
            _ => AgentError::LlmError(e.to_string()),
        }
    }

    fn ok_reasoning() -> AgentResult<Reasoning> {
        Ok(Reasoning::with_answer("", "test response"))
    }

    fn http_error(status: u16) -> AgentResult<Reasoning> {
        Err(AgentError::LlmHttpError {
            status,
            message: format!("API 错误 {}", status),
        })
    }

    fn network_error(msg: &str) -> AgentResult<Reasoning> {
        Err(AgentError::LlmError(msg.to_string()))
    }

    /// 前两次 503，第三次成功 → 最终 Ok
    #[tokio::test]
    async fn test_retry_then_success() {
        let mock = MockLLM::new(vec![
            http_error(503),
            http_error(503),
            ok_reasoning(),
        ]);
        let retry = RetryableLLM::new(mock, RetryConfig::default().with_base_delay_ms(1));
        let result = retry.generate_reasoning(&[], &[]).await;
        assert!(result.is_ok());
    }

    /// 400 错误立即返回，不重试
    #[tokio::test]
    async fn test_non_retryable_immediate_return() {
        let mock = MockLLM::new(vec![http_error(400)]);
        let retry = RetryableLLM::new(mock, RetryConfig::default().with_base_delay_ms(1));
        let result = retry.generate_reasoning(&[], &[]).await;
        assert!(result.is_err());
        if let Err(AgentError::LlmHttpError { status, .. }) = result {
            assert_eq!(status, 400);
        } else {
            panic!("Expected LlmHttpError(400)");
        }
    }

    /// 重试耗尽，返回最后一次错误
    #[tokio::test]
    async fn test_retry_exhausted() {
        let mock = MockLLM::new(vec![
            http_error(429),
            http_error(429),
            http_error(429),
        ]);
        let config = RetryConfig::default().with_max_retries(2).with_base_delay_ms(1);
        let retry = RetryableLLM::new(mock, config);
        let result = retry.generate_reasoning(&[], &[]).await;
        assert!(result.is_err());
        if let Err(AgentError::LlmHttpError { status, .. }) = result {
            assert_eq!(status, 429);
        } else {
            panic!("Expected LlmHttpError(429)");
        }
    }

    /// 网络错误可重试
    #[tokio::test]
    async fn test_network_error_retryable() {
        let mock = MockLLM::new(vec![
            network_error("connection refused"),
            ok_reasoning(),
        ]);
        let retry = RetryableLLM::new(mock, RetryConfig::default().with_base_delay_ms(1));
        let result = retry.generate_reasoning(&[], &[]).await;
        assert!(result.is_ok());
    }

    /// 退避延迟范围验证
    #[test]
    fn test_exponential_delay_range() {
        let config = RetryConfig::default();
        for attempt in 0..=5 {
            let delay = config.exponential_delay(attempt);
            let effective = attempt + 1;
            let base = (config.base_delay_ms as f64 * 2f64.powi(effective as i32))
                .min(config.max_delay_ms as f64);
            let lower = base as u64;
            let upper = (base * 1.25) as u64;
            assert!(
                delay >= lower && delay <= upper,
                "attempt {}: delay {} not in [{}, {}]",
                attempt, delay, lower, upper,
            );
        }
    }

    /// 验证最终尝试（重试耗尽后）直接返回结果，不进入重试逻辑
    #[tokio::test]
    async fn test_final_attempt_no_retry() {
        // 重试耗尽后的最终尝试：可重试错误也直接返回
        let mock = MockLLM::new(vec![
            http_error(429),
            http_error(429),
            http_error(429),
        ]);
        let config = RetryConfig::default().with_max_retries(2).with_base_delay_ms(1);
        let retry = RetryableLLM::new(mock, config);
        let result = retry.generate_reasoning(&[], &[]).await;
        assert!(result.is_err());
        // 重试 2 次（attempt 0,1）+ 最终尝试（返回错误）= 共 3 次调用
        // 脚本只有 3 个错误，恰好覆盖
        if let Err(AgentError::LlmHttpError { status, .. }) = result {
            assert_eq!(status, 429, "最终尝试应返回最后一次错误");
        } else {
            panic!("Expected LlmHttpError(429)");
        }
    }

    /// 验证 max_retries=0 时只执行一次调用（无重试）
    #[tokio::test]
    async fn test_zero_retries_single_attempt() {
        let mock = MockLLM::new(vec![ok_reasoning()]);
        let config = RetryConfig::default().with_max_retries(0);
        let retry = RetryableLLM::new(mock, config);
        let result = retry.generate_reasoning(&[], &[]).await;
        assert!(result.is_ok(), "max_retries=0 时应直接返回结果");
    }
}
