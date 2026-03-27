# Specialized Agents 执行计划

**目标:** 创建 Explorer Agent 和 Web Research Agent 两个专用子 Agent 定义文件

**技术栈:** Markdown + YAML frontmatter（`.claude/agents/*.md` 声明式配置）

**设计文档:** [spec-design.md](./spec-design.md)

---

### Task 1: Explorer Agent 定义文件

**涉及文件:**
- 新建: `.claude/agents/explorer.md`

**执行步骤:**
- [x] 创建 `.claude/agents/explorer.md`，写入 YAML frontmatter
  - `tools`: `read_file`、`glob_files`、`search_files_rg`、`bash`
  - `disallowedTools`: `write_file`、`edit_file`、`folder_operations`
  - `maxTurns: 30`
- [x] 在 frontmatter 之后写入系统提示词 body，包含以下要点：
  - 角色定义：代码库探索专家，只读模式
  - 探索方法论五步骤（全局扫描 → 架构定位 → 深度分析 → 历史追踪 → 结构化输出）
  - bash 仅限只读命令（git log/blame/diff/show、find、wc 等），明确禁止写操作
  - 输出格式：目录树 + 核心模块清单 + 关键接口 + 数据流描述

**检查步骤:**
- [x] 验证文件存在
  - `ls .claude/agents/explorer.md`
  - 预期: 文件路径正常输出，无报错
- [x] 验证 tools 字段包含四个工具
  - `grep -A 10 "^tools:" .claude/agents/explorer.md`
  - 预期: 输出包含 `read_file`、`glob_files`、`search_files_rg`、`bash`
- [x] 验证 disallowedTools 字段存在且包含写相关工具
  - `grep -A 5 "disallowedTools:" .claude/agents/explorer.md`
  - 预期: 输出包含 `write_file`、`edit_file`、`folder_operations`
- [x] 验证 maxTurns 设置
  - `grep "maxTurns:" .claude/agents/explorer.md`
  - 预期: 输出 `maxTurns: 30`

---

### Task 2: Web Research Agent 定义文件

**涉及文件:**
- 新建: `.claude/agents/web-researcher.md`

**执行步骤:**
- [x] 创建 `.claude/agents/web-researcher.md`，写入 YAML frontmatter
  - `tools`: `bash`、`write_file`、`read_file`
  - `disallowedTools`: `edit_file`、`folder_operations`、`glob_files`、`search_files_rg`
  - `maxTurns: 40`
- [x] 在 frontmatter 之后写入系统提示词 body，包含以下要点：
  - 角色定义：网络研究专家
  - 研究方法论六步骤（制定策略 → 搜索引擎查询 → 内容抓取 → 多页追踪 → 中间结果落盘 → 综合输出）
  - DuckDuckGo HTML 接口用法：`curl "https://html.duckduckgo.com/html/?q=QUERY" -A "Mozilla/5.0" -L --max-time 30`
  - 中间结果统一写入 `/tmp/research_TIMESTAMP.md`
  - 安全约束：禁止爬取需登录页面、curl 加 `--max-time 30`、每轮最多追踪 5 个 URL、深度 ≤ 2 层
  - 输出格式：带引用链接的 Markdown 报告

**检查步骤:**
- [x] 验证文件存在
  - `ls .claude/agents/web-researcher.md`
  - 预期: 文件路径正常输出，无报错
- [x] 验证 tools 字段包含三个工具
  - `grep -A 6 "^tools:" .claude/agents/web-researcher.md`
  - 预期: 输出包含 `bash`、`write_file`、`read_file`
- [x] 验证 disallowedTools 字段存在且包含编辑和文件系统工具
  - `grep -A 6 "disallowedTools:" .claude/agents/web-researcher.md`
  - 预期: 输出包含 `edit_file`、`folder_operations`、`glob_files`、`search_files_rg`
- [x] 验证 maxTurns 设置
  - `grep "maxTurns:" .claude/agents/web-researcher.md`
  - 预期: 输出 `maxTurns: 40`

---

### Task 3: Specialized Agents Acceptance

**Prerequisites:**
- Start command: `cargo build -p rust-agent-middlewares 2>&1 | tail -5`（验证代码编译无误）
- 确认 `.claude/agents/` 目录下两个文件均已创建

**End-to-end verification:**

1. [x] 验证 explorer.md 完整格式（frontmatter + body）
   - `cat .claude/agents/explorer.md`
   - Expected: 文件包含 YAML frontmatter（`---` 开头结尾）、`tools` 字段、`disallowedTools` 字段、`maxTurns: 30`，以及 frontmatter 之后的系统提示词正文
   - On failure: 检查 Task 1 执行步骤

2. [x] 验证 web-researcher.md 完整格式（frontmatter + body）
   - `cat .claude/agents/web-researcher.md`
   - Expected: 文件包含 YAML frontmatter、`tools` 字段（3 个工具）、`disallowedTools` 字段（4 个工具）、`maxTurns: 40`，以及系统提示词正文含 DuckDuckGo 搜索示例
   - On failure: 检查 Task 2 执行步骤

3. [x] 验证 agent_define.rs 的 YAML 解析逻辑能兼容 YAML 列表格式
   - `cargo test -p rust-agent-middlewares -- claude_agent_parser 2>&1 | tail -10`
   - Expected: 测试全部通过（`test result: ok`），无 FAILED
   - On failure: 检查 frontmatter YAML 格式是否符合 serde_yaml 解析要求（字段名、列表缩进）
