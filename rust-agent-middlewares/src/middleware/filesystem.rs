use async_trait::async_trait;
use rust_create_agent::agent::state::State;
use rust_create_agent::middleware::r#trait::Middleware;
use rust_create_agent::tools::{BaseTool, ToolProvider};

use crate::tools::{
    EditFileTool, FolderOperationsTool, GlobFilesTool, ReadFileTool, SearchFilesRgTool,
    WriteFileTool,
};

/// FilesystemMiddleware - 与 TypeScript FilesystemMiddleware 对齐
pub struct FilesystemMiddleware;

impl FilesystemMiddleware {
    pub fn new() -> Self {
        Self
    }

    pub fn build_tools(cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![
            Box::new(ReadFileTool::new(cwd)),
            Box::new(WriteFileTool::new(cwd)),
            Box::new(EditFileTool::new(cwd)),
            Box::new(GlobFilesTool::new(cwd)),
            Box::new(SearchFilesRgTool::new(cwd)),
            Box::new(FolderOperationsTool::new(cwd)),
        ]
    }

    pub fn tool_names() -> Vec<&'static str> {
        vec![
            "read_file",
            "write_file",
            "edit_file",
            "glob_files",
            "search_files_rg",
            "folder_operations",
        ]
    }
}

impl Default for FilesystemMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolProvider for FilesystemMiddleware {
    fn tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>> {
        Self::build_tools(cwd)
    }
}

#[async_trait]
impl<S: State> Middleware<S> for FilesystemMiddleware {
    fn collect_tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>> {
        Self::build_tools(cwd)
    }

    fn name(&self) -> &str {
        "FilesystemMiddleware"
    }
}
