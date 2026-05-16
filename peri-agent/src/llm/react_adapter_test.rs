    struct MockBaseModel {
        id: &'static str,
        window: u32,
    }
    #[async_trait::async_trait]
    impl super::super::BaseModel for MockBaseModel {
        async fn invoke(
            &self,
            _: super::super::types::LlmRequest,
        ) -> crate::error::AgentResult<super::super::types::LlmResponse> {
            unimplemented!()
        }
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_id(&self) -> &str {
            self.id
        }
        fn context_window(&self) -> u32 {
            self.window
        }
    }

    #[test]
    fn test_context_window_delegates_to_model() {
        let llm = BaseModelReactLLM::new(Box::new(MockBaseModel {
            id: "any-model",
            window: 128_000,
        }));
        assert_eq!(llm.context_window(), 128_000);
    }

    #[test]
    fn test_context_window_default_from_trait() {
        let llm = BaseModelReactLLM::new(Box::new(MockBaseModel {
            id: "unknown",
            window: 200_000,
        }));
        assert_eq!(llm.context_window(), 200_000);
    }

    /// 验证：当 stop_reason == EndTurn 但响应含 tool_use blocks 时，
    /// generate_reasoning 仍走工具调用路径（而非最终回答路径），
    /// 防止 source_message 中的 tool_use 成为孤儿导致 API 400。
    #[tokio::test]
    async fn test_stop_reason_mismatch_with_tool_use_blocks_treated_as_tool_call() {
        use super::*;
        use crate::llm::types::{LlmResponse, StopReason};
        use crate::messages::BaseMessage;

        // 模拟 DeepSeek 返回 stop_reason=end_turn 但内容含 tool_use
        struct DeepSeekStopReasonMock;
        #[async_trait::async_trait]
        impl super::super::BaseModel for DeepSeekStopReasonMock {
            async fn invoke(
                &self,
                _: super::super::types::LlmRequest,
            ) -> crate::error::AgentResult<super::super::types::LlmResponse> {
                let msg = BaseMessage::ai_with_tool_calls(
                    crate::messages::MessageContent::text("I'll write that file"),
                    vec![crate::messages::ToolCallRequest::new(
                        "call_00_abc".to_string(),
                        "Write".to_string(),
                        serde_json::json!({"file_path": "/tmp/test.txt", "content": "hello"}),
                    )],
                );
                Ok(LlmResponse {
                    message: msg,
                    stop_reason: StopReason::EndTurn, // 关键：stop_reason 不是 ToolUse
                    usage: None,
                    request_id: None,
                })
            }
            fn provider_name(&self) -> &str {
                "deepseek"
            }
            fn model_id(&self) -> &str {
                "deepseek-chat"
            }
            fn context_window(&self) -> u32 {
                128_000
            }
        }

        let llm = BaseModelReactLLM::new(Box::new(DeepSeekStopReasonMock));
        let tools: Vec<&dyn crate::tools::BaseTool> = vec![];
        let result = llm
            .generate_reasoning(&[], &tools, None)
            .await
            .expect("generate_reasoning 应成功");

        // 关键断言：即使 stop_reason 是 EndTurn，tool_use blocks 存在时应走工具调用路径
        assert!(
            result.needs_tool_call(),
            "stop_reason=EndTurn 但内容含 tool_use 时，应走工具调用路径，实际走了最终回答路径"
        );
        assert_eq!(result.tool_calls.len(), 1, "应提取到 1 个工具调用");
        assert_eq!(result.tool_calls[0].name, "Write");
        assert_eq!(result.tool_calls[0].id, "call_00_abc");
    }

    /// 验证：stop_reason == EndTurn 且内容不含 tool_use 时，正常走最终回答路径。
    #[tokio::test]
    async fn test_stop_reason_end_turn_without_tool_use_treated_as_answer() {
        use super::*;
        use crate::llm::types::{LlmResponse, StopReason};
        use crate::messages::BaseMessage;

        struct NormalEndTurnMock;
        #[async_trait::async_trait]
        impl super::super::BaseModel for NormalEndTurnMock {
            async fn invoke(
                &self,
                _: super::super::types::LlmRequest,
            ) -> crate::error::AgentResult<super::super::types::LlmResponse> {
                let msg = BaseMessage::ai("This is a normal response");
                Ok(LlmResponse {
                    message: msg,
                    stop_reason: StopReason::EndTurn,
                    usage: None,
                    request_id: None,
                })
            }
            fn provider_name(&self) -> &str {
                "mock"
            }
            fn model_id(&self) -> &str {
                "mock-model"
            }
            fn context_window(&self) -> u32 {
                128_000
            }
        }

        let llm = BaseModelReactLLM::new(Box::new(NormalEndTurnMock));
        let tools: Vec<&dyn crate::tools::BaseTool> = vec![];
        let result = llm
            .generate_reasoning(&[], &tools, None)
            .await
            .expect("generate_reasoning 应成功");

        assert!(
            !result.needs_tool_call(),
            "stop_reason=EndTurn 且无 tool_use 时，应走最终回答路径"
        );
        assert_eq!(result.final_answer.as_deref(), Some("This is a normal response"));
    }
