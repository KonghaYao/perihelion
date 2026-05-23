//! Compact 中间件 — 在 ReAct 循环内原地压缩上下文
//!
//! `before_model` 钩子: 每轮 LLM 调用前检查 token 阈值，超过时执行
//! micro/full compact。compact 后不改变控制流，ReAct 循环自然继续。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::mpsc;
use tracing::{info, warn};

use peri_agent::agent::compact::config::CompactConfig;
use peri_agent::agent::compact::{full_compact, micro_compact_enhanced, re_inject};
use peri_agent::agent::events::{AgentEvent as ExecutorEvent, CompactFileInfo};
use peri_agent::agent::state::State;
use peri_agent::agent::token::ContextBudget;
use peri_agent::agent::AgentCancellationToken;
use peri_agent::error::AgentResult;
use peri_agent::llm::BaseModel;
use peri_agent::messages::BaseMessage;
use peri_agent::middleware::r#trait::Middleware;

use crate::hooks::{self, RegisteredHook};

/// Compact 中间件
///
/// 在 `before_model` 钩子中检查 token 使用量，触发 micro/full compact。
/// full compact 使用 LLM 生成对话摘要，re_inject 恢复关键文件/skills。
pub struct CompactMiddleware {
    /// LLM 模型（full compact 摘要生成用），None 则跳过 full compact
    model: Option<Arc<dyn BaseModel>>,
    /// Compact 配置
    config: CompactConfig,
    /// 上下文窗口预算
    budget: ContextBudget,
    /// 工作目录（re_inject 需要）
    cwd: String,
    /// 事件通道
    event_tx: Arc<Mutex<Option<mpsc::UnboundedSender<ExecutorEvent>>>>,
    /// 取消令牌
    cancel: AgentCancellationToken,
    /// Hooks（PreCompact/PostCompact）
    hooks: Vec<RegisteredHook>,
    /// Session ID（hook 上下文）
    session_id: String,
    /// Provider 名称（hook 上下文）
    provider_name: String,
    /// micro compact 在当前 prompt 执行中是否已触发过。
    /// 每次 execute_prompt 创建新的 CompactMiddleware 实例，天然 per-prompt 作用域。
    /// 防止 micro compact 反复触发（压缩量 < 新增量时会在每轮重复触发）。
    micro_compact_done: AtomicBool,
}

impl CompactMiddleware {
    /// 创建新的 CompactMiddleware
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        model: Option<Arc<dyn BaseModel>>,
        config: CompactConfig,
        budget: ContextBudget,
        cwd: String,
        event_tx: Arc<Mutex<Option<mpsc::UnboundedSender<ExecutorEvent>>>>,
        cancel: AgentCancellationToken,
        hooks: Vec<RegisteredHook>,
        session_id: String,
        provider_name: String,
    ) -> Self {
        Self {
            model,
            config,
            budget,
            cwd,
            event_tx,
            cancel,
            hooks,
            session_id,
            provider_name,
            micro_compact_done: AtomicBool::new(false),
        }
    }

    fn is_disabled(&self) -> bool {
        std::env::var("DISABLE_COMPACT").is_ok()
            || std::env::var("DISABLE_AUTO_COMPACT").is_ok()
            || !self.config.auto_compact_enabled
    }

    fn send_event(&self, event: ExecutorEvent) {
        if let Some(tx) = self.event_tx.lock().unwrap().as_ref() {
            let _ = tx.send(event);
        }
    }

    /// 提取 re_inject 结果中的文件信息
    fn extract_file_info(messages: &[BaseMessage]) -> Vec<CompactFileInfo> {
        let mut files = Vec::new();
        for msg in messages {
            let content = msg.content();
            if let Some(rest) = content.strip_prefix("[最近读取的文件: ") {
                let path = rest.lines().next().unwrap_or("");
                let line_count = rest.lines().count().saturating_sub(1);
                if !path.is_empty() {
                    files.push(CompactFileInfo {
                        path: path.to_string(),
                        lines: line_count,
                    });
                }
            }
        }
        files
    }

    /// 提取 re_inject 结果中的 skill 名称
    fn extract_skill_names(messages: &[BaseMessage]) -> Vec<String> {
        let mut skills = Vec::new();
        for msg in messages {
            let content = msg.content();
            if let Some(rest) = content.strip_prefix("[激活的 Skill 指令: ") {
                let name = rest.lines().next().unwrap_or("");
                if !name.is_empty() {
                    skills.push(name.to_string());
                }
            }
        }
        skills
    }

    async fn fire_hooks(&self, event: hooks::types::HookEvent, msg_count: usize) {
        hooks::middleware::fire_standalone_lifecycle_hooks(
            &self.hooks,
            event,
            &self.cwd,
            &self.session_id,
            "",
            &self.provider_name,
            Some(msg_count),
        )
        .await
    }

    /// 执行 full compact：full_compact + re_inject + hooks + 事件
    async fn do_full_compact(&self, state: &mut impl State) -> AgentResult<()> {
        let model = match &self.model {
            Some(m) => m,
            None => {
                warn!("CompactMiddleware: model is None, skipping full compact");
                self.send_event(ExecutorEvent::CompactError {
                    message: "模型不可用，跳过压缩".to_string(),
                });
                return Ok(());
            }
        };

        let messages = state.messages().to_vec();
        let msg_count = messages.len();

        info!(msg_count, "CompactMiddleware: 触发 full compact");

        // PreCompact hooks
        self.fire_hooks(hooks::types::HookEvent::PreCompact, msg_count)
            .await;

        self.send_event(ExecutorEvent::CompactStarted);

        // full_compact with cancellation
        let compact_result = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => {
                self.send_event(ExecutorEvent::CompactError {
                    message: "已取消".to_string(),
                });
                self.fire_hooks(hooks::types::HookEvent::PostCompact, msg_count).await;
                return Ok(());
            }
            result = full_compact(&messages, model.as_ref(), &self.config, "") => {
                match result {
                    Ok(r) => r,
                    Err(e) => {
                        warn!(error = %e, "CompactMiddleware: full_compact 失败");
                        self.send_event(ExecutorEvent::CompactError {
                            message: e.to_string(),
                        });
                        self.fire_hooks(hooks::types::HookEvent::PostCompact, msg_count).await;
                        return Ok(());
                    }
                }
            }
        };

        // Cancel check before re_inject
        if self.cancel.is_cancelled() {
            self.send_event(ExecutorEvent::CompactError {
                message: "已取消".to_string(),
            });
            self.fire_hooks(hooks::types::HookEvent::PostCompact, msg_count)
                .await;
            return Ok(());
        }

        info!(
            summary_len = compact_result.summary.len(),
            "CompactMiddleware: full_compact 完成"
        );

        // re_inject
        let re_inject_result = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => {
                self.send_event(ExecutorEvent::CompactError {
                    message: "已取消".to_string(),
                });
                self.fire_hooks(hooks::types::HookEvent::PostCompact, msg_count).await;
                return Ok(());
            }
            result = re_inject(&messages, &self.config, &self.cwd) => result,
        };

        info!(
            files_injected = re_inject_result.files_injected,
            skills_injected = re_inject_result.skills_injected,
            "CompactMiddleware: re_inject 完成"
        );

        let files = Self::extract_file_info(&re_inject_result.messages);
        let skills = Self::extract_skill_names(&re_inject_result.messages);

        // 保留原始头部 System 消息（with_system_prompt + before_agent 注入）
        // compact 整体替换 messages 时这些消息会丢失，但它们包含系统提示词、
        // CLAUDE.md、agent 定义等关键内���，必须在 compact 后重新前置。
        let system_prefix: Vec<BaseMessage> = messages
            .iter()
            .take_while(|m| m.is_system())
            .cloned()
            .collect();

        // 摘要作为 Human 消息（与 Claude Code 实现对齐）。
        // 原因：LLM 适配器将 System 消息提取到 system 字段，不进入 messages 数组。
        // 若摘要为 System 类型，compact 后 messages 数组可能只有 system 角色消息，
        // DeepSeek/OpenAI 兼容 API 要求至少一条 user/assistant 消息，否则返回 400。
        let summary_content = format!(
            "{}\n\n[上下文已压缩，请根据摘要继续工作]",
            compact_result.summary
        );
        let mut new_messages = vec![BaseMessage::human(summary_content)];
        new_messages.extend(re_inject_result.messages.clone());

        // 将系统提示词前缀重新前置到 new_messages 头部
        for sys_msg in system_prefix.into_iter().rev() {
            new_messages.insert(0, sys_msg);
        }

        self.send_event(ExecutorEvent::CompactCompleted {
            summary: compact_result.summary.clone(),
            files: files.clone(),
            skills: skills.clone(),
            micro_cleared: 0,
            messages: new_messages.clone(),
        });

        // PostCompact hooks
        self.fire_hooks(hooks::types::HookEvent::PostCompact, msg_count)
            .await;

        // Replace state messages and reset tracker
        *state.messages_mut() = new_messages;
        state.token_tracker_mut().reset();

        info!("CompactMiddleware: full compact 完成，状态已更新");
        Ok(())
    }

    /// 执行 micro compact：原地压缩旧工具结果
    fn do_micro_compact(&self, state: &mut impl State) {
        let messages = state.messages_mut();
        let cleared = micro_compact_enhanced(&self.config, messages);
        if cleared > 0 {
            info!(cleared, "CompactMiddleware: micro-compact 完成");
            self.send_event(ExecutorEvent::CompactCompleted {
                summary: String::new(),
                files: vec![],
                skills: vec![],
                micro_cleared: cleared,
                messages: messages.to_vec(),
            });
        }
    }
}

#[async_trait]
impl<S: State> Middleware<S> for CompactMiddleware {
    fn name(&self) -> &str {
        "CompactMiddleware"
    }

    /// LLM 调用前：检查 token 阈值，必要时执行 compact
    async fn before_model(&self, state: &mut S) -> AgentResult<()> {
        if self.is_disabled() {
            return Ok(());
        }

        // Step 1: 不可变借用读取阈值（在块作用域内）
        let (should_full, should_micro) = {
            let tracker = state.token_tracker();
            let full = self.budget.should_auto_compact(tracker);
            // micro compact 每轮只触发一次，防止压缩量 < 新增量时的反复触发振荡
            let micro = !full
                && self.budget.should_warn(tracker)
                && !self.micro_compact_done.load(Ordering::Relaxed);
            (full, micro)
        };

        // Step 2: 可变借用（tracker 引用已 drop）
        if should_full {
            self.do_full_compact(state).await?;
        } else if should_micro {
            self.micro_compact_done.store(true, Ordering::Relaxed);
            self.do_micro_compact(state);
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "compact_middleware_test.rs"]
mod tests;
