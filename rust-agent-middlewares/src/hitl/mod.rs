use std::sync::Arc;

use async_trait::async_trait;
use rust_create_agent::agent::react::ToolCall;
use rust_create_agent::agent::state::State;
use rust_create_agent::error::{AgentError, AgentResult};
use rust_create_agent::interaction::{
    ApprovalDecision, ApprovalItem, InteractionContext, InteractionResponse, UserInteractionBroker,
};
use rust_create_agent::middleware::r#trait::Middleware;

pub mod auto_classifier;
pub mod shared_mode;

pub use auto_classifier::{AutoClassifier, Classification, LlmAutoClassifier};
pub use rust_create_agent::hitl::{BatchItem, HitlDecision};
pub use shared_mode::{PermissionMode, SharedPermissionMode};

// ─── YOLO 模式检测 ─────────────────────────────────────────────────────────────

/// 检测是否处于 YOLO 模式（默认启用）
///
/// - `YOLO_MODE` 未设置或为 `true`/`1` → YOLO（跳过审批）
/// - `YOLO_MODE=false`/`0` → 启用 HITL 审批
pub fn is_yolo_mode() -> bool {
    std::env::var("YOLO_MODE")
        .map(|v| !v.eq_ignore_ascii_case("false") && v != "0")
        .unwrap_or(true)
}

// ─── 默认规则 ──────────────────────────────────────────────────────────────────

/// 默认敏感工具判断规则
///
/// - `bash`：所有 bash 命令
/// - `Write`：文件写入
/// - `Edit`：文件编辑
/// - `folder_operations`：目录操作
/// - `launch_agent`：子 Agent 委派（子 Agent 不含 HITL，可传递绕过审批）
pub fn default_requires_approval(tool_name: &str) -> bool {
    tool_name == "Bash"
        || tool_name == "folder_operations"
        || tool_name == "Agent"
        || tool_name == "Write"
        || tool_name == "Edit"
        || tool_name.starts_with("delete_")
        || tool_name.starts_with("rm_")
}

/// 判断工具是否为文件编辑类工具（AcceptEdits 模式使用）
///
/// `Write`、`Edit`、`folder_operations` 归类为编辑工具，在 AcceptEdits 模式下自动放行。
/// `Bash`、`Agent`、`delete_*`、`rm_*` 不属于编辑工具，仍需审批。
pub fn is_edit_tool(tool_name: &str) -> bool {
    tool_name == "Write"
        || tool_name == "Edit"
        || tool_name == "folder_operations"
}

// ─── HumanInTheLoopMiddleware ──────────────────────────────────────────────────

/// HumanInTheLoopMiddleware — 敏感工具调用前需用户确认
///
/// 在 `before_tool` 时拦截工具调用，通过注入的 [`UserInteractionBroker`] 请求用户审批。
///
/// # HITL 模式
/// 通过 `HumanInTheLoopMiddleware::new(...)` 或环境变量 `YOLO_MODE=false` 启用审批。
pub struct HumanInTheLoopMiddleware {
    broker: Option<Arc<dyn UserInteractionBroker>>,
    requires_approval: fn(&str) -> bool,
    /// 共享权限模式（动态切换），None 时走原有 Some/None broker 逻辑（向后兼容）
    mode: Option<Arc<SharedPermissionMode>>,
    /// Auto 模式的 LLM 分类器，仅在 mode=Auto 时使用
    auto_classifier: Option<Arc<dyn AutoClassifier>>,
}

impl HumanInTheLoopMiddleware {
    /// 创建启用的 HITL 中间件，使用注入的 broker
    pub fn new(
        broker: Arc<dyn UserInteractionBroker>,
        requires_approval: fn(&str) -> bool,
    ) -> Self {
        Self {
            broker: Some(broker),
            requires_approval,
            mode: None,
            auto_classifier: None,
        }
    }

    /// YOLO 模式：所有工具调用直接放行
    pub fn disabled() -> Self {
        Self {
            broker: None,
            requires_approval: default_requires_approval,
            mode: None,
            auto_classifier: None,
        }
    }

    /// 从环境变量决定是否启用（默认 YOLO；`YOLO_MODE=false` 则启用审批）
    pub fn from_env(
        broker: Arc<dyn UserInteractionBroker>,
        requires_approval: fn(&str) -> bool,
    ) -> Self {
        if is_yolo_mode() {
            Self::disabled()
        } else {
            Self::new(broker, requires_approval)
        }
    }

    /// 创建带共享权限模式的 HITL 中间件
    pub fn with_shared_mode(
        broker: Arc<dyn UserInteractionBroker>,
        requires_approval: fn(&str) -> bool,
        mode: Arc<SharedPermissionMode>,
        auto_classifier: Option<Arc<dyn AutoClassifier>>,
    ) -> Self {
        Self {
            broker: Some(broker),
            requires_approval,
            mode: Some(mode),
            auto_classifier,
        }
    }
}

/// 将 `ApprovalDecision` 映射为 `AgentResult<ToolCall>`
fn apply_decision(call: &ToolCall, decision: ApprovalDecision) -> AgentResult<ToolCall> {
    match decision {
        ApprovalDecision::Approve => Ok(call.clone()),
        ApprovalDecision::Edit { new_input } => {
            let mut modified = call.clone();
            modified.input = new_input;
            Ok(modified)
        }
        ApprovalDecision::Reject { reason } => Err(AgentError::ToolRejected {
            tool: call.name.clone(),
            reason,
        }),
        ApprovalDecision::Respond { message } => Err(AgentError::ToolRejected {
            tool: call.name.clone(),
            reason: message,
        }),
    }
}

impl HumanInTheLoopMiddleware {
    /// 批量处理一批工具调用：收集所有需要审批的项，一次性弹窗，返回每个 call 的处理结果
    pub async fn process_batch(&self, calls: &[ToolCall]) -> Vec<AgentResult<ToolCall>> {
        let mut results: Vec<AgentResult<ToolCall>> = Vec::with_capacity(calls.len());

        // 快照当前 mode，确保整个批处理内评估一致（避免迭代过程中 mode 被外部修改）
        let mode_snapshot = self.mode.clone();

        for (i, call) in calls.iter().enumerate() {
            // 非敏感工具 → 直接放行
            if !(self.requires_approval)(&call.name) {
                results.push(Ok(call.clone()));
                continue;
            }

            // 有 mode → 使用快照模式决策
            if let Some(mode) = &mode_snapshot {
                results.push(self.decide_by_mode(mode, call).await);
                continue;
            }

            // 无 mode 且无 broker → 放行
            let Some(broker) = &self.broker else {
                results.push(Ok(call.clone()));
                continue;
            };

            // 无 mode 但有 broker → 收集后批量弹窗
            return self
                .batch_broker_approve(broker, calls, i, &mut results)
                .await;
        }

        results
    }

    /// 通过 broker 请求用户审批单个工具调用
    async fn broker_approve(
        &self,
        broker: &Arc<dyn UserInteractionBroker>,
        tool_call: &ToolCall,
    ) -> AgentResult<ToolCall> {
        let ctx = InteractionContext::Approval {
            items: vec![ApprovalItem {
                tool_call_id: tool_call.id.clone(),
                tool_name: tool_call.name.clone(),
                tool_input: tool_call.input.clone(),
            }],
        };
        let response = broker.request(ctx).await;
        let decision = match response {
            InteractionResponse::Decisions(mut d) => d.pop().unwrap_or(ApprovalDecision::Reject {
                reason: "用户拒绝".to_string(),
            }),
            _ => ApprovalDecision::Reject {
                reason: "用户拒绝".to_string(),
            },
        };
        apply_decision(tool_call, decision)
    }

    /// 根据共享权限模式决策单个工具调用
    async fn decide_by_mode(
        &self,
        mode: &Arc<SharedPermissionMode>,
        tool_call: &ToolCall,
    ) -> AgentResult<ToolCall> {
        match mode.load() {
            PermissionMode::Bypass => Ok(tool_call.clone()),
            PermissionMode::DontAsk => Err(AgentError::ToolRejected {
                tool: tool_call.name.clone(),
                reason: "Don't Ask 模式：自动拒绝".to_string(),
            }),
            PermissionMode::AcceptEdit => {
                if is_edit_tool(&tool_call.name) {
                    Ok(tool_call.clone())
                } else {
                    match &self.broker {
                        Some(broker) => self.broker_approve(broker, tool_call).await,
                        None => Ok(tool_call.clone()),
                    }
                }
            }
            PermissionMode::AutoMode => match &self.auto_classifier {
                Some(classifier) => {
                    let result = classifier.classify(&tool_call.name, &tool_call.input).await;
                    match result {
                        Classification::Allow => Ok(tool_call.clone()),
                        Classification::Deny => Err(AgentError::ToolRejected {
                            tool: tool_call.name.clone(),
                            reason: "Auto 模式：分类器拒绝".to_string(),
                        }),
                        Classification::Unsure => match &self.broker {
                            Some(broker) => self.broker_approve(broker, tool_call).await,
                            None => Err(AgentError::ToolRejected {
                                tool: tool_call.name.clone(),
                                reason: "Auto 模式：分类器不确定且无 broker".to_string(),
                            }),
                        },
                    }
                }
                None => match &self.broker {
                    Some(broker) => self.broker_approve(broker, tool_call).await,
                    None => Err(AgentError::ToolRejected {
                        tool: tool_call.name.clone(),
                        reason: "Auto 模式：无分类器且无 broker".to_string(),
                    }),
                },
            },
            PermissionMode::Default => match &self.broker {
                Some(broker) => self.broker_approve(broker, tool_call).await,
                None => {
                    tracing::warn!("HITL Default 模式但无 broker，拒绝工具调用");
                    Err(anyhow::anyhow!("HITL 审批不可用：未配置 broker").into())
                }
            },
        }
    }

    /// 无 mode 时原有的批量 broker 审批逻辑（向后兼容）
    async fn batch_broker_approve(
        &self,
        broker: &Arc<dyn UserInteractionBroker>,
        calls: &[ToolCall],
        start_idx: usize,
        initial_results: &mut Vec<AgentResult<ToolCall>>,
    ) -> Vec<AgentResult<ToolCall>> {
        let mut results: Vec<AgentResult<ToolCall>> = initial_results.drain(..).collect();

        let needs_approval: Vec<(usize, &ToolCall)> = calls
            .iter()
            .enumerate()
            .skip(start_idx)
            .filter(|(_, c)| (self.requires_approval)(&c.name))
            .collect();

        if needs_approval.is_empty() {
            results.extend(calls.iter().skip(start_idx).map(|c| Ok(c.clone())));
            return results;
        }

        let items: Vec<ApprovalItem> = needs_approval
            .iter()
            .map(|(_, c)| ApprovalItem {
                tool_call_id: c.id.clone(),
                tool_name: c.name.clone(),
                tool_input: c.input.clone(),
            })
            .collect();

        let ctx = InteractionContext::Approval { items };
        let response = broker.request(ctx).await;

        let decisions = match response {
            InteractionResponse::Decisions(d) => d,
            _ => vec![
                ApprovalDecision::Reject {
                    reason: "unexpected response".to_string()
                };
                needs_approval.len()
            ],
        };

        let mut decision_iter = decisions.into_iter();

        for idx in start_idx..calls.len() {
            let call = &calls[idx];
            if (self.requires_approval)(&call.name) {
                let decision = decision_iter.next().unwrap_or(ApprovalDecision::Reject {
                    reason: "用户拒绝".to_string(),
                });
                results.push(apply_decision(call, decision));
            } else {
                results.push(Ok(call.clone()));
            }
        }

        results
    }
}

#[async_trait]
impl<S: State> Middleware<S> for HumanInTheLoopMiddleware {
    fn name(&self) -> &str {
        "HumanInTheLoopMiddleware"
    }

    /// 批量工具调用前处理：对一批工具调用一次性收集所有需审批的项，
    /// 通过 broker 弹出一个 [多工具审批] 弹窗，避免逐个弹窗打断用户。
    async fn before_tools_batch(
        &self,
        _state: &mut S,
        calls: &[ToolCall],
    ) -> Vec<AgentResult<ToolCall>> {
        self.process_batch(calls).await
    }

    async fn before_tool(&self, _state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
        // 1. 非敏感工具 → 所有模式都放行
        if !(self.requires_approval)(&tool_call.name) {
            return Ok(tool_call.clone());
        }

        // 2. 有 mode → 按权限模式决策
        if let Some(mode) = &self.mode {
            return self.decide_by_mode(mode, tool_call).await;
        }

        // 3. 无 mode 且无 broker → 放行（disabled() 路径）
        let Some(broker) = &self.broker else {
            return Ok(tool_call.clone());
        };

        // 4. 无 mode 但有 broker → 原有弹窗审批逻辑
        self.broker_approve(broker, tool_call).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::agent::state::AgentState;

    /// 自动批准 broker
    struct AutoApproveBroker;

    #[async_trait]
    impl UserInteractionBroker for AutoApproveBroker {
        async fn request(&self, ctx: InteractionContext) -> InteractionResponse {
            match ctx {
                InteractionContext::Approval { items } => InteractionResponse::Decisions(
                    items.iter().map(|_| ApprovalDecision::Approve).collect(),
                ),
                _ => InteractionResponse::Decisions(vec![]),
            }
        }
    }

    /// 自动拒绝 broker
    struct AutoRejectBroker;

    #[async_trait]
    impl UserInteractionBroker for AutoRejectBroker {
        async fn request(&self, ctx: InteractionContext) -> InteractionResponse {
            match ctx {
                InteractionContext::Approval { items } => InteractionResponse::Decisions(
                    items
                        .iter()
                        .map(|_| ApprovalDecision::Reject {
                            reason: "用户拒绝".to_string(),
                        })
                        .collect(),
                ),
                _ => InteractionResponse::Decisions(vec![]),
            }
        }
    }

    fn make_tool_call(name: &str) -> ToolCall {
        ToolCall {
            id: "test-id".to_string(),
            name: name.to_string(),
            input: serde_json::json!({"command": "ls"}),
        }
    }

    #[tokio::test]
    async fn test_disabled_allows_all() {
        let mw = HumanInTheLoopMiddleware::disabled();
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "Bash");
    }

    #[tokio::test]
    async fn test_approve_passes_through() {
        let mw =
            HumanInTheLoopMiddleware::new(Arc::new(AutoApproveBroker), default_requires_approval);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "Bash");
    }

    #[tokio::test]
    async fn test_reject_returns_error() {
        let mw =
            HumanInTheLoopMiddleware::new(Arc::new(AutoRejectBroker), default_requires_approval);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Bash");
        let result = mw.before_tool(&mut state, &tc).await;
        assert!(matches!(result, Err(AgentError::ToolRejected { .. })));
    }

    #[tokio::test]
    async fn test_read_file_not_intercepted() {
        let mw =
            HumanInTheLoopMiddleware::new(Arc::new(AutoRejectBroker), default_requires_approval);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Read");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "Read");
    }

    #[test]
    fn test_default_requires_approval() {
        assert!(default_requires_approval("Bash"));
        assert!(default_requires_approval("Write"));
        assert!(default_requires_approval("Edit"));
        assert!(default_requires_approval("folder_operations"));
        assert!(default_requires_approval("delete_something"));
        assert!(default_requires_approval("rm_rf"));
        assert!(default_requires_approval("Agent"));

        assert!(!default_requires_approval("Read"));
        assert!(!default_requires_approval("Glob"));
        assert!(!default_requires_approval("Grep"));
        assert!(!default_requires_approval("TodoWrite"));
        assert!(!default_requires_approval("ask_user"));
    }

    #[tokio::test]
    async fn test_edit_modifies_input() {
        struct EditBroker;

        #[async_trait]
        impl UserInteractionBroker for EditBroker {
            async fn request(&self, ctx: InteractionContext) -> InteractionResponse {
                match ctx {
                    InteractionContext::Approval { items } => InteractionResponse::Decisions(
                        items
                            .iter()
                            .map(|_| ApprovalDecision::Edit {
                                new_input: serde_json::json!({"command": "echo safe"}),
                            })
                            .collect(),
                    ),
                    _ => InteractionResponse::Decisions(vec![]),
                }
            }
        }

        let mw = HumanInTheLoopMiddleware::new(Arc::new(EditBroker), default_requires_approval);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "Bash");
        assert_eq!(result.input, serde_json::json!({"command": "echo safe"}));
    }

    #[tokio::test]
    async fn test_respond_returns_error_with_reason() {
        struct RespondBroker;

        #[async_trait]
        impl UserInteractionBroker for RespondBroker {
            async fn request(&self, ctx: InteractionContext) -> InteractionResponse {
                match ctx {
                    InteractionContext::Approval { items } => InteractionResponse::Decisions(
                        items
                            .iter()
                            .map(|_| ApprovalDecision::Respond {
                                message: "请改用 echo 命令".to_string(),
                            })
                            .collect(),
                    ),
                    _ => InteractionResponse::Decisions(vec![]),
                }
            }
        }

        let mw = HumanInTheLoopMiddleware::new(Arc::new(RespondBroker), default_requires_approval);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Bash");
        let result = mw.before_tool(&mut state, &tc).await;
        match result {
            Err(AgentError::ToolRejected { reason, .. }) => {
                assert_eq!(reason, "请改用 echo 命令");
            }
            other => unreachable!("期望 ToolRejected，实际: {:?}", other),
        }
    }

    // ─── 多模式测试 ─────────────────────────────────────────────────────────────

    #[test]
    fn test_is_edit_tool() {
        assert!(is_edit_tool("Write"));
        assert!(is_edit_tool("Edit"));
        assert!(is_edit_tool("folder_operations"));
        assert!(!is_edit_tool("Bash"));
        assert!(!is_edit_tool("Agent"));
        assert!(!is_edit_tool("delete_x"));
        assert!(!is_edit_tool("rm_x"));
        assert!(!is_edit_tool("Read"));
    }

    /// Mock 自动分类器
    struct MockClassifier {
        result: Classification,
    }
    impl MockClassifier {
        fn new(result: Classification) -> Self {
            Self { result }
        }
    }
    #[async_trait]
    impl AutoClassifier for MockClassifier {
        async fn classify(
            &self,
            _tool_name: &str,
            _tool_input: &serde_json::Value,
        ) -> Classification {
            self.result
        }
    }

    fn make_mw_with_mode(
        mode: PermissionMode,
        classifier: Option<Arc<dyn AutoClassifier>>,
    ) -> HumanInTheLoopMiddleware {
        let broker = Arc::new(AutoApproveBroker);
        let shared = SharedPermissionMode::new(mode);
        HumanInTheLoopMiddleware::with_shared_mode(
            broker,
            default_requires_approval,
            shared,
            classifier,
        )
    }

    #[tokio::test]
    async fn test_bypass_permissions_allows_all() {
        let mw = make_mw_with_mode(PermissionMode::Bypass, None);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "Bash");
    }

    #[tokio::test]
    async fn test_dont_ask_rejects_all() {
        let mw = make_mw_with_mode(PermissionMode::DontAsk, None);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Bash");
        let result = mw.before_tool(&mut state, &tc).await;
        assert!(matches!(result, Err(AgentError::ToolRejected { .. })));
    }

    #[tokio::test]
    async fn test_accept_edits_allows_write_file() {
        let mw = make_mw_with_mode(PermissionMode::AcceptEdit, None);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Write");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "Write");
    }

    #[tokio::test]
    async fn test_accept_edits_approves_bash_via_broker() {
        let mw = make_mw_with_mode(PermissionMode::AcceptEdit, None);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "Bash");
    }

    #[tokio::test]
    async fn test_default_mode_approves_bash_via_broker() {
        let mw = make_mw_with_mode(PermissionMode::Default, None);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "Bash");
    }

    #[tokio::test]
    async fn test_auto_mode_allow() {
        let mw = make_mw_with_mode(
            PermissionMode::AutoMode,
            Some(Arc::new(MockClassifier::new(Classification::Allow))),
        );
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "Bash");
    }

    #[tokio::test]
    async fn test_auto_mode_deny() {
        let mw = make_mw_with_mode(
            PermissionMode::AutoMode,
            Some(Arc::new(MockClassifier::new(Classification::Deny))),
        );
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Bash");
        let result = mw.before_tool(&mut state, &tc).await;
        assert!(matches!(result, Err(AgentError::ToolRejected { .. })));
    }

    #[tokio::test]
    async fn test_auto_mode_unsure_falls_back_to_broker() {
        let mw = make_mw_with_mode(
            PermissionMode::AutoMode,
            Some(Arc::new(MockClassifier::new(Classification::Unsure))),
        );
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "Bash");
    }

    #[tokio::test]
    async fn test_auto_mode_no_classifier_falls_back_to_broker() {
        let mw = make_mw_with_mode(PermissionMode::AutoMode, None);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("Bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "Bash");
    }

    #[tokio::test]
    async fn test_process_batch_bypass_permissions() {
        let mw = make_mw_with_mode(PermissionMode::Bypass, None);
        let calls = vec![
            make_tool_call("Bash"),
            make_tool_call("Write"),
            make_tool_call("Read"),
        ];
        let results = mw.process_batch(&calls).await;
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.is_ok()));
    }

    #[tokio::test]
    async fn test_process_batch_dont_ask_rejects_sensitive() {
        let mw = make_mw_with_mode(PermissionMode::DontAsk, None);
        let calls = vec![make_tool_call("Bash"), make_tool_call("Read")];
        let results = mw.process_batch(&calls).await;
        assert_eq!(results.len(), 2);
        assert!(results[0].is_err(), "bash 应被拒绝");
        assert!(results[1].is_ok(), "read_file 应放行");
    }

    #[tokio::test]
    async fn test_process_batch_accept_edits_mixed() {
        let mw = make_mw_with_mode(PermissionMode::AcceptEdit, None);
        let calls = vec![
            make_tool_call("Write"),
            make_tool_call("Bash"),
            make_tool_call("Read"),
        ];
        let results = mw.process_batch(&calls).await;
        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok(), "write_file 应放行");
        assert!(
            results[1].is_ok(),
            "bash 走 broker 审批（AutoApproveBroker）"
        );
        assert!(results[2].is_ok(), "read_file 应放行");
    }
}
