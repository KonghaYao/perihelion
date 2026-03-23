use async_trait::async_trait;
use rust_create_agent::agent::state::State;
use rust_create_agent::middleware::r#trait::Middleware;
use rust_create_agent::tools::{BaseTool, ToolProvider};
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// BashTool - 终端命令执行工具，与 TypeScript TerminalMiddleware 对齐
pub struct BashTool {
    pub cwd: String,
}

impl BashTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

#[async_trait::async_trait]
impl BaseTool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute shell commands in a persistent working directory context. Parameters (JSON): command: string (required), timeout_secs: number (optional, default 120)"
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command":      { "type": "string", "description": "The shell command to execute" },
                "timeout_secs": { "type": "number", "description": "Command timeout in seconds (default 120)" }
            },
            "required": ["command"]
        })
    }

    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let command = input["command"]
            .as_str()
            .ok_or("Missing command parameter")?;

        let timeout_secs = input["timeout_secs"].as_u64().unwrap_or(120);

        let (shell, flag) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("bash", "-c")
        };

        let result = timeout(
            Duration::from_secs(timeout_secs),
            Command::new(shell)
                .arg(flag)
                .arg(command)
                .current_dir(&self.cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                // 超时时 future 被 drop → Child 被 drop → 自动 SIGKILL 终止子进程
                .kill_on_drop(true)
                .output(),
        )
        .await;

        match result {
            Err(_) => Ok(format!(
                "Error: Command timed out after {timeout_secs} seconds.\nCommand: {command}"
            )),
            Ok(Err(e)) => Ok(format!("Error executing command: {e}")),
            Ok(Ok(out)) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                let exit_code = out.status.code().unwrap_or(-1);

                let mut output = String::new();

                if !stdout.is_empty() {
                    output.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str("[stderr]\n");
                    output.push_str(&stderr);
                }
                if exit_code != 0 {
                    output.push_str(&format!("\n[Exit code: {exit_code}]"));
                }

                if output.is_empty() {
                    output = format!("[Command completed with exit code {exit_code}]");
                }

                Ok(output)
            }
        }
    }
}

/// TerminalMiddleware - 与 TypeScript TerminalMiddleware 对齐
pub struct TerminalMiddleware;

impl TerminalMiddleware {
    pub fn new() -> Self {
        Self
    }

    pub fn build_tools(cwd: &str) -> Vec<Box<dyn BaseTool>> {
        vec![Box::new(BashTool::new(cwd))]
    }

    pub fn tool_names() -> Vec<&'static str> {
        vec!["bash"]
    }
}

impl Default for TerminalMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolProvider for TerminalMiddleware {
    fn tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>> {
        Self::build_tools(cwd)
    }
}

#[async_trait]
impl<S: State> Middleware<S> for TerminalMiddleware {
    fn collect_tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>> {
        Self::build_tools(cwd)
    }

    fn name(&self) -> &str {
        "TerminalMiddleware"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_create_agent::tools::BaseTool;
    use std::time::Instant;

    #[tokio::test]
    async fn test_bash_normal_command() {
        let tool = BashTool::new("/tmp");
        let result = tool
            .invoke(serde_json::json!({"command": "echo hello"}))
            .await
            .unwrap();
        assert!(result.contains("hello"));
    }

    #[tokio::test]
    async fn test_bash_nonzero_exit_code() {
        let tool = BashTool::new("/tmp");
        let result = tool
            .invoke(serde_json::json!({"command": "exit 42"}))
            .await
            .unwrap();
        assert!(result.contains("42"), "应包含退出码: {result}");
    }

    /// 验证超时后在合理时间内返回，且 kill_on_drop 确保子进程被清理
    #[tokio::test]
    async fn test_bash_timeout_returns_quickly() {
        let tool = BashTool::new("/tmp");
        let start = Instant::now();
        let result = tool
            .invoke(serde_json::json!({
                "command": "sleep 60",
                "timeout_secs": 1
            }))
            .await
            .unwrap();
        let elapsed = start.elapsed();

        // 应在约 1 秒内返回（不超过 3 秒），不等待 sleep 60 完成
        assert!(
            elapsed.as_secs() < 3,
            "超时后应快速返回，实际耗时 {:?}",
            elapsed
        );
        assert!(
            result.contains("timed out"),
            "返回值应包含超时提示: {result}"
        );
    }

    #[tokio::test]
    async fn test_bash_stderr_captured() {
        let tool = BashTool::new("/tmp");
        let result = tool
            .invoke(serde_json::json!({"command": "echo err >&2"}))
            .await
            .unwrap();
        assert!(result.contains("err"), "stderr 应被捕获: {result}");
    }
}
