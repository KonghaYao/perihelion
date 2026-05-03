use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use rust_create_agent::agent::events::AgentEventHandler;
use rust_create_agent::agent::react::{AgentInput, ReactLLM};
use rust_create_agent::agent::state::AgentState;
use rust_create_agent::agent::{AgentCancellationToken, ReActAgent};
use rust_create_agent::messages::BaseMessage;
use rust_create_agent::tools::BaseTool;

use crate::agent_define::{AgentDefineMiddleware, AgentOverrides};
use crate::agents_md::AgentsMdMiddleware;
use crate::claude_agent_parser::{parse_agent_file, ToolsValue};
use crate::middleware::todo::TodoMiddleware;
use crate::skills::SkillsMiddleware;
use crate::subagent::skill_preload::SkillPreloadMiddleware;
use crate::tools::ArcToolWrapper;
use tokio::sync::mpsc;

/// SubAgentTool - implements the `Agent` tool, allowing LLM to delegate sub-tasks to specialized sub-agents
///
/// LLM calls this tool with `subagent_type` and `prompt` to trigger execution of the corresponding agent definition file.
/// The sub-agent inherits the parent's tool set (filtered by tools/disallowedTools fields),
/// does not include HITL middleware, and returns execution results as a string to the parent agent.

const AGENT_DESCRIPTION: &str = r#"Launch a sub-agent with an independent context to handle a specialized sub-task. The sub-agent executes based on the configuration defined in .claude/agents/{subagent_type}.md or .claude/agents/{subagent_type}/agent.md.

Fork mode (fork: true):
- Inherits the parent agent's full conversation history, system prompt, and tool set
- The prompt is treated as a directive within the existing context, not a standalone briefing
- Do NOT re-explain background that is already in the conversation history
- Use for tasks that require context from the ongoing conversation (e.g., continuing a multi-file refactor)
- The forked agent follows a structured output format: Scope, Result, Key files, Files changed

Usage:
- Provide a clear, self-contained task description via the prompt parameter. The sub-agent has no access to the parent conversation history
- Specify subagent_type matching an existing agent definition file. When not provided, creates a fork of the current agent
- The sub-agent inherits the parent's tool set by default, excluding Agent itself (to prevent recursion)
- Agent definitions may restrict available tools via the tools and disallowedTools fields in frontmatter
- The sub-agent executes in isolated state — it cannot access the parent's message history or intermediate results

When to use:
- For tasks that benefit from independent context isolation (e.g., code review while working on a different feature)
- For tasks requiring specialized persona or behavior defined in agent configuration files
- For parallelizable sub-tasks that do not depend on each other's results
- When you need to break a complex task into smaller, independently executable pieces

Return format:
- If the sub-agent made tool calls, the result includes a summary of tools used followed by the final response
- If no tool calls were made, only the final response text is returned"#;
pub struct SubAgentTool {
    /// Parent agent tool set (Arc shared, read-only)
    parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
    /// Parent agent event handler (transparent forwarding of sub-agent events)
    event_handler: Option<Arc<dyn AgentEventHandler>>,
    /// Parent agent working directory (inherited when LLM does not specify cwd)
    parent_cwd: String,
    /// LLM factory function, creates independent LLM instance for each sub-agent (no system, injected via with_system_prompt())
    /// Parameter is optional model alias (e.g., "haiku"/"sonnet"/"opus"), None means inherit parent model
    llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    /// System prompt builder: (agent overrides, cwd) -> system prompt string
    ///
    /// The returned content is injected into the sub-agent's state messages via `with_system_prompt()`,
    /// making it visible in Langfuse and other tracing tools. When None, no system prompt is injected.
    system_builder: Option<Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>>,
    /// Optional cancellation token for interrupting sub-agent execution
    cancel: Option<AgentCancellationToken>,
    /// Shared reference to parent agent message snapshot (used by Fork path)
    /// RwLock.read() obtains a deep copy, RwLock.write() is updated by SubAgentMiddleware::before_agent
    parent_messages: Option<Arc<RwLock<Vec<BaseMessage>>>>,
}

impl SubAgentTool {
    pub fn new(
        parent_tools: Arc<Vec<Arc<dyn BaseTool>>>,
        event_handler: Option<Arc<dyn AgentEventHandler>>,
        llm_factory: Arc<dyn Fn(Option<&str>) -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
        parent_cwd: String,
    ) -> Self {
        Self {
            parent_tools,
            event_handler,
            llm_factory,
            parent_cwd,
            system_builder: None,
            cancel: None,
            parent_messages: None,
        }
    }

    /// Set system prompt builder for injecting full system prompt including tone/proactiveness to sub-agent
    pub fn with_system_builder(
        mut self,
        builder: Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>,
    ) -> Self {
        self.system_builder = Some(builder);
        self
    }

    /// Set cancellation token for supporting user interruption of sub-agent execution
    pub fn with_cancel(mut self, cancel: AgentCancellationToken) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Set shared parent message reference, Fork path obtains deep copy via RwLock.read()
    pub fn with_parent_messages(
        mut self,
        messages: Arc<RwLock<Vec<BaseMessage>>>,
    ) -> Self {
        self.parent_messages = Some(messages);
        self
    }

    /// Extract AgentOverrides from already-parsed agent_def to avoid redundant I/O
    fn overrides_from_agent_def(
        system_prompt: &str,
        tone: &Option<String>,
        proactiveness: &Option<String>,
    ) -> Option<AgentOverrides> {
        let persona = if system_prompt.is_empty() {
            None
        } else {
            Some(system_prompt.to_string())
        };
        let overrides = AgentOverrides {
            persona,
            tone: tone.clone(),
            proactiveness: proactiveness.clone(),
        };
        if overrides.is_empty() {
            None
        } else {
            Some(overrides)
        }
    }

    /// Filter available tools from parent tool set based on agent definition's tools/disallowedTools fields
    ///
    /// Rules:
    /// - tools is Empty -> inherit all parent tools (but always exclude Agent itself to prevent recursion)
    /// - tools has value -> only keep tools in the list (also exclude Agent)
    /// - then remove tools listed in disallowed_tools from the result
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
                let name_lower = name.to_lowercase();
                // Always exclude Agent to prevent recursion
                if name == "Agent" {
                    return false;
                }
                // If allowed_list is non-empty, only keep tools in the list (case-insensitive)
                if !allowed_list.is_empty()
                    && !allowed_list.iter().any(|n| n.to_lowercase() == name_lower)
                {
                    return false;
                }
                // Exclude tools in the disallowed list (case-insensitive)
                if disallowed_list
                    .iter()
                    .any(|n| n.to_lowercase() == name_lower)
                {
                    return false;
                }
                true
            })
            .map(|tool| Box::new(ArcToolWrapper(Arc::clone(tool))) as Box<dyn BaseTool>)
            .collect()
    }

    /// Fork path: sub-agent inherits parent's full message history + system prompt + tool set
    async fn invoke_fork(
        &self,
        prompt: &str,
        cwd: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // 1. Obtain deep copy of parent messages
        let parent_msgs: Vec<BaseMessage> = match &self.parent_messages {
            Some(pm) => pm.read().clone(),
            None => return Ok("Error: Fork path requires parent message history, but parent_messages is not set".to_string()),
        };

        // 2. Build fork directive Human message
        let fork_directive = format!(
            "<fork_directive>\n\
             You are a forked agent continuing from the parent conversation.\n\
             You have full access to the conversation history above.\n\
             \n\
             RULES:\n\
             1. Do NOT spawn sub-agents — execute directly using your tools\n\
             2. Do NOT ask questions — act on the directive below\n\
             3. Stay strictly within your assigned scope\n\
             4. Report structured facts, then stop\n\
             5. Keep your response under 500 words unless specified otherwise\n\
             \n\
             Output format:\n\
               Scope: <your assigned scope in one sentence>\n\
               Result: <the answer or key findings>\n\
               Key files: <relevant file paths>\n\
               Files changed: <list if you modified files>\n\
             </fork_directive>\n\n\
             {prompt}"
        );

        // 3. Build child AgentState using deep copy of parent messages
        let mut fork_state = AgentState::with_messages(cwd.to_string(), parent_msgs);

        // 4. Assemble child ReActAgent (same middleware chain as Normal path)
        let llm = (self.llm_factory)(None);
        let mut agent_builder = ReActAgent::new(llm).max_iterations(200);

        // Middleware chain: AgentsMd -> Skills -> SkillPreload -> Todo
        agent_builder = agent_builder
            .add_middleware(Box::new(AgentsMdMiddleware::new()))
            .add_middleware(Box::new(SkillsMiddleware::new().with_global_config()))
            .add_middleware(Box::new(TodoMiddleware::new({
                let (tx, _rx) = mpsc::channel(8);
                tx
            })));

        // 5. Inject system prompt (obtained via system_builder, consistent with Normal path)
        if let Some(ref builder) = self.system_builder {
            let system_content = builder(None, cwd);
            agent_builder = agent_builder.with_system_prompt(system_content);
        }

        // 6. Register full parent tools (no filtering, including Agent itself to maintain cache hit)
        for tool in self.parent_tools.iter() {
            agent_builder = agent_builder.register_tool(
                Box::new(ArcToolWrapper(Arc::clone(tool)))
                    as Box<dyn BaseTool>
            );
        }

        // 7. Transparently forward parent event handler
        if let Some(handler) = &self.event_handler {
            agent_builder = agent_builder.with_event_handler(Arc::clone(handler));
        }

        // 8. Execute (input = fork directive, appended as Human message by execute())
        match agent_builder
            .execute(
                AgentInput::text(fork_directive),
                &mut fork_state,
                self.cancel.clone(),
            )
            .await
        {
            Ok(output) => Ok(format_subagent_result(&output)),
            Err(rust_create_agent::error::AgentError::Interrupted) => {
                Ok("Fork sub-agent execution was interrupted".to_string())
            }
            Err(e) => {
                let msg = format!("Fork sub-agent execution failed: {}", e);
                Err(msg.into())
            }
        }
    }
}

#[async_trait]
impl BaseTool for SubAgentTool {
    fn name(&self) -> &str {
        "Agent"
    }

    fn description(&self) -> &str {
        AGENT_DESCRIPTION
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["prompt"],
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The task description to delegate to the sub-agent. Must be clear and self-contained, as the sub-agent has no access to the parent conversation history. Include all necessary context"
                },
                "description": {
                    "type": "string",
                    "description": "A short description of the task (3-5 words), used for UI display and logging"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The agent type/ID matching an existing agent definition file at .claude/agents/{subagent_type}.md or .claude/agents/{subagent_type}/agent.md. When empty or not provided, creates a fork of the current agent with all tools"
                },
                "name": {
                    "type": "string",
                    "description": "A short alias for the sub-agent, used for UI identification"
                },
                "isolation": {
                    "type": "string",
                    "description": "Isolation mode for the sub-agent. Use 'worktree' to create an isolated git worktree. Currently reserved for future use"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run the sub-agent in the background. Currently reserved for future use"
                },
                "cwd": {
                    "type": "string",
                    "description": "The working directory for the sub-agent. Defaults to inheriting the parent agent's current working directory if not specified"
                },
                "fork": {
                    "type": "boolean",
                    "description": "Set to true to fork the current agent with full conversation context. The forked agent inherits all messages, tools, and system prompt from the parent. Use when the task requires context from the ongoing conversation"
                }
            }
        })
    }

    async fn invoke(
        &self,
        input: serde_json::Value,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let prompt = match input.get("prompt").and_then(|v| v.as_str()) {
            Some(p) => p.to_string(),
            None => return Ok("Error: missing required parameter prompt".to_string()),
        };
        let subagent_type = input.get("subagent_type").and_then(|v| v.as_str()).map(|s| s.to_string());
        let _description = input.get("description").and_then(|v| v.as_str());
        let _name = input.get("name").and_then(|v| v.as_str());
        let _isolation = input.get("isolation").and_then(|v| v.as_str());
        let _run_in_background = input.get("run_in_background").and_then(|v| v.as_bool()).unwrap_or(false);
        // cwd defaults to inheriting parent agent's working directory
        let cwd = input
            .get("cwd")
            .and_then(|v| v.as_str())
            .unwrap_or(&self.parent_cwd)
            .to_string();

        // Fork detection branch
        let is_fork = input.get("fork").and_then(|v| v.as_bool()).unwrap_or(false);
        if is_fork {
            return self.invoke_fork(&prompt, &cwd).await;
        }

        // 1. Find agent definition file
        let agent_id = match &subagent_type {
            Some(id) => id.clone(),
            None => return Ok("Error: please provide subagent_type parameter to specify the agent type".to_string()),
        };

        let agent_path = AgentDefineMiddleware::candidate_paths(&cwd, &agent_id)
            .into_iter()
            .find(|p| p.is_file());

        let agent_path = match agent_path {
            Some(p) => p,
            None => {
                return Ok(format!(
                    "Error: cannot find agent definition file '{}', please check .claude/agents/ directory",
                    agent_id
                ))
            }
        };

        // 2. Read and parse agent definition file
        let content = match std::fs::read_to_string(&agent_path) {
            Ok(c) => c,
            Err(e) => return Ok(format!("Error: failed to read agent definition file: {}", e)),
        };
        let agent_def = match parse_agent_file(&content) {
            Some(a) => a,
            None => {
                return Ok(format!(
                    "Error: failed to parse agent definition file '{}', please check YAML frontmatter format",
                    agent_path.display()
                ))
            }
        };

        // 3. Tool filtering
        let filtered_tools = self.filter_tools(
            &agent_def.frontmatter.tools,
            &agent_def.frontmatter.disallowed_tools,
        );

        // 4. Assemble child ReActAgent
        // Extract model alias: non-"inherit" and non-empty passed to factory, None means inherit parent model
        let model_alias: Option<&str> = agent_def
            .frontmatter
            .model
            .as_deref()
            .filter(|m| !m.is_empty() && *m != "inherit");
        let llm = (self.llm_factory)(model_alias);
        let raw_turns = agent_def.frontmatter.max_turns.unwrap_or(200);
        let max_iterations = if raw_turns == 0 {
            200
        } else {
            raw_turns as usize
        };

        let mut agent_builder = ReActAgent::new(llm).max_iterations(max_iterations);

        // 5. Add missing context middleware (aligned with parent agent)
        //    Registration order: AgentsMdMiddleware -> SkillsMiddleware -> TodoMiddleware
        //    TodoMiddleware's _rx is immediately discarded, send failures are silently ignored
        agent_builder = agent_builder
            .add_middleware(Box::new(AgentsMdMiddleware::new()))
            .add_middleware(Box::new(SkillsMiddleware::new().with_global_config()));

        // If agent def declares skills, inject SkillPreloadMiddleware (full text preload)
        if !agent_def.frontmatter.skills.is_empty() {
            agent_builder = agent_builder.add_middleware(Box::new(SkillPreloadMiddleware::new(
                agent_def.frontmatter.skills.clone(),
                &cwd,
            )));
        }

        agent_builder = agent_builder.add_middleware(Box::new(TodoMiddleware::new({
            let (tx, _rx) = mpsc::channel(8);
            tx
        })));

        // 6. Inject system prompt via with_system_prompt (visible in Langfuse tracing)
        //    System prompt = build_system_prompt(agent overrides, cwd), includes tone/proactiveness
        //    Reuse already-parsed agent_def to extract overrides, avoiding redundant I/O
        if let Some(ref builder) = self.system_builder {
            let overrides = Self::overrides_from_agent_def(
                &agent_def.system_prompt,
                &agent_def.frontmatter.tone,
                &agent_def.frontmatter.proactiveness,
            );
            let system_content = builder(overrides.as_ref(), &cwd);
            agent_builder = agent_builder.with_system_prompt(system_content);
        }

        // Register filtered tools
        for tool in filtered_tools {
            agent_builder = agent_builder.register_tool(tool);
        }

        // Transparently forward parent agent event handler
        if let Some(handler) = &self.event_handler {
            agent_builder = agent_builder.with_event_handler(Arc::clone(handler));
        }

        // 7. Execute child agent
        let mut state = AgentState::new(cwd.clone());
        match agent_builder
            .execute(AgentInput::text(prompt), &mut state, self.cancel.clone())
            .await
        {
            Ok(output) => Ok(format_subagent_result(&output)),
            Err(rust_create_agent::error::AgentError::Interrupted) => {
                Ok("Sub-agent execution was interrupted".to_string())
            }
            Err(e) => {
                let msg = format!("Sub-agent execution failed: {}", e);
                Err(msg.into())
            }
        }
    }
}

/// Format sub-agent execution result as a summary string returned to the parent agent.
///
/// Summary format:
/// - If tool calls exist, list tool names (excluding intermediate results to avoid token bloat)
/// - Preserve final answer text
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
        "[Sub-agent executed {} tool calls: {}]\n\n{}",
        output.tool_calls.len(),
        tool_summary,
        output.text
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::agent::react::Reasoning;
    use tempfile::tempdir;

    // Mock LLM: returns final answer directly
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
            Arc::new(|_: Option<&str>| Box::new(EchoLLM) as Box<dyn ReactLLM + Send + Sync>),
            "/tmp".to_string(),
        )
    }

    #[test]
    fn test_tool_name() {
        let t = make_subagent_tool(vec![]);
        assert_eq!(t.name(), "Agent");
    }

    #[test]
    fn test_agent_parameters_required_is_prompt_only() {
        let t = make_subagent_tool(vec![]);
        let params = t.parameters();
        let required = params["required"].as_array().unwrap();
        let names: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();
        assert!(names.contains(&"prompt"));
        assert!(!names.contains(&"agent_id"));
        assert!(!names.contains(&"task"));
    }

    /// Verify error returned when prompt parameter is missing
    #[tokio::test]
    async fn test_agent_prompt_missing_returns_error() {
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
                "subagent_type": "test-agent",
                "cwd": dir.path().to_str().unwrap()
            }))
            .await
            .unwrap();
        assert!(result.contains("prompt"), "Should return missing prompt error: {}", result);
    }

    /// Verify error returned when subagent_type parameter is missing
    #[tokio::test]
    async fn test_agent_subagent_type_missing_returns_error() {
        let t = make_subagent_tool(vec![]);
        let result = t
            .invoke(serde_json::json!({
                "prompt": "do something"
            }))
            .await
            .unwrap();
        assert!(result.contains("subagent_type"), "Should return missing subagent_type error: {}", result);
    }

    #[tokio::test]
    async fn test_tool_agent_not_found() {
        let t = make_subagent_tool(vec![]);
        let result = t
            .invoke(serde_json::json!({
                "subagent_type": "nonexistent-agent",
                "prompt": "do something",
                "cwd": "/tmp"
            }))
            .await
            .unwrap();
        assert!(result.contains("cannot find"), "Should return not found error: {}", result);
    }

    #[tokio::test]
    async fn test_tool_filter_inherit_all() {
        // tools is Empty -> inherit all parent tools, but exclude Agent
        let parent_tools = vec![
            make_tool("Read"),
            make_tool("Write"),
            make_tool("Agent"), // this should be excluded
        ];
        let t = make_subagent_tool(parent_tools);

        let allowed = ToolsValue::Empty;
        let disallowed = ToolsValue::Empty;
        let filtered = t.filter_tools(&allowed, &disallowed);
        let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"Read"));
        assert!(names.contains(&"Write"));
        assert!(!names.contains(&"Agent"), "Agent should not be inherited");
    }

    #[test]
    fn test_tool_filter_allowlist() {
        // tools has value -> only keep specified tools
        let parent_tools = vec![
            make_tool("Read"),
            make_tool("Write"),
            make_tool("Glob"),
        ];
        let t = make_subagent_tool(parent_tools);

        let allowed = ToolsValue::List(vec!["Read".to_string(), "Glob".to_string()]);
        let disallowed = ToolsValue::Empty;
        let filtered = t.filter_tools(&allowed, &disallowed);
        let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"Read"));
        assert!(names.contains(&"Glob"));
        assert!(
            !names.contains(&"Write"),
            "Write not in allowlist should be excluded"
        );
    }

    #[test]
    fn test_tool_filter_disallow() {
        // disallowedTools -> exclude from inherited set
        let parent_tools = vec![
            make_tool("Read"),
            make_tool("Write"),
            make_tool("Edit"),
        ];
        let t = make_subagent_tool(parent_tools);

        let allowed = ToolsValue::Empty;
        let disallowed = ToolsValue::List(vec!["Write".to_string(), "Edit".to_string()]);
        let filtered = t.filter_tools(&allowed, &disallowed);
        let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"Read"));
        assert!(
            !names.contains(&"Write"),
            "Write in disallow list should be excluded"
        );
        assert!(
            !names.contains(&"Edit"),
            "Edit in disallow list should be excluded"
        );
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
                "subagent_type": "test-agent",
                "prompt": "hello",
                "cwd": dir.path().to_str().unwrap()
            }))
            .await
            .unwrap();
        // EchoLLM returns echo: hello
        assert!(result.contains("echo"), "Should receive sub-agent output: {}", result);
    }

    /// Verify Agent reserved fields (isolation/run_in_background/description/name) don't affect execution
    #[tokio::test]
    async fn test_agent_reserved_fields_parsed() {
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
                "prompt": "hello",
                "subagent_type": "test-agent",
                "description": "test desc",
                "name": "test-alias",
                "isolation": "worktree",
                "run_in_background": true,
                "cwd": dir.path().to_str().unwrap()
            }))
            .await
            .unwrap();
        // Reserved fields don't affect execution, should still return normal result
        assert!(result.contains("echo"), "Should execute normally: {}", result);
    }

    #[tokio::test]
    async fn test_agent_tool_in_list() {
        // Verify SubAgentTool's tool name is correct, can join tool list
        let t = make_subagent_tool(vec![]);
        assert_eq!(t.name(), "Agent");
        let def = t.definition();
        assert_eq!(def.name, "Agent");
    }

    /// Recursion prevention: even if agent.md tools field explicitly includes Agent, it must be excluded
    #[test]
    fn test_agent_excluded_even_when_explicitly_allowed() {
        let parent_tools = vec![
            make_tool("Read"),
            make_tool("Agent"), // parent tool set has Agent
        ];
        let t = make_subagent_tool(parent_tools);

        // agent.md has tools: ["Agent", "Read"]
        let allowed = ToolsValue::List(vec!["Agent".to_string(), "Read".to_string()]);
        let disallowed = ToolsValue::Empty;
        let filtered = t.filter_tools(&allowed, &disallowed);
        let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"Read"), "Read should be kept");
        assert!(
            !names.contains(&"Agent"),
            "Agent must be excluded even when explicitly in allowlist (recursion prevention)"
        );
    }

    /// tools/disallowedTools filtering: case-insensitive (users often write PascalCase)
    #[test]
    fn test_tool_filter_case_insensitive() {
        let parent_tools = vec![
            make_tool("Read"),
            make_tool("Write"),
            make_tool("Glob"),
        ];
        let t = make_subagent_tool(parent_tools);

        // User writes different cases in agent.md: tools: READ, glob
        let allowed = ToolsValue::List(vec!["READ".to_string(), "glob".to_string()]);
        let disallowed = ToolsValue::Empty;
        let filtered = t.filter_tools(&allowed, &disallowed);
        let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

        assert!(
            names.contains(&"Read"),
            "Case-insensitive: READ should match Read"
        );
        assert!(
            names.contains(&"Glob"),
            "Case-insensitive: glob should match Glob"
        );
        assert!(
            !names.contains(&"Write"),
            "Write not in allowlist should be excluded"
        );

        // disallowedTools case-insensitive
        let allowed2 = ToolsValue::Empty;
        let disallowed2 = ToolsValue::List(vec!["WRITE".to_string()]);
        let filtered2 = t.filter_tools(&allowed2, &disallowed2);
        let names2: Vec<&str> = filtered2.iter().map(|t| t.name()).collect();

        assert!(names2.contains(&"Read"));
        assert!(names2.contains(&"Glob"));
        assert!(
            !names2.contains(&"Write"),
            "WRITE should case-insensitively exclude Write"
        );
    }

    /// Recursion prevention: Agent in disallowedTools is redundant but should not error
    #[test]
    fn test_agent_excluded_when_in_disallowed() {
        let parent_tools = vec![make_tool("Read"), make_tool("Agent")];
        let t = make_subagent_tool(parent_tools);

        let allowed = ToolsValue::Empty;
        let disallowed = ToolsValue::List(vec!["Agent".to_string()]);
        let filtered = t.filter_tools(&allowed, &disallowed);
        let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"Read"));
        assert!(!names.contains(&"Agent"), "Agent should not appear");
    }

    /// Verify with_system_builder correctly injects system prompt
    #[tokio::test]
    async fn test_system_builder_injects_system_message() {
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("tone-test.md"),
            "---\nname: tone-test\ndescription: Test tone injection\n---\n\nYou are a tone tester.\n",
        )
        .unwrap();

        // LLM echoes system message content
        struct SystemEchoLLM;
        #[async_trait::async_trait]
        impl ReactLLM for SystemEchoLLM {
            async fn generate_reasoning(
                &self,
                messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> rust_create_agent::error::AgentResult<Reasoning> {
                // Find system message and return its content
                let system_content = messages
                    .iter()
                    .find(|m| matches!(m, BaseMessage::System { .. }))
                    .map(|m| m.content())
                    .unwrap_or_else(|| "no-system".to_string());
                Ok(Reasoning::with_answer(
                    "",
                    format!("system={system_content}"),
                ))
            }
        }

        let t = SubAgentTool::new(
            Arc::new(vec![]),
            None,
            Arc::new(|_: Option<&str>| Box::new(SystemEchoLLM) as Box<dyn ReactLLM + Send + Sync>),
            dir.path().to_str().unwrap().to_string(),
        )
        .with_system_builder(Arc::new(|_overrides, _cwd| "tone: be concise".to_string()));

        let result = t
            .invoke(serde_json::json!({
                "subagent_type": "tone-test",
                "prompt": "hello",
                "cwd": dir.path().to_str().unwrap()
            }))
            .await
            .unwrap();
        assert!(
            result.contains("tone: be concise"),
            "System prompt should be injected: {}",
            result
        );
    }

    /// Verify SkillPreloadMiddleware is correctly registered when agent.md contains skills field
    /// LLM received messages should contain "(system: preloaded skill file)"
    #[tokio::test]
    async fn test_skill_preload_registered() {
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        let skills_dir = dir.path().join(".claude").join("skills").join("test-skill");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::create_dir_all(&skills_dir).unwrap();

        // agent.md with skills field
        std::fs::write(
            agents_dir.join("skill-user.md"),
            "---\nname: skill-user\ndescription: Uses skills\nskills:\n  - test-skill\n---\n\nYou use skills.\n",
        )
        .unwrap();

        // SKILL.md content
        std::fs::write(
            skills_dir.join("SKILL.md"),
            "---\nname: 'test-skill'\ndescription: 'A test skill'\n---\n\n# Test Skill\n\nThis is the test skill content.\n",
        )
        .unwrap();

        // LLM searches all messages for "preloaded skill file" keyword
        struct SkillPreloadCheckLLM;
        #[async_trait::async_trait]
        impl ReactLLM for SkillPreloadCheckLLM {
            async fn generate_reasoning(
                &self,
                messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> rust_create_agent::error::AgentResult<Reasoning> {
                let found = messages
                    .iter()
                    .any(|m| m.content().contains("预加载 skill 文件"));
                Ok(Reasoning::with_answer(
                    "",
                    if found {
                        "skill_preload_found"
                    } else {
                        "skill_preload_not_found"
                    },
                ))
            }
        }

        let t = SubAgentTool::new(
            Arc::new(vec![]),
            None,
            Arc::new(|_: Option<&str>| {
                Box::new(SkillPreloadCheckLLM) as Box<dyn ReactLLM + Send + Sync>
            }),
            dir.path().to_str().unwrap().to_string(),
        );

        let result = t
            .invoke(serde_json::json!({
                "subagent_type": "skill-user",
                "prompt": "test task",
                "cwd": dir.path().to_str().unwrap()
            }))
            .await
            .unwrap();

        assert!(
            result.contains("skill_preload_found"),
            "LLM should receive message containing 'preloaded skill file', actual result: {}",
            result
        );
    }

    #[test]
    fn test_agent_description_extended() {
        let t = make_subagent_tool(vec![]);
        let desc = t.description();
        assert!(desc.contains("Usage:"), "description should contain Usage section");
        assert!(
            desc.contains("sub-agent") || desc.contains("sub agent"),
            "description should mention sub-agent"
        );
        assert!(
            desc.contains("isolated") || desc.contains("isolation"),
            "description should mention context isolation"
        );
        assert!(desc.contains("Fork mode"), "description should mention Fork mode");
        assert!(desc.len() > 300, "description should be extended multi-paragraph text");
    }

    /// Verify overrides_from_agent_def correctly extracts AgentOverrides from parsed data
    #[test]
    fn test_overrides_from_agent_def_with_all_fields() {
        let ov = SubAgentTool::overrides_from_agent_def(
            "You are a reviewer.",
            &Some("Be thorough.".to_string()),
            &Some("Proactively suggest.".to_string()),
        );
        let ov = ov.unwrap();
        assert_eq!(ov.persona.as_deref().unwrap(), "You are a reviewer.");
        assert_eq!(ov.tone.as_deref().unwrap(), "Be thorough.");
        assert_eq!(ov.proactiveness.as_deref().unwrap(), "Proactively suggest.");
    }

    #[test]
    fn test_overrides_from_agent_def_empty() {
        let ov = SubAgentTool::overrides_from_agent_def("", &None, &None);
        assert!(ov.is_none(), "All-empty fields should return None");
    }

    #[test]
    fn test_overrides_from_agent_def_persona_only() {
        let ov = SubAgentTool::overrides_from_agent_def("I am a helper.", &None, &None);
        let ov = ov.unwrap();
        assert_eq!(ov.persona.as_deref().unwrap(), "I am a helper.");
        assert!(ov.tone.is_none());
        assert!(ov.proactiveness.is_none());
    }

    /// Verify cancellation token can interrupt sub-agent execution
    #[tokio::test]
    async fn test_cancel_token_interrupts_subagent() {
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("forever.md"),
            "---\nname: forever\ndescription: Runs forever\n---\n\nYou run forever.\n",
        )
        .unwrap();

        // LLM always calls a never-registered tool, causing ToolNotFound but no infinite loop
        struct ToolNotFoundLLM;
        #[async_trait::async_trait]
        impl ReactLLM for ToolNotFoundLLM {
            async fn generate_reasoning(
                &self,
                messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> rust_create_agent::error::AgentResult<Reasoning> {
                if messages
                    .iter()
                    .any(|m| matches!(m, BaseMessage::Tool { .. }))
                {
                    Ok(Reasoning::with_answer("", "done"))
                } else {
                    Ok(Reasoning::with_tools(
                        "call missing",
                        vec![rust_create_agent::agent::react::ToolCall::new(
                            "id1",
                            "nonexistent",
                            serde_json::json!({}),
                        )],
                    ))
                }
            }
        }

        let cancel = AgentCancellationToken::new();
        // Trigger cancellation before sub-agent execution
        cancel.cancel();

        let t = SubAgentTool::new(
            Arc::new(vec![]),
            None,
            Arc::new(|_: Option<&str>| {
                Box::new(ToolNotFoundLLM) as Box<dyn ReactLLM + Send + Sync>
            }),
            dir.path().to_str().unwrap().to_string(),
        )
        .with_cancel(cancel);

        let result = t
            .invoke(serde_json::json!({
                "subagent_type": "forever",
                "prompt": "run",
                "cwd": dir.path().to_str().unwrap()
            }))
            .await
            .unwrap();
        assert!(
            result.contains("interrupted"),
            "Cancellation should cause interrupt message, actual: {}",
            result
        );
    }

    // ─── Fork path tests ────────────────────────────────────────────────────

    /// Mock LLM that captures messages and tools for inspection
    struct CaptureLLM {
        messages: Arc<std::sync::Mutex<Vec<usize>>>,
        tools: Arc<std::sync::Mutex<Vec<String>>>,
        last_content: Arc<std::sync::Mutex<String>>,
    }

    impl CaptureLLM {
        fn new() -> Self {
            Self {
                messages: Arc::new(std::sync::Mutex::new(Vec::new())),
                tools: Arc::new(std::sync::Mutex::new(Vec::new())),
                last_content: Arc::new(std::sync::Mutex::new(String::new())),
            }
        }
    }

    #[async_trait::async_trait]
    impl ReactLLM for CaptureLLM {
        async fn generate_reasoning(
            &self,
            messages: &[BaseMessage],
            tools: &[&dyn BaseTool],
        ) -> rust_create_agent::error::AgentResult<Reasoning> {
            *self.messages.lock().unwrap() = vec![messages.len()];
            *self.tools.lock().unwrap() = tools.iter().map(|t| t.name().to_string()).collect();
            let last = messages.last().map(|m| m.content()).unwrap_or_default();
            *self.last_content.lock().unwrap() = last;
            Ok(Reasoning::with_answer("", "capture-done"))
        }
    }

    /// Fork inherits parent messages
    #[tokio::test]
    async fn test_fork_inherits_parent_messages() {
        let parent_messages: Arc<RwLock<Vec<BaseMessage>>> =
            Arc::new(RwLock::new(Vec::new()));
        parent_messages.write().push(BaseMessage::human("Hello"));
        parent_messages.write().push(BaseMessage::ai("Hi there"));

        let msg_capture: Arc<std::sync::Mutex<usize>> = Arc::new(std::sync::Mutex::new(0));
        let msg_capture_clone = Arc::clone(&msg_capture);

        struct ForkTestLLM {
            msg_count: Arc<std::sync::Mutex<usize>>,
        }
        #[async_trait::async_trait]
        impl ReactLLM for ForkTestLLM {
            async fn generate_reasoning(
                &self,
                messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> rust_create_agent::error::AgentResult<Reasoning> {
                *self.msg_count.lock().unwrap() = messages.len();
                Ok(Reasoning::with_answer("", "fork-done"))
            }
        }

        let t = SubAgentTool::new(
            Arc::new(vec![]),
            None,
            Arc::new(move |_: Option<&str>| {
                Box::new(ForkTestLLM {
                    msg_count: Arc::clone(&msg_capture_clone),
                }) as Box<dyn ReactLLM + Send + Sync>
            }),
            "/tmp".to_string(),
        )
        .with_parent_messages(Arc::clone(&parent_messages));

        let result = t
            .invoke(serde_json::json!({
                "fork": true,
                "prompt": "do the thing"
            }))
            .await
            .unwrap();

        assert!(
            result.contains("fork-done"),
            "Fork should execute: {}",
            result
        );
        // Messages should include: 2 parent history + 1 system + 1 fork directive (human) = 4+
        let count = *msg_capture.lock().unwrap();
        assert!(
            count >= 3,
            "Fork should receive parent messages (got {})", count
        );
    }

    /// Fork registers all tools including Agent (no hard-coded exclusion)
    #[tokio::test]
    async fn test_fork_registers_all_tools_including_agent() {
        let parent_messages: Arc<RwLock<Vec<BaseMessage>>> =
            Arc::new(RwLock::new(Vec::new()));

        let tools_capture: Arc<std::sync::Mutex<Vec<String>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let tools_capture_clone = Arc::clone(&tools_capture);

        struct ToolsCheckLLM {
            captured: Arc<std::sync::Mutex<Vec<String>>>,
        }
        #[async_trait::async_trait]
        impl ReactLLM for ToolsCheckLLM {
            async fn generate_reasoning(
                &self,
                _messages: &[BaseMessage],
                tools: &[&dyn BaseTool],
            ) -> rust_create_agent::error::AgentResult<Reasoning> {
                *self.captured.lock().unwrap() =
                    tools.iter().map(|t| t.name().to_string()).collect();
                Ok(Reasoning::with_answer("", "tools-check"))
            }
        }

        let parent_tools = vec![make_tool("Read"), make_tool("Agent")];

        let t = SubAgentTool::new(
            Arc::new(parent_tools),
            None,
            Arc::new(move |_: Option<&str>| {
                Box::new(ToolsCheckLLM {
                    captured: Arc::clone(&tools_capture_clone),
                }) as Box<dyn ReactLLM + Send + Sync>
            }),
            "/tmp".to_string(),
        )
        .with_parent_messages(parent_messages);

        t.invoke(serde_json::json!({
            "fork": true,
            "prompt": "check tools"
        }))
        .await
        .unwrap();

        let captured = tools_capture.lock().unwrap();
        assert!(
            captured.contains(&"Agent".to_string()),
            "Fork should register Agent tool (no exclusion), got: {:?}",
            *captured
        );
        assert!(
            captured.contains(&"Read".to_string()),
            "Fork should register Read tool, got: {:?}",
            *captured
        );
    }

    /// Fork without parent_messages returns error
    #[tokio::test]
    async fn test_fork_without_parent_messages_returns_error() {
        let t = make_subagent_tool(vec![]);

        let result = t
            .invoke(serde_json::json!({
                "fork": true,
                "prompt": "do something"
            }))
            .await
            .unwrap();

        assert!(
            result.contains("parent_messages is not set") || result.contains("parent message history"),
            "Fork without parent_messages should return error, got: {}",
            result
        );
    }

    /// Fork system prompt is consistent with system_builder
    #[tokio::test]
    async fn test_fork_system_prompt_consistent() {
        let parent_messages: Arc<RwLock<Vec<BaseMessage>>> =
            Arc::new(RwLock::new(Vec::new()));

        let sys_capture: Arc<std::sync::Mutex<String>> =
            Arc::new(std::sync::Mutex::new(String::new()));
        let sys_capture_clone = Arc::clone(&sys_capture);

        struct SystemCheckLLM {
            captured: Arc<std::sync::Mutex<String>>,
        }
        #[async_trait::async_trait]
        impl ReactLLM for SystemCheckLLM {
            async fn generate_reasoning(
                &self,
                messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> rust_create_agent::error::AgentResult<Reasoning> {
                let sys = messages
                    .iter()
                    .find(|m| matches!(m, BaseMessage::System { .. }))
                    .map(|m| m.content())
                    .unwrap_or_default();
                *self.captured.lock().unwrap() = sys;
                Ok(Reasoning::with_answer("", "sys-check"))
            }
        }

        let t = SubAgentTool::new(
            Arc::new(vec![]),
            None,
            Arc::new(move |_: Option<&str>| {
                Box::new(SystemCheckLLM {
                    captured: Arc::clone(&sys_capture_clone),
                }) as Box<dyn ReactLLM + Send + Sync>
            }),
            "/tmp".to_string(),
        )
        .with_parent_messages(parent_messages)
        .with_system_builder(Arc::new(|_ov, _cwd| "FORK-TEST-SYSTEM".to_string()));

        t.invoke(serde_json::json!({
            "fork": true,
            "prompt": "check system"
        }))
        .await
        .unwrap();

        let captured = sys_capture.lock().unwrap();
        assert!(
            captured.contains("FORK-TEST-SYSTEM"),
            "Fork system prompt should contain builder output, got: {}",
            *captured
        );
    }

    /// Fork directive includes RULES
    #[tokio::test]
    async fn test_fork_directive_includes_rules() {
        let parent_messages: Arc<RwLock<Vec<BaseMessage>>> =
            Arc::new(RwLock::new(Vec::new()));

        let last_capture: Arc<std::sync::Mutex<String>> =
            Arc::new(std::sync::Mutex::new(String::new()));
        let last_capture_clone = Arc::clone(&last_capture);

        struct DirectiveCheckLLM {
            last: Arc<std::sync::Mutex<String>>,
        }
        #[async_trait::async_trait]
        impl ReactLLM for DirectiveCheckLLM {
            async fn generate_reasoning(
                &self,
                messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> rust_create_agent::error::AgentResult<Reasoning> {
                let last = messages.last().map(|m| m.content()).unwrap_or_default();
                *self.last.lock().unwrap() = last;
                Ok(Reasoning::with_answer("", "directive-check"))
            }
        }

        let t = SubAgentTool::new(
            Arc::new(vec![]),
            None,
            Arc::new(move |_: Option<&str>| {
                Box::new(DirectiveCheckLLM {
                    last: Arc::clone(&last_capture_clone),
                }) as Box<dyn ReactLLM + Send + Sync>
            }),
            "/tmp".to_string(),
        )
        .with_parent_messages(parent_messages);

        t.invoke(serde_json::json!({
            "fork": true,
            "prompt": "my directive task"
        }))
        .await
        .unwrap();

        let last = last_capture.lock().unwrap();
        assert!(
            last.contains("<fork_directive>"),
            "Fork directive should contain <fork_directive>, got: {}",
            *last
        );
        assert!(
            last.contains("RULES"),
            "Fork directive should contain RULES, got: {}",
            *last
        );
        assert!(
            last.contains("my directive task"),
            "Fork directive should contain the prompt, got: {}",
            *last
        );
    }
}
