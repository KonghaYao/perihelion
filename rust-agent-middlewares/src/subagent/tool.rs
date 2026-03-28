use std::sync::Arc;

use async_trait::async_trait;
use rust_create_agent::agent::events::AgentEventHandler;
use rust_create_agent::agent::react::{AgentInput, ReactLLM};
use rust_create_agent::agent::state::AgentState;
use rust_create_agent::agent::ReActAgent;
use rust_create_agent::tools::BaseTool;

use crate::agent_define::{AgentDefineMiddleware, AgentOverrides};
use crate::agents_md::AgentsMdMiddleware;
use crate::claude_agent_parser::{parse_agent_file, ToolsValue};
use crate::middleware::todo::TodoMiddleware;
use crate::skills::SkillsMiddleware;
use crate::subagent::skill_preload::SkillPreloadMiddleware;
use crate::tools::ArcToolWrapper;
use tokio::sync::mpsc;

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
    /// LLM 工厂函数，每次为子 agent 创建独立 LLM 实例（不设 system，由 with_system_prompt() 注入）
    llm_factory: Arc<dyn Fn() -> Box<dyn ReactLLM + Send + Sync> + Send + Sync>,
    /// 系统提示词构建器：(agent overrides, cwd) → system prompt 字符串
    ///
    /// 返回的内容会通过 `with_system_prompt()` 注入到子 agent 的 state 消息中，
    /// 使其在 Langfuse 等追踪工具中可见。为 None 时不注入系统提示词。
    system_builder: Option<Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>>,
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
            system_builder: None,
        }
    }

    /// 设置系统提示词构建器，用于向子 agent 注入包含 tone/proactiveness 的完整系统提示
    pub fn with_system_builder(
        mut self,
        builder: Arc<dyn Fn(Option<&AgentOverrides>, &str) -> String + Send + Sync>,
    ) -> Self {
        self.system_builder = Some(builder);
        self
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
                let name_lower = name.to_lowercase();
                // 始终排除 launch_agent，防止递归
                if name == "launch_agent" {
                    return false;
                }
                // 若 allowed_list 非空，则仅保留列表中的工具（大小写不敏感）
                if !allowed_list.is_empty()
                    && !allowed_list
                        .iter()
                        .any(|n| n.to_lowercase() == name_lower)
                {
                    return false;
                }
                // 排除 disallowed 列表中的工具（大小写不敏感）
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
        let max_iterations = agent_def.frontmatter.max_turns.unwrap_or(20) as usize;

        let mut agent_builder = ReActAgent::new(llm).max_iterations(max_iterations);

        // 5. 补全缺失的上下文中间件（与父 agent 对齐）
        //    注册顺序：AgentsMdMiddleware → SkillsMiddleware → TodoMiddleware
        //    TodoMiddleware 的 _rx 立即丢弃，send 失败静默忽略，不向 TUI 透传
        agent_builder = agent_builder
            .add_middleware(Box::new(AgentsMdMiddleware::new()))
            .add_middleware(Box::new(SkillsMiddleware::new().with_global_config()));

        // 若 agent def 声明了 skills，注入 SkillPreloadMiddleware（全文预加载）
        if !agent_def.frontmatter.skills.is_empty() {
            agent_builder = agent_builder.add_middleware(Box::new(
                SkillPreloadMiddleware::new(agent_def.frontmatter.skills.clone(), &cwd),
            ));
        }

        agent_builder = agent_builder.add_middleware(Box::new(TodoMiddleware::new({
            let (tx, _rx) = mpsc::channel(8);
            tx
        })));

        // 6. 通过 with_system_prompt 将系统提示词注入 state（使其对 Langfuse 等追踪可见）
        //    系统提示词 = build_system_prompt(agent overrides, cwd)，包含 tone/proactiveness
        if let Some(ref builder) = self.system_builder {
            let overrides = AgentDefineMiddleware::load_overrides(&cwd, &agent_id);
            let system_content = builder(overrides.as_ref(), &cwd);
            agent_builder = agent_builder.with_system_prompt(system_content);
        }

        // 注册过滤后的工具
        for tool in filtered_tools {
            agent_builder = agent_builder.register_tool(tool);
        }

        // 透传父 agent 事件处理器
        if let Some(handler) = &self.event_handler {
            agent_builder = agent_builder.with_event_handler(Arc::clone(handler));
        }

        // 7. 执行子 agent
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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::agent::react::Reasoning;
    use rust_create_agent::messages::BaseMessage;
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

    /// 防递归：即使 agent.md tools 字段显式包含 launch_agent，也必须被排除
    #[test]
    fn test_launch_agent_excluded_even_when_explicitly_allowed() {
        let parent_tools = vec![
            make_tool("read_file"),
            make_tool("launch_agent"), // 父工具集中有 launch_agent
        ];
        let t = make_subagent_tool(parent_tools);

        // agent.md 中 tools: ["launch_agent", "read_file"]
        let allowed = ToolsValue::List(vec![
            "launch_agent".to_string(),
            "read_file".to_string(),
        ]);
        let disallowed = ToolsValue::Empty;
        let filtered = t.filter_tools(&allowed, &disallowed);
        let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"read_file"), "read_file 应保留");
        assert!(
            !names.contains(&"launch_agent"),
            "launch_agent 即使在显式 allowlist 中也必须排除（防递归）"
        );
    }

    /// tools/disallowedTools 过滤：大小写不敏感（用户常写 PascalCase）
    #[test]
    fn test_tool_filter_case_insensitive() {
        let parent_tools = vec![
            make_tool("read_file"),
            make_tool("write_file"),
            make_tool("glob_files"),
        ];
        let t = make_subagent_tool(parent_tools);

        // 用户在 agent.md 中写 PascalCase：tools: Read_File, Glob_Files
        let allowed = ToolsValue::List(vec!["Read_File".to_string(), "Glob_Files".to_string()]);
        let disallowed = ToolsValue::Empty;
        let filtered = t.filter_tools(&allowed, &disallowed);
        let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"read_file"), "大小写不敏感：Read_File 应匹配 read_file");
        assert!(names.contains(&"glob_files"), "大小写不敏感：Glob_Files 应匹配 glob_files");
        assert!(!names.contains(&"write_file"), "write_file 不在 allowlist 中应被排除");

        // disallowedTools 大小写不敏感
        let allowed2 = ToolsValue::Empty;
        let disallowed2 = ToolsValue::List(vec!["Write_File".to_string()]);
        let filtered2 = t.filter_tools(&allowed2, &disallowed2);
        let names2: Vec<&str> = filtered2.iter().map(|t| t.name()).collect();

        assert!(names2.contains(&"read_file"));
        assert!(names2.contains(&"glob_files"));
        assert!(!names2.contains(&"write_file"), "Write_File 应大小写不敏感地排除 write_file");
    }

    /// 防递归：launch_agent 在 disallowedTools 中是冗余但不应出错
    #[test]
    fn test_launch_agent_excluded_when_in_disallowed() {
        let parent_tools = vec![
            make_tool("read_file"),
            make_tool("launch_agent"),
        ];
        let t = make_subagent_tool(parent_tools);

        let allowed = ToolsValue::Empty;
        let disallowed = ToolsValue::List(vec!["launch_agent".to_string()]);
        let filtered = t.filter_tools(&allowed, &disallowed);
        let names: Vec<&str> = filtered.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"read_file"));
        assert!(!names.contains(&"launch_agent"), "launch_agent 不应出现");
    }

    /// 验证 with_system_builder 能正确注入系统提示词
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

        // LLM 回显 system 消息内容
        struct SystemEchoLLM;
        #[async_trait::async_trait]
        impl ReactLLM for SystemEchoLLM {
            async fn generate_reasoning(
                &self,
                messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> rust_create_agent::error::AgentResult<Reasoning> {
                // 找到 system 消息并返回其内容
                let system_content = messages
                    .iter()
                    .find(|m| matches!(m, BaseMessage::System { .. }))
                    .map(|m| m.content())
                    .unwrap_or_else(|| "no-system".to_string());
                Ok(Reasoning::with_answer("", format!("system={system_content}")))
            }
        }

        let t = SubAgentTool::new(
            Arc::new(vec![]),
            None,
            Arc::new(|| Box::new(SystemEchoLLM) as Box<dyn ReactLLM + Send + Sync>),
        )
        .with_system_builder(Arc::new(|_overrides, _cwd| "tone: be concise".to_string()));

        let result = t
            .invoke(serde_json::json!({
                "agent_id": "tone-test",
                "task": "hello",
                "cwd": dir.path().to_str().unwrap()
            }))
            .await
            .unwrap();
        assert!(result.contains("tone: be concise"), "系统提示应被注入: {}", result);
    }

    /// 验证当 agent.md 包含 skills 字段时，SkillPreloadMiddleware 被正确注册
    /// LLM 收到的消息中应包含 "（系统：预加载 skill 文件）"
    #[tokio::test]
    async fn test_skill_preload_registered() {
        let dir = tempdir().unwrap();
        let agents_dir = dir.path().join(".claude").join("agents");
        let skills_dir = dir.path().join(".claude").join("skills").join("test-skill");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::create_dir_all(&skills_dir).unwrap();

        // agent.md 含 skills 字段
        std::fs::write(
            agents_dir.join("skill-user.md"),
            "---\nname: skill-user\ndescription: Uses skills\nskills:\n  - test-skill\n---\n\nYou use skills.\n",
        )
        .unwrap();

        // SKILL.md 内容
        std::fs::write(
            skills_dir.join("SKILL.md"),
            "---\nname: 'test-skill'\ndescription: 'A test skill'\n---\n\n# Test Skill\n\nThis is the test skill content.\n",
        )
        .unwrap();

        // LLM 搜索所有消息，找 "预加载 skill 文件" 关键字
        struct SkillPreloadCheckLLM;
        #[async_trait::async_trait]
        impl ReactLLM for SkillPreloadCheckLLM {
            async fn generate_reasoning(
                &self,
                messages: &[BaseMessage],
                _tools: &[&dyn BaseTool],
            ) -> rust_create_agent::error::AgentResult<Reasoning> {
                let found = messages.iter().any(|m| m.content().contains("预加载 skill 文件"));
                Ok(Reasoning::with_answer(
                    "",
                    if found { "skill_preload_found" } else { "skill_preload_not_found" },
                ))
            }
        }

        let t = SubAgentTool::new(
            Arc::new(vec![]),
            None,
            Arc::new(|| Box::new(SkillPreloadCheckLLM) as Box<dyn ReactLLM + Send + Sync>),
        );

        let result = t
            .invoke(serde_json::json!({
                "agent_id": "skill-user",
                "task": "test task",
                "cwd": dir.path().to_str().unwrap()
            }))
            .await
            .unwrap();

        assert!(
            result.contains("skill_preload_found"),
            "LLM 应收到包含 '预加载 skill 文件' 的消息，实际结果: {}",
            result
        );
    }
}
