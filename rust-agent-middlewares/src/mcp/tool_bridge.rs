use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{Content, Tool};
use rust_create_agent::tools::BaseTool;
use thiserror::Error;

use super::client::{McpClientHandle, McpClientPool};

/// MCP 工具调用错误
#[derive(Debug, Error)]
pub enum ToolCallError {
    #[error("MCP 服务器 \"{server}\" 未连接 (状态: {status:?})")]
    NotConnected { server: String, status: String },
    #[error("MCP 服务器 \"{server}\" 工具 \"{tool}\" 调用失败: {reason}")]
    CallFailed {
        server: String,
        tool: String,
        reason: String,
    },
    #[error("MCP 服务器 \"{server}\" 工具 \"{tool}\" 调用超时 ({timeout_secs}s)")]
    Timeout {
        server: String,
        tool: String,
        timeout_secs: u64,
    },
}

/// 将单个 MCP tool 包装为 BaseTool 实现
pub struct McpToolBridge {
    server_name: String,
    tool_name: String,
    full_name: String,
    description: String,
    input_schema: serde_json::Value,
    client: Arc<McpClientHandle>,
}

const TOOL_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

impl McpToolBridge {
    pub fn new(server_name: &str, tool: &Tool, client: Arc<McpClientHandle>) -> Self {
        let tool_name = tool.name.to_string();
        let full_name = format!("mcp__{}__{}", server_name, tool_name);
        let description = format!(
            "[MCP:{}] {}",
            server_name,
            tool.description.as_ref().map(|d| d.as_ref()).unwrap_or("")
        );
        let input_schema = serde_json::to_value(&*tool.input_schema)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        Self {
            server_name: server_name.to_string(),
            tool_name,
            full_name,
            description,
            input_schema,
            client,
        }
    }
}

#[async_trait]
impl BaseTool for McpToolBridge {
    fn name(&self) -> &str {
        &self.full_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.input_schema.clone()
    }

    async fn invoke(
        &self,
        input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // 1. 检查连接状态
        match &self.client.peer {
            Some(_) => {}
            None => {
                return Err(Box::new(ToolCallError::NotConnected {
                    server: self.server_name.clone(),
                    status: format!("{:?}", self.client.status),
                }));
            }
        }

        let peer = self.client.peer.as_ref().unwrap();

        // 2. 构建 rmcp 请求参数
        let arguments = input.as_object().cloned().unwrap_or_default();
        let request = rmcp::model::CallToolRequestParams::new(self.tool_name.clone())
            .with_arguments(arguments);

        // 3. 带超时调用 peer.call_tool()
        let result = tokio::time::timeout(TOOL_CALL_TIMEOUT, peer.call_tool(request))
            .await
            .map_err(|_| ToolCallError::Timeout {
                server: self.server_name.clone(),
                tool: self.tool_name.clone(),
                timeout_secs: TOOL_CALL_TIMEOUT.as_secs(),
            })?
            .map_err(|e| ToolCallError::CallFailed {
                server: self.server_name.clone(),
                tool: self.tool_name.clone(),
                reason: e.to_string(),
            })?;

        // 4. 处理 is_error 标志
        if result.is_error.unwrap_or(false) {
            let error_text = format_contents(&result.content);
            return Err(Box::new(ToolCallError::CallFailed {
                server: self.server_name.clone(),
                tool: self.tool_name.clone(),
                reason: error_text,
            }));
        }

        // 5. 格式化返回
        Ok(format_contents(&result.content))
    }
}

/// 将 content 列表格式化为纯文本字符串
fn format_contents(contents: &[Content]) -> String {
    let mut parts = Vec::new();
    for content in contents {
        match &content.raw {
            rmcp::model::RawContent::Text(text_content) => {
                parts.push(text_content.text.clone());
            }
            rmcp::model::RawContent::Image(image_content) => {
                parts.push(format!("[image: {}]", image_content.mime_type));
            }
            rmcp::model::RawContent::Resource(resource_content) => {
                let uri = match &resource_content.resource {
                    rmcp::model::ResourceContents::TextResourceContents { uri, .. } => uri.clone(),
                    rmcp::model::ResourceContents::BlobResourceContents { uri, .. } => uri.clone(),
                };
                parts.push(format!("[resource: {}]", uri));
            }
            rmcp::model::RawContent::Audio(audio_content) => {
                parts.push(format!("[audio: {}]", audio_content.mime_type));
            }
            rmcp::model::RawContent::ResourceLink(link) => {
                parts.push(format!("[resource_link: {}]", link.uri));
            }
        }
    }
    parts.join("\n")
}

/// 从 McpClientPool 的所有已连接客户端中批量创建 McpToolBridge
pub fn build_tool_bridges(pool: &McpClientPool) -> Vec<Box<dyn BaseTool>> {
    let mut bridges: Vec<Box<dyn BaseTool>> = Vec::new();
    for client in pool.get_all_clients() {
        for tool in &client.tools {
            bridges.push(Box::new(McpToolBridge::new(
                &client.name,
                tool,
                Arc::clone(&client),
            )));
        }
    }
    bridges
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::RawTextContent;
    use crate::mcp::client::ClientStatus;

    fn make_tool(name: &str, description: Option<&str>) -> Tool {
        let json = serde_json::json!({
            "name": name,
            "description": description.unwrap_or(""),
            "inputSchema": {
                "type": "object",
                "properties": { "path": { "type": "string" } }
            }
        });
        serde_json::from_value(json).unwrap()
    }

    fn make_disconnected_handle(name: &str) -> Arc<McpClientHandle> {
        Arc::new(McpClientHandle {
            name: name.to_string(),
            peer: None,
            tools: vec![],
            resources: vec![],
            status: ClientStatus::Failed("connection lost".to_string()),
            oauth_status: Default::default(),
        })
    }

    #[test]
    fn test_new_creates_correct_full_name() {
        let tool = make_tool("read_file", Some("Read a file"));
        let handle = make_disconnected_handle("fs");
        let bridge = McpToolBridge::new("fs", &tool, handle);
        assert_eq!(bridge.name(), "mcp__fs__read_file");
    }

    #[test]
    fn test_new_creates_correct_description() {
        let tool = make_tool("read_file", Some("Read a file"));
        let handle = make_disconnected_handle("fs");
        let bridge = McpToolBridge::new("fs", &tool, handle);
        assert_eq!(bridge.description(), "[MCP:fs] Read a file");
    }

    #[test]
    fn test_new_preserves_input_schema() {
        let tool = make_tool("read_file", None);
        let handle = make_disconnected_handle("fs");
        let bridge = McpToolBridge::new("fs", &tool, handle);
        let params = bridge.parameters();
        assert!(params.get("properties").is_some());
    }

    #[test]
    fn test_new_empty_description() {
        let tool = make_tool("read_file", None);
        let handle = make_disconnected_handle("fs");
        let bridge = McpToolBridge::new("fs", &tool, handle);
        assert_eq!(bridge.description(), "[MCP:fs] ");
    }

    #[tokio::test]
    async fn test_invoke_not_connected() {
        let tool = make_tool("read_file", None);
        let handle = make_disconnected_handle("fs");
        let bridge = McpToolBridge::new("fs", &tool, handle);
        let result = bridge.invoke(serde_json::json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("未连接"));
    }

    #[test]
    fn test_format_content_text_only() {
        let contents = vec![Content {
            raw: rmcp::model::RawContent::Text(RawTextContent {
                text: "hello".to_string(),
                meta: None,
            }),
            annotations: None,
        }];
        assert_eq!(format_contents(&contents), "hello");
    }

    #[test]
    fn test_format_content_mixed() {
        let contents = vec![
            Content {
                raw: rmcp::model::RawContent::Text(RawTextContent {
                    text: "line1".to_string(),
                    meta: None,
                }),
                annotations: None,
            },
            Content {
                raw: rmcp::model::RawContent::Text(RawTextContent {
                    text: "line2".to_string(),
                    meta: None,
                }),
                annotations: None,
            },
        ];
        assert_eq!(format_contents(&contents), "line1\nline2");
    }

    #[test]
    fn test_build_tool_bridges_empty_pool() {
        let pool = McpClientPool::new_empty();
        let bridges = build_tool_bridges(&pool);
        assert!(bridges.is_empty());
    }
}
