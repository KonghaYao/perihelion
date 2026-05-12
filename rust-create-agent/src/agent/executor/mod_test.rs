use super::*;
use crate::agent::react::{AgentInput, Reasoning};
use crate::agent::state::AgentState;
use crate::messages::BaseMessage;
use crate::tools::BaseTool;
use std::time::{Duration, Instant};

// ─── Mock LLM：第一步返回两个并发工具调用，第二步返回最终答案 ───────────

struct TwoToolCallLLM;

#[async_trait::async_trait]
impl ReactLLM for TwoToolCallLLM {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        _tools: &[&dyn BaseTool],
    ) -> crate::error::AgentResult<Reasoning> {
        let has_tool_result = messages
            .iter()
            .any(|m| matches!(m, BaseMessage::Tool { .. }));
        if !has_tool_result {
            Ok(Reasoning::with_tools(
                "need both tools",
                vec![
                    ToolCall::new("id1", "slow_tool_a", serde_json::json!({})),
                    ToolCall::new("id2", "slow_tool_b", serde_json::json!({})),
                ],
            ))
        } else {
            Ok(Reasoning::with_answer("done", "parallel ok"))
        }
    }
}

// ─── Mock 工具：sleep 100ms ────────────────────────────────────────────────

struct SlowTool {
    tool_name: &'static str,
}

#[async_trait::async_trait]
impl BaseTool for SlowTool {
    fn name(&self) -> &str {
        self.tool_name
    }
    fn description(&self) -> &str {
        "slow test tool"
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    async fn invoke(
        &self,
        _input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(format!("{} done", self.tool_name))
    }
}

/// 验证两个各耗时 100ms 的工具并发执行，总耗时应 < 160ms（串行需 ≥ 200ms）
#[tokio::test]
async fn test_parallel_tool_execution() {
    let agent = ReActAgent::new(TwoToolCallLLM)
        .max_iterations(5)
        .register_tool(Box::new(SlowTool {
            tool_name: "slow_tool_a",
        }))
        .register_tool(Box::new(SlowTool {
            tool_name: "slow_tool_b",
        }));

    let mut state = AgentState::new("/tmp");
    let start = Instant::now();
    let output = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await
        .unwrap();
    let elapsed = start.elapsed();

    assert_eq!(output.text, "parallel ok");
    assert_eq!(output.tool_calls.len(), 2);
    assert!(
        elapsed < Duration::from_millis(160),
        "并行执行耗时 {:?}，应 < 160ms（串行需 ≥ 200ms）",
        elapsed
    );
}

/// 验证取消 token 触发时，工具以 error 收尾并返回 Interrupted
#[tokio::test]
async fn test_cancel_during_tool_execution() {
    struct HangingTool;
    #[async_trait::async_trait]
    impl BaseTool for HangingTool {
        fn name(&self) -> &str {
            "hanging_tool"
        }
        fn description(&self) -> &str {
            "hangs forever"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _input: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok("never".to_string())
        }
    }

    struct OneToolLLM;
    #[async_trait::async_trait]
    impl ReactLLM for OneToolLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            let has_tool = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool {
                Ok(Reasoning::with_tools(
                    "call tool",
                    vec![ToolCall::new("id1", "hanging_tool", serde_json::json!({}))],
                ))
            } else {
                Ok(Reasoning::with_answer("done", "ok"))
            }
        }
    }

    let cancel = CancellationToken::new();
    let agent = ReActAgent::new(OneToolLLM)
        .max_iterations(5)
        .register_tool(Box::new(HangingTool));

    // 50ms 后触发取消
    let token = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        token.cancel();
    });

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("go"), &mut state, Some(cancel))
        .await;

    assert!(matches!(result, Err(AgentError::Interrupted)));
    // 工具 error 结果已写入 state（可用于断点续跑）
    let has_tool_error = state
        .messages()
        .iter()
        .any(|m| matches!(m, BaseMessage::Tool { is_error: true, .. }));
    assert!(has_tool_error, "取消后工具 error 消息应已写入 state");
}

/// 验证 HITL 拒绝（ToolRejected）不终止 Agent，LLM 能收到拒绝原因后继续
#[tokio::test]
async fn test_tool_rejection_continues_loop() {
    use crate::middleware::r#trait::Middleware;

    struct RejectAllMiddleware;
    #[async_trait::async_trait]
    impl<S: State> Middleware<S> for RejectAllMiddleware {
        fn name(&self) -> &str {
            "RejectAllMiddleware"
        }
        async fn before_tool(&self, _state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
            Err(AgentError::ToolRejected {
                tool: tool_call.name.clone(),
                reason: "用户拒绝".to_string(),
            })
        }
    }

    // LLM：先调用工具，收到拒绝结果后返回最终答案
    struct TestLLM;
    #[async_trait::async_trait]
    impl ReactLLM for TestLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> AgentResult<Reasoning> {
            let has_tool_result = messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }));
            if !has_tool_result {
                Ok(Reasoning::with_tools(
                    "try tool",
                    vec![ToolCall::new(
                        "id1",
                        "Bash",
                        serde_json::json!({"command": "ls"}),
                    )],
                ))
            } else {
                Ok(Reasoning::with_answer("adjusted", "done after rejection"))
            }
        }
    }

    let agent = ReActAgent::new(TestLLM)
        .max_iterations(5)
        .add_middleware(Box::new(RejectAllMiddleware));

    let mut state = AgentState::new("/tmp");
    let output = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await
        .unwrap();

    assert_eq!(output.text, "done after rejection");
    // 拒绝结果应写入 state（is_error=true）
    let has_rejection = state
        .messages()
        .iter()
        .any(|m| matches!(m, BaseMessage::Tool { is_error: true, .. }));
    assert!(has_rejection, "拒绝结果应写入 state");
    // Agent 总工具调用记录中应有 1 条（被拒绝的）
    assert_eq!(output.tool_calls.len(), 1);
}

/// 验证 TextChunk 携带的 message_id 与前一条 MessageAdded(Ai) 的 id 一致
#[tokio::test]
async fn test_text_chunk_message_id() {
    use crate::agent::events::{AgentEvent, FnEventHandler};
    use std::sync::{Arc, Mutex};

    struct FinalAnswerLLM;
    #[async_trait::async_trait]
    impl ReactLLM for FinalAnswerLLM {
        async fn generate_reasoning(
            &self,
            _messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            Ok(Reasoning::with_answer("thinking", "final answer"))
        }
    }

    let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let agent = ReActAgent::new(FinalAnswerLLM)
        .max_iterations(3)
        .with_event_handler(Arc::new(FnEventHandler(move |event| {
            events_clone.lock().unwrap().push(event);
        })));

    let mut state = AgentState::new("/tmp");
    agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await
        .unwrap();

    let evs = events.lock().unwrap();

    // 找到 MessageAdded(Ai) 的 id（最终答案那条）
    let ai_msg_id = evs.iter().find_map(|e| {
        if let AgentEvent::MessageAdded(BaseMessage::Ai { id, tool_calls, .. }) = e {
            if tool_calls.is_empty() {
                Some(*id)
            } else {
                None
            }
        } else {
            None
        }
    });

    // 找到 TextChunk 的 message_id
    let chunk_msg_id = evs.iter().find_map(|e| {
        if let AgentEvent::TextChunk { message_id, .. } = e {
            Some(*message_id)
        } else {
            None
        }
    });

    assert!(ai_msg_id.is_some(), "应有 MessageAdded(Ai) 事件");
    assert!(chunk_msg_id.is_some(), "应有 TextChunk 事件");
    assert_eq!(
        ai_msg_id.unwrap(),
        chunk_msg_id.unwrap(),
        "TextChunk.message_id 应与 MessageAdded(Ai).id 相同"
    );
}

/// 验证 ToolStart/ToolEnd 携带的 message_id 与同轮次 MessageAdded(Ai) 的 id 一致
#[tokio::test]
async fn test_tool_message_id() {
    use crate::agent::events::{AgentEvent, FnEventHandler};
    use std::sync::{Arc, Mutex};

    struct OneToolLLM;
    #[async_trait::async_trait]
    impl ReactLLM for OneToolLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            if messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }))
            {
                Ok(Reasoning::with_answer("done", "ok"))
            } else {
                Ok(Reasoning::with_tools(
                    "call tool",
                    vec![ToolCall::new("tc1", "echo_tool", serde_json::json!({}))],
                ))
            }
        }
    }

    struct EchoTool;
    #[async_trait::async_trait]
    impl BaseTool for EchoTool {
        fn name(&self) -> &str {
            "echo_tool"
        }
        fn description(&self) -> &str {
            "echoes"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("echo".to_string())
        }
    }

    let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let agent = ReActAgent::new(OneToolLLM)
        .max_iterations(5)
        .register_tool(Box::new(EchoTool))
        .with_event_handler(Arc::new(FnEventHandler(move |event| {
            events_clone.lock().unwrap().push(event);
        })));

    let mut state = AgentState::new("/tmp");
    agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await
        .unwrap();

    let evs = events.lock().unwrap();

    // 找到第一个带工具调用的 MessageAdded(Ai) 的 id
    let ai_msg_id = evs.iter().find_map(|e| {
        if let AgentEvent::MessageAdded(BaseMessage::Ai { id, tool_calls, .. }) = e {
            if !tool_calls.is_empty() {
                Some(*id)
            } else {
                None
            }
        } else {
            None
        }
    });
    let tool_start_msg_id = evs.iter().find_map(|e| {
        if let AgentEvent::ToolStart { message_id, .. } = e {
            Some(*message_id)
        } else {
            None
        }
    });
    let tool_end_msg_id = evs.iter().find_map(|e| {
        if let AgentEvent::ToolEnd { message_id, .. } = e {
            Some(*message_id)
        } else {
            None
        }
    });

    assert!(
        ai_msg_id.is_some(),
        "应有带工具调用的 MessageAdded(Ai) 事件"
    );
    assert!(tool_start_msg_id.is_some(), "应有 ToolStart 事件");
    assert!(tool_end_msg_id.is_some(), "应有 ToolEnd 事件");
    assert_eq!(
        ai_msg_id.unwrap(),
        tool_start_msg_id.unwrap(),
        "ToolStart.message_id 应与 MessageAdded(Ai).id 相同"
    );
    assert_eq!(
        ai_msg_id.unwrap(),
        tool_end_msg_id.unwrap(),
        "ToolEnd.message_id 应与 MessageAdded(Ai).id 相同"
    );
}

/// 验证 with_system_prompt 注入的 system 消息位于消息列表第一位
#[tokio::test]
async fn test_system_prompt_is_first() {
    struct EchoLLM;
    #[async_trait::async_trait]
    impl ReactLLM for EchoLLM {
        async fn generate_reasoning(
            &self,
            _messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            Ok(Reasoning::with_answer("", "done"))
        }
    }

    let agent = ReActAgent::new(EchoLLM)
        .max_iterations(3)
        .with_system_prompt("system content here");

    let mut state = AgentState::new("/tmp");
    agent
        .execute(AgentInput::text("hi"), &mut state, None)
        .await
        .unwrap();

    let messages = state.messages();
    let first = messages.first().expect("应至少有一条消息");
    assert!(
        matches!(first, BaseMessage::System { .. }),
        "第一条消息应为 System，实际为: {:?}",
        first
    );
    assert!(
        first.content().contains("system content here"),
        "system 内容应包含注入文本"
    );
}

/// 验证不论其他中间件注册顺序如何，with_system_prompt 的 system 消息始终在最前
#[tokio::test]
async fn test_system_prompt_order_independent() {
    use crate::middleware::r#trait::Middleware;

    // 一个会在 before_agent 中 prepend 自己消息的中间件
    struct PrefixMiddleware;
    #[async_trait::async_trait]
    impl<S: State> Middleware<S> for PrefixMiddleware {
        fn name(&self) -> &str {
            "PrefixMiddleware"
        }
        async fn before_agent(&self, state: &mut S) -> AgentResult<()> {
            state.prepend_message(BaseMessage::system("middleware injected"));
            Ok(())
        }
    }

    struct EchoLLM;
    #[async_trait::async_trait]
    impl ReactLLM for EchoLLM {
        async fn generate_reasoning(
            &self,
            _messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            Ok(Reasoning::with_answer("", "done"))
        }
    }

    // 中间件在 with_system_prompt 之前注册——但 system prompt 应在最前
    let agent = ReActAgent::new(EchoLLM)
        .add_middleware(Box::new(PrefixMiddleware))
        .with_system_prompt("top level system");

    let mut state = AgentState::new("/tmp");
    agent
        .execute(AgentInput::text("hi"), &mut state, None)
        .await
        .unwrap();

    let messages = state.messages();
    let first = messages.first().expect("应至少有一条消息");
    assert!(
        first.content().contains("top level system"),
        "with_system_prompt 注入的消息应在最前，实际第一条: {:?}",
        first.content()
    );
}

/// 验证 TextChunk/ToolStart/ToolEnd 序列化后含 message_id 字段
#[test]
fn test_agent_event_message_id_serialization() {
    use crate::agent::events::AgentEvent;
    use crate::messages::MessageId;

    let mid = MessageId::new();

    let ev = AgentEvent::TextChunk {
        message_id: mid,
        chunk: "hello".to_string(),
    };
    let json = serde_json::to_value(&ev).unwrap();
    assert!(
        json["message_id"].is_string(),
        "TextChunk JSON 应含 message_id 字段"
    );
    assert_eq!(json["chunk"].as_str().unwrap(), "hello");

    let ev = AgentEvent::ToolStart {
        message_id: mid,
        tool_call_id: "tc1".to_string(),
        name: "Bash".to_string(),
        input: serde_json::json!({}),
    };
    let json = serde_json::to_value(&ev).unwrap();
    assert!(
        json["message_id"].is_string(),
        "ToolStart JSON 应含 message_id 字段"
    );

    let ev = AgentEvent::ToolEnd {
        message_id: mid,
        tool_call_id: "tc1".to_string(),
        name: "Bash".to_string(),
        output: "ok".to_string(),
        is_error: false,
    };
    let json = serde_json::to_value(&ev).unwrap();
    assert!(
        json["message_id"].is_string(),
        "ToolEnd JSON 应含 message_id 字段"
    );
}

/// 验证最终回答路径也会发出 StateSnapshot，确保多轮对话不丢失 AI 回复
#[tokio::test]
async fn test_state_snapshot_on_final_answer() {
    use crate::agent::events::{AgentEvent, FnEventHandler};
    use std::sync::{Arc, Mutex};

    struct FinalAnswerLLM;
    #[async_trait::async_trait]
    impl ReactLLM for FinalAnswerLLM {
        async fn generate_reasoning(
            &self,
            _messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            Ok(Reasoning::with_answer("thinking", "final answer"))
        }
    }

    let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let agent = ReActAgent::new(FinalAnswerLLM)
        .max_iterations(3)
        .with_event_handler(Arc::new(FnEventHandler(move |event| {
            events_clone.lock().unwrap().push(event);
        })));

    let mut state = AgentState::new("/tmp");
    agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await
        .unwrap();

    let evs = events.lock().unwrap();
    let snapshots: Vec<_> = evs
        .iter()
        .filter(|e| matches!(e, AgentEvent::StateSnapshot(_)))
        .collect();

    assert!(!snapshots.is_empty(), "最终回答路径应发出 StateSnapshot");

    // 最后一个 snapshot 应包含 AI 最终回答
    if let AgentEvent::StateSnapshot(msgs) = snapshots.last().unwrap() {
        let has_ai_text = msgs
            .iter()
            .any(|m| matches!(m, BaseMessage::Ai { tool_calls, .. } if tool_calls.is_empty()));
        assert!(has_ai_text, "StateSnapshot 应包含不带工具调用的 AI 消息");
    }
}

/// 验证达到最大迭代次数时返回 MaxIterationsExceeded 错误
#[tokio::test]
async fn test_max_iterations_exceeded() {
    struct AlwaysToolLLM;
    #[async_trait::async_trait]
    impl ReactLLM for AlwaysToolLLM {
        async fn generate_reasoning(
            &self,
            _messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            Ok(Reasoning::with_tools(
                "loop",
                vec![ToolCall::new("id1", "echo_tool", serde_json::json!({}))],
            ))
        }
    }

    struct EchoTool;
    #[async_trait::async_trait]
    impl BaseTool for EchoTool {
        fn name(&self) -> &str {
            "echo_tool"
        }
        fn description(&self) -> &str {
            "echoes"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("echo".to_string())
        }
    }

    let agent = ReActAgent::new(AlwaysToolLLM)
        .max_iterations(3)
        .register_tool(Box::new(EchoTool));

    let mut state = AgentState::new("/tmp");
    let result = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await;

    assert!(matches!(result, Err(AgentError::MaxIterationsExceeded(3))));
    // 1 human + 3*(ai + tool_result)
    assert_eq!(state.messages().len(), 7);
}

/// 验证两个工具调用通过批量 before_tools_batch 处理（HITL 批量审批路径）
#[tokio::test]
async fn test_batch_before_tools_execution() {
    use crate::agent::events::{AgentEvent, FnEventHandler};
    use std::sync::{Arc, Mutex};

    struct TwoToolLLM;
    #[async_trait::async_trait]
    impl ReactLLM for TwoToolLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            if messages
                .iter()
                .any(|m| matches!(m, BaseMessage::Tool { .. }))
            {
                Ok(Reasoning::with_answer("done", "ok"))
            } else {
                Ok(Reasoning::with_tools(
                    "need both",
                    vec![
                        ToolCall::new("id1", "tool_a", serde_json::json!({})),
                        ToolCall::new("id2", "tool_b", serde_json::json!({})),
                    ],
                ))
            }
        }
    }

    struct EchoTool {
        name_str: &'static str,
    }
    #[async_trait::async_trait]
    impl BaseTool for EchoTool {
        fn name(&self) -> &str {
            self.name_str
        }
        fn description(&self) -> &str {
            "echo"
        }
        fn parameters(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn invoke(
            &self,
            _: serde_json::Value,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok(format!("{} done", self.name_str))
        }
    }

    let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let agent = ReActAgent::new(TwoToolLLM)
        .max_iterations(5)
        .register_tool(Box::new(EchoTool { name_str: "tool_a" }))
        .register_tool(Box::new(EchoTool { name_str: "tool_b" }))
        .with_event_handler(Arc::new(FnEventHandler(move |event| {
            events_clone.lock().unwrap().push(event);
        })));

    let mut state = AgentState::new("/tmp");
    let output = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await
        .unwrap();

    assert_eq!(output.text, "ok");
    assert_eq!(output.tool_calls.len(), 2);

    let evs = events.lock().unwrap();
    let tool_starts: Vec<_> = evs
        .iter()
        .filter(|e| matches!(e, AgentEvent::ToolStart { .. }))
        .collect();
    assert_eq!(tool_starts.len(), 2, "应有 2 个 ToolStart 事件");
}

/// 验证 with_context_budget 设置后 executor 使用 ContextBudget 阈值
#[tokio::test]
async fn test_context_budget_wiring() {
    struct TokenLLM {
        input_tokens: u32,
        output_tokens: u32,
    }
    #[async_trait::async_trait]
    impl ReactLLM for TokenLLM {
        async fn generate_reasoning(
            &self,
            _messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            let mut r = Reasoning::with_answer("", "ok");
            r.usage = Some(crate::llm::types::TokenUsage {
                input_tokens: self.input_tokens,
                output_tokens: self.output_tokens,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            });
            Ok(r)
        }
    }

    // context_window=1000, warning_threshold=0.5 → 600/1000=60% > 50%
    let budget = crate::agent::token::ContextBudget::new(1000).with_warning_threshold(0.5);
    let agent = ReActAgent::new(TokenLLM {
        input_tokens: 400,
        output_tokens: 200,
    })
    .max_iterations(3)
    .with_context_budget(budget);
    let mut state = AgentState::new("/tmp");
    let output = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await
        .unwrap();
    assert_eq!(output.text, "ok");
    let t = state.token_tracker();
    assert_eq!(t.total_input_tokens, 400);
    assert_eq!(t.total_output_tokens, 200);
    assert_eq!(t.llm_call_count, 1);
}

/// 验证无 ContextBudget 时回退到硬编码 80% 阈值（向后兼容）
#[tokio::test]
async fn test_no_context_budget_fallback() {
    struct LowTokenLLM;
    #[async_trait::async_trait]
    impl ReactLLM for LowTokenLLM {
        async fn generate_reasoning(
            &self,
            _messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            let mut r = Reasoning::with_answer("", "ok");
            r.usage = Some(crate::llm::types::TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            });
            Ok(r)
        }
    }
    let agent = ReActAgent::new(LowTokenLLM).max_iterations(3);
    let mut state = AgentState::new("/tmp");
    let output = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await
        .unwrap();
    assert_eq!(output.text, "ok");
    assert_eq!(state.token_tracker().llm_call_count, 1);
}

/// 验证 ContextBudget 路径下 ContextWarning 事件被发出
#[tokio::test]
async fn test_context_budget_emits_warning_event() {
    use crate::agent::events::{AgentEvent, FnEventHandler};
    use std::sync::{Arc, Mutex};

    struct TokenLLM {
        input_tokens: u32,
        output_tokens: u32,
    }
    #[async_trait::async_trait]
    impl ReactLLM for TokenLLM {
        async fn generate_reasoning(
            &self,
            _messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            let mut r = Reasoning::with_answer("", "ok");
            r.usage = Some(crate::llm::types::TokenUsage {
                input_tokens: self.input_tokens,
                output_tokens: self.output_tokens,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            });
            Ok(r)
        }
    }

    let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(vec![]));
    let events_clone = events.clone();

    // context_window=1000, warning_threshold=0.5 → input=600/1000=60% > 50%
    let budget = crate::agent::token::ContextBudget::new(1000).with_warning_threshold(0.5);
    let agent = ReActAgent::new(TokenLLM {
        input_tokens: 600,
        output_tokens: 200,
    })
    .max_iterations(3)
    .with_context_budget(budget)
    .with_event_handler(Arc::new(FnEventHandler(move |ev| {
        events_clone.lock().unwrap().push(ev);
    })));

    let mut state = AgentState::new("/tmp");
    let output = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await
        .unwrap();
    assert_eq!(output.text, "ok");

    let evs = events.lock().unwrap();
    let warnings: Vec<_> = evs
        .iter()
        .filter(|e| matches!(e, AgentEvent::ContextWarning { .. }))
        .collect();
    assert_eq!(warnings.len(), 1, "ContextWarning 应在超过警告阈值时发出");
    if let AgentEvent::ContextWarning {
        used_tokens,
        total_tokens,
        percentage,
    } = warnings[0]
    {
        assert_eq!(*used_tokens, 600, "used_tokens = input = 600");
        assert_eq!(*total_tokens, 1000, "total_tokens = budget.context_window");
        assert!((*percentage - 60.0).abs() < 1.0, "percentage ≈ 60%");
    }
}

/// 验证无 ContextBudget 时回退路径也发出 ContextWarning 事件
#[tokio::test]
async fn test_fallback_path_emits_warning_event() {
    use crate::agent::events::{AgentEvent, FnEventHandler};
    use std::sync::{Arc, Mutex};

    struct HighTokenLLM;
    #[async_trait::async_trait]
    impl ReactLLM for HighTokenLLM {
        async fn generate_reasoning(
            &self,
            _messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            let mut r = Reasoning::with_answer("", "ok");
            // context_window 默认 200K，input=170K → 170K/200K = 85% > 80% 硬编码阈值
            r.usage = Some(crate::llm::types::TokenUsage {
                input_tokens: 170000,
                output_tokens: 80000,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            });
            Ok(r)
        }
    }

    let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(vec![]));
    let events_clone = events.clone();

    let agent = ReActAgent::new(HighTokenLLM)
        .max_iterations(3)
        .with_event_handler(Arc::new(FnEventHandler(move |ev| {
            events_clone.lock().unwrap().push(ev);
        })));

    let mut state = AgentState::new("/tmp");
    let output = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await
        .unwrap();
    assert_eq!(output.text, "ok");

    let evs = events.lock().unwrap();
    let warnings: Vec<_> = evs
        .iter()
        .filter(|e| matches!(e, AgentEvent::ContextWarning { .. }))
        .collect();
    assert_eq!(
        warnings.len(),
        1,
        "无 budget 时回退路径也应发出 ContextWarning"
    );
    if let AgentEvent::ContextWarning {
        used_tokens,
        total_tokens: _,
        percentage,
    } = warnings[0]
    {
        assert_eq!(*used_tokens, 170000, "used_tokens = input = 170K");
        assert!((*percentage - 85.0).abs() < 1.0, "percentage ≈ 85%");
    }
}

/// 验证低 token 用量时不发出 ContextWarning
#[tokio::test]
async fn test_low_usage_no_warning_event() {
    use crate::agent::events::{AgentEvent, FnEventHandler};
    use std::sync::{Arc, Mutex};

    struct LowTokenLLM;
    #[async_trait::async_trait]
    impl ReactLLM for LowTokenLLM {
        async fn generate_reasoning(
            &self,
            _messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> crate::error::AgentResult<Reasoning> {
            let mut r = Reasoning::with_answer("", "ok");
            r.usage = Some(crate::llm::types::TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            });
            Ok(r)
        }
    }

    let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::new(Mutex::new(vec![]));
    let events_clone = events.clone();

    let agent = ReActAgent::new(LowTokenLLM)
        .max_iterations(3)
        .with_event_handler(Arc::new(FnEventHandler(move |ev| {
            events_clone.lock().unwrap().push(ev);
        })));

    let mut state = AgentState::new("/tmp");
    let output = agent
        .execute(AgentInput::text("go"), &mut state, None)
        .await
        .unwrap();
    assert_eq!(output.text, "ok");

    let evs = events.lock().unwrap();
    // LLM 必然有 LlmCallEnd，但 low usage 不触发 ContextWarning
    let has_warning = evs
        .iter()
        .any(|e| matches!(e, AgentEvent::ContextWarning { .. }));
    assert!(!has_warning, "低 token 用量不应发出 ContextWarning");
}
