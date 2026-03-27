# Feature: 20260326_F001 - specialized-agents

## 需求背景

目前 Perihelion 框架的父 Agent 承担了所有任务（代码探索、网络研究、文件操作等），导致系统提示词过于通用，工具集过宽，执行效率和专注度不足。

通过 SubAgentMiddleware 的 `launch_agent` 工具，父 Agent 可以将专项子任务委派给专门的子 Agent 执行。本 feature 将提供两个开箱即用的专用 Agent 定义：

- **Explorer Agent**：代码库探索与架构分析专家，只读模式运行
- **Web Research Agent**：网络研究专家，通过 curl 进行搜索、抓取与多页分析

## 目标

- 提供 `explorer` Agent 定义，专注代码库文件结构与逻辑探索，无写权限
- 提供 `web-researcher` Agent 定义，专注网络搜索与页面抓取，支持落盘中间结果
- 两个 Agent 均通过 `.claude/agents/*.md` 声明式配置，无需新增 Rust 代码

## 方案设计

### Agent 文件结构

两个 Agent 均放置于项目根目录的 `.claude/agents/` 路径下：

```
.claude/agents/
├── explorer.md        ← Explorer Agent 定义
└── web-researcher.md  ← Web Research Agent 定义
```

SubAgentMiddleware 通过 `launch_agent(agent_id: "explorer", ...)` 读取对应文件，解析 YAML frontmatter 获取工具白名单/黑名单，并将 body 作为子 Agent 的系统提示词注入。

### Explorer Agent 设计

**YAML frontmatter：**

```yaml
---
name: Explorer Agent
description: 代码库探索专家，分析文件结构、模块依赖和代码逻辑
tools:
  - read_file
  - glob_files
  - search_files_rg
  - bash
disallowedTools:
  - write_file
  - edit_file
  - folder_operations
maxTurns: 30
---
```

**工具职责说明：**

| 工具 | 用途 |
|------|------|
| `glob_files` | 扫描目录结构，获取全局文件列表 |
| `read_file` | 读取关键文件（Cargo.toml、入口点、核心模块） |
| `search_files_rg` | 定位关键符号（trait/struct/fn/impl） |
| `bash` | 执行只读 git 命令（log/blame/diff/show）和 find/wc 等 |

**探索方法论（系统提示词要点）：**

1. **全局扫描**：用 `glob_files` 获取完整目录树，识别 Cargo.toml、README、主入口点
2. **架构定位**：读取 Workspace 配置，理解 crate 分层与模块划分
3. **深度分析**：用 `search_files_rg` 定位关键符号，用 `read_file` 深入核心模块
4. **历史追踪**（可选）：`bash` 执行 `git log --oneline -20` 了解近期变更
5. **输出报告**：结构化输出——目录树、核心模块清单、关键接口定义、数据流描述

**安全约束：**
- 严格禁止写操作（write/edit/folder_operations 均在 disallowedTools）
- bash 仅用于只读命令，LLM 系统提示词明确声明此约束
- Explorer 不触发 HITL（只读工具无需审批；bash 在 YOLO 模式下免 HITL，非 YOLO 仍需审批）

### Web Research Agent 设计

**YAML frontmatter：**

```yaml
---
name: Web Research Agent
description: 网络研究专家，通过 curl 抓取网页、搜索引擎查询、多页内容分析
tools:
  - bash
  - write_file
  - read_file
disallowedTools:
  - edit_file
  - folder_operations
  - glob_files
  - search_files_rg
maxTurns: 40
---
```

**工具职责说明：**

| 工具 | 用途 |
|------|------|
| `bash` | 执行 curl/python 抓取页面，解析 HTML |
| `write_file` | 将中间抓取结果落盘（/tmp/research_*.md） |
| `read_file` | 读取已落盘的中间结果用于综合分析 |

**研究方法论（系统提示词要点）：**

1. **制定策略**：将任务分解为 2-3 个搜索关键词
2. **搜索引擎查询**：使用 DuckDuckGo HTML 接口（无 API Key 要求）：
   ```bash
   curl "https://html.duckduckgo.com/html/?q=QUERY" -A "Mozilla/5.0" -L --max-time 30
   ```
   解析返回 HTML，提取标题 + URL 列表
3. **页面内容抓取**：对相关 URL 执行 curl，用 `sed`/`grep`/`python3 -c` 提取正文
4. **多页追踪**：识别重要链接，递归抓取（深度 ≤ 2 层，每轮 URL ≤ 5 个）
5. **中间结果落盘**：重要内容写入 `/tmp/research_TIMESTAMP.md`，避免上下文膨胀
6. **综合输出**：整合所有来源，输出带引用链接的 Markdown 格式报告

**安全约束：**
- 禁止爬取需要登录的页面
- curl 统一加 `--max-time 30`，防止挂起
- bash 触发 HITL（非 YOLO 模式），用户可审批每次网络请求

### 数据流

![Agent 委派与执行流程](./images/01-flow.png)

```
父 Agent
  └─ launch_agent(agent_id: "explorer" / "web-researcher", task)
       ├─ 读取 .claude/agents/{id}.md
       ├─ 解析 YAML frontmatter（tools 白名单 + disallowedTools 黑名单）
       ├─ 过滤父工具集 → 子 Agent 可用工具集
       ├─ 将 body 作为系统提示词注入子 Agent
       └─ 创建独立 ReActAgent 实例
            └─ ReAct 循环（独立上下文，共享父 EventHandler）
                 └─ 返回：[N 个工具调用摘要] + 最终回答文本
```

### 与现有功能的关系

本 feature 完全基于现有 SubAgentMiddleware 和 agent_define.rs 的解析机制实现，无需修改 Rust 代码。复用以下现有功能：

- **SubAgentMiddleware**：`launch_agent` 工具的核心执行逻辑（已有）
- **agent_define.rs**：`.claude/agents/*.md` YAML frontmatter 解析（已有）
- **FilesystemMiddleware**：Explorer 和 Web Agent 的文件工具来源（已有）
- **TerminalMiddleware**：两个 Agent 的 bash 工具来源（已有）
- **HitlMiddleware**：bash 调用的审批拦截（已有，非 YOLO 模式生效）

## 实现要点

1. **只需创建两个 Markdown 文件**，无需任何 Rust 代码改动
2. **工具过滤优先级**：`tools` 字段为白名单（取交集），`disallowedTools` 为额外排除。`launch_agent` 始终排除自身防递归（现有约束）
3. **系统提示词质量**：方法论描述需足够具体，减少 LLM 偏离预期行为的概率
4. **Web Agent 的 HTML 解析**：依赖 bash + Python3（生产环境需确认 Python3 可用）；可降级为 grep/sed 处理纯文本
5. **中间结果路径**：统一写入 `/tmp/`，避免污染项目目录

## 约束一致性

- **禁止下层依赖上层**：本 feature 仅添加配置文件，不涉及 crate 依赖
- **工具系统**：严格使用现有 `BaseTool` / `ToolProvider` 体系，无扩展
- **安全约束**：Explorer 的 disallowedTools 覆盖所有 HITL 敏感工具（write/edit/folder），符合现有 HITL 默认拦截清单约束
- **SubAgent 防递归**：两个 Agent 均不包含 `launch_agent` 工具（由 SubAgentMiddleware 自动排除）

## 验收标准

- [ ] `.claude/agents/explorer.md` 存在，frontmatter 工具白名单包含 `read_file`/`glob_files`/`search_files_rg`/`bash`，disallowedTools 包含 `write_file`/`edit_file`/`folder_operations`
- [ ] `.claude/agents/web-researcher.md` 存在，frontmatter 工具白名单包含 `bash`/`write_file`/`read_file`，disallowedTools 包含 `edit_file`/`folder_operations`/`glob_files`/`search_files_rg`
- [ ] Explorer Agent 完成"分析某目录文件结构"任务，输出包含目录树和核心模块说明
- [ ] Web Research Agent 完成"搜索某技术主题"任务，输出包含来源链接的 Markdown 报告
- [ ] 非 YOLO 模式下，Web Agent 的 bash 工具触发 HITL 审批弹窗
- [ ] Explorer Agent 的只读工具（read_file/glob_files/search_files_rg）不触发 HITL
