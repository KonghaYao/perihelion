pub mod anthropic;
pub mod openai;
pub mod retry;
pub mod types;

mod adapter;
mod react_adapter;

use crate::error::AgentResult;
use crate::llm::types::{LlmRequest, LlmResponse};
use async_trait::async_trait;

/// BaseModel trait - 统一 LLM 接口，对齐 LangChain Python BaseModel
#[async_trait]
pub trait BaseModel: Send + Sync {
    async fn invoke(&self, request: LlmRequest) -> AgentResult<LlmResponse>;
    fn provider_name(&self) -> &str;
    fn model_id(&self) -> &str;

    /// 模型的上下文窗口大小（token 数）
    ///
    /// 用于 token 用量追踪和上下文压缩决策。
    /// 默认返回 200_000（适用于大多数 modern LLM）。
    fn context_window(&self) -> u32 {
        200_000
    }
}

pub use adapter::MockLLM;
pub use anthropic::ChatAnthropic;
pub use openai::ChatOpenAI;
pub use retry::{RetryConfig, RetryableLLM};
// BaseModelReactLLM 保留用于向后兼容，但不再是推荐用法
pub use react_adapter::BaseModelReactLLM;
