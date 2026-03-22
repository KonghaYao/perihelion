use std::sync::Arc;

use async_trait::async_trait;
use rust_create_agent::agent::events::AgentEventHandler;
use rust_create_agent::agent::react::{AgentInput, ReactLLM, Reasoning};
use rust_create_agent::agent::state::AgentState;
use rust_create_agent::agent::ReActAgent;
use rust_create_agent::messages::BaseMessage;
use rust_create_agent::tools::BaseTool;

use crate::agent_define::AgentDefineMiddleware;
use crate::claude_agent_parser::{parse_agent_file, ToolsValue};
use crate::tools::ArcToolWrapper;

/// SubAgentTool - 实现 `launch_agent` 工具，允许 LLM 将子任务委派给专门的子 agent 执行
///
/// LLM 通过调用此工具并传入 `agent_id` 和 `task`，触发对应 agent 定义文件的执行。
/// 子 agent 继承父 agent 的工具集（根据 tools/disallowedTools 字段过滤），
/// 不包含 HITL 中间件，执行结果以字符串形式返回给父 agent。
pub struct SubAgentTool {
    /// 父 agent 工具集（Arc 共享，只读）
    parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
    /// 父 agent 事件处理器（透传子 agent 事件）
    event_handler: Option<Arc<dyn AgentEventHandler>>,
    /// LLM 工厂函数，每次为子 agent 创建独立 LLM 实例
    llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
}

impl SubAgentTool {
    pub fn new(
        parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
        event_handler: Option<Arc<dyn AgentEventHandler>>,
        llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    ) -> Self {
        Self {
            parent_tools,
            event_handler,
            llm_factory,
        }
    }

    /// 根据 agent 定义的 tools/disallowedTools 字段，从父工具集中过滤出子 agent 可用的工具
    ///
    /// 规则：
    /// - tools 为 Empty → 继承所有父工具（但始终排除 launch_agent 自身，防止递归）
    /// - tools 有值    → 仅保留名称在列表中的工具（同时排除 launch_agent）
    /// - 再从结果中移除 disallowed_tools 列出的工具
    fn filter_tools(
        &self,
        allowed: &ToolsValue,
        disallowed: &ToolsValue,
    ) -> Vec<Box<dyn BaseTool>> {
        let allowed_list = allowed.to_vec();
        let disallowed_list = disallowed.to_vec();

        self.parent_tools
            .iter()
            .filter(|tool| {
                let name = tool.name();
                // 始终排除 launch_agent，防止递归
                if name == "launch_agent" {
                    return false;
                }
                // 若 allowed_list 非空，则仅保留列表中的工具
                if !allowed_list.is_empty() && !allowed_list.iter().any(|n| n == name) {
                    return false;
                }
                // 排除 disallowed 列表中的工具
                if disallowed_list.iter().any(|n| n == name) {
                    return false;
                }
                true
            })
            .map(|tool| Box::new(ArcToolWrapper(Arc::clone(tool))) as Box<dyn BaseTool>)
            .collect()
    }
}

#[async_trait]
impl BaseTool for SubAgentTool {
    fn name(&self) -> &str {
        "launch_agent"
    }

    fn description(&self) -> &str {
        "委派子任务给专门配置的子 agent 执行。子 agent 根据 .claude/agents/{agent_id}.md 或 agents/{agent_id}.md 中的配置文件运行，独立完成任务后返回结果。适用于需要专门技能或独立上下文的子任务。"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["agent_id", "task"],
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "Agent 定义文件名（不含 .md 扩展名），如 'code-reviewer'。对应 .claude/agents/{agent_id}.md 文件"
                },
                "task": {
                    "type": "string",
                    "description": "委派给子 agent 的任务描述，应清晰说明要完成的工作"
                },
                "cwd": {
                    "type": "string",
                    "description": "子 agent 工作目录，默认继承父 agent 的当前工作目录"
                }
            }
        })
    }

    async fn invoke(
        &self,
        input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let agent_id = match input.get("agent_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => return Ok("错误：缺少必需参数 agent_id".to_string()),
        };
        let task = match input.get("task").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => return Ok("错误：缺少必需参数 task".to_string()),
        };
        // cwd 默认使用当前目录
        let cwd = input
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or(".")
            .to_string();

        // 1. 查找 agent 定义文件
        let agent_path = AgentDefineMiddleware::candidate_paths(&cwd, &agent_id)
            .into_iter()
            .find(|p| p.is_file());

        let agent_path = match agent_path {
            Some(p) => p,
            None => {
                return Ok(format!(
                    "错误：找不到 agent 定义文件 '{}'，请检查 .claude/agents/ 目录",
                    agent_id
                ))
            }
        };

        // 2. 读取并解析 agent 定义文件
        let content = match std::fs::read_to_string(&agent_path) {
            Ok(c) => c,
            Err(e) => return Ok(format!("错误：读取 agent 定义文件失败：{}", e)),
        };
        let agent_def = match parse_agent_file(&content) {
            Some(a) => a,
            None => {
                return Ok(format!(
                    "错误：解析 agent 定义文件 '{}' 失败，请检查 YAML frontmatter 格式",
                    agent_path.display()
                ))
            }
        };

        // 3. 工具过滤
        let filtered_tools =
            self.filter_tools(&agent_def.frontmatter.tools, &agent_def.frontmatter.disallowed_tools);

        // 4. 组装子 ReActAgent
        let llm = (self.llm_factory)();
        // 若 system_prompt 非空，用其覆盖默认 system
        let llm: Box<dyn ReactLLM + Send + Sync> = if !agent_def.system_prompt.is_empty() {
            Box::new(WithSystemLlm {
                inner: llm,
                system: agent_def.system_prompt.clone(),
            })
        } else {
            llm
        };

        let max_iterations = agent_def.frontmatter.max_turns.unwrap_or(20) as usize;

        let mut agent_builder = ReActAgent::new(llm).max_iterations(max_iterations);

        // 注册过滤后的工具
        for tool in filtered_tools {
            agent_builder = agent_builder.register_tool(tool);
        }

        // 透传父 agent 事件处理器
        if let Some(handler) = &self.event_handler {
            agent_builder = agent_builder.with_event_handler(Arc::clone(handler));
        }

        // 5. 执行子 agent
        let mut state = AgentState::new(cwd.clone());
        match agent_builder
            .execute(AgentInput::text(task), &mut state, None)
            .await
        {
            Ok(output) => Ok(format_subagent_result(&output)),
            Err(e) => Ok(format!("子 agent 执行失败：{}", e)),
        }
    }
}

/// 将子 agent 的执行结果格式化为摘要字符串返回给父 agent。
///
/// 摘要格式：
/// - 若有工具调用，列出工具名称（不含中间结果，避免 token 膨胀）
/// - 保留最终回答文本
fn format_subagent_result(output: &rust_create_agent::agent::react::AgentOutput) -> String {
    if output.tool_calls.is_empty() {
        return output.text.clone();
    }

    let tool_summary = output
        .tool_calls
        .iter()
        .map(|(call, _result)| call.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "[子 agent 执行了 {} 个工具调用: {}]\n\n{}",
        output.tool_calls.len(),
        tool_summary,
        output.text
    )
}

/// WithSystemLlm - 为子 agent 注入 system prompt 的 LLM 包装
struct WithSystemLlm {
    inner: Box<dyn ReactLLM + Send + Sync>,
    system: String,
}

#[async_trait]
impl ReactLLM for WithSystemLlm {
    async fn generate_reasoning(
        &self,
        messages: &[BaseMessage],
        tools: &[&dyn BaseTool],
    ) -> rust_create_agent::error::AgentResult<Reasoning> {
        // 在消息开头注入 system 消息（如果还没有）
        let mut msgs = messages.to_vec();
        let has_system = msgs.iter().any(|m| matches!(m, BaseMessage::System { .. }));
        if !has_system {
            msgs.insert(0, BaseMessage::system(self.system.as_str()));
        }
        self.inner.generate_reasoning(&msgs, tools).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::agent::react::Reasoning;
    use tempfile::tempdir;

    // Mock LLM：直接返回最终答案
    struct EchoLLM;

    #[async_trait::async_trait]
    impl ReactLLM for EchoLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            _tools: &[&dyn BaseTool],
        ) -> rust_create_agent::error::AgentResult<Reasoning> {
            let last = messages.last().map(|m| m.content()).unwrap_or_default();
            Ok(Reasoning::with_answer("", format!("echo: {}", last)))
        }
    }

    fn make_tool(name: &'static str) -> Arc<dyn BaseTool> {
        struct DummyTool(&'static str);

        #[async_trait::async_trait]
        impl BaseTool for DummyTool {
            fn name(&self) -> &str {
                self.0
            }
            fn description(&self) -> &str {
                "dummy"
            }
            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn invoke(
                &self,
                _input: serde_json::Value,
            ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
                Ok(format!("{} result", self.0))
            }
        }

        Arc::new(DummyTool(name))
    }

    fn make_subagent_tool(parent_tools: Vec<Arc<dyn BaseTool>>) -> SubAgentTool {
        SubAgentTool::new(
            Arc::new(parent_tools),
            None,
            Arc::new(|| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        )
    }

    #[test]
    fn test_tool_name() {
        let t = make_subagent_tool(vec![]);
        assert_eq!(t.name(), "launch_agent");
    }

    #[test]
    fn test_tool_parameters_has_required_fields() {
        let t = make_subagent_tool(vec![]);
        let params = t.parameters();
        let required = params["required"].as_array().unwrap();
        let names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(names.contains(&"agent_id"));
        assert!(names.contains(&"task"));
    }

    #[tokio::test]
    async fn test_tool_agent_not_found() {
        let t = make_subagent_tool(vec![]);
        let result = t
            .invoke(serde_json::json!({
                "agent_id": "nonexistent-agent",
                "task": "do something",
                "cwd": "/tmp"
            }))
            .await
            .unwrap();
        assert!(result.contains("找不到"), "应返回找不到错误: {}", result);
    }

    #[tokio::test]
    async fn test_tool_filter_inherit_all() {
        // tools 为 Empty → 继承所有父工具，但排除 launch_agent
        let parent_tools = vec![
            make_tool("read_file"),
            make_tool("write_file"),
            make_tool("launch_agent"), // 这个应该被排除
        ];
        let t = make_subagent_tool(parent_tools);

        let allowed = ToolsValue::Empty;
        let disallowed = ToolsValue::Empty;
        let filtered = t.filter_tools(&allowed, &disallowed);
        let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(!names.contains(&"launch_agent"), "launch_agent 不应被继承");
    }

    #[test]
    fn test_tool_filter_allowlist() {
        // tools 有值 → 仅保留指定工具
        let parent_tools = vec![
            make_tool("read_file"),
            make_tool("write_file"),
            make_tool("glob_files"),
        ];
        let t = make_subagent_tool(parent_tools);

        let allowed = ToolsValue::List(vec!["read_file".to_string(), "glob_files".to_string()]);
        let disallowed = ToolsValue::Empty;
        let filtered = t.filter_tools(&allowed, &disallowed);
        let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"glob_files"));
        assert!(!names.contains(&"write_file"), "write_file 不在 allowlist 中应被排除");
    }

    #[test]
    fn test_tool_filter_disallow() {
        // disallowedTools → 从继承集合中排除
        let parent_tools = vec![
            make_tool("read_file"),
            make_tool("write_file"),
            make_tool("edit_file"),
        ];
        let t = make_subagent_tool(parent_tools);

        let allowed = ToolsValue::Empty;
        let disallowed =
            ToolsValue::List(vec!["write_file".to_string(), "edit_file".to_string()]);
        let filtered = t.filter_tools(&allowed, &disallowed);
        let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"read_file"));
        assert!(!names.contains(&"write_file"), "write_file 在 disallow 列表中应被排除");
        assert!(!names.contains(&"edit_file"), "edit_file 在 disallow 列表中应被排除");
    }

    #[tokio::test]
    async fn test_tool_executes_with_valid_agent_file() {
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("test-agent.md"),
            "---\nname: test-agent\ndescription: A test agent\n---\n\nYou are a test agent.\n",
        )
        .unwrap();

        let t = make_subagent_tool(vec![]);
        let result = t
            .invoke(serde_json::json!({
                "agent_id": "test-agent",
                "task": "hello",
                "cwd": dir.path().to_str().unwrap()
            }))
            .await
            .unwrap();
        // EchoLLM 返回 echo: hello
        assert!(result.contains("echo"), "应收到子 agent 的输出: {}", result);
    }

    #[tokio::test]
    async fn test_launch_agent_tool_in_list() {
        // 验证 SubAgentTool 的工具名称正确，可加入工具列表
        let t = make_subagent_tool(vec![]);
        assert_eq!(t.name(), "launch_agent");
        let def = t.definition();
        assert_eq!(def.name, "launch_agent");
    }
}
