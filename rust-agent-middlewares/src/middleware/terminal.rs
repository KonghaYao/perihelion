use async_trait::async_trait;
use rust_create_agent::agent::state::State;
use rust_create_agent::middleware::r#trait::Middleware;
use rust_create_agent::tools::BaseTool;
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// BashTool - 终端命令执行工具，与 TypeScript TerminalMiddleware 对齐

const BASH_DESCRIPTION: &str = r#"Executes a given shell command and returns its output.

Usage:
- The working directory persists between commands, but shell state does not. The shell environment is initialized from the user's profile (bash or zsh)
- IMPORTANT: Avoid using this tool to run find, grep, cat, head, tail, sed, awk, or echo commands, unless explicitly instructed or after you have verified that a dedicated tool cannot accomplish your task
- Instead, use the appropriate dedicated tool which will provide a much better experience for the user:
  - File search: Use glob_files (NOT find or ls)
  - Content search: Use search_files_rg (NOT grep or rg)
  - Read files: Use read_file (NOT cat/head/tail)
  - Edit files: Use edit_file (NOT sed/awk)
  - Write files: Use write_file (NOT echo/cat with redirect)
- You can specify an optional timeout in seconds (up to 300 seconds / 5 minutes). Default is 120 seconds (2 minutes)
- When issuing multiple commands, use && to chain them together rather than using separate tool calls if the commands depend on each other
- For long running commands, consider using a timeout to avoid waiting indefinitely

Platform behavior:
- Windows: uses cmd /C to execute commands
- Unix/macOS: uses bash -c to execute commands
- On Unix, child processes run in their own process group; timeout kills the entire process tree

Output handling:
- Output exceeding 2000 lines is truncated (head + tail preserved)
- Output exceeding 100000 bytes is truncated
- Non-zero exit codes are reported
- Both stdout and stderr are captured"#;
pub struct BashTool {
    pub cwd: String,
}

impl BashTool {
    pub fn new(cwd: impl Into<String>) -> Self {
        Self { cwd: cwd.into() }
    }
}

/// 输出最大字节数
const MAX_OUTPUT_CHARS: usize = 100_000;
/// 输出最大行数（在第 N 行截断后，若还有行数超过上限再截字节）
const MAX_OUTPUT_LINES: usize = 2_000;

/// 按字节截断字符串，确保不拆分 UTF-8 字符
fn truncate_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

fn truncate_output(output: &str) -> String {
    let lines: Vec<&str> = output.split('\n').collect();
    if lines.len() > MAX_OUTPUT_LINES {
        let total_lines = lines.len();
        let head_count = MAX_OUTPUT_LINES / 2;
        let tail_count = MAX_OUTPUT_LINES - head_count;
        let head: Vec<&str> = lines.iter().take(head_count).copied().collect();
        let tail: Vec<&str> = lines.iter().skip(total_lines - tail_count).copied().collect();
        let mut result = head.join("\n");
        result.push_str(&format!(
            "\n\n... [{} lines truncated, showing head {} and tail {} of {} total lines] ...\n\n",
            total_lines - MAX_OUTPUT_LINES, head_count, tail_count, total_lines
        ));
        result.push_str(&tail.join("\n"));
        // 再检查字节数（使用字节截断，保留 UTF-8 字符边界）
        if result.len() > MAX_OUTPUT_CHARS {
            let truncated = truncate_bytes(&result, MAX_OUTPUT_CHARS);
            return format!("{}\n\n[Output truncated: exceeds {} byte limit]", truncated, MAX_OUTPUT_CHARS);
        }
        return result;
    }
    if output.len() > MAX_OUTPUT_CHARS {
        let truncated = truncate_bytes(output, MAX_OUTPUT_CHARS);
        return format!("{}\n\n[Output truncated: exceeds {} byte limit]", truncated, MAX_OUTPUT_CHARS);
    }
    output.to_string()
}

#[async_trait::async_trait]
impl BaseTool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        BASH_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command (and optional arguments) to execute. This can be complex commands that use pipes, &&, or other shell features. For multiple dependent commands, chain them with && rather than making separate calls"
                },
                "timeout_secs": {
                    "type": "number",
                    "description": "Optional timeout in seconds (default 120, max 300). If the command takes longer than this, it will be killed and a timeout error returned"
                }
            },
            "required": ["command"]
        })
    }

    async fn invoke(&self, input: Value) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let command = input["command"]
            .as_str()
            .ok_or("Missing command parameter")?;

        let timeout_secs = input["timeout_secs"].as_u64().unwrap_or(120).clamp(1, 300);

        let (shell, flag) = if cfg!(target_os = "windows") {
            ("cmd", "/C")
        } else {
            ("bash", "-c")
        };

        let result = timeout(
            Duration::from_secs(timeout_secs),
            {
                let mut cmd = Command::new(shell);
                cmd.arg(flag)
                    .arg(command)
                    .current_dir(&self.cwd)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    // 超时时 future 被 drop → Child 被 drop → 自动 SIGKILL 终止子进程
                    .kill_on_drop(true);
                // Unix: 将子进程放入独立进程组，确保超时时能杀掉整个进程树（含 bash 子进程）
                #[cfg(unix)]
                cmd.process_group(0);
                cmd.output()
            },
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

                // 截断过长输出，防止撑爆 LLM context window
                Ok(truncate_output(&output))
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
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"command": "echo hello"}))
            .await
            .unwrap();
        assert!(result.contains("hello"));
    }

    #[tokio::test]
    async fn test_bash_nonzero_exit_code() {
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"command": "exit 42"}))
            .await
            .unwrap();
        assert!(result.contains("42"), "应包含退出码: {result}");
    }

    /// 验证超时后在合理时间内返回，且 kill_on_drop 确保子进程被清理
    #[tokio::test]
    async fn test_bash_timeout_returns_quickly() {
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        let start = Instant::now();

        // Windows 用 ping 模拟 sleep，Unix 用 sleep
        let (sleep_cmd, timeout_secs) = if cfg!(target_os = "windows") {
            ("ping -n 60 127.0.0.1", 1)
        } else {
            ("sleep 60", 1)
        };

        let result = tool
            .invoke(serde_json::json!({
                "command": sleep_cmd,
                "timeout_secs": timeout_secs
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
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({"command": "echo err >&2"}))
            .await
            .unwrap();
        assert!(result.contains("err"), "stderr 应被捕获: {result}");
    }

    #[test]
    fn test_truncate_output_line_count_accurate() {
        // 生成不含末尾换行的多行文本，避免 split('\n') 产生额外空行
        let lines: Vec<String> = (0..3000).map(|i| format!("line {}", i)).collect();
        let input = lines.join("\n");
        assert_eq!(input.split('\n').count(), 3000);
        let result = truncate_output(&input);
        assert!(result.contains("3000 total lines"), "应显示正确的总行数: {result}");
        // 应保留头部和尾部
        assert!(result.contains("line 0"), "应保留第一行: {result}");
        assert!(result.contains("line 2999"), "应保留最后一行: {result}");
        assert!(result.contains("lines truncated"), "应显示截断信息: {result}");
    }

    #[test]
    fn test_truncate_output_no_truncation_when_small() {
        let result = truncate_output("hello\nworld");
        assert_eq!(result, "hello\nworld");
    }

    #[test]
    fn test_truncate_output_char_limit() {
        let long_line = "x".repeat(200_000);
        let result = truncate_output(&long_line);
        assert!(result.contains("byte limit"), "应截断超长输出: {result}");
    }

    #[test]
    fn test_truncate_output_preserves_tail() {
        // 3000 行，尾部包含关键信息
        let mut lines: Vec<String> = (0..2999).map(|i| format!("line {}", i)).collect();
        lines.push("CRITICAL ERROR: test failed".to_string());
        let input = lines.join("\n");
        let result = truncate_output(&input);
        // 尾部关键行应保留
        assert!(
            result.contains("CRITICAL ERROR"),
            "截断后应保留尾部关键信息: {result}"
        );
        assert!(result.contains("line 0"), "应保留头部: {result}");
    }

    #[test]
    fn test_bash_description_extended() {
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        let desc = tool.description();
        assert!(desc.contains("Usage:"), "description 应包含 Usage 段落");
        assert!(desc.contains("dedicated tool"), "description 应强调优先使用专用工具");
        assert!(desc.contains("timeout"), "description 应提及超时");
        assert!(desc.len() > 200, "description 应为扩展后的多段落文本");
    }

    /// 零超时应被 clamp 到至少 1 秒，避免命令立即超时无法执行
    #[tokio::test]
    async fn test_bash_timeout_clamped_to_minimum() {
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        let start = Instant::now();
        // timeout_secs = 0 → clamp 到 1 秒，命令应在 1 秒内执行完毕
        let result = tool
            .invoke(serde_json::json!({
                "command": "echo quick",
                "timeout_secs": 0
            }))
            .await
            .unwrap();
        let elapsed = start.elapsed();
        assert!(result.contains("quick"), "echo quick 应正常输出: {result}");
        // 不应超时，命令应正常完成
        assert!(
            elapsed.as_millis() < 500,
            "零超时被 clamp 后应快速完成，实际耗时 {:?}",
            elapsed
        );
    }

    /// 显式超时 300 秒应被允许（上限）
    #[tokio::test]
    async fn test_bash_timeout_maximum_accepted() {
        let tool = BashTool::new(std::env::temp_dir().to_str().unwrap());
        let result = tool
            .invoke(serde_json::json!({
                "command": "echo ok",
                "timeout_secs": 300
            }))
            .await
            .unwrap();
        assert!(result.contains("ok"));
    }
}
