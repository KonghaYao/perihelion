# Compact 系统重设计 执行计划（二）

**目标:** 实现重新注入、TUI 层集成和端到端验收

**技术栈:** Rust 2021, tokio async, serde, thiserror, tracing

**设计文档:** spec/feature_20260428_F001_compact-redesign/spec-design.md

## 改动总览

本计划（spec-plan-2）覆盖重新注入模块和 `compact_task()` 统一入口重写 + TUI 集成。
- Task 5（重新注入）新建 `rust-create-agent/src/agent/compact/re_inject.rs`，从消息历史中提取最近读取的文件路径和 SkillPreloadMiddleware 注入的 Skills 路径，异步读取文件内容并以 System 消息重新注入
- Task 6（`compact_task()` 重写 + TUI 集成）重写 TUI 层的 `compact_task()` 函数，集成 Micro-compact、Full Compact、重新注入三阶段流程
- Task 5 和 Task 6 是顺序依赖：Task 6 的 `compact_task()` 调用 Task 5 的 `re_inject()` 函数
- 关键决策：Skills 路径识别通过检查 Ai 消息中 `read_file` 工具调用的 `arguments.path` 字段——SkillPreloadMiddleware 注入的 fake read_file 调用的 path 指向 `.claude/skills/` 目录下的 SKILL.md 文件，通过路径后缀匹配 `/skills/` 来区分普通 read_file 和 Skill 注入

---

### Task 0: 环境准备

**背景:**
确保 spec-plan-1 中的 Task 1-4 已完成，compact 核心层模块可用。本计划的 Task 5-6 依赖 spec-plan-1 的输出。

**执行步骤:**
- [x] 验证 spec-plan-1 的所有 Task 已完成
  - `ls rust-create-agent/src/agent/compact/`
  - 预期: 看到 config.rs / invariant.rs / micro.rs / full.rs / mod.rs 五个文件

**检查步骤:**
- [x] compact 核心模块编译通过
  - `cargo build -p rust-create-agent 2>&1 | tail -3`
  - 预期: 输出包含 "Finished"，无 error
- [x] compact 核心模块测试通过
  - `cargo test -p rust-create-agent --lib 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"

---

### Task 5: 重新注入（Post-compact Re-injection）

**背景:**
Full Compact 将整个对话历史压缩为一条摘要 System 消息，agent 压缩后丢失当前工作上下文——最近读取的文件内容和激活的 Skills 指令都不在新消息历史中。如果不重新注入，agent 需要重新读取文件、重新加载 Skills 才能继续工作，严重影响工作连续性。本 Task 新建 `re_inject.rs` 模块，实现从压缩前消息历史中提取最近文件路径和 Skills 路径、异步读取文件内容、以 System 消息形式重新注入的完整流程。本 Task 依赖 Task 1 的 `CompactConfig`（`re_inject_max_files`、`re_inject_file_budget` 等字段），不依赖 Task 2-4。本 Task 的输出 `re_inject()` 函数将被 Task 6 的 `compact_task()` 在 Full Compact 后调用。

**涉及文件:**
- 新建: `rust-create-agent/src/agent/compact/re_inject.rs`
- 修改: `rust-create-agent/src/agent/compact/mod.rs`（Task 1 创建，添加 `pub mod re_inject;`）

**执行步骤:**

- [x] 新建 `re_inject.rs`，定义 `ReInjectResult` 结构体和模块导入
  - 位置: `rust-create-agent/src/agent/compact/re_inject.rs`（新文件）
  - 引入依赖:
    ```rust
    use crate::agent::compact::config::CompactConfig;
    use crate::messages::{BaseMessage, MessageContent};
    use std::path::Path;
    use tracing::{debug, warn};
    ```
  - 定义结果结构体:
    ```rust
    /// 重新注入结果
    #[derive(Debug, Clone)]
    pub struct ReInjectResult {
        /// 重新注入的 System 消息列表
        pub messages: Vec<BaseMessage>,
        /// 成功注入的文件数量
        pub files_injected: usize,
        /// 成功注入的 Skills 数量
        pub skills_injected: usize,
    }
    ```
  - 原因: `ReInjectResult` 封装注入结果和元数据，供 `compact_task()` 记录日志和决策

- [x] 实现 `extract_recent_files()` 函数
  - 位置: `rust-create-agent/src/agent/compact/re_inject.rs`，在 `ReInjectResult` 定义之后
  - 函数签名: `fn extract_recent_files(messages: &[BaseMessage], max_files: usize) -> Vec<String>`
  - 核心逻辑:
    ```rust
    /// 从消息历史中提取最近通过 read_file 工具读取的文件路径（去重，保留最新）
    fn extract_recent_files(messages: &[BaseMessage], max_files: usize) -> Vec<String> {
        let mut seen = std::collections::HashSet::<String>::new();
        let mut paths = Vec::new();

        // 反向遍历，优先取最新的
        for msg in messages.iter().rev() {
            if let Some(tool_calls) = msg.tool_calls().get(0).map(|_| msg.tool_calls()) {
                for tc in tool_calls {
                    if tc.name == "read_file" {
                        if let Some(path) = tc.arguments.get("path").and_then(|v| v.as_str()) {
                            // 排除 Skills 路径（路径中包含 /skills/ 且以 SKILL.md 结尾）
                            if is_skills_path(path) {
                                continue;
                            }
                            if seen.insert(path.to_string()) {
                                paths.push(path.to_string());
                                if paths.len() >= max_files {
                                    return paths;
                                }
                            }
                        }
                    }
                }
            }
        }

        paths
    }
    ```
  - 原因: 遍历 Ai 消息的 `tool_calls` 字段（经代码确认 `BaseMessage::tool_calls()` 方法返回 `&[ToolCallRequest]`，每个 `ToolCallRequest` 有 `name` 和 `arguments` 字段），找到 `name == "read_file"` 的调用，从 `arguments["path"]` 提取文件路径。反向遍历确保最新路径优先，`HashSet` 去重

- [x] 实现 `is_skills_path()` 辅助函数
  - 位置: `rust-create-agent/src/agent/compact/re_inject.rs`，在 `extract_recent_files()` 之前
  - 函数签名: `fn is_skills_path(path: &str) -> bool`
  - 核心逻辑:
    ```rust
    /// 判断路径是否为 Skills 目录下的 SKILL.md 文件
    /// SkillPreloadMiddleware 注入的 read_file 调用的 path 指向 skills 目录
    /// 匹配规则：路径标准化后包含 "/.claude/skills/" 段，或包含 "/skills/" 且文件名为 SKILL.md
    fn is_skills_path(path: &str) -> bool {
        let normalized = path.replace('\\', "/");
        normalized.contains("/.claude/skills/")
            || (normalized.contains("/skills/") && normalized.ends_with("SKILL.md"))
    }
    ```
  - 原因: 经代码确认，`SkillPreloadMiddleware` 注入的 `read_file` 调用的 `arguments.path` 值来自 `s.path.to_string_lossy()`，其中 `s.path` 是 `SkillMetadata.path`，指向 skills 目录下的 SKILL.md 文件。Skills 目录搜索路径为 `~/.claude/skills/`、全局配置 `skillsDir`、`{cwd}/.claude/skills/`，这些路径都包含 `.claude/skills/` 或 `skills/` 段

- [x] 实现 `extract_skills_paths()` 函数
  - 位置: `rust-create-agent/src/agent/compact/re_inject.rs`，在 `is_skills_path()` 之后
  - 函数签名: `fn extract_skills_paths(messages: &[BaseMessage]) -> Vec<String>`
  - 核心逻辑:
    ```rust
    /// 从消息历史中提取 SkillPreloadMiddleware 注入的 Skills 路径（去重，保留出现顺序）
    fn extract_skills_paths(messages: &[BaseMessage]) -> Vec<String> {
        let mut seen = std::collections::HashSet::<String>::new();
        let mut paths = Vec::new();

        for msg in messages.iter() {
            for tc in msg.tool_calls() {
                if tc.name == "read_file" {
                    if let Some(path) = tc.arguments.get("path").and_then(|v| v.as_str()) {
                        if is_skills_path(path) && seen.insert(path.to_string()) {
                            paths.push(path.to_string());
                        }
                    }
                }
            }
        }

        paths
    }
    ```
  - 原因: SkillPreloadMiddleware 在 `before_agent` 时将 skill 全文以 fake `read_file` 工具调用注入——经代码确认，注入的 Ai 消息中 `tool_calls` 包含 `name="read_file"`、`id="skill_preload_{i}"`、`arguments={"path": "{skill_path}"}` 的 `ToolCallRequest`。通过 `is_skills_path()` 匹配路径来识别 Skill 注入

- [x] 实现 `read_file_with_budget()` 异步函数
  - 位置: `rust-create-agent/src/agent/compact/re_inject.rs`，在 `extract_skills_paths()` 之后
  - 函数签名: `async fn read_file_with_budget(path: &str, max_tokens: u32) -> Option<String>`
  - 核心逻辑:
    ```rust
    /// 异步读取文件并截断到指定 token 预算（字符数 / 4 估算）
    async fn read_file_with_budget(path: &str, max_tokens: u32) -> Option<String> {
        let path_owned = path.to_string();
        let content = tokio::task::spawn_blocking(move || {
            std::fs::read_to_string(&path_owned)
        })
        .await
        .ok()?
        .ok()?;

        let max_chars = max_tokens as usize * 4;
        if content.chars().count() > max_chars {
            let truncated: String = content.chars().take(max_chars).collect();
            debug!(path, max_tokens, "文件内容截断到 {} 字符", max_chars);
            Some(format!("{}...(已截断)", truncated))
        } else {
            Some(content)
        }
    }
    ```
  - 原因: 使用 `tokio::task::spawn_blocking` 避免阻塞异步运行时（文件 I/O 是同步操作）。使用字符数/4 估算 token 数（与 Task 3/4 中的 `estimate_tokens()` 一致），超出预算时截断并添加 `...(已截断)` 后缀

- [x] 实现 `truncate_to_budget()` 辅助函数
  - 位置: `rust-create-agent/src/agent/compact/re_inject.rs`，在 `read_file_with_budget()` 之后
  - 函数签名: `fn truncate_to_budget(contents: &mut Vec<(String, String)>, budget: u32) -> usize`
  - 核心逻辑:
    ```rust
    /// 按总 token 预算截断内容列表，返回保留的条目数
    /// contents: (path, content) 对，按优先级排列
    /// budget: 总 token 预算
    fn truncate_to_budget(contents: &mut Vec<(String, String)>, budget: u32) -> usize {
        let budget_chars = budget as usize * 4;
        let mut used_chars = 0;
        let mut keep_count = 0;

        for (_, content) in contents.iter() {
            let chars = content.chars().count();
            if used_chars + chars > budget_chars {
                break;
            }
            used_chars += chars;
            keep_count += 1;
        }

        contents.truncate(keep_count);
        keep_count
    }
    ```
  - 原因: 文件注入和 Skills 注入各有独立的 token 预算（`re_inject_file_budget` 和 `re_inject_skills_budget`），此函数按优先级顺序逐条累加，超出预算时截断

- [x] 实现 `re_inject()` 核心异步函数
  - 位置: `rust-create-agent/src/agent/compact/re_inject.rs`，在 `truncate_to_budget()` 之后
  - 函数签名:
    ```rust
    /// 执行重新注入：从压缩前消息中提取文件路径和 Skills 路径，
    /// 异步读取内容，以 System 消息形式返回注入列表
    pub async fn re_inject(
        messages: &[BaseMessage],
        config: &CompactConfig,
        cwd: &str,
    ) -> ReInjectResult
    ```
  - 核心逻辑:
    ```rust
    pub async fn re_inject(
        messages: &[BaseMessage],
        config: &CompactConfig,
        cwd: &str,
    ) -> ReInjectResult {
        let mut result_messages: Vec<BaseMessage> = Vec::new();

        // 1. 提取并注入最近读取的文件
        let file_paths = extract_recent_files(messages, config.re_inject_max_files);
        let mut files_injected = 0;

        if !file_paths.is_empty() {
            // 将相对路径转为绝对路径（基于 cwd）
            let resolved_paths: Vec<String> = file_paths.iter().map(|p| {
                if Path::new(p).is_absolute() {
                    p.clone()
                } else {
                    let abs = Path::new(cwd).join(p);
                    abs.to_string_lossy().to_string()
                }
            }).collect();

            // 并发读取所有文件
            let mut file_futures = Vec::new();
            for path in &resolved_paths {
                file_futures.push(read_file_with_budget(path, config.re_inject_max_tokens_per_file));
            }
            let file_contents: Vec<Option<String>> = futures::future::join_all(file_futures).await;

            // 收集成功读取的文件内容
            let mut valid_files: Vec<(String, String)> = Vec::new();
            for (path, content) in file_paths.iter().zip(file_contents.into_iter()) {
                if let Some(content) = content {
                    valid_files.push((path.clone(), content));
                } else {
                    debug!(path, "文件读取失败或不存在，跳过重新注入");
                }
            }

            // 按总预算截断
            truncate_to_budget(&mut valid_files, config.re_inject_file_budget);

            // 生成 System 消息
            for (path, content) in &valid_files {
                let system_content = format!("[最近读取的文件: {}]\n{}", path, content);
                result_messages.push(BaseMessage::system(system_content));
            }
            files_injected = valid_files.len();
        }

        // 2. 提取并注入激活的 Skills
        let skills_paths = extract_skills_paths(messages);
        let mut skills_injected = 0;

        if !skills_paths.is_empty() {
            // 并发读取所有 Skills 文件
            let mut skill_futures = Vec::new();
            for path in &skills_paths {
                skill_futures.push(read_file_with_budget(path, 5000));
            }
            let skill_contents: Vec<Option<String>> = futures::future::join_all(skill_futures).await;

            // 收集成功读取的 Skills 内容
            let mut valid_skills: Vec<(String, String)> = Vec::new();
            for (path, content) in skills_paths.iter().zip(skill_contents.into_iter()) {
                if let Some(content) = content {
                    valid_skills.push((path.clone(), content));
                } else {
                    warn!(path, "Skill 文件读取失败，跳过重新注入");
                }
            }

            // 按总预算截断
            truncate_to_budget(&mut valid_skills, config.re_inject_skills_budget);

            // 生成 System 消息
            for (path, content) in &valid_skills {
                let system_content = format!("[激活的 Skill 指令: {}]\n{}", path, content);
                result_messages.push(BaseMessage::system(system_content));
            }
            skills_injected = valid_skills.len();
        }

        debug!(
            files_injected,
            skills_injected,
            total_messages = result_messages.len(),
            "重新注入完成"
        );

        ReInjectResult {
            messages: result_messages,
            files_injected,
            skills_injected,
        }
    }
    ```
  - **关键设计决策**:
    - 文件路径提取和 Skills 路径提取分别处理，互不干扰
    - 文件路径从 Ai 消息的 `tool_calls` 字段提取（经代码确认 `BaseMessage::tool_calls()` 返回 `&[ToolCallRequest]`，每个 ToolCallRequest 有 `name: String` 和 `arguments: serde_json::Value`）
    - Skills 路径通过 `is_skills_path()` 从所有 read_file 调用中筛选——SkillPreloadMiddleware 注入的 fake read_file 的 path 指向 skills 目录下的 SKILL.md 文件
    - 相对路径基于 `cwd` 参数转为绝对路径，确保文件可读
    - 使用 `futures::future::join_all` 并发读取所有文件，提高性能
    - 每个文件独立截断到 `re_inject_max_tokens_per_file`，然后按总预算 `re_inject_file_budget` 再次截断
    - 注入顺序：先文件（最高优先级），后 Skills
  - 原因: 确保 Full Compact 后 agent 能无缝继续工作——最近读取的文件内容保持可访问，激活的 Skills 指令保持有效

- [x] 修改 `compact/mod.rs`，注册 `re_inject` 子模块并导出公共 API
  - 位置: `rust-create-agent/src/agent/compact/mod.rs`（Task 1 创建）
  - 在 `pub mod full;` 行之后添加:
    ```rust
    pub mod re_inject;
    ```
  - 在 `pub use full::{full_compact, FullCompactResult};` 行之后添加:
    ```rust
    pub use re_inject::{re_inject, ReInjectResult};
    ```
  - 原因: 将 `re_inject` 和 `ReInjectResult` 暴露为 compact 模块的公共 API，供 Task 6 的 `compact_task()` 统一入口调用

- [x] 为 `re_inject` 模块编写单元测试
  - 测试文件: `rust-create-agent/src/agent/compact/re_inject.rs`（内联 `#[cfg(test)] mod tests`）
  - 引入测试依赖:
    ```rust
    use super::*;
    use crate::messages::ToolCallRequest;
    use serde_json::json;
    use std::io::Write;
    ```
  - 辅助函数:
    ```rust
    /// 构造一条含 read_file 工具调用的 Ai 消息
    fn ai_read_file(tc_id: &str, path: &str) -> BaseMessage {
        BaseMessage::ai_with_tool_calls(
            MessageContent::text("reading file"),
            vec![ToolCallRequest::new(
                tc_id,
                "read_file",
                json!({"path": path}),
            )],
        )
    }

    /// 构造一条含 SkillPreload 注入的 Ai 消息
    fn ai_skill_preload(index: usize, skill_path: &str) -> BaseMessage {
        BaseMessage::ai_with_tool_calls(
            MessageContent::text(""),
            vec![ToolCallRequest::new(
                format!("skill_preload_{}", index),
                "read_file",
                json!({"path": skill_path}),
            )],
        )
    }

    /// 构造一条普通 Ai 消息
    fn ai_plain(text: &str) -> BaseMessage {
        BaseMessage::ai(text)
    }

    /// 创建临时文件并返回路径
    fn create_temp_file(dir: &std::path::Path, name: &str, content: &str) -> String {
        let file_path = dir.join(name);
        std::fs::write(&file_path, content).unwrap();
        file_path.to_string_lossy().to_string()
    }

    /// 创建临时 Skill 文件
    fn create_temp_skill(dir: &std::path::Path, name: &str, content: &str) -> String {
        let skill_dir = dir.join(".claude").join("skills").join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let skill_path = skill_dir.join("SKILL.md");
        std::fs::write(&skill_path, content).unwrap();
        skill_path.to_string_lossy().to_string()
    }
    ```
  - 测试场景:

    **is_skills_path:**
    - `test_is_skills_path_cclaude`:
      - 输入: `/home/user/.claude/skills/my-skill/SKILL.md`
      - 预期: 返回 true（包含 `/.claude/skills/`）
    - `test_is_skills_path_project`:
      - 输入: `/project/.claude/skills/other/SKILL.md`
      - 预期: 返回 true
    - `test_is_skills_path_custom_dir`:
      - 输入: `/custom/skills/my-skill/SKILL.md`
      - 预期: 返回 true（包含 `/skills/` 且以 `SKILL.md` 结尾）
    - `test_is_skills_path_normal_file`:
      - 输入: `/project/src/main.rs`
      - 预期: 返回 false
    - `test_is_skills_path_skills_but_not_skill_md`:
      - 输入: `/project/.claude/skills/some-config.json`
      - 预期: 返回 false（不以 SKILL.md 结尾，但包含 `/.claude/skills/`，所以返回 true——注意：包含 `/.claude/skills/` 段即判定为 skills 路径）

    **extract_recent_files:**
    - `test_extract_recent_files_basic`:
      - 输入: `[ai_read_file("tc1", "/a.rs"), ai_read_file("tc2", "/b.rs"), ai_read_file("tc3", "/c.rs")]`，max_files=2
      - 预期: 返回 `["/c.rs", "/b.rs"]`（反向遍历，取最近 2 个）
    - `test_extract_recent_files_dedup`:
      - 输入: `[ai_read_file("tc1", "/a.rs"), ai_plain("done"), ai_read_file("tc2", "/a.rs")]`，max_files=5
      - 预期: 返回 `["/a.rs"]`（去重，仅 1 个）
    - `test_extract_recent_files_excludes_skills`:
      - 输入: `[ai_read_file("tc1", "/project/.claude/skills/test/SKILL.md"), ai_read_file("tc2", "/src/main.rs")]`，max_files=5
      - 预期: 返回 `["/src/main.rs"]`（Skills 路径被排除）
    - `test_extract_recent_files_empty`:
      - 输入: `[ai_plain("no tools")]`
      - 预期: 返回空 Vec
    - `test_extract_recent_files_max_files`:
      - 输入: 10 个 read_file 调用（不同路径），max_files=3
      - 预期: 返回最近 3 个

    **extract_skills_paths:**
    - `test_extract_skills_paths_basic`:
      - 输入: `[ai_skill_preload(0, "/home/.claude/skills/a/SKILL.md"), ai_skill_preload(1, "/home/.claude/skills/b/SKILL.md")]`
      - 预期: 返回 2 个路径
    - `test_extract_skills_paths_dedup`:
      - 输入: `[ai_skill_preload(0, "/skills/a/SKILL.md"), ai_skill_preload(1, "/skills/a/SKILL.md")]`
      - 预期: 返回 1 个路径（去重）
    - `test_extract_skills_paths_excludes_normal_files`:
      - 输入: `[ai_read_file("tc1", "/src/main.rs"), ai_skill_preload(0, "/skills/x/SKILL.md")]`
      - 预期: 仅返回 Skills 路径
    - `test_extract_skills_paths_empty`:
      - 输入: `[ai_plain("no tools")]`
      - 预期: 返回空 Vec

    **truncate_to_budget:**
    - `test_truncate_to_budget_within_budget`:
      - 输入: 3 条内容（每条约 1000 字符），budget=5000（20000 字符）
      - 预期: 保留全部 3 条
    - `test_truncate_to_budget_exceeds_budget`:
      - 输入: 3 条内容（每条 8000 字符），budget=5000（20000 字符）
      - 预期: 保留前 2 条（2x8000=16000 < 20000，第 3 条 24000 > 20000）
    - `test_truncate_to_budget_empty`:
      - 输入: 空列表
      - 预期: 返回 0

    **read_file_with_budget:**
    - `test_read_file_with_budget_basic`:
      - 创建临时文件（内容 "hello world"），max_tokens=100
      - 预期: 返回 "hello world"（不截断）
    - `test_read_file_with_budget_truncation`:
      - 创建临时文件（内容 1000 字符），max_tokens=10（40 字符限制）
      - 预期: 返回内容以 "...(已截断)" 结尾
    - `test_read_file_with_budget_nonexistent`:
      - 输入: 不存在的路径
      - 预期: 返回 None

    **re_inject（集成）:**
    - `test_re_inject_with_files`:
      - 在临时目录创建 2 个文件，构造消息历史包含 read_file 调用（使用绝对路径），config 使用默认值
      - 预期: `files_injected==2`，`skills_injected==0`，`messages` 包含 2 条 System 消息，内容分别包含 `[最近读取的文件: ...]` 前缀
    - `test_re_inject_with_skills`:
      - 在临时目录创建 1 个 Skill 文件，构造消息历史包含 SkillPreload 注入的 Ai 消息
      - 预期: `files_injected==0`，`skills_injected==1`，`messages` 包含 1 条 System 消息，内容包含 `[激活的 Skill 指令: ...]` 前缀
    - `test_re_inject_with_both`:
      - 同时创建文件和 Skill，构造包含两者的消息历史
      - 预期: `files_injected>=1`，`skills_injected>=1`，messages 的文件注入消息排在 Skills 消息之前
    - `test_re_inject_empty_messages`:
      - 输入: 空消息列表
      - 预期: `files_injected==0`，`skills_injected==0`，`messages` 为空
    - `test_re_inject_no_matching_files`:
      - 输入: 消息历史仅包含 bash 工具调用（不包含 read_file）
      - 预期: `files_injected==0`，`messages` 为空
    - `test_re_inject_file_not_found`:
      - 输入: read_file 调用指向不存在的文件路径
      - 预期: `files_injected==0`（文件读取失败静默跳过）
    - `test_re_inject_respects_file_budget`:
      - 输入: 3 个大文件（每个 8000 字符），`re_inject_file_budget=5000`（20000 字符），`re_inject_max_files=5`
      - 预期: 注入的文件数 < 3（受总预算限制截断）
    - `test_re_inject_respects_max_files`:
      - 输入: 10 个文件路径，`re_inject_max_files=3`
      - 预期: `files_injected<=3`
    - `test_re_inject_relative_path_resolution`:
      - 输入: read_file 调用的 path 为相对路径 "src/main.rs"，cwd 为临时目录
      - 预期: 文件内容成功读取（相对路径基于 cwd 解析为绝对路径）

  - 运行命令: `cargo test -p rust-create-agent --lib -- compact::re_inject::tests`
  - 预期: 所有测试通过

**检查步骤:**

- [x] 验证 re_inject.rs 编译通过
- [x] 验证 re_inject 模块在 compact/mod.rs 中正确注册
- [x] 验证 re_inject 函数签名完整
- [x] 验证 ReInjectResult 结构体定义完整
- [x] 验证 extract_recent_files 和 extract_skills_paths 函数存在
- [x] 验证全部单元测试通过
- [x] 验证全量测试无回归

---

### Task 6: compact_task() 统一入口重写 + TUI 集成

**背景:**
Full Compact 的核心流程（结构化摘要 + PTL 降级 + 重新注入）已在前序 Task 中实现，但 TUI 层仍使用旧的 `compact_task()` 函数（自由格式摘要、无重新注入、无 PTL 降级）。本 Task 将 `compact_task()` 完全重写为调用核心层 `full_compact()` + `re_inject()` 的三阶段流程，并将 TUI 层的自动触发机制改为 CompactConfig 驱动。这是整个 Compact 系统重设计的最后一个功能 Task，完成后 Agent 在上下文压力下自动触发压缩，压缩质量对齐 Claude Code 水平，压缩后 Agent 可无缝继续工作。

**上下游影响:**
- 本 Task 依赖 Task 1 的 `CompactConfig`、Task 4 的 `full_compact()` / `FullCompactResult`、Task 5 的 `re_inject()` / `ReInjectResult`
- 本 Task 完成后，Compact 系统的所有功能 Task 全部完成，可直接进入端到端验收

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent.rs`（重写 `compact_task` 函数，L302-396）
- 修改: `rust-agent-tui/src/app/agent_ops.rs`（扩展 auto-compact 触发逻辑 L202-217、Done 分支 L401-412、CompactDone 处理 L587-650、start_micro_compact L764-775）
- 修改: `rust-agent-tui/src/app/thread_ops.rs`（扩展 `start_compact` 传递 CompactConfig + cwd，L110-150）
- 修改: `rust-agent-tui/src/config/types.rs`（AppConfig 新增 compact 字段）
- 修改: `rust-agent-tui/src/app/mod.rs`（新增 `get_compact_config()` 辅助方法）
- 修改: `rust-agent-tui/src/command/compact.rs`（扩展命令描述）
- 修改: `rust-create-agent/src/agent/token.rs`（ContextBudget 新增 builder 方法）

**执行步骤:**

- [x] 在 AppConfig 中新增 `compact` 字段
  - 位置: `rust-agent-tui/src/config/types.rs`，`AppConfig` 结构体（~L72），在 `env` 字段（~L90）之后、`extra` 字段（~L93）之前
  - 新增字段:
    ```rust
    /// Compact 系统配置（缺失时使用 CompactConfig::default()）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact: Option<rust_create_agent::agent::compact::CompactConfig>,
    ```
  - 原因: TUI 层通过 AppConfig 从 `settings.json` 读取 CompactConfig，传递给核心层的 `full_compact()` 和 `re_inject()`。使用 `Option<CompactConfig>` + `skip_serializing_if` 保持向后兼容——旧配置文件中无 compact 字段时为 None，运行时使用 `CompactConfig::default()` + `apply_env_overrides()`

- [x] 新增 `App::get_compact_config()` 辅助方法
  - 位置: `rust-agent-tui/src/app/mod.rs`，在 `App` 结构体的 impl 块中（`App::new()` 方法之后）
  - 方法签名和逻辑:
    ```rust
    /// 获取当前 CompactConfig，优先从 settings.json 读取，缺失时使用默认值，再应用环境变量覆盖
    pub fn get_compact_config(&self) -> rust_create_agent::agent::compact::CompactConfig {
        let mut config = self.zen_config
            .as_ref()
            .and_then(|zc| zc.config.compact.clone())
            .unwrap_or_default();
        config.apply_env_overrides();
        config
    }
    ```
  - 原因: 集中 CompactConfig 获取逻辑，避免在每个调用点重复"读 settings -> fallback default -> apply env"的三段代码

- [x] 扩展 ContextBudget 支持自定义阈值
  - 位置: `rust-create-agent/src/agent/token.rs`，`ContextBudget` impl 块（~L65），在 `should_warn()` 方法之后
  - 新增两个 builder 方法:
    ```rust
    /// 设置自定义 auto_compact_threshold
    pub fn with_auto_compact_threshold(mut self, threshold: f64) -> Self {
        self.auto_compact_threshold = threshold;
        self
    }

    /// 设置自定义 warning_threshold
    pub fn with_warning_threshold(mut self, threshold: f64) -> Self {
        self.warning_threshold = threshold;
        self
    }
    ```
  - 原因: 当前 `ContextBudget::new()` 硬编码 `auto_compact_threshold = 0.85`、`warning_threshold = 0.70`，TUI 层需要通过 CompactConfig 自定义这两个阈值。添加 builder 方法保持向后兼容（现有调用不受影响）

- [x] 扩展 `start_compact` 传递 CompactConfig 和 cwd
  - 位置: `rust-agent-tui/src/app/thread_ops.rs`，`start_compact()` 方法（~L110-150）
  - 修改内容：在 `tokio::spawn` 闭包前获取 CompactConfig 和 cwd，传递给新签名的 `compact_task`:
    ```rust
    pub fn start_compact(&mut self, instructions: String) {
        // ... 前置检查（空消息 ~L111-118、Provider 获取 ~L120-137）保持不变 ...

        let messages = self.agent.agent_state_messages.clone();
        let model = provider.into_model();
        let config = self.get_compact_config(); // 新增
        let cwd = self.cwd.clone();             // 新增

        let (tx, rx) = mpsc::channel::<AgentEvent>(8);
        self.agent.agent_rx = Some(rx);
        self.set_loading(true);
        self.agent.session_token_tracker.reset();

        tokio::spawn(async move {
            agent::compact_task(messages, model, instructions, config, cwd, tx).await;
        });
    }
    ```
  - 原因: 重写后的 `compact_task` 需要调用 `full_compact(messages, model, &config, &instructions)` 和 `re_inject(&messages, &config, &cwd)`，因此需要 CompactConfig 和 cwd 两个额外参数

- [x] 重写 `compact_task()` 函数——调用核心层三阶段流程
  - 位置: `rust-agent-tui/src/app/agent.rs`，`compact_task()` 函数（L302-396，全部替换）
  - 新函数签名:
    ```rust
    pub async fn compact_task(
        messages: Vec<rust_create_agent::messages::BaseMessage>,
        model: Box<dyn rust_create_agent::llm::BaseModel>,
        instructions: String,
        config: rust_create_agent::agent::compact::CompactConfig,
        cwd: String,
        tx: mpsc::Sender<super::AgentEvent>,
    )
    ```
  - 核心逻辑（替换旧的三段式自由格式摘要）:
    ```rust
    {
        use rust_create_agent::agent::compact::{full_compact, re_inject};
        use rust_create_agent::messages::BaseMessage;

        // -- 1. Full Compact: LLM 生成结构化摘要 --
        tracing::info!(msg_count = messages.len(), "compact_task: 开始 Full Compact");

        let compact_result = match full_compact(&messages, model, &config, &instructions).await {
            Ok(result) => result,
            Err(e) => {
                tracing::error!(error = %e, "compact_task: Full Compact 失败");
                let _ = tx.send(super::AgentEvent::CompactError(e.to_string())).await;
                return;
            }
        };

        tracing::info!(
            summary_len = compact_result.summary.len(),
            messages_used = compact_result.messages_used,
            "compact_task: Full Compact 完成"
        );

        // -- 2. Re-inject: 重新注入最近文件和 Skills --
        let re_inject_result = re_inject(&messages, &config, &cwd).await;

        tracing::info!(
            files_injected = re_inject_result.files_injected,
            skills_injected = re_inject_result.skills_injected,
            "compact_task: 重新注入完成"
        );

        // -- 3. 组合结果：摘要 + 重新注入内容通过分隔符拼接 --
        let summary_text = format!(
            "此会话从之前的对话延续。以下是之前对话的摘要。\n\n{}",
            compact_result.summary
        );

        let re_inject_content = if re_inject_result.messages.is_empty() {
            String::new()
        } else {
            let mut parts = Vec::new();
            for msg in &re_inject_result.messages {
                parts.push(msg.content());
            }
            format!("\n\n---RE_INJECT_SEPARATOR---\n{}", parts.join("\n\n"))
        };

        let combined_summary = format!("{}{}", summary_text, re_inject_content);

        let _ = tx.send(super::AgentEvent::CompactDone {
            summary: combined_summary,
            new_thread_id: String::new(),
        }).await;
    }
    ```
  - **关键设计决策:**
    - `compact_task` 不再自行格式化消息、构造 prompt、调用 LLM——全部委托给核心层的 `full_compact()`
    - 重新注入通过 `re_inject()` 函数（从 `compact` 模块 re-export）获取 System 消息列表
    - 结果合并策略：摘要文本和重新注入内容通过 `---RE_INJECT_SEPARATOR---` 标记拼接在 CompactDone.summary 中传递。CompactDone 处理端按此标记拆分：前半部分为摘要（作为 System 消息注入），后半部分为重新注入内容（也作为 System 消息注入）。避免修改 AgentEvent 枚举（保持事件接口稳定）
  - 原因: 旧的 `compact_task` 使用自由格式摘要（500 字限制、3 段式），缺乏结构化指导、PTL 降级和重新注入。重写后调用核心层 `full_compact()`（9 段结构化摘要 + PTL 降级重试）和 `re_inject()`（文件 + Skills 重新注入），确保压缩质量和上下文连续性

- [x] 扩展 CompactDone 处理——解析重新注入内容
  - 位置: `rust-agent-tui/src/app/agent_ops.rs`，`CompactDone` 分支（~L587-650）
  - 替换核心逻辑（保持 Thread 创建、持久化、渲染通知的框架不变，修改消息构造和显示部分）:
    ```rust
    AgentEvent::CompactDone { summary, new_thread_id: _ } => {
        // 拆分摘要和重新注入内容
        let (summary_text, re_inject_messages) = if let Some(idx) = summary.find("---RE_INJECT_SEPARATOR---\n") {
            let parts: (&str, &str) = summary.split_at(idx);
            let re_inject_part = parts.1.strip_prefix("---RE_INJECT_SEPARATOR---\n").unwrap_or("");
            let re_inject_msgs: Vec<BaseMessage> = re_inject_part
                .split("\n\n")
                .filter(|s| !s.trim().is_empty())
                .map(|s| BaseMessage::system(s.to_string()))
                .collect();
            (parts.0.to_string(), re_inject_msgs)
        } else {
            (summary.clone(), Vec::new())
        };

        // 创建新 Thread（标题截断逻辑保持不变）
        let truncated: String = summary_text.chars().take(30).collect();
        let ellipsis = if summary_text.chars().count() > 30 { "..." } else { "" };
        let thread_title = format!("Compact: {}{}", truncated, ellipsis);
        let mut meta = ThreadMeta::new(&self.cwd);
        meta.title = Some(thread_title);
        let store = self.thread_store.clone();
        let new_tid = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.create_thread(meta))
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "compact: 创建新 thread 失败，使用临时 ID");
                    uuid::Uuid::now_v7().to_string()
                })
        });

        // 构造新 Thread 的消息：摘要(System) + 重新注入内容(System)
        // 变更：摘要从 Ai 消息改为 System 消息，追加重新注入 System 消息
        let mut new_messages = vec![BaseMessage::system(summary_text.clone())];
        new_messages.extend(re_inject_messages);

        // 持久化新 Thread 消息（保持不变）
        let store = self.thread_store.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.append_messages(&new_tid, &new_messages))
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, thread_id = %new_tid, "compact: 持久化新 thread 消息失败");
                });
        });

        // 切换到新 Thread
        self.current_thread_id = Some(new_tid.clone());
        self.agent.agent_state_messages = new_messages.clone();

        // 清空显示消息，插入压缩提示 + 摘要 + 重新注入信息
        self.core.view_messages.clear();
        let compact_vm = MessageViewModel::system(
            "上下文已压缩（从旧对话迁移到新 Thread）".to_string(),
        );
        self.core.view_messages.push(compact_vm);
        let summary_vm = MessageViewModel::from_base_message(
            &BaseMessage::ai(format!("压缩摘要：\n{}", summary_text)),
            &[],
        );
        self.core.view_messages.push(summary_vm);

        // 显示重新注入信息（如果有）
        let inject_count = new_messages.len() - 1; // 减去摘要消息
        if inject_count > 0 {
            let inject_vm = MessageViewModel::system(
                format!("已重新注入 {} 条上下文（文件/Skills）", inject_count),
            );
            self.core.view_messages.push(inject_vm);
        }

        // 通知渲染线程重建显示（保持不变）
        let _ = self
            .core.render_tx
            .send(crate::ui::render_thread::RenderEvent::Clear);
        for vm in &self.core.view_messages {
            let _ = self
                .core.render_tx
                .send(crate::ui::render_thread::RenderEvent::AddMessage(
                    vm.clone(),
                ));
        }

        self.set_loading(false);
        self.agent.agent_rx = None;

        // 重置 Langfuse session（保持不变）
        self.langfuse.langfuse_session = None;
        self.agent.auto_compact_failures = 0;

        // 刷新 compact 期间缓冲的消息（保持不变）
        // ... 后续代码与现有 CompactDone 处理完全一致 ...
    }
    ```
  - **关键变更:**
    - 新 Thread 的消息从 `vec![BaseMessage::ai(summary)]` 改为 `vec![BaseMessage::system(summary)] + re_inject_messages`——摘要改为 System 消息（避免 Agent 误认为自己的回复），重新注入内容也是 System 消息
    - 持久化的消息包含摘要 + 重新注入，agent_state_messages 同步更新
    - UI 显示新增重新注入条数的提示
  - 原因: CompactDone 处理需要将重新注入的 System 消息正确持久化到新 Thread 中，作为 Agent 后续对话的上下文

- [x] 扩展 auto-compact 触发逻辑——使用 CompactConfig 驱动阈值
  - 位置: `rust-agent-tui/src/app/agent_ops.rs`，`TokenUsageUpdate` 分支（~L202-217）
  - 替换硬编码的 `auto_compact_failures < 3` 和 `ContextBudget::new()` 为 CompactConfig 驱动:
    ```rust
    AgentEvent::TokenUsageUpdate { usage, model: _model } => {
        // 累积到会话追踪器
        self.agent.session_token_tracker.accumulate(&usage);
        // 更新 spinner 的 token 显示
        let total = self.agent.session_token_tracker.total_input_tokens
            + self.agent.session_token_tracker.total_output_tokens;
        self.spinner_state.set_token_count(total as usize);

        // compact 被完全禁用
        if std::env::var("DISABLE_COMPACT").is_ok() {
            return (true, false, false);
        }

        // 从 settings.json 获取 CompactConfig
        let compact_config = self.get_compact_config();

        // auto-compact 被禁用
        if !compact_config.auto_compact_enabled {
            return (true, false, false);
        }

        // circuit breaker: 连续失败达到上限后不再自动触发
        if self.agent.auto_compact_failures < compact_config.max_consecutive_failures {
            let budget = rust_create_agent::agent::token::ContextBudget::new(
                self.agent.context_window,
            )
            .with_auto_compact_threshold(compact_config.auto_compact_threshold);
            if budget.should_auto_compact(&self.agent.session_token_tracker) {
                self.agent.needs_auto_compact = true;
            }
        }
        (true, false, false)
    }
    ```
  - 原因: 当前硬编码 `auto_compact_failures < 3` 和 `ContextBudget::new(context_window)` 使用默认阈值 0.85。改为从 CompactConfig 读取 `max_consecutive_failures` 和 `auto_compact_threshold`，实现配置可调

- [x] 扩展 Done 分支中的 micro-compact 触发——使用 CompactConfig 驱动
  - 位置: `rust-agent-tui/src/app/agent_ops.rs`，`Done` 分支中 micro-compact 区间判断（~L401-412）
  - 替换:
    ```rust
    // Auto-compact 两级策略
    if self.agent.needs_auto_compact {
        self.agent.needs_auto_compact = false;
        tracing::info!("auto-compact: context threshold reached, triggering full compact");
        self.start_compact("auto".to_string());
        return (true, false, true);
    } else {
        // micro-compact 区间: 使用 CompactConfig 的 micro_compact_threshold
        let compact_config = self.get_compact_config();
        let budget = rust_create_agent::agent::token::ContextBudget::new(
            self.agent.context_window,
        )
        .with_warning_threshold(compact_config.micro_compact_threshold);
        if budget.should_warn(&self.agent.session_token_tracker) {
            self.start_micro_compact();
        }
    }
    ```
  - 原因: 当前 micro-compact 触发阈值硬编码为 0.70（`ContextBudget::DEFAULT_WARNING_THRESHOLD`），改为从 CompactConfig 的 `micro_compact_threshold` 字段读取

- [x] 扩展 `start_micro_compact` 使用 Task 3 的增强版 Micro-compact
  - 位置: `rust-agent-tui/src/app/agent_ops.rs`，`start_micro_compact()` 方法（~L764-775）
  - 替换为调用核心层增强版:
    ```rust
    pub fn start_micro_compact(&mut self) {
        use rust_create_agent::agent::compact::micro_compact_enhanced;
        let config = self.get_compact_config();
        let cleared = micro_compact_enhanced(
            &mut self.agent.agent_state_messages,
            &config,
        );
        if cleared > 0 {
            tracing::info!(cleared, "micro-compact: enhanced compact completed");
            let vm = MessageViewModel::system(
                format!("Micro-compact: 清除了 {} 个旧工具结果", cleared)
            );
            self.core.view_messages.push(vm.clone());
            let _ = self.core.render_tx.send(RenderEvent::AddMessage(vm));
        }
    }
    ```
  - 原因: 当前 `start_micro_compact` 调用旧的 `micro_compact(messages, keep_recent=10)`（按字符数 500 清除），改为调用 Task 3 的增强版 `micro_compact_enhanced(messages, &config)`（白名单 + 时间衰减 + 图片清除 + 工具对保护）

- [x] 扩展 `/compact` 命令描述
  - 位置: `rust-agent-tui/src/command/compact.rs`，`description()` 方法（~L11）
  - 替换:
    ```rust
    fn description(&self) -> &str {
        "压缩对话上下文（结构化摘要 + 重新注入最近文件/Skills）"
    }
    ```
  - 原因: 更新命令描述，反映新的压缩能力（结构化摘要 + 重新注入），帮助用户了解命令行为

- [x] 为 AppConfig compact 字段编写序列化测试
  - 测试文件: `rust-agent-tui/src/config/types.rs`（内联 `#[cfg(test)] mod tests` 块，在现有测试之后追加）
  - 测试场景:
    - `test_app_config_compact_serde_roundtrip`:
      - 构造含 `compact` 字段的 JSON（`{"compact": {"autoCompactEnabled": false, "autoCompactThreshold": 0.9}}`）
      - 反序列化为 AppConfig，验证 `compact.is_some()`，`auto_compact_enabled == false`，`auto_compact_threshold == 0.9`
    - `test_app_config_compact_none_when_absent`:
      - 构造不含 compact 字段的 JSON
      - 预期: `app_config.compact.is_none()`
    - `test_app_config_compact_skip_when_none`:
      - AppConfig::default() 序列化
      - 预期: 输出不包含 "compact"
  - 运行命令: `cargo test -p rust-agent-tui --lib -- config::types::tests::test_app_config_compact`
  - 预期: 所有测试通过

- [x] 为 ContextBudget builder 方法编写单元测试
  - 测试文件: `rust-create-agent/src/agent/token.rs`（内联 `#[cfg(test)] mod tests` 块，在现有测试之后追加）
  - 测试场景:
    - `test_context_budget_with_auto_compact_threshold`:
      - `ContextBudget::new(200_000).with_auto_compact_threshold(0.9)`，构造使用量 85% 的 tracker
      - 预期: `should_auto_compact` 返回 false（85% < 90%）
    - `test_context_budget_with_warning_threshold`:
      - `ContextBudget::new(200_000).with_warning_threshold(0.5)`，构造使用量 55% 的 tracker
      - 预期: `should_warn` 返回 true（55% > 50%）
  - 运行命令: `cargo test -p rust-create-agent --lib -- token::tests::test_context_budget_with`
  - 预期: 所有测试通过

- [x] 为 TUI 集成编写 Headless 集成测试
  - 测试文件: `rust-agent-tui/src/ui/headless.rs`（在现有 `#[cfg(test)] mod tests` 块中追加）
  - 引入测试依赖:
    ```rust
    use crate::app::events::AgentEvent;
    use crate::app::App;
    use rust_create_agent::messages::BaseMessage;
    ```
  - 辅助函数:
    ```rust
    /// 构造模拟的 CompactDone 事件（包含摘要 + 重新注入内容）
    fn make_compact_done_event(summary: &str, re_inject_parts: &[&str]) -> AgentEvent {
        let re_inject_content = if re_inject_parts.is_empty() {
            String::new()
        } else {
            format!("\n\n---RE_INJECT_SEPARATOR---\n{}", re_inject_parts.join("\n\n"))
        };
        let combined = format!("{}{}", summary, re_inject_content);
        AgentEvent::CompactDone {
            summary: combined,
            new_thread_id: String::new(),
        }
    }
    ```
  - 测试场景:

    **CompactDone 事件处理——重新注入拆分:**
    - `test_compact_done_with_re_inject`:
      - 创建 Headless App（120x30），push `CompactDone` 事件（摘要 "Test summary" + 2 条重新注入内容 "[最近读取的文件: /a.rs]\ncontent1" 和 "[激活的 Skill 指令: skill.md]\ncontent2"），process_pending_events
      - 预期: view_messages 中包含 "上下文已压缩" + "Test summary" + "已重新注入 2 条上下文"
    - `test_compact_done_without_re_inject`:
      - 创建 Headless App，push `CompactDone` 事件（仅摘要 "Simple summary"，无重新注入），process_pending_events
      - 预期: view_messages 中包含 "上下文已压缩" + "Simple summary"，不包含 "重新注入"

    **CompactConfig 获取:**
    - `test_get_compact_config_default`:
      - App::new_headless() 的 zen_config 为 None
      - 调用 `app.get_compact_config()`
      - 预期: 返回的配置与 `CompactConfig::default()` 各字段一致
    - `test_get_compact_config_from_settings`:
      - 构造含 compact 字段的 ZenConfig（`auto_compact_threshold=0.9`），设置到 App.zen_config
      - 预期: `app.get_compact_config().auto_compact_threshold` 约 0.9

    **Auto-compact 禁用:**
    - `test_auto_compact_disabled_by_env`:
      - 设置 `DISABLE_AUTO_COMPACT=1` 环境变量，push TokenUsageUpdate 事件（高 token 使用量 >85%），process_pending_events
      - 预期: `app.agent.needs_auto_compact` 保持 false
      - 测试后清理: `std::env::remove_var("DISABLE_AUTO_COMPACT")`

  - 运行命令: `cargo test -p rust-agent-tui --lib -- ui::headless::tests::compact 2>&1 | tail -20`
  - 预期: 所有测试通过

  - **注意**: Headless 测试中 `CompactDone` 处理涉及 ThreadStore 持久化和 `tokio::task::block_in_place`——经代码确认 `App::new_headless()` 使用内存 ThreadStore（`InMemoryThreadStore`），block_in_place 在 tokio test runtime 中可正常工作

**检查步骤:**

- [x] 验证 AppConfig compact 字段编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 "Compiling" 或 "Finished" 且无 error

- [x] 验证 compact_task 新签名正确
  - `grep -n 'pub async fn compact_task' rust-agent-tui/src/app/agent.rs`
  - 预期: 输出一行，包含 `config:` 和 `cwd:` 参数

- [x] 验证 compact_task 不再包含旧的自由格式摘要逻辑
  - `grep -c 'truncate_content\|500 字以内' rust-agent-tui/src/app/agent.rs`
  - 预期: 计数为 0

- [x] 验证 compact_task 调用核心层函数
  - `grep -c 'full_compact\|re_inject(\|RE_INJECT_SEPARATOR' rust-agent-tui/src/app/agent.rs`
  - 预期: 计数 >= 3

- [x] 验证 start_compact 传递 CompactConfig 和 cwd
  - `grep -n 'get_compact_config\|cwd.clone' rust-agent-tui/src/app/thread_ops.rs`
  - 预期: 输出包含 `get_compact_config` 和 `cwd.clone`

- [x] 验证 auto-compact 触发使用 CompactConfig
  - `grep -n 'compact_config\|max_consecutive_failures\|auto_compact_threshold' rust-agent-tui/src/app/agent_ops.rs`
  - 预期: 输出包含这些字段引用

- [x] 验证 CompactDone 处理拆分重新注入内容
  - `grep -c 'RE_INJECT_SEPARATOR\|re_inject_messages' rust-agent-tui/src/app/agent_ops.rs`
  - 预期: 计数 >= 2

- [x] 验证 /compact 命令描述已更新
  - `grep 'description' rust-agent-tui/src/command/compact.rs`
  - 预期: 输出包含 "结构化摘要" 和 "重新注入"

- [x] 验证 ContextBudget 新增 builder 方法
  - `grep -n 'with_auto_compact_threshold\|with_warning_threshold' rust-create-agent/src/agent/token.rs`
  - 预期: 输出包含这两个方法签名

- [x] 验证 AppConfig compact 序列化测试通过
  - `cargo test -p rust-agent-tui --lib -- config::types::tests::test_app_config_compact 2>&1 | tail -10`
  - 预期: 输出包含 "test result: ok"

- [x] 验证 Headless 集成测试通过
  - `cargo test -p rust-agent-tui --lib -- ui::headless::tests::compact 2>&1 | tail -15`
  - 预期: 输出包含 "test result: ok" 且无 FAILED

- [x] 验证全量测试无回归
  - `cargo test 2>&1 | tail -15`
  - 预期: 输出包含 "test result: ok"，所有 crate 测试通过

---

### Task 7: Compact 系统重设计总体验收

**前置条件:**
- spec-plan-1 的 Task 1-4 和本计划的 Task 5-6 全部完成
- `cargo build` 全 workspace 编译通过
- 所有单元测试通过

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test 2>&1 | tail -15`
   - 预期: 全部测试通过，输出包含 "test result: ok"
   - 失败排查: 按 crate 定位失败测试（`cargo test -p rust-create-agent` / `cargo test -p rust-agent-tui`），检查对应 Task 的测试步骤

2. 验证 CompactConfig 从 settings.json 加载
   - `cargo test -p rust-agent-tui --lib -- config::types::tests 2>&1 | tail -10`
   - 预期: 所有 config 测试通过，包括新增的 compact 字段序列化测试
   - 失败排查: 检查 Task 6 的 AppConfig 扩展步骤

3. 验证环境变量覆盖生效
   - `DISABLE_COMPACT=1 cargo test -p rust-create-agent --lib -- compact::config::tests::test_from_env 2>&1 | tail -5`
   - 预期: 测试通过，DISABLE_COMPACT 使 auto_compact_enabled = false
   - 失败排查: 检查 Task 1 的 from_env() 实现

4. 验证 Micro-compact 增强策略（白名单 + 时间衰减 + 工具对保护）
   - `cargo test -p rust-create-agent --lib -- compact::micro::tests 2>&1 | tail -10`
   - 预期: 所有 micro 测试通过
   - 失败排查: 检查 Task 3 的 micro.rs 实现

5. 验证 Full Compact 结构化摘要 + PTL 降级
   - `cargo test -p rust-create-agent --lib -- compact::full::tests 2>&1 | tail -10`
   - 预期: 所有 full 测试通过
   - 失败排查: 检查 Task 4 的 full.rs 实现

6. 验证重新注入模块
   - `cargo test -p rust-create-agent --lib -- compact::re_inject::tests 2>&1 | tail -10`
   - 预期: 所有 re_inject 测试通过
   - 失败排查: 检查 Task 5 的 re_inject.rs 实现

7. 验证 TUI 层集成（Headless 测试）
   - `cargo test -p rust-agent-tui --lib -- compact 2>&1 | tail -10`
   - 预期: Headless 集成测试通过，验证 CompactDone 事件处理包含重新注入内容
   - 失败排查: 检查 Task 6 的 agent.rs 重写和 agent_ops.rs 扩展

8. 验证全 workspace 编译无错误无警告
   - `cargo build 2>&1 | grep -E "error|warning" | head -10`
   - 预期: 无 error 输出（warning 可接受）
   - 失败排查: 按编译错误信息定位到具体 Task 的代码修改
