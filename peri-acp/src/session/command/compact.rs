//! `/compact` 命令 — 手动触发上下文压缩。
//!
//! 移植自 `peri-tui/src/acp_server/compact.rs`，
//! 改为接收 [`CommandContext`]、返回 [`CommandResult`]。

use std::sync::Arc;

use peri_agent::{
    agent::{
        compact::{extract_file_info, extract_skill_names, full_compact, re_inject},
        events::AgentEvent as ExecutorEvent,
    },
    messages::BaseMessage,
};
use tracing::{info, warn};

use super::{AgentCommand, CommandContext, CommandKind, CommandResult};
use crate::session::executor::PromptStopReason;

/// 手动 compact 命令。
pub struct CompactCommand;

impl CompactCommand {
    pub const NAME: &'static str = "compact";
}

#[async_trait::async_trait]
impl AgentCommand for CompactCommand {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["compress"]
    }

    fn description(&self) -> &str {
        "压缩对话历史以释放上下文空间"
    }

    fn kind(&self) -> CommandKind {
        CommandKind::Immediate
    }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        let CommandContext {
            session_id,
            history,
            cwd,
            peri_config,
            compact_model,
            event_sink,
            cancel_token,
            ..
        } = ctx;

        tracing::debug!(history_len = history.len(), "compact: execute called");

        if history.is_empty() {
            warn!("compact: 无历史消息可压缩");
            event_sink
                .push_event(
                    &session_id,
                    &ExecutorEvent::CompactError {
                        message: "no history to compact".into(),
                    },
                    0,
                )
                .await;
            return CommandResult {
                messages: history,
                stop_reason: PromptStopReason::EndTurn,
            };
        }

        // compact 配置
        let mut compact_config = peri_config.config.compact.clone().unwrap_or_default();
        compact_config.apply_env_overrides();

        // 获取 compact model
        let compact_model: Arc<dyn peri_agent::llm::BaseModel> = match compact_model {
            Some(m) => m,
            None => {
                warn!("compact: 无可用模型");
                event_sink
                    .push_event(
                        &session_id,
                        &ExecutorEvent::CompactError {
                            message: "no model available for compact".into(),
                        },
                        0,
                    )
                    .await;
                return CommandResult {
                    messages: history,
                    stop_reason: PromptStopReason::EndTurn,
                };
            }
        };

        // 发送 CompactStarted 事件
        event_sink
            .push_event(&session_id, &ExecutorEvent::CompactStarted, 0)
            .await;

        // 执行 full_compact（支持 Ctrl+C 取消）
        let compact_result = tokio::select! {
            r = full_compact(&history, compact_model.as_ref(), &compact_config, "") => {
                match r {
                    Ok(r) => r,
                    Err(e) => {
                        warn!(error = %e, "compact: full_compact 失败");
                        event_sink
                            .push_event(
                                &session_id,
                                &ExecutorEvent::CompactError {
                                    message: e.to_string(),
                                },
                                0,
                            )
                            .await;
                        return CommandResult {
                            messages: history,
                            stop_reason: PromptStopReason::EndTurn,
                        };
                    }
                }
            }
            _ = cancel_token.cancelled() => {
                tracing::info!(session_id = %session_id, "compact cancelled by user");
                event_sink
                    .push_event(
                        &session_id,
                        &ExecutorEvent::CompactError {
                            message: "compact cancelled".into(),
                        },
                        0,
                    )
                    .await;
                return CommandResult {
                    messages: history,
                    stop_reason: PromptStopReason::Cancelled,
                };
            }
        };

        info!(
            summary_len = compact_result.summary.len(),
            "compact: full_compact 完成"
        );

        // 执行 re_inject
        let re_inject_result = re_inject(&history, &compact_config, &cwd).await;

        info!(
            files_injected = re_inject_result.files_injected,
            skills_injected = re_inject_result.skills_injected,
            "compact: re_inject 完成"
        );

        // 提取文件和 skill 信息
        let files = extract_file_info(&re_inject_result.messages);
        let skills = extract_skill_names(&re_inject_result.messages);

        // 摘要作为 Human 消息（与 auto-compact 路径和 Claude Code 实现对齐）
        let summary_content = format!(
            "{}\n\n[上下文已压缩，请根据摘要继续工作]",
            compact_result.summary
        );
        let mut new_messages = vec![BaseMessage::human(summary_content)];
        new_messages.extend(re_inject_result.messages.clone());

        // 发送 CompactCompleted 事件
        event_sink
            .push_event(
                &session_id,
                &ExecutorEvent::CompactCompleted {
                    summary: compact_result.summary,
                    files: files.clone(),
                    skills: skills.clone(),
                    micro_cleared: 0,
                    messages: new_messages.clone(),
                },
                0,
            )
            .await;

        info!("compact: 完成，session 已更新");

        CommandResult {
            messages: new_messages,
            stop_reason: PromptStopReason::EndTurn,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use peri_agent::agent::events::AgentEvent as ExecutorEvent;

    use super::*;
    use crate::session::executor::PromptStopReason;

    // ── Mock EventSink ────────────────────────────────────────────────────

    struct MockEventSink {
        events: Mutex<Vec<(String, String)>>,
    }

    impl MockEventSink {
        fn new() -> Self {
            Self {
                events: Mutex::new(Vec::new()),
            }
        }

        fn events(&self) -> Vec<(String, String)> {
            self.events.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl crate::session::event_sink::EventSink for MockEventSink {
        async fn push_event(&self, session_id: &str, event: &ExecutorEvent, _context_window: u32) {
            let json = serde_json::to_string(event).unwrap_or_default();
            self.events
                .lock()
                .unwrap()
                .push((session_id.to_string(), json));
        }

        async fn push_done(&self, _session_id: &str) {}
    }

    fn make_ctx(
        sink: Arc<dyn crate::session::event_sink::EventSink>,
        history: Vec<BaseMessage>,
    ) -> super::super::CommandContext {
        super::super::CommandContext {
            session_id: "test-session".to_string(),
            history,
            cwd: "/tmp".to_string(),
            peri_config: Arc::new(Default::default()),
            compact_model: None,
            event_sink: sink,
            args: String::new(),
            cancel_token: peri_agent::agent::AgentCancellationToken::new(),
            thread_store: None,
            thread_id: None,
        }
    }

    // ── extract_file_info 测试 ───────────────────────────────────────────

    #[test]
    fn test_extract_file_info_single_file() {
        // Arrange: 一条包含文件路径的 System 消息
        let msgs = vec![BaseMessage::system(
            "[最近读取的文件: /src/main.rs\nfn main() {}\n",
        )];

        // Act
        let files = extract_file_info(&msgs);

        // Assert: 提取到文件路径和行数（内容行数 = 总行数 - 1(路径行)）
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "/src/main.rs");
        assert_eq!(files[0].lines, 1); // "fn main() {}" — 1 行内容
    }

    #[test]
    fn test_extract_file_info_multiple_files() {
        // Arrange: 多条文件消息
        let msgs = vec![
            BaseMessage::system("[最近读取的文件: /a.rs\nline1\nline2\n"),
            BaseMessage::system("[最近读取的文件: /b.rs\nline1\n"),
        ];

        // Act
        let files = extract_file_info(&msgs);

        // Assert
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "/a.rs");
        assert_eq!(files[0].lines, 2);
        assert_eq!(files[1].path, "/b.rs");
        assert_eq!(files[1].lines, 1);
    }

    #[test]
    fn test_extract_file_info_empty_messages() {
        // Arrange: 空消息列表
        let msgs: Vec<BaseMessage> = vec![];

        // Act
        let files = extract_file_info(&msgs);

        // Assert
        assert!(files.is_empty());
    }

    #[test]
    fn test_extract_file_info_skips_non_file_messages() {
        // Arrange: 非文件 System 消息 + Human/Ai 消息
        let msgs = vec![
            BaseMessage::system("普通系统提示"),
            BaseMessage::human("用户消息"),
            BaseMessage::ai("助手回复"),
        ];

        // Act
        let files = extract_file_info(&msgs);

        // Assert: 全部跳过
        assert!(files.is_empty());
    }

    #[test]
    fn test_extract_file_info_file_with_no_content_lines() {
        // Arrange: 只有路径行，无内容
        let msgs = vec![BaseMessage::system("[最近读取的文件: /empty.rs\n")];

        // Act
        let files = extract_file_info(&msgs);

        // Assert: 路径行存在但无内容行（lines = 0）
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "/empty.rs");
        assert_eq!(files[0].lines, 0);
    }

    // ── extract_skill_names 测试 ─────────────────────────────────────────

    #[test]
    fn test_extract_skill_names_single_skill() {
        // Arrange: 一条包含 Skill 名称的 System 消息
        let msgs = vec![BaseMessage::system("[激活的 Skill 指令: tdd")];

        // Act
        let skills = extract_skill_names(&msgs);

        // Assert
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0], "tdd");
    }

    #[test]
    fn test_extract_skill_names_multiple_skills() {
        // Arrange: 多条 Skill 消息
        let msgs = vec![
            BaseMessage::system("[激活的 Skill 指令: tdd"),
            BaseMessage::system("[激活的 Skill 指令: code-review"),
        ];

        // Act
        let skills = extract_skill_names(&msgs);

        // Assert
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0], "tdd");
        assert_eq!(skills[1], "code-review");
    }

    #[test]
    fn test_extract_skill_names_empty_messages() {
        // Arrange: 空消息列表
        let msgs: Vec<BaseMessage> = vec![];

        // Act
        let skills = extract_skill_names(&msgs);

        // Assert
        assert!(skills.is_empty());
    }

    #[test]
    fn test_extract_skill_names_skips_non_skill_messages() {
        // Arrange: 非技能消息
        let msgs = vec![
            BaseMessage::system("[最近读取的文件: /src/main.rs\n"),
            BaseMessage::human("你好"),
        ];

        // Act
        let skills = extract_skill_names(&msgs);

        // Assert: 全部跳过
        assert!(skills.is_empty());
    }

    #[test]
    fn test_extract_skill_names_extracts_only_first_line() {
        // Arrange: Skill 名称后有多行内容，只取第一行
        let msgs = vec![BaseMessage::system(
            "[激活的 Skill 指令: my-skill\n额外内容\n更多内容",
        )];

        // Act
        let skills = extract_skill_names(&msgs);

        // Assert: 只提取第一行名称
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0], "my-skill");
    }

    // ── CompactCommand execute 测试 ──────────────────────────────────────

    #[tokio::test]
    async fn test_compact_empty_history_returns_original_with_error_event() {
        // Arrange: 空历史 + mock sink
        let sink = Arc::new(MockEventSink::new());
        let ctx = make_ctx(sink.clone(), vec![]);
        let cmd = CompactCommand;

        // Act
        let result = cmd.execute(ctx).await;

        // Assert: 返回空消息 + EndTurn
        assert_eq!(result.messages.len(), 0);
        assert_eq!(result.stop_reason, PromptStopReason::EndTurn);

        // 应推送 CompactError 事件
        let events = sink.events();
        assert_eq!(events.len(), 1);
        assert!(
            events[0].1.contains("compact_error"),
            "空历史应推送 compact_error，实际: {}",
            events[0].1
        );
        assert!(
            events[0].1.contains("no history to compact"),
            "错误消息应包含 'no history to compact'，实际: {}",
            events[0].1
        );
    }

    #[tokio::test]
    async fn test_compact_no_model_returns_original_with_error_event() {
        // Arrange: 有历史但无 compact_model（默认 None）
        let sink = Arc::new(MockEventSink::new());
        let history = vec![BaseMessage::human("你好"), BaseMessage::ai("世界")];
        let ctx = make_ctx(sink.clone(), history.clone());
        let cmd = CompactCommand;

        // Act
        let result = cmd.execute(ctx).await;

        // Assert: 返回原消息 + EndTurn
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.stop_reason, PromptStopReason::EndTurn);

        // 应推送 CompactError 事件
        let events = sink.events();
        assert_eq!(events.len(), 1);
        assert!(
            events[0].1.contains("compact_error"),
            "无模型应推送 compact_error，实际: {}",
            events[0].1
        );
        assert!(
            events[0].1.contains("no model available"),
            "错误消息应包含 'no model available'，实际: {}",
            events[0].1
        );
    }

    // ── CompactCommand 属性测试 ──────────────────────────────────────────

    #[test]
    fn test_compact_command_name_and_aliases() {
        let cmd = CompactCommand;

        assert_eq!(cmd.name(), "compact");
        let aliases = cmd.aliases();
        assert!(aliases.contains(&"compress"), "应包含 compress 别名");
        assert_eq!(cmd.kind(), CommandKind::Immediate);
        assert!(!cmd.description().is_empty());
    }
}
