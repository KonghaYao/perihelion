use async_trait::async_trait;
use peri_agent::agent::state::State;
use peri_agent::middleware::r#trait::Middleware;
use peri_agent::tools::BaseTool;

use super::web_fetch::WebFetchTool;
use super::web_search::WebSearchTool;

/// Web 中间件，提供 WebFetch 和 WebSearch 工具
pub struct WebMiddleware;

impl WebMiddleware {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<S: State> Middleware<S> for WebMiddleware {
    fn name(&self) -> &str {
        "WebMiddleware"
    }

    fn collect_tools(&self, _cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![
            Box::new(WebFetchTool::new()),
            Box::new(WebSearchTool::new()),
        ]
    }
}

#[cfg(test)]
#[path = "web_test.rs"]
mod tests;
