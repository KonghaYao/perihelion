use async_trait::async_trait;
use rust_create_agent::prelude::*;

// ── 辅助工具（实现 BaseTool trait） ────────────────────────────────────────────

/// 提供 echo 工具的中间件（用于测试 collect_tools 自动注册流程）
struct EchoMiddleware;

#[async_trait]
impl<S: rust_create_agent::agent::state::State> rust_create_agent::middleware::r#trait::Middleware<S> for EchoMiddleware {
    fn name(&self) -> &str { "EchoMiddleware" }

    fn collect_tools(&self, _cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![Box::new(EchoTool)]
    }
}

/// 覆盖 echo 工具的中间件（返回不同输出，用于测试优先级）
struct OverrideEchoTool;

#[async_trait]
impl BaseTool for OverrideEchoTool {
    fn name(&self) -> &str { "echo" }
    fn description(&self) -> &str { "Override echo" }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": { "text": { "type": "string" } } })
    }
    async fn invoke(&self, input: serde_json::Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(format!("override: {}", input["text"].as_str().unwrap_or("")))
    }
}

struct EchoTool;

#[async_trait]
impl BaseTool for EchoTool {
    fn name(&self) -> &str { "echo" }
    fn description(&self) -> &str { "Echoes the input back" }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": { "text": { "type": "string" } } })
    }
    async fn invoke(&self, input: serde_json::Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Ok(format!("echo: {}", input["text"].as_str().unwrap_or("")))
    }
}

struct FailingTool;

#[async_trait]
impl BaseTool for FailingTool {
    fn name(&self) -> &str { "fail" }
    fn description(&self) -> &str { "Always fails" }
    fn parameters(&self) -> serde_json::Value { serde_json::json!({}) }
    async fn invoke(&self, _input: serde_json::Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        Err("intentional failure".into())
    }
}

// ── 测试 ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_agent_simple_answer() {
    let agent = AgentExecutor::new(MockLLM::always_answer("simple answer"));
    let mut state = AgentState::new("/test");

    let output = agent.execute(AgentInput::text("hello"), &mut state).await.unwrap();

    assert_eq!(output.text, "simple answer");
    assert_eq!(output.steps, 1);
    assert!(output.tool_calls.is_empty());
}

#[tokio::test]
async fn test_agent_tool_call_then_answer() {
    let llm = MockLLM::tool_then_answer(
        "echo",
        serde_json::json!({ "text": "hello world" }),
        "The echo said: hello world",
    );

    let agent = AgentExecutor::new(llm).register_tool(Box::new(EchoTool));
    let mut state = AgentState::new("/test");

    let output = agent.execute(AgentInput::text("echo something"), &mut state).await.unwrap();

    assert_eq!(output.text, "The echo said: hello world");
    assert_eq!(output.tool_calls.len(), 1);
    assert_eq!(output.tool_calls[0].0.name, "echo");
    assert_eq!(output.tool_calls[0].1.output, "echo: hello world");
}

#[tokio::test]
async fn test_agent_tool_not_found() {
    let llm = MockLLM::tool_then_answer("nonexistent_tool", serde_json::json!({}), "done");
    let agent = AgentExecutor::new(llm);
    let mut state = AgentState::new("/test");

    let result = agent.execute(AgentInput::text("use missing tool"), &mut state).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        AgentError::ToolNotFound(name) => assert_eq!(name, "nonexistent_tool"),
        e => panic!("Unexpected error: {e}"),
    }
}

#[tokio::test]
async fn test_agent_failing_tool_is_recorded() {
    let llm = MockLLM::tool_then_answer("fail", serde_json::json!({}), "got error but continuing");
    let agent = AgentExecutor::new(llm).register_tool(Box::new(FailingTool));
    let mut state = AgentState::new("/test");

    let output = agent.execute(AgentInput::text("try failing tool"), &mut state).await.unwrap();

    assert_eq!(output.tool_calls.len(), 1);
    assert!(output.tool_calls[0].1.is_error);
}

#[tokio::test]
async fn test_agent_max_iterations() {
    let calls: Vec<Reasoning> = (0..20)
        .map(|_| Reasoning::with_tools(
            "still thinking",
            vec![ToolCall::new("c", "echo", serde_json::json!({"text":"hi"}))],
        ))
        .collect();

    let agent = AgentExecutor::new(MockLLM::new(calls))
        .max_iterations(3)
        .register_tool(Box::new(EchoTool));
    let mut state = AgentState::new("/test");

    let result = agent.execute(AgentInput::text("loop forever"), &mut state).await;
    assert!(matches!(result, Err(AgentError::MaxIterationsExceeded(3))));
}

// ── 中间件工具自注册测试 ──────────────────────────────────────────────────────

/// 验证通过 add_middleware 自动注册工具（无需手动 register_tool）
#[tokio::test]
async fn test_middleware_auto_registers_tools() {
    let llm = MockLLM::tool_then_answer(
        "echo",
        serde_json::json!({ "text": "from middleware" }),
        "got: echo: from middleware",
    );

    // 只通过 add_middleware 注册中间件，不手动调用 register_tool
    let agent = AgentExecutor::new(llm)
        .add_middleware(Box::new(EchoMiddleware));
    let mut state = AgentState::new("/test");

    let output = agent.execute(AgentInput::text("use echo"), &mut state).await.unwrap();

    assert_eq!(output.text, "got: echo: from middleware");
    assert_eq!(output.tool_calls.len(), 1);
    assert_eq!(output.tool_calls[0].1.output, "echo: from middleware");
    assert!(!output.tool_calls[0].1.is_error);
}

/// 验证手动 register_tool 的同名工具优先于中间件提供的工具
#[tokio::test]
async fn test_manual_tool_overrides_middleware_tool() {
    let llm = MockLLM::tool_then_answer(
        "echo",
        serde_json::json!({ "text": "priority test" }),
        "done",
    );

    // EchoMiddleware 提供 echo 工具，但 register_tool(OverrideEchoTool) 应优先
    let agent = AgentExecutor::new(llm)
        .add_middleware(Box::new(EchoMiddleware))
        .register_tool(Box::new(OverrideEchoTool));
    let mut state = AgentState::new("/test");

    let output = agent.execute(AgentInput::text("echo with override"), &mut state).await.unwrap();

    assert_eq!(output.tool_calls.len(), 1);
    // 应使用 OverrideEchoTool 的输出，而非 EchoTool 的输出
    assert_eq!(output.tool_calls[0].1.output, "override: priority test");
}

#[tokio::test]
async fn test_state_messages_grow() {
    let agent = AgentExecutor::new(MockLLM::always_answer("ok"));
    let mut state = AgentState::new("/test");

    assert_eq!(state.messages().len(), 0);
    agent.execute(AgentInput::text("hello"), &mut state).await.unwrap();

    // user message + assistant answer
    assert_eq!(state.messages().len(), 2);
}
