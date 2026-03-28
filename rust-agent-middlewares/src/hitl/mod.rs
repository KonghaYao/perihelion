use std::sync::Arc;

use async_trait::async_trait;
use rust_create_agent::agent::react::ToolCall;
use rust_create_agent::agent::state::State;
use rust_create_agent::error::{AgentError, AgentResult};
use rust_create_agent::interaction::{
    ApprovalDecision, ApprovalItem, InteractionContext, InteractionResponse, UserInteractionBroker,
};
use rust_create_agent::middleware::r#trait::Middleware;

// 保留旧类型重导出以向后兼容（已废弃，请改用 UserInteractionBroker）
#[allow(deprecated)]
pub use rust_create_agent::hitl::{BatchItem, HitlDecision, HitlHandler};

// ─── YOLO 模式检测 ─────────────────────────────────────────────────────────────

/// 检测是否处于 YOLO 模式（`YOLO_MODE=true` 或 `YOLO_MODE=1`）
pub fn is_yolo_mode() -> bool {
    std::env::var("YOLO_MODE")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

// ─── 默认规则 ──────────────────────────────────────────────────────────────────

/// 默认敏感工具判断规则
///
/// - `bash`：所有 bash 命令
/// - `write_*`：文件写入
/// - `edit_*`：文件编辑
/// - `folder_operations`：目录操作
/// - `launch_agent`：子 Agent 委派（子 Agent 不含 HITL，可传递绕过审批）
pub fn default_requires_approval(tool_name: &str) -> bool {
    tool_name == "bash"
        || tool_name == "folder_operations"
        || tool_name == "launch_agent"
        || tool_name.starts_with("write_")
        || tool_name.starts_with("edit_")
        || tool_name.starts_with("delete_")
        || tool_name.starts_with("rm_")
}

// ─── HumanInTheLoopMiddleware ──────────────────────────────────────────────────

/// HumanInTheLoopMiddleware — 敏感工具调用前需用户确认
///
/// 在 `before_tool` 时拦截工具调用，通过注入的 [`UserInteractionBroker`] 请求用户审批。
///
/// # YOLO 模式
/// 通过 `HumanInTheLoopMiddleware::disabled()` 或环境变量 `YOLO_MODE=true` 禁用。
pub struct HumanInTheLoopMiddleware {
    broker: Option<Arc<dyn UserInteractionBroker>>,
    requires_approval: fn(&str) -> bool,
}

impl HumanInTheLoopMiddleware {
    /// 创建启用的 HITL 中间件，使用注入的 broker
    pub fn new(broker: Arc<dyn UserInteractionBroker>, requires_approval: fn(&str) -> bool) -> Self {
        Self {
            broker: Some(broker),
            requires_approval,
        }
    }

    /// YOLO 模式：所有工具调用直接放行
    pub fn disabled() -> Self {
        Self {
            broker: None,
            requires_approval: default_requires_approval,
        }
    }

    /// 从环境变量决定是否启用（`YOLO_MODE=true` 则禁用）
    pub fn from_env(broker: Arc<dyn UserInteractionBroker>, requires_approval: fn(&str) -> bool) -> Self {
        if is_yolo_mode() {
            Self::disabled()
        } else {
            Self::new(broker, requires_approval)
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
        let Some(broker) = &self.broker else {
            return calls.iter().map(|c| Ok(c.clone())).collect();
        };

        let needs_approval: Vec<(usize, &ToolCall)> = calls
            .iter()
            .enumerate()
            .filter(|(_, c)| (self.requires_approval)(&c.name))
            .collect();

        if needs_approval.is_empty() {
            return calls.iter().map(|c| Ok(c.clone())).collect();
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
            _ => vec![ApprovalDecision::Reject { reason: "unexpected response".to_string() }; needs_approval.len()],
        };

        let mut decision_iter = decisions.into_iter();
        let mut results: Vec<AgentResult<ToolCall>> = calls.iter().map(|c| Ok(c.clone())).collect();

        for (idx, call) in needs_approval {
            let decision = decision_iter
                .next()
                .unwrap_or(ApprovalDecision::Reject { reason: "用户拒绝".to_string() });
            results[idx] = apply_decision(call, decision);
        }

        results
    }
}

#[async_trait]
impl<S: State> Middleware<S> for HumanInTheLoopMiddleware {
    fn name(&self) -> &str {
        "HumanInTheLoopMiddleware"
    }

    async fn before_tool(&self, _state: &mut S, tool_call: &ToolCall) -> AgentResult<ToolCall> {
        let Some(broker) = &self.broker else {
            return Ok(tool_call.clone());
        };

        if !(self.requires_approval)(&tool_call.name) {
            return Ok(tool_call.clone());
        }

        let ctx = InteractionContext::Approval {
            items: vec![ApprovalItem {
                tool_call_id: tool_call.id.clone(),
                tool_name: tool_call.name.clone(),
                tool_input: tool_call.input.clone(),
            }],
        };

        let response = broker.request(ctx).await;
        let decision = match response {
            InteractionResponse::Decisions(mut d) => d
                .pop()
                .unwrap_or(ApprovalDecision::Reject { reason: "用户拒绝".to_string() }),
            _ => ApprovalDecision::Reject { reason: "用户拒绝".to_string() },
        };

        apply_decision(tool_call, decision)
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
                InteractionContext::Approval { items } => {
                    InteractionResponse::Decisions(
                        items.iter().map(|_| ApprovalDecision::Approve).collect(),
                    )
                }
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
                InteractionContext::Approval { items } => {
                    InteractionResponse::Decisions(
                        items.iter().map(|_| ApprovalDecision::Reject { reason: "用户拒绝".to_string() }).collect(),
                    )
                }
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
        let tc = make_tool_call("bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "bash");
    }

    #[tokio::test]
    async fn test_approve_passes_through() {
        let mw = HumanInTheLoopMiddleware::new(Arc::new(AutoApproveBroker), default_requires_approval);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "bash");
    }

    #[tokio::test]
    async fn test_reject_returns_error() {
        let mw = HumanInTheLoopMiddleware::new(Arc::new(AutoRejectBroker), default_requires_approval);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("bash");
        let result = mw.before_tool(&mut state, &tc).await;
        assert!(matches!(result, Err(AgentError::ToolRejected { .. })));
    }

    #[tokio::test]
    async fn test_read_file_not_intercepted() {
        let mw = HumanInTheLoopMiddleware::new(Arc::new(AutoRejectBroker), default_requires_approval);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("read_file");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "read_file");
    }

    #[test]
    fn test_default_requires_approval() {
        assert!(default_requires_approval("bash"));
        assert!(default_requires_approval("write_file"));
        assert!(default_requires_approval("edit_file"));
        assert!(default_requires_approval("folder_operations"));
        assert!(default_requires_approval("delete_something"));
        assert!(default_requires_approval("rm_rf"));
        assert!(default_requires_approval("launch_agent"));

        assert!(!default_requires_approval("read_file"));
        assert!(!default_requires_approval("glob_files"));
        assert!(!default_requires_approval("search_files_rg"));
        assert!(!default_requires_approval("todo_write"));
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
                        items.iter().map(|_| ApprovalDecision::Edit {
                            new_input: serde_json::json!({"command": "echo safe"}),
                        }).collect(),
                    ),
                    _ => InteractionResponse::Decisions(vec![]),
                }
            }
        }

        let mw = HumanInTheLoopMiddleware::new(Arc::new(EditBroker), default_requires_approval);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "bash");
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
                        items.iter().map(|_| ApprovalDecision::Respond {
                            message: "请改用 echo 命令".to_string(),
                        }).collect(),
                    ),
                    _ => InteractionResponse::Decisions(vec![]),
                }
            }
        }

        let mw = HumanInTheLoopMiddleware::new(Arc::new(RespondBroker), default_requires_approval);
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("bash");
        let result = mw.before_tool(&mut state, &tc).await;
        match result {
            Err(AgentError::ToolRejected { reason, .. }) => {
                assert_eq!(reason, "请改用 echo 命令");
            }
            other => unreachable!("期望 ToolRejected，实际: {:?}", other),
        }
    }
}
