# 系统提示词 领域

## 领域综述

系统提示词领域负责 Agent 系统提示词的架构设计，将单体提示词拆分为独立段落文件，支持基于功能的条件注入。

核心职责：
- 12 个 .md 段落文件按编号排序，8 个静态 + 4 个 feature-gated
- include_str! 编译时嵌入，零运行时开销
- PromptFeatures 从环境变量推断功能开关
- 动态覆盖块从 AgentOverrides 生成

## 核心流程

### 提示词合成流程

```
build_system_prompt(overrides, cwd, features)
  → 静态段落（01-08）: 始终 include_str!
  → Feature-gated 段落（10-13）: PromptFeatures 条件判断
  → 环境变量替换: {{cwd}}, {{is_git_repo}}, {{platform}}, {{os_version}}, {{date}}
  → AgentOverrides 覆盖块: persona/tone/proactiveness 注入到最前面
```

## 技术方案总结

| 维度 | 选型 |
|------|------|
| 段落文件 | prompts/sections/ 目录，12 个 .md 文件按编号排序 |
| 静态段落 | 01_intro, 02_system, 03_doing_tasks, 04_actions, 05_using_tools, 06_tone_style, 07_communicating, 08_env |
| Feature-gated | 10_hitl, 11_subagent, 12_cron, 13_skills |
| 编译嵌入 | include_str! 宏，零运行时开销 |
| 条件注入 | PromptFeatures::detect() 从环境变量推断 |
| 环境变量 | PromptEnv::detect() 运行时环境检测 |

## Feature 附录

### feature_20260430_F001_system-prompt-restructure
**摘要:** 系统提示词拆分为独立段落文件并支持 Feature 条件注入
**关键决策:**
- 提示词从单体文件拆分为 sections/ 子目录下 12 个按编号排序的 .md 文件
- 8 个静态段落始终包含，4 个 Feature-gated 段落通过 PromptFeatures 条件注入
- 使用 include_str! 编译时嵌入，零运行时开销
- PromptFeatures::detect() 从环境变量推断，长期改为从中间件注册列表推断
- 同步 claude-code 工具 description 详细版本
- 工具名从 PascalCase 映射为 snake_case
**归档:** [链接](../../archive/feature_20260430_F001_system-prompt-restructure/)
**归档日期:** 2026-04-30

---

## 相关 Feature
- → [agent.md](./agent.md) — ReActAgent.with_system_prompt() 注入
- → [tui.md](./tui.md) — TUI 层 build_system_prompt() 调用
