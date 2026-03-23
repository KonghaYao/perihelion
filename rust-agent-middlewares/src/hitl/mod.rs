use std::sync::Arc;

use async_trait::async_trait;
use rust_create_agent::agent::react::ToolCall;
use rust_create_agent::agent::state::State;
use rust_create_agent::error::{AgentError, AgentResult};
use rust_create_agent::middleware::r#trait::Middleware;

// 从核心库导入 trait 定义
pub use rust_create_agent::hitl::{BatchItem, HitlDecision, HitlHandler};

// ─── YOLO 模式检测 ─────────────────────────────────────────────────────────────

/// 检测是否处于 YOLO 模式（`YOLO_MODE=true` 或 `YOLO_MODE=1`）
pub fn is_yolo_mode() -> bool {
    std::env::var("YOLO_MODE")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

// ─── 默认规则 ──────────────────────────────────────────────────────────────────

/// 默认敏感工具判断规则（无注入时使用）
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
/// 在 `before_tool` 时拦截工具调用，通过注入的 [`HitlHandler`] 请求用户审批。
///
/// # YOLO 模式
/// 通过 `HumanInTheLoopMiddleware::disabled()` 或环境变量 `YOLO_MODE=true` 禁用。
pub struct HumanInTheLoopMiddleware {
    handler: Option<Arc<dyn HitlHandler>>,
    enabled: bool,
}

impl HumanInTheLoopMiddleware {
    /// 创建启用的 HITL 中间件，使用注入的 handler
    pub fn new(handler: Arc<dyn HitlHandler>) -> Self {
        Self {
            handler: Some(handler),
            enabled: true,
        }
    }

    /// YOLO 模式：所有工具调用直接放行
    pub fn disabled() -> Self {
        Self {
            handler: None,
            enabled: false,
        }
    }

    /// 从环境变量决定是否启用（`YOLO_MODE=true` 则禁用）
    pub fn from_env(handler: Arc<dyn HitlHandler>) -> Self {
        if is_yolo_mode() {
            Self::disabled()
        } else {
            Self::new(handler)
        }
    }
}

impl HumanInTheLoopMiddleware {
    /// 批量处理一批工具调用：收集所有需要审批的项，一次性弹窗，返回每个 call 的处理结果
    pub async fn process_batch(&self, calls: &[ToolCall]) -> Vec<AgentResult<ToolCall>> {
        let Some(handler) = &self.handler else {
            return calls.iter().map(|c| Ok(c.clone())).collect();
        };
        if !self.enabled {
            return calls.iter().map(|c| Ok(c.clone())).collect();
        }

        let needs_approval: Vec<(usize, &ToolCall)> = calls
            .iter()
            .enumerate()
            .filter(|(_, c)| handler.requires_approval(&c.name, &c.input))
            .collect();

        if needs_approval.is_empty() {
            return calls.iter().map(|c| Ok(c.clone())).collect();
        }

        let batch_items: Vec<BatchItem> = needs_approval
            .iter()
            .map(|(_, c)| BatchItem {
                tool_name: c.name.clone(),
                input: c.input.clone(),
            })
            .collect();

        let decisions = handler.request_approval_batch(&batch_items).await;

        let mut approval_iter = decisions.into_iter();
        let mut results: Vec<AgentResult<ToolCall>> = calls.iter().map(|c| Ok(c.clone())).collect();

        for (idx, call) in needs_approval {
            let decision = approval_iter.next().unwrap_or(HitlDecision::Reject);
            results[idx] = match decision {
                HitlDecision::Approve => Ok(call.clone()),
                HitlDecision::Edit(new_input) => {
                    let mut modified = call.clone();
                    modified.input = new_input;
                    Ok(modified)
                }
                HitlDecision::Reject => Err(AgentError::ToolRejected {
                    tool: call.name.clone(),
                    reason: "用户拒绝".to_string(),
                }),
                HitlDecision::Respond(msg) => Err(AgentError::ToolRejected {
                    tool: call.name.clone(),
                    reason: msg,
                }),
            };
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
        let Some(handler) = &self.handler else {
            return Ok(tool_call.clone());
        };
        if !self.enabled {
            return Ok(tool_call.clone());
        }

        if !handler.requires_approval(&tool_call.name, &tool_call.input) {
            return Ok(tool_call.clone());
        }

        match handler.request_approval(&tool_call.name, &tool_call.input).await {
            HitlDecision::Approve => Ok(tool_call.clone()),
            HitlDecision::Edit(new_input) => {
                let mut modified = tool_call.clone();
                modified.input = new_input;
                Ok(modified)
            }
            HitlDecision::Reject => Err(AgentError::ToolRejected {
                tool: tool_call.name.clone(),
                reason: "用户拒绝".to_string(),
            }),
            HitlDecision::Respond(msg) => Err(AgentError::ToolRejected {
                tool: tool_call.name.clone(),
                reason: msg,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::agent::state::AgentState;

    struct AutoApproveHandler;

    #[async_trait]
    impl HitlHandler for AutoApproveHandler {
        fn requires_approval(&self, tool_name: &str, _input: &serde_json::Value) -> bool {
            default_requires_approval(tool_name)
        }

        async fn request_approval(
            &self,
            _tool_name: &str,
            _input: &serde_json::Value,
        ) -> HitlDecision {
            HitlDecision::Approve
        }
    }

    struct AutoRejectHandler;

    #[async_trait]
    impl HitlHandler for AutoRejectHandler {
        fn requires_approval(&self, _tool_name: &str, _input: &serde_json::Value) -> bool {
            true
        }

        async fn request_approval(
            &self,
            _tool_name: &str,
            _input: &serde_json::Value,
        ) -> HitlDecision {
            HitlDecision::Reject
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
        let mw = HumanInTheLoopMiddleware::new(Arc::new(AutoApproveHandler));
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "bash");
    }

    #[tokio::test]
    async fn test_reject_returns_error() {
        let mw = HumanInTheLoopMiddleware::new(Arc::new(AutoRejectHandler));
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("bash");
        let result = mw.before_tool(&mut state, &tc).await;
        assert!(matches!(result, Err(AgentError::ToolRejected { .. })));
    }

    #[tokio::test]
    async fn test_read_file_not_intercepted() {
        let mw = HumanInTheLoopMiddleware::new(Arc::new(AutoApproveHandler));
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("read_file");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "read_file");
    }

    #[test]
    fn test_default_requires_approval() {
        // 需要审批的工具
        assert!(default_requires_approval("bash"));
        assert!(default_requires_approval("write_file"));
        assert!(default_requires_approval("edit_file"));
        assert!(default_requires_approval("folder_operations"));
        assert!(default_requires_approval("delete_something"));
        assert!(default_requires_approval("rm_rf"));
        // launch_agent 子 Agent 不含 HITL，可传递绕过审批，必须审批
        assert!(default_requires_approval("launch_agent"));

        // 不需要审批的工具（只读或无副作用）
        assert!(!default_requires_approval("read_file"));
        assert!(!default_requires_approval("glob_files"));
        assert!(!default_requires_approval("search_files_rg"));
        assert!(!default_requires_approval("todo_write")); // 内存操作，无磁盘副作用
        assert!(!default_requires_approval("ask_user"));   // 仅询问用户，无副作用
    }

    /// Edit 决策：修改工具调用参数后继续执行
    #[tokio::test]
    async fn test_edit_modifies_input() {
        struct EditHandler;

        #[async_trait]
        impl HitlHandler for EditHandler {
            fn requires_approval(&self, _: &str, _: &serde_json::Value) -> bool {
                true
            }
            async fn request_approval(&self, _: &str, _: &serde_json::Value) -> HitlDecision {
                HitlDecision::Edit(serde_json::json!({"command": "echo safe"}))
            }
        }

        let mw = HumanInTheLoopMiddleware::new(Arc::new(EditHandler));
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("bash");
        let result = mw.before_tool(&mut state, &tc).await.unwrap();
        assert_eq!(result.name, "bash");
        assert_eq!(result.input, serde_json::json!({"command": "echo safe"}));
    }

    /// Respond 决策：拒绝并携带用户消息
    #[tokio::test]
    async fn test_respond_returns_error_with_reason() {
        struct RespondHandler;

        #[async_trait]
        impl HitlHandler for RespondHandler {
            fn requires_approval(&self, _: &str, _: &serde_json::Value) -> bool {
                true
            }
            async fn request_approval(&self, _: &str, _: &serde_json::Value) -> HitlDecision {
                HitlDecision::Respond("请改用 echo 命令".to_string())
            }
        }

        let mw = HumanInTheLoopMiddleware::new(Arc::new(RespondHandler));
        let mut state = AgentState::new("/tmp");
        let tc = make_tool_call("bash");
        let result = mw.before_tool(&mut state, &tc).await;
        match result {
            Err(AgentError::ToolRejected { reason, .. }) => {
                assert_eq!(reason, "请改用 echo 命令");
            }
            other => panic!("期望 ToolRejected，实际: {:?}", other),
        }
    }
}
