//! # rust-agent-middlewares
//!
//! Rust middleware implementations aligned with `@langgraph-js/agent-middlewares` (TypeScript).
//!
//! ## 文件系统与终端（原 rust-agent-middlewares）
//! - [`middleware::FilesystemMiddleware`]：文件系统操作
//! - [`middleware::TerminalMiddleware`]：终端命令执行
//!
//! ## 认知增强与安全（原 rust-standard-middlewares）
//! - [`AgentsMdMiddleware`]：注入 AGENTS.md / CLAUDE.md 项目指引
//! - [`SkillsMiddleware`]：渐进式 Skills 摘要注入
//! - [`HumanInTheLoopMiddleware`]：敏感工具调用前需用户确认

pub mod agents_md;
pub mod agent_define;
pub mod subagent;
pub mod claude_agent_parser;
pub use claude_agent_parser::{format_agent_id, parse_agent_file, ClaudeAgent, ClaudeAgentFrontmatter, ToolsValue};
pub mod ask_user;
pub mod hitl;
pub mod middleware;
pub mod skills;
pub mod tools;

pub use agents_md::AgentsMdMiddleware;
pub use agent_define::{AgentDefineMiddleware, AgentOverrides};
pub use middleware::PrependSystemMiddleware;
pub use ask_user::{
    ask_user_tool_definition, parse_ask_user, AskUserBatchRequest, AskUserOption,
    AskUserQuestionData,
};
pub use hitl::{
    default_requires_approval, is_yolo_mode, BatchItem, HitlDecision, HitlHandler,
    HumanInTheLoopMiddleware,
};
// AskUserInvoker 从核心库直通，AskUserHandler 是其别名
pub use rust_create_agent::ask_user::AskUserInvoker;
pub use rust_create_agent::ask_user::AskUserInvoker as AskUserHandler;
pub use skills::{load_global_skills_dir, list_skills, load_skill_metadata, SkillsMiddleware, SkillMetadata};
pub use tools::{ArcToolWrapper, AskUserTool, BoxToolWrapper};
pub use subagent::{SubAgentMiddleware, SubAgentTool, SkillPreloadMiddleware};

/// Prelude - 常用类型一次性导入
pub mod prelude {
    pub use crate::agents_md::AgentsMdMiddleware;
    pub use crate::agent_define::AgentDefineMiddleware;
    pub use crate::ask_user::{
        ask_user_tool_definition, parse_ask_user, AskUserBatchRequest, AskUserHandler,
        AskUserOption, AskUserQuestionData,
    };
    pub use crate::hitl::{
        default_requires_approval, is_yolo_mode, BatchItem, HitlDecision, HitlHandler,
        HumanInTheLoopMiddleware,
    };
    pub use crate::middleware::{FilesystemMiddleware, PrependSystemMiddleware, TerminalMiddleware, TodoMiddleware};
    pub use rust_create_agent::tools::ToolProvider;
    pub use crate::skills::{SkillMetadata, SkillsMiddleware};
    pub use crate::tools::{
        ArcToolWrapper, AskUserInvoker, AskUserTool, BoxToolWrapper, EditFileTool, FolderOperationsTool,
        GlobFilesTool, ReadFileTool, SearchFilesRgTool, TodoItem, TodoStatus, TodoWriteTool,
        WriteFileTool,
    };
    pub use crate::subagent::{SubAgentMiddleware, SubAgentTool, SkillPreloadMiddleware};

    // 重导出 rust-create-agent 核心类型
    pub use rust_create_agent::prelude::*;
}
