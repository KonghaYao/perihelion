use crate::agent::react::{AgentOutput, ToolCall, ToolResult};
use crate::agent::state::State;
use crate::error::AgentResult;
use crate::middleware::r#trait::Middleware;
use crate::tools::BaseTool;

/// 中间件链 - 按顺序执行所有中间件
pub struct MiddlewareChain<S: State> {
    middlewares: Vec<Box<dyn Middleware<S>>>,
}

impl<S: State> MiddlewareChain<S> {
    pub fn new() -> Self {
        Self {
            middlewares: Vec::new(),
        }
    }

    /// 添加中间件（追加到链尾）
    pub fn add(&mut self, middleware: Box<dyn Middleware<S>>) {
        self.middlewares.push(middleware);
    }

    /// 中间件数量
    pub fn len(&self) -> usize {
        self.middlewares.len()
    }

    pub fn is_empty(&self) -> bool {
        self.middlewares.is_empty()
    }

    /// 获取所有中间件名称
    pub fn names(&self) -> Vec<&str> {
        self.middlewares.iter().map(|m| m.name()).collect()
    }

    /// 收集所有中间件提供的工具（按注册顺序，后注册的同名工具被忽略）
    pub fn collect_tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>> {
        self.middlewares.iter().flat_map(|m| m.collect_tools(cwd)).collect()
    }

    /// 顺序执行 before_agent 钩子
    pub async fn run_before_agent(&self, state: &mut S) -> AgentResult<()> {
        for middleware in &self.middlewares {
            middleware.before_agent(state).await?;
        }
        Ok(())
    }

    /// 顺序执行 before_tool 钩子（每个中间件可修改 tool_call）
    pub async fn run_before_tool(&self, state: &mut S, tool_call: ToolCall) -> AgentResult<ToolCall> {
        let mut current = tool_call;
        for middleware in &self.middlewares {
            current = middleware.before_tool(state, &current).await?;
        }
        Ok(current)
    }

    /// 顺序执行 after_tool 钩子
    pub async fn run_after_tool(
        &self,
        state: &mut S,
        tool_call: &ToolCall,
        result: &ToolResult,
    ) -> AgentResult<()> {
        for middleware in &self.middlewares {
            middleware.after_tool(state, tool_call, result).await?;
        }
        Ok(())
    }

    /// 顺序执行 after_agent 钩子（每个中间件可修改 output）
    pub async fn run_after_agent(
        &self,
        state: &mut S,
        output: AgentOutput,
    ) -> AgentResult<AgentOutput> {
        let mut current = output;
        for middleware in &self.middlewares {
            current = middleware.after_agent(state, &current).await?;
        }
        Ok(current)
    }

    /// 顺序执行 on_error 钩子
    pub async fn run_on_error(
        &self,
        state: &mut S,
        error: &crate::error::AgentError,
    ) -> AgentResult<()> {
        for middleware in &self.middlewares {
            middleware.on_error(state, error).await?;
        }
        Ok(())
    }
}

impl<S: State> Default for MiddlewareChain<S> {
    fn default() -> Self {
        Self::new()
    }
}
