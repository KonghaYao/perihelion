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

    /// 收集所有中间件提供的工具（按注册顺序，后注册的同名工具覆盖先注册的）
    pub fn collect_tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>> {
        self.middlewares
            .iter()
            .flat_map(|m| m.collect_tools(cwd))
            .collect()
    }

    /// 顺序执行 before_agent 钩子
    pub async fn run_before_agent(&self, state: &mut S) -> AgentResult<()> {
        for middleware in &self.middlewares {
            middleware.before_agent(state).await?;
        }
        Ok(())
    }

    /// 顺序执行 before_tool 钩子（每个中间件可修改 tool_call）
    pub async fn run_before_tool(
        &self,
        state: &mut S,
        tool_call: ToolCall,
    ) -> AgentResult<ToolCall> {
        let mut current = tool_call;
        for middleware in &self.middlewares {
            current = middleware.before_tool(state, &current).await?;
        }
        Ok(current)
    }

    /// 批量执行 before_tool 钩子（优化路径）
    ///
    /// 对每个中间件依次调用其 `before_tools_batch` 方法。
    /// 中间件的 batch 实现可将多个 tool call 合并处理（如 HITL 批量审批）。
    /// 当所有中间件都使用默认逐条实现时，效果等同于逐个调用 `run_before_tool`。
    ///
    /// 返回结果按输入顺序一一对应。若某个中间件返回非 `ToolRejected` 错误，
    /// 链式处理中断，后续中间件不再执行，其余位置填充相同错误。
    pub async fn run_before_tools_batch(
        &self,
        state: &mut S,
        calls: Vec<ToolCall>,
    ) -> Vec<AgentResult<ToolCall>> {
        let mut results: Vec<AgentResult<ToolCall>> = calls.into_iter().map(Ok).collect();

        for middleware in &self.middlewares {
            let current_calls: Vec<ToolCall> = results
                .iter()
                .filter_map(|r| r.as_ref().ok().cloned())
                .collect();
            if current_calls.is_empty() {
                break;
            }

            let batch_results = middleware.before_tools_batch(state, &current_calls).await;

            // 将 batch 结果按位置回写（消费结果，避免 AgentError::Clone 要求）
            let mut batch_iter = batch_results.into_iter();
            for result in results.iter_mut() {
                if result.is_ok() {
                    if let Some(batch_result) = batch_iter.next() {
                        *result = batch_result;
                    }
                }
            }
        }

        results
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::state::AgentState;
    use crate::error::{AgentError, AgentResult};
    use crate::middleware::r#trait::Middleware;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    /// 记录调用顺序的中间件
    struct OrderRecorder {
        name: String,
        log: Arc<Mutex<Vec<String>>>,
    }

    impl OrderRecorder {
        fn new(name: &str, log: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                name: name.to_string(),
                log,
            }
        }
    }

    #[async_trait]
    impl Middleware<AgentState> for OrderRecorder {
        fn name(&self) -> &str {
            &self.name
        }

        async fn before_agent(&self, _state: &mut AgentState) -> AgentResult<()> {
            self.log
                .lock()
                .unwrap()
                .push(format!("{}.before_agent", self.name));
            Ok(())
        }

        async fn before_tool(
            &self,
            _state: &mut AgentState,
            tool_call: &ToolCall,
        ) -> AgentResult<ToolCall> {
            self.log
                .lock()
                .unwrap()
                .push(format!("{}.before_tool", self.name));
            Ok(tool_call.clone())
        }

        async fn after_tool(
            &self,
            _state: &mut AgentState,
            _tool_call: &ToolCall,
            _result: &ToolResult,
        ) -> AgentResult<()> {
            self.log
                .lock()
                .unwrap()
                .push(format!("{}.after_tool", self.name));
            Ok(())
        }
    }

    /// 修改 ToolCall 的中间件（用于验证 before_tool 链式传播）
    struct InputModifier {
        suffix: String,
    }

    #[async_trait]
    impl Middleware<AgentState> for InputModifier {
        fn name(&self) -> &str {
            "InputModifier"
        }

        async fn before_tool(
            &self,
            _state: &mut AgentState,
            tool_call: &ToolCall,
        ) -> AgentResult<ToolCall> {
            let mut modified = tool_call.clone();
            let new_name = format!("{}{}", tool_call.name, self.suffix);
            modified.name = new_name;
            Ok(modified)
        }
    }

    /// 总是返回错误的中间件（用于验证短路行为）
    struct FailMiddleware;

    #[async_trait]
    impl Middleware<AgentState> for FailMiddleware {
        fn name(&self) -> &str {
            "FailMiddleware"
        }

        async fn before_agent(&self, _state: &mut AgentState) -> AgentResult<()> {
            Err(AgentError::MiddlewareError {
                middleware: "FailMiddleware".to_string(),
                reason: "intentional failure".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn test_multiple_middlewares_sequential_order() {
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut chain = MiddlewareChain::<AgentState>::new();
        chain.add(Box::new(OrderRecorder::new("A", Arc::clone(&log))));
        chain.add(Box::new(OrderRecorder::new("B", Arc::clone(&log))));
        chain.add(Box::new(OrderRecorder::new("C", Arc::clone(&log))));

        let mut state = AgentState::new("/tmp");
        chain.run_before_agent(&mut state).await.unwrap();

        let calls = log.lock().unwrap().clone();
        assert_eq!(
            calls,
            vec!["A.before_agent", "B.before_agent", "C.before_agent"]
        );
    }

    #[tokio::test]
    async fn test_error_short_circuits_chain() {
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut chain = MiddlewareChain::<AgentState>::new();
        chain.add(Box::new(OrderRecorder::new("A", Arc::clone(&log))));
        chain.add(Box::new(FailMiddleware));
        chain.add(Box::new(OrderRecorder::new("B", Arc::clone(&log))));

        let mut state = AgentState::new("/tmp");
        let result = chain.run_before_agent(&mut state).await;

        assert!(result.is_err(), "应该返回错误");
        // B.before_agent 不应被执行
        let calls = log.lock().unwrap().clone();
        assert_eq!(calls, vec!["A.before_agent"]);
    }

    #[tokio::test]
    async fn test_before_tool_modification_propagates() {
        let mut chain = MiddlewareChain::<AgentState>::new();
        chain.add(Box::new(InputModifier {
            suffix: "_modified".to_string(),
        }));

        let mut state = AgentState::new("/tmp");
        let original = ToolCall::new("id1", "my_tool", serde_json::json!({}));
        let result = chain.run_before_tool(&mut state, original).await.unwrap();

        assert_eq!(result.name, "my_tool_modified");
    }

    #[tokio::test]
    async fn test_before_tool_chained_modifications() {
        let mut chain = MiddlewareChain::<AgentState>::new();
        chain.add(Box::new(InputModifier {
            suffix: "_a".to_string(),
        }));
        chain.add(Box::new(InputModifier {
            suffix: "_b".to_string(),
        }));

        let mut state = AgentState::new("/tmp");
        let original = ToolCall::new("id1", "tool", serde_json::json!({}));
        let result = chain.run_before_tool(&mut state, original).await.unwrap();

        assert_eq!(result.name, "tool_a_b");
    }

    #[tokio::test]
    async fn test_empty_chain_runs_ok() {
        let chain = MiddlewareChain::<AgentState>::new();
        let mut state = AgentState::new("/tmp");
        chain.run_before_agent(&mut state).await.unwrap();

        let original = ToolCall::new("id", "tool", serde_json::json!({}));
        let result = chain
            .run_before_tool(&mut state, original.clone())
            .await
            .unwrap();
        assert_eq!(result.name, original.name);
    }

    #[tokio::test]
    async fn test_after_tool_sequential_order() {
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut chain = MiddlewareChain::<AgentState>::new();
        chain.add(Box::new(OrderRecorder::new("A", Arc::clone(&log))));
        chain.add(Box::new(OrderRecorder::new("B", Arc::clone(&log))));

        let mut state = AgentState::new("/tmp");
        let call = ToolCall::new("id", "tool", serde_json::json!({}));
        let result = ToolResult {
            tool_call_id: "id".to_string(),
            tool_name: "tool".to_string(),
            output: "ok".to_string(),
            is_error: false,
        };
        chain
            .run_after_tool(&mut state, &call, &result)
            .await
            .unwrap();

        let calls = log.lock().unwrap().clone();
        assert_eq!(calls, vec!["A.after_tool", "B.after_tool"]);
    }

    /// 批量工具调用：一个中间件批准、下一个中间件拒绝（混合结果）
    #[tokio::test]
    async fn test_before_tools_batch_mixed_approval() {
        // 第一个中间件：所有工具加 _a 后缀
        struct SuffixA;
        #[async_trait]
        impl Middleware<AgentState> for SuffixA {
            fn name(&self) -> &str {
                "SuffixA"
            }
            async fn before_tool(
                &self,
                _state: &mut AgentState,
                tc: &ToolCall,
            ) -> AgentResult<ToolCall> {
                let mut m = tc.clone();
                m.name = format!("{}{}", tc.name, "_a");
                Ok(m)
            }
        }

        // 第二个中间件：第二个工具调用返回 ToolRejected，第一个和第三个放行
        struct RejectSecond;
        #[async_trait]
        impl Middleware<AgentState> for RejectSecond {
            fn name(&self) -> &str {
                "RejectSecond"
            }
            async fn before_tools_batch(
                &self,
                _state: &mut AgentState,
                calls: &[ToolCall],
            ) -> Vec<AgentResult<ToolCall>> {
                calls
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        if i == 1 {
                            Err(AgentError::ToolRejected {
                                tool: c.name.clone(),
                                reason: "拒绝第二个".to_string(),
                            })
                        } else {
                            Ok(c.clone())
                        }
                    })
                    .collect()
            }
        }

        let mut chain = MiddlewareChain::<AgentState>::new();
        chain.add(Box::new(SuffixA));
        chain.add(Box::new(RejectSecond));
        let mut state = AgentState::new("/tmp");

        let calls = vec![
            ToolCall::new("id1", "tool1", serde_json::json!({})),
            ToolCall::new("id2", "tool2", serde_json::json!({})),
            ToolCall::new("id3", "tool3", serde_json::json!({})),
        ];
        let results = chain.run_before_tools_batch(&mut state, calls).await;

        assert_eq!(results.len(), 3);
        // 第一个：通过，名称被 SuffixA 修改为 tool1_a
        assert!(results[0].is_ok());
        assert_eq!(results[0].as_ref().unwrap().name, "tool1_a");
        // 第二个：被 RejectSecond 拒绝
        assert!(
            matches!(&results[1], Err(AgentError::ToolRejected { tool, .. }) if tool == "tool2_a")
        );
        // 第三个：通过
        assert!(results[2].is_ok());
        assert_eq!(results[2].as_ref().unwrap().name, "tool3_a");
    }

    /// 批量工具调用：所有中间件使用默认逐条实现，结果应与逐个调用一致
    #[tokio::test]
    async fn test_before_tools_batch_equivalent_to_individual() {
        struct SuffixX;
        #[async_trait]
        impl Middleware<AgentState> for SuffixX {
            fn name(&self) -> &str {
                "SuffixX"
            }
            async fn before_tool(
                &self,
                _state: &mut AgentState,
                tc: &ToolCall,
            ) -> AgentResult<ToolCall> {
                let mut m = tc.clone();
                m.name = format!("{}{}", tc.name, "_x");
                Ok(m)
            }
        }

        let mut chain = MiddlewareChain::<AgentState>::new();
        chain.add(Box::new(SuffixX));
        let mut state = AgentState::new("/tmp");

        let calls = vec![
            ToolCall::new("id1", "t1", serde_json::json!({})),
            ToolCall::new("id2", "t2", serde_json::json!({})),
        ];

        let batch_results = chain
            .run_before_tools_batch(&mut state, calls.clone())
            .await;
        assert_eq!(batch_results.len(), 2);
        assert_eq!(batch_results[0].as_ref().unwrap().name, "t1_x");
        assert_eq!(batch_results[1].as_ref().unwrap().name, "t2_x");
    }
}
