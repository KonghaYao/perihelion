# Feature: 20260430_F001 - system-prompt-restructure

## 需求背景

当前系统提示词存在三个问题：

1. **单体文件，难以维护**：`system.md` 和 `default.md` 两个文件承载了所有提示词内容，各段落职责边界模糊，无法独立演进。
2. **无 Feature 控制**：HITL 审批模式、SubAgent、Cron、Skills 等功能的提示词无法条件注入，要么全部包含、要么完全不包含。
3. **工具提示词过于简略**：工具 `description()` 只有一行文本，缺少 claude-code 中的详细用法指导（使用示例、决策树、错误处理建议），导致 LLM 不能充分利用工具能力。

## 目标

- 将系统提示词拆分为语义明确的独立 `.md` 文件，每个文件有清晰的段落职责说明
- 建立 Feature-gated 的条件注入机制，根据运行时功能开关动态合成提示词
- 同步 claude-code 项目的系统提示词段落和工具 description/parameters，对齐提示词质量
- 保持 `build_system_prompt()` 函数式架构，新增 `PromptFeatures` 参数控制条件段落

## 方案设计

### 1. 文件结构

提示词文件从 `rust-agent-tui/prompts/` 迁移到 `rust-agent-tui/prompts/sections/` 子目录，按编号排序：

```
rust-agent-tui/prompts/
├── sections/
│   ├── 01_intro.md              # 身份定义 + 安全策略
│   ├── 02_system.md             # 系统级行为指导
│   ├── 03_doing_tasks.md        # 任务执行行为规范
│   ├── 04_actions.md            # 危险操作谨慎原则
│   ├── 05_using_tools.md        # 工具选择决策树 + 搜索策略
│   ├── 06_tone_style.md         # 语气风格
│   ├── 07_communicating.md      # 用户沟通方式
│   ├── 08_env.md                # 环境变量模板（含占位符）
│   ├── 10_hitl.md               # [Feature] HITL 审批模式
│   ├── 11_subagent.md           # [Feature] SubAgent 工具使用
│   ├── 12_cron.md               # [Feature] Cron 定时任务
│   └── 13_skills.md             # [Feature] Skills 使用与发现
├── system.md                    # 删除（拆分到 sections/）
└── default.md                   # 删除（拆分到 sections/）
```

### 2. 各段落职责与内容来源

#### 静态段落（始终包含）

**`01_intro.md`** — 身份定义
- 来源：claude-code `getSimpleIntroSection()` + 现有 `default.md` 开头
- 内容：AI 助手身份声明、安全策略（仅防御性安全任务）、URL 生成禁令

**`02_system.md`** — 系统级行为
- 来源：claude-code `getSimpleSystemSection()`
- 内容：文本输出显示规则、工具权限模式说明、工具列表延迟加载、system-reminder 标签说明、外部数据注入检测、hooks 反馈处理、上下文自动压缩

**`03_doing_tasks.md`** — 任务执行规范
- 来源：claude-code `getSimpleDoingTasksSection()`
- 内容：软件工程任务定位、主动性平衡、代码修改原则（先读后改）、文件创建决策、安全漏洞防范、代码风格指导（不加多余注释/错误处理/抽象）、结果如实报告、用户反馈处理方式

**`04_actions.md`** — 危险操作谨慎原则
- 来源：claude-code `getActionsSection()`
- 内容：操作可逆性与影响范围评估、高风险操作需确认的判断标准、障碍处理原则

**`05_using_tools.md`** — 工具使用策略
- 来源：claude-code `getUsingYourToolsSection()`
- 内容：工具选择决策树（4 步）、专用工具优先于 Bash 原则、搜索查询构建指导、成本不对称原则、渐进式搜索回退链、搜索策略按复杂度分级
- 注意：工具名常量需要替换为 perihelion 的工具名（如 `Read` → `read_file`）

**`06_tone_style.md`** — 语气风格
- 来源：claude-code `getSimpleToneAndStyleSection()`
- 内容：emoji 使用限制、用户能力正面假设、代码引用格式（file_path:line_number）

**`07_communicating.md`** — 用户沟通方式
- 来源：claude-code `getOutputEfficiencySection()`
- 内容：面向人而非控制台的写作风格、不叙述内部机制、更新时的上下文恢复、避免过度格式化、完成后直接报告

**`08_env.md`** — 环境信息模板
- 来源：现有 `system.md` 中的 `<env>` 块
- 内容：保持 `{{cwd}}`、`{{is_git_repo}}`、`{{platform}}`、`{{os_version}}`、`{{date}}` 占位符

#### Feature-gated 段落（条件包含）

**`10_hitl.md`** — HITL 审批模式
- 触发条件：`PromptFeatures::hitl_enabled == true`
- 内容：工具审批行为指导、审批决策类型说明、被拒绝后的应对策略
- 来源：claude-code 中 HITL 相关段落 + 现有 HITL 中间件逻辑

**`11_subagent.md`** — SubAgent 工具使用
- 触发条件：`PromptFeatures::subagent_enabled == true`
- 内容：launch_agent 工具使用指导、子 agent 委派策略、上下文隔离原则
- 来源：claude-code `getAgentToolSection()` + SubAgent 中间件说明

**`12_cron.md`** — Cron 定时任务
- 触发条件：`PromptFeatures::cron_enabled == true`
- 内容：定时任务的 cron 表达式格式、持久化行为、触发规则
- 来源：现有 Cron 中间件相关提示词

**`13_skills.md`** — Skills 使用与发现
- 触发条件：`PromptFeatures::skills_enabled == true`
- 内容：Skills 搜索顺序、# 前缀触发方式、Skill 定义文件结构
- 来源：现有 Skills 中间件说明 + claude-code Skills 指导

#### 动态生成段落

**Agent 覆盖块**（无文件，代码动态生成）
- 保留现有 `build_agent_overrides_block()` 逻辑
- 从 `AgentOverrides` 的 persona/tone/proactiveness 字段生成
- 置于提示词最前面

### 3. 代码架构

#### PromptFeatures 结构体

```rust
// rust-agent-tui/src/prompt.rs

/// 控制 Feature-gated 提示词段落的注入
pub struct PromptFeatures {
    pub hitl_enabled: bool,
    pub subagent_enabled: bool,
    pub cron_enabled: bool,
    pub skills_enabled: bool,
}

impl PromptFeatures {
    /// 根据 Agent 组装时的中间件配置自动推断
    pub fn detect() -> Self {
        Self {
            hitl_enabled: std::env::var("YOLO_MODE").as_deref() == Ok("false"),
            subagent_enabled: true,  // TODO: 从中间件注册状态推断
            cron_enabled: true,      // TODO: 从中间件注册状态推断
            skills_enabled: true,    // TODO: 从中间件注册状态推断
        }
    }
}
```

#### build_system_prompt 重构

```rust
pub fn build_system_prompt(
    overrides: Option<&AgentOverrides>,
    cwd: &str,
    features: PromptFeatures,
) -> String {
    let env = PromptEnv::detect(cwd);

    // 静态段落（编译时嵌入，按编号顺序）
    let static_sections = [
        include_str!("../prompts/sections/01_intro.md"),
        include_str!("../prompts/sections/02_system.md"),
        include_str!("../prompts/sections/03_doing_tasks.md"),
        include_str!("../prompts/sections/04_actions.md"),
        include_str!("../prompts/sections/05_using_tools.md"),
        include_str!("../prompts/sections/06_tone_style.md"),
        include_str!("../prompts/sections/07_communicating.md"),
        include_str!("../prompts/sections/08_env.md"),
    ];

    // Feature-gated 段落（条件拼接）
    let mut gated_sections = Vec::new();
    if features.hitl_enabled {
        gated_sections.push(include_str!("../prompts/sections/10_hitl.md"));
    }
    if features.subagent_enabled {
        gated_sections.push(include_str!("../prompts/sections/11_subagent.md"));
    }
    if features.cron_enabled {
        gated_sections.push(include_str!("../prompts/sections/12_cron.md"));
    }
    if features.skills_enabled {
        gated_sections.push(include_str!("../prompts/sections/13_skills.md"));
    }

    // 合成
    let overrides_block = overrides
        .map(build_agent_overrides_block)
        .unwrap_or_default();

    let mut parts = Vec::new();
    parts.push(overrides_block);
    parts.extend(static_sections);
    parts.extend(gated_sections);

    parts
        .join("\n\n")
        .replace("{{cwd}}", &env.cwd)
        .replace("{{is_git_repo}}", if env.is_git_repo { "Yes" } else { "No" })
        .replace("{{platform}}", &env.platform)
        .replace("{{os_version}}", &env.os_version)
        .replace("{{date}}", &env.date)
}
```

#### 调用方改动

`rust-agent-tui/src/app/agent.rs` 中组装 Agent 时：

```rust
// 之前
let system_prompt = build_system_prompt(overrides, cwd);

// 之后
let features = PromptFeatures::detect();
let system_prompt = build_system_prompt(overrides, cwd, features);
```

### 4. 工具提示词同步

#### 同步清单

| 工具 | claude-code 对应 | 同步内容 |
|------|------------------|---------|
| `read_file` | `FileReadTool/prompt.ts` | description 扩展（含用法、行号格式、大文件建议），注释掉 PDF/image/notebook 支持段落 |
| `write_file` | `FileWriteTool/prompt.ts` | description 扩展（原子写入说明、父目录自动创建），参数 description 丰富 |
| `edit_file` | `FileEditTool/prompt.ts` | description 扩展（old_string 唯一性说明、replace_all 用法），行号前缀格式说明 |
| `glob_files` | `GlobTool/prompt.ts` | description 扩展（glob 模式语法、排序规则），参数说明丰富 |
| `search_files_rg` | `GrepTool/prompt.ts` | description 扩展（正则语法、输出模式、上下文行），参数说明丰富 |
| `bash` | `BashTool/prompt.ts` | description 扩展（超时说明、跨平台命令），参数说明丰富 |
| `ask_user_question` | `AskUserQuestionTool/prompt.ts` | 保持现有格式（已与 claude-code 基本对齐） |
| `todo_write` | `TodoWriteTool/prompt.ts` | description 扩展（用法指导） |
| `launch_agent` | `AgentTool/prompt.ts` | description 扩展（委派策略、工具过滤规则） |

#### 同步规则

1. **已实现功能**：完整搬运 claude-code 的 description 文本
2. **未实现功能**：用 `// TODO:` 注释标记对应行，不在 description 中输出。例如：
   ```rust
   fn description(&self) -> &str {
       // TODO: 支持图片后取消注释 → "This tool allows reading images..."
       "Reads a file from the local filesystem. ..."
   }
   ```
3. **工具名替换**：claude-code 的 `Read` → `read_file`、`Write` → `write_file`、`Edit` → `edit_file` 等
4. **参数 schema 不新增**：只在已有参数上丰富 description 字段，不为未实现的功能添加新参数

#### 工具 description 存储方式

每个工具的 description 从 `&str` 常量改为 `const` 或 `include_str!`，以便长文本维护：

```rust
// 选项 A：直接 const 字符串（适合中等长度）
const DESCRIPTION: &str = "Reads a file from the local filesystem...\n\nUsage:\n...";

// 选项 B：include_str! 外部文件（适合超长 description）
fn description(&self) -> &str {
    include_str!("../prompts/tools/read_file.md")
}
```

推荐选项 A（const 字符串），与现有代码风格一致。如果 description 超过 50 行再考虑外部文件。

### 5. 实现步骤

分四个阶段，每阶段可独立验证：

**阶段 1：目录结构与静态段落迁移**
1. 创建 `rust-agent-tui/prompts/sections/` 目录
2. 编写 01-08 号段落文件（从 claude-code 同步 + 现有 system.md 拆分）
3. 修改 `build_system_prompt()` 使用 `include_str!` 加载段落
4. 删除旧的 `system.md` 和 `default.md`
5. 验证：运行 TUI 确认提示词正常

**阶段 2：Feature-gated 段落**
1. 编写 10-13 号段落文件
2. 添加 `PromptFeatures` 结构体
3. 修改 `build_system_prompt()` 签名，加入条件拼接
4. 修改调用方传入 `PromptFeatures`
5. 验证：分别测试各 feature 开关的效果

**阶段 3：工具提示词同步**
1. 逐个工具扩展 description 和 parameters
2. 标注未实现功能的 TODO 注释
3. 验证：对比 claude-code 工具输出，确认对齐

**阶段 4：清理与测试**
1. 删除旧文件引用
2. 更新 headless 测试中的系统提示词断言
3. 更新 CLAUDE.md 中的相关说明

## 实现要点

- **include_str! 编译时嵌入**：零运行时开销，但修改 `.md` 文件后需重新编译
- **工具名映射**：claude-code 使用 PascalCase 工具名（`Read`、`Write`），perihelion 使用 snake_case（`read_file`、`write_file`），同步时需全部替换
- **Feature 检测方式**：当前 `PromptFeatures::detect()` 用环境变量推断，长期应改为从中间件注册列表推断（更准确）
- **子 Agent 提示词**：SubAgent 调用 `build_system_prompt()` 时也需要传入对应的 `PromptFeatures`，需同步修改 SubAgent 的 system_builder 闭包

## 约束一致性

- **Workspace 多 crate 分层**：提示词文件和 `build_system_prompt()` 位于 `rust-agent-tui`（应用层），不违反分层约束
- **Middleware Chain 模式**：系统提示词注入通过 `ReActAgent::with_system_prompt()` 完成，不使用已废弃的 `PrependSystemMiddleware`，与 M3 方案一致
- **工具系统**：工具 description/parameters 修改仅影响 `BaseTool` trait 的返回值，不改变 trait 签名

## 验收标准

- [ ] `rust-agent-tui/prompts/sections/` 下有 12 个 `.md` 文件（8 静态 + 4 feature-gated）
- [ ] 旧 `system.md` 和 `default.md` 已删除
- [ ] `build_system_prompt()` 使用 `include_str!` 加载段落 + `PromptFeatures` 条件注入
- [ ] 9 个工具的 description 已同步 claude-code 详细版本
- [ ] 未实现功能已用 TODO 注释标记
- [ ] 所有现有测试通过
- [ ] `cargo run -p rust-agent-tui` 启动后功能正常
- [ ] `-a` 参数启用 HITL 后，10_hitl.md 段落被注入
