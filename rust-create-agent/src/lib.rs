//! # rust-create-agent
//!
//! Rust Agent framework with middleware system.
//! Aligned with `@langgraph-js/standard-agent` (TypeScript).

pub mod agent;
pub mod ask_user;
pub mod error;
pub mod hitl;
pub mod llm;
pub mod messages;
pub mod middleware;
pub mod telemetry;
pub mod thread;
pub mod tools;

/// Prelude - 常用类型一次性导入
pub mod prelude {
    pub use crate::agent::{
        events::{AgentEvent, AgentEventHandler, FnEventHandler},
        react::{AgentInput, AgentOutput, ReactLLM, Reasoning, ToolCall, ToolResult},
        state::{AgentState, State},
        ReActAgent,
    };
    pub use crate::ask_user::{
        AskUserBatchRequest, AskUserInvoker, AskUserOption, AskUserQuestionData,
    };
    pub use crate::error::{AgentError, AgentResult};
    pub use crate::hitl::{BatchItem, HitlDecision, HitlHandler};
    pub use crate::llm::{BaseModel, BaseModelReactLLM, ChatAnthropic, ChatOpenAI, MockLLM};
    pub use crate::messages::{
        BaseMessage, ContentBlock, DocumentSource, ImageSource, MessageContent, ToolCallRequest,
    };
    pub use crate::middleware::{
        r#trait::Middleware, LoggingMiddleware, MetricsMiddleware, MiddlewareChain, NoopMiddleware,
    };
    pub use crate::tools::{BaseTool, ToolDefinition, ToolProvider};
}
