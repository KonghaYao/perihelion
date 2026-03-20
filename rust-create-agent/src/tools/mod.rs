pub mod provider;

pub use provider::ToolProvider;

use serde::{Deserialize, Serialize};

/// 工具定义（JSON Schema 格式参数描述）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema for parameters
    pub parameters: serde_json::Value,
}

/// BaseTool trait - 对齐 LangChain Python BaseTool
///
/// 所有工具必须实现此 trait，不再依赖 langchain-rust::tools::Tool。
#[async_trait::async_trait]
pub trait BaseTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;

    /// 返回完整工具定义（默认实现，组合 name/description/parameters）
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters(),
        }
    }

    /// 执行工具，输入为 JSON Value
    async fn invoke(
        &self,
        input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>>;
}
