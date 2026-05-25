# SkillPreload 未将工具调用附加到主 Agent 消息历史

**状态**：Closed/Fixed
**优先级**：中
**创建日期**：2026-05-25
**关闭日期**：2026-05-25

## 问题描述

主 Agent 的 `SkillPreloadMiddleware` 虽然在中间件链中注册（`builder.rs:352`），但 `preload_skills` 始终为空列表（`executor.rs:380` 硬编码 `Vec::new()`），导致用户在消息中使用 `/skill-name` 触发 skill 时，skill 全文内容**不会**以 fake Read 工具调用注入到消息历史中。

SubAgent 路径正常工作——agent 定义 frontmatter 中的 `skills` 字段通过 `SubAgentMiddlewareConfig.skill_names` 正确传递给 `SkillPreloadMiddleware`。

## 症状详情

| 路径 | preload_skills 来源 | 是否生效 |
|------|---------------------|----------|
| 主 Agent（TUI submit） | `executor.rs:380` 硬编码 `Vec::new()` | **不生效** |
| SubAgent（agent 定义） | `SubAgentMiddlewareConfig.skill_names`（frontmatter `skills` 字段） | 正常 |

### 具体表现

1. 用户输入 `/skill-name` 提交消息
2. TUI 识别到 skill 名称，走 `Action::Submit(text)` 提交（`keyboard.rs:774`）
3. `submit_message()` 调用 `client.prompt(&message_content)`（`agent_submit.rs:248`）
4. `executor.rs` 构建 `AcpAgentConfig { preload_skills: Vec::new(), ... }`
5. `builder.rs:352` 创建 `SkillPreloadMiddleware::new(vec![], &cwd)` → `before_agent` 直接返回（空列表 early return）
6. LLM 收到消息但看不到 skill 全文内容，只能靠 system prompt 中的 skill 摘要

### 相关死代码

`agent_submit.rs:11` 中的 `parse_skill_names_from_input` 函数被标记为 `#[allow(dead_code)]`，该函数能从用户输入中解析 `/skill-name` 模式，但从未被调用。

## 复现条件

- **复现频率**：必现
- **触发步骤**：
  1. 启动 TUI
  2. 输入包含 `/skill-name` 的消息（如 `/caveman 帮我分析代码`）
  3. 提交后观察消息历史中无 fake Read 工具调用
- **环境**：所有环境

## 涉及文件

- `peri-acp/src/session/executor.rs:380` —— `preload_skills` 硬编码为空
- `peri-acp/src/agent/builder.rs:352` —— 主 Agent 链中注册 SkillPreloadMiddleware（但传入空列表）
- `peri-middlewares/src/subagent/skill_preload.rs:70` —— `before_agent` 空列表时 early return
- `peri-tui/src/app/agent_submit.rs:11` —— `parse_skill_names_from_input` 死代码，未被调用
- `peri-tui/src/event/keyboard.rs:762` —— skill 匹配后仅走 Submit 流程，未传递 skill 名称

## 修复方案

采用**中间件自检测**方案（不修改 ACP 协议管线）：

- `SkillPreloadMiddleware::before_agent` 在 `skill_names` 为空时，从 state 最后一条 Human 消息中提取 `/xxx` token，与可用 skill 列表匹配后注入 fake Read 工具调用
- 新增 `extract_skill_names_from_text()` 公共函数替代 TUI 侧死代码 `parse_skill_names_from_input`
- SubAgent 路径不受影响（仍使用构造时传入的显式 `skill_names`）

**Commits**: `fd85fb2` + `5745e10`
**实现计划**: `docs/superpowers/plans/2026-05-25-skill-preload-main-agent.md`
