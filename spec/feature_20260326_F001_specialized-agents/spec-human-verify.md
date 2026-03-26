# Specialized Agents 人工验收清单

**生成时间:** 2026-03-26
**关联计划:** [spec-plan.md](./spec-plan.md)
**关联设计:** [spec-design.md](./spec-design.md)

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 检查 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 确认 `.claude/agents/` 目录存在: `ls .claude/agents/`
- [ ] [AUTO] 确认两个 agent 文件均已创建: `ls .claude/agents/explorer.md .claude/agents/web-researcher.md`
- [ ] [AUTO] 编译 rust-agent-middlewares: `cargo build -p rust-agent-middlewares 2>&1 | tail -3`

---

## 验收项目

### 场景 1：Explorer Agent 配置验证

#### - [x] 1.1 文件完整结构（frontmatter + body）

- **来源:** Task 1 / Task 3 端到端验证
- **操作步骤:**
  1. [A] `grep -c "^---$" .claude/agents/explorer.md` → 期望: 输出 `2`（开头和结尾各一个 `---`，即完整 frontmatter 块）
  2. [A] `wc -l .claude/agents/explorer.md` → 期望: 行数 > 20（frontmatter + 系统提示词正文均存在）
- **异常排查:**
  - 如果 grep 输出 < 2：frontmatter 格式不完整，检查文件是否缺少结尾 `---`

#### - [x] 1.2 tools 白名单包含四个正确工具

- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep -A 10 "^tools:" .claude/agents/explorer.md` → 期望: 输出包含 `read_file`、`glob_files`、`search_files_rg`、`bash` 四项
  2. [A] `grep -c "read_file\|glob_files\|search_files_rg\|bash" .claude/agents/explorer.md` → 期望: 输出 ≥ 4
- **异常排查:**
  - 如果某工具名缺失：用 `cat .claude/agents/explorer.md` 确认 tools 字段内容，手动添加缺失项

#### - [x] 1.3 disallowedTools 覆盖所有写操作工具

- **来源:** Task 1 检查步骤 / spec-design.md 验收标准
- **操作步骤:**
  1. [A] `grep -A 5 "^disallowedTools:" .claude/agents/explorer.md` → 期望: 输出包含 `write_file`、`edit_file`、`folder_operations`
  2. [A] `grep -c "write_file\|edit_file\|folder_operations" .claude/agents/explorer.md` → 期望: 输出 ≥ 3
- **异常排查:**
  - 如果 disallowedTools 缺少某项：写操作未被禁用，Explorer 将无法以只读模式安全运行

#### - [x] 1.4 maxTurns=30 且系统提示词包含只读约束说明

- **来源:** Task 1 检查步骤
- **操作步骤:**
  1. [A] `grep "maxTurns:" .claude/agents/explorer.md` → 期望: 输出 `maxTurns: 30`
  2. [A] `grep -i "read.only\|no write\|never modif\|write operation" .claude/agents/explorer.md` → 期望: 至少有一行匹配（确认系统提示词包含只读约束描述）
- **异常排查:**
  - 如果 maxTurns 不是 30：编辑文件修正
  - 如果系统提示词无只读约束：LLM 可能不会主动拒绝写操作请求，需补充约束文字

---

### 场景 2：Web Research Agent 配置验证

#### - [x] 2.1 文件完整结构（frontmatter + body）

- **来源:** Task 2 / Task 3 端到端验证
- **操作步骤:**
  1. [A] `grep -c "^---$" .claude/agents/web-researcher.md` → 期望: 输出 `2`
  2. [A] `wc -l .claude/agents/web-researcher.md` → 期望: 行数 > 20
- **异常排查:**
  - 同 1.1

#### - [x] 2.2 tools 白名单包含三个正确工具

- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -A 6 "^tools:" .claude/agents/web-researcher.md` → 期望: 输出包含 `bash`、`write_file`、`read_file`，不含 `glob_files` 或 `search_files_rg`
  2. [A] `grep -c "bash\|write_file\|read_file" .claude/agents/web-researcher.md` → 期望: 输出 ≥ 3
- **异常排查:**
  - 如果工具数量不对：检查 tools 字段列表是否缩进正确（YAML 列表格式）

#### - [x] 2.3 disallowedTools 包含四个文件系统/编辑工具

- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -A 6 "^disallowedTools:" .claude/agents/web-researcher.md` → 期望: 输出包含 `edit_file`、`folder_operations`、`glob_files`、`search_files_rg`
- **异常排查:**
  - 如果缺少某项：Web Agent 可能意外获得代码库搜索权限，与其专用角色不符

#### - [x] 2.4 maxTurns=40 且系统提示词包含 web-crawler 使用说明

- **来源:** Task 2 检查步骤 / spec-design.md
- **操作步骤:**
  1. [A] `grep "maxTurns:" .claude/agents/web-researcher.md` → 期望: 输出 `maxTurns: 40`
  2. [A] `grep "langgraph-js/web-fetch\|npx.*web-fetch" .claude/agents/web-researcher.md` → 期望: 至少一行匹配（确认系统提示词包含 web-crawler CLI 用法）
- **异常排查:**
  - 如果无 web-fetch 引用：系统提示词使用旧的 curl 方案，需更新为 `#web-crawler` skill

---

### 场景 3：YAML 解析兼容性与编译验证

#### - [x] 3.1 rust-agent-middlewares 编译通过

- **来源:** Task 3 前置条件
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-middlewares 2>&1 | grep -E "^error|Finished"` → 期望: 输出包含 `Finished`，不含 `error[`
- **异常排查:**
  - 如果出现编译错误：本 feature 仅新增 `.md` 配置文件，不修改 Rust 代码，编译错误与本 feature 无关，排查其他改动

#### - [x] 3.2 SubAgentTool 工具过滤单元测试通过

- **来源:** Task 3 端到端验证
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares -- subagent 2>&1 | tail -5` → 期望: 输出包含 `test result: ok`，不含 `FAILED`
  2. [A] `cargo test -p rust-agent-middlewares 2>&1 | grep -E "test result"` → 期望: 所有测试套件均为 `ok`
- **异常排查:**
  - 如果有测试失败：查看失败的具体测试名称，大概率与 YAML 格式不兼容相关；检查 frontmatter 字段名是否与 `AgentDefinition` struct 字段一致

---

### 场景 4：HITL 运行时行为验证

#### - [!] 4.1 非 YOLO 模式下 Web Agent bash 调用触发 HITL 审批弹窗
> **备注:** 子 Agent 架构上不含 HitlMiddleware（见 subagent/tool.rs 实现 + CLAUDE.md 说明），bash 在子 Agent 中不触发 HITL 属于设计决定。spec-design.md 验收标准需修订。

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 以非 YOLO 模式启动 TUI（`cargo run -p rust-agent-tui`，不加 `-y` 参数），输入消息要求父 Agent 委派 web-research agent 执行一次搜索任务（如："请用 web-research agent 搜索 Rust 异步编程"）。观察是否在 bash 工具被调用前弹出 HITL 审批弹窗 → 是/否
- **异常排查:**
  - 如果未弹出：确认 `YOLO_MODE` 环境变量未设置；确认 HitlMiddleware 已在父 Agent 中注册

#### - [x] 4.2 Explorer Agent 只读工具不触发 HITL 审批

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 以非 YOLO 模式启动 TUI，输入消息委派 explorer agent 分析当前目录（如："请用 explorer agent 分析项目结构"）。观察 `read_file`、`glob_files`、`search_files_rg` 工具调用时**是否不弹出** HITL 弹窗，直接执行 → 是/否（是 = 没有弹窗，符合预期）
- **异常排查:**
  - 如果弹出审批弹窗：检查 HitlMiddleware 的 `requires_approval` 函数，确认只读工具（read_file/glob_files/search_files_rg）不在审批名单中

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 场景 1 Explorer 配置 | 1.1 | 文件完整结构 | 2 | 0 | ✅ | |
| 场景 1 Explorer 配置 | 1.2 | tools 白名单四个工具 | 2 | 0 | ✅ | |
| 场景 1 Explorer 配置 | 1.3 | disallowedTools 覆盖写操作 | 2 | 0 | ✅ | |
| 场景 1 Explorer 配置 | 1.4 | maxTurns=30 & 只读约束说明 | 2 | 0 | ✅ | |
| 场景 2 Web 配置 | 2.1 | 文件完整结构 | 2 | 0 | ✅ | |
| 场景 2 Web 配置 | 2.2 | tools 白名单三个工具 | 2 | 0 | ✅ | |
| 场景 2 Web 配置 | 2.3 | disallowedTools 四个工具 | 1 | 0 | ✅ | |
| 场景 2 Web 配置 | 2.4 | maxTurns=40 & npx web-fetch 引用 | 2 | 0 | ✅ | |
| 场景 3 YAML 解析 | 3.1 | 编译通过 | 1 | 0 | ✅ | |
| 场景 3 YAML 解析 | 3.2 | 单元测试通过 | 2 | 0 | ✅ | |
| 场景 4 HITL 行为 | 4.1 | Web bash 触发 HITL 弹窗 | 0 | 1 | ❌ | 子 Agent 架构无 HITL，spec 需修订 |
| 场景 4 HITL 行为 | 4.2 | Explorer 只读工具不触发 HITL | 0 | 1 | ✅ | |

**验收结论:** ⬜ 全部通过 / ✅ 存在问题（1 项）
