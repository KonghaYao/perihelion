# Peri Agent 缺陷分析研究报告

> 分析日期：2026-06-01
> 数据范围：2026-05-15 ~ 2026-06-01（18 天）
> 数据量：528 可见会话 / 555 SubAgent 会话 / 79,099 条消息

---

## 一、数据概览

| 指标 | 值 |
|------|-----|
| 可见会话 | 528 |
| SubAgent 会话 | 555 |
| 总消息 | 79,099 |
| 用户消息 | 3,255 |
| Assistant 消息 | 34,211 |
| Tool 消息 | 41,040 |
| System 消息 | 763 |
| 工具错误总数 | 129 |
| 含错误的会话 | 60 / 528 (11.4%) |

角色分布中 tool 消息占比 51.8%，assistant 占 43.3%，用户消息仅占 4.1%。Agent 每收到一条用户指令，平均产出 21.7 条内部消息（含工具调用和推理），说明这是一个高工具使用密度的 Agent 框架。

---

## 二、缺陷清单

### DEF-001 | Agent 调用不存在的工具 | HIGH | 置信度 90%

**现象**：Agent 产生了 51 次对不存在工具的调用，涉及 8 种幻觉工具名，影响 23 个会话。

| 幻觉工具名 | 调用次数 | 首次出现 | 可能来源 |
|-----------|---------|---------|---------|
| Bash | 31 | 2026-05-25 | LLM 输出 `Bash` 而系统注册的是 `bash`（大小写不匹配） |
| Task | 13 | 2026-05-15 | Claude Code 使用 `Task` 工具名，Peri 未兼容 |
| Agent | 2 | 2026-05-25 | Agent 工具名的变体 |
| fabricate::Brainstorming | 1 | 2026-05-15 | 插件 skill 的完全限定名被当作工具名 |
| bash | 1 | 2026-05-26 | 大小写变体 |
| Shell | 1 | 2026-05-26 | 别名 |
| Reading | 1 | 2026-05-20 | Read 工具的变体 |
| Global | 1 | 2026-05-15 | 不明 |

**根因分析**：

1. **Bash (31次)** — 几乎全部来自 SubAgent（explore agent 等），标题显示为 `explore` 或 `bg-bg-*`。这些 SubAgent 的系统提示词中可能描述了 `Bash`（大写），但实际注册的工具名是 `bash`（小写）或其他变体。工具名大小写不一致是典型的 LLM prompt 工程问题。

2. **Task (13次)** — 全部集中在 2026-05-15（项目早期），这是 Claude Code 原生的 `Task` 工具名。Peri 使用 `Agent` 作为子代理工具名，迁移后未在工具发现层做别名映射。

3. **其他 (7次)** — 零星出现，属于 LLM 创造性幻觉（fabricate::Brainstorming）或用户在系统中输入了非标准工具名。

**建议**：
- 在 `ToolSearchMiddleware` 中增加模糊匹配/别名映射（Bash→bash, Task→Agent）
- SubAgent 构建时验证工具名大小写与注册名一致
- 对 `fabricate::` 前缀的工具名做 namespace strip 后重试

---

### DEF-002 | 工具执行频繁失败 — Missing file_path | MEDIUM | 置信度 70%

**现象**：72 次 `Tool execution failed`，其中 42 次是 `Missing file_path`，占工具错误的 32.6%。

| 失败工具 | Missing file_path 次数 |
|---------|----------------------|
| Read | 28 |
| Write | 14 |
| 其他 | 1 |

另有 19 次用户中断，11 次其他原因。

**根因分析**：Agent 调用 Read/Write 时未传入 `file_path` 参数。这通常发生在：
- LLM 生成的工具调用 JSON 中 `file_path` 字段缺失
- 流式解析时 tool_use 块被截断
- Agent 试图对整个目录而非文件操作

**建议**：
- 在工具执行前增加参数校验，缺失必填参数时返回更明确的错误而非执行失败
- 考虑在 Agent 系统提示中强化 `file_path` 为必填参数的描述

---

### STR-001 | 无效重试循环 | HIGH | 置信度 80%

**现象**：17 个会话中检测到重试循环（同工具连续失败 ≥2 次），影响 17 个会话。

| 工具 | 最大重试次数 | 错误模式 |
|------|------------|---------|
| Read | 7 | Missing file_path — 反复不传参数 |
| Read | 6 | Missing file_path |
| Write | 5 | Missing file_path |
| Write | 4 | Missing file_path |
| Read | 4 | Missing file_path |
| Task | 3 | 工具不存在（DEF-001 关联） |
| Bash | 2 | interrupted by user |

**根因分析**：Agent 在工具失败后不会改变策略。Read 因 `Missing file_path` 失败 7 次，说明 Agent 生成的每一轮 tool_use 都犯了相同的错误（不传 file_path），LLM 没有从错误消息中学到任何东西。

这与 DEF-002 强关联：42 次 Missing file_path 中，部分是同一个会话中的连续重试。

**建议**：
- 在 ReAct 循环中增加"同工具+同参数连续失败检测"：连续 2 次失败后应暂停并报告给用户
- 改进错误反馈：将 `Missing file_path` 改为更明确的提示（如"你调用 Read 时必须传入 file_path 参数"）
- 在 after_tool hook 中检测连续失败模式，注入纠正消息

---

### STR-003 | 部分长会话工具使用过于单一 | MEDIUM | 置信度 50%

**现象**：7 个超过 5 轮的会话只使用了 ≤2 种工具。

**证据**：
- 019e2ad9: 仅 Grep + Read
- 019e2fc5: 仅 Bash
- 019e3b13: 仅 Grep + Read

**可能原因**：
- ToolSearch 的 deferred tools 未正确暴露（Agent 不知道有更好的工具可用）
- 特定场景确实只需要搜索工具（低置信度原因）

---

### UX-001 | 部分会话交互比极高 | MEDIUM | 置信度 60%

**现象**：50 个会话的交互比 >50（每条用户消息触发 50+ 条系统消息）。

| Session | 交互比 | 总消息 | 用户消息 | 标题 |
|---------|-------|--------|---------|------|
| 019e35da | 196.0 | 196 | 1 | "我们现在要给 Agent 显示加上 id" |
| 019e3440 | 125.0 | 125 | 1 | Compact 延续会话 |
| 019e32bf | 104.0 | 208 | 2 | Compact 延续会话 |
| 019e3309 | 104.0 | 624 | 6 | 实现计划 |

**分析**：
- 最高 196:1 的会话是用户发了一条长指令后 Agent 自主跑了 196 条消息，期间用户没有干预
- 多个高交互比会话以 "Compact: 此会话从之前的对话延续" 开头，说明是 compact 后继续的长任务
- 这不一定是坏事——有些任务确实需要 Agent 自主执行大量操作。但 196:1 意味着用户可能在等待很久

**建议**：
- 在 Agent 执行 N 轮后主动向用户汇报进展
- 对高交互比会话检查是否存在冗余搜索循环

---

### UX-002 | 大量重复话题 | LOW | 置信度 40%

**现象**：26 个话题被多次讨论。

| 话题 | 重复次数 | 说明 |
|------|---------|------|
| hello | 59 | 测试消息 |
| Compact: 此会话从之前的对话延续 | 42 | compact 后自动生成的标题 |
| '/users/konghayao/code/ai/peri... | 26 | 同一项目路径开头的会话 |
| /writing-plans | 20 | 反复执行相同 skill |
| 请直接派出三个同步非 bg 的 hello agent | 19 | SubAgent 测试 |
| 请使用 subagent say hello | 16 | SubAgent 测试 |
| 请使用 bg fork subagent say hell | 14 | SubAgent 测试 |

**分析**：去掉测试消息（hello、subagent 测试），真正的重复话题主要是 `/writing-plans`（20次）和同项目下的不同实现任务。42 次 Compact 延续标题说明 compact 是高频操作，但标题缺乏区分度。

**建议**：
- Compact 延续会话应继承原始标题而非用通用文本
- `/writing-plans` 高频使用表明这是核心工作流，可考虑优化其效率

---

### UX-003 | 大量极短用户消息 | LOW | 置信度 35%

**现象**：48.4% 的用户消息 ≤20 字，中位数仅 21 字。

| 长度区间 | 数量 | 占比 |
|---------|------|------|
| 极短 (1-20字) | 1,297 | 48.4% |
| 短 (21-50字) | 724 | 27.0% |
| 中 (51-150字) | 424 | 15.8% |
| 长 (151-500字) | 162 | 6.1% |
| 超长 (>500字) | 70 | 2.6% |

**分析**：这不一定是缺陷。75% 消息 ≤50 字说明用户习惯用简短指令，Agent 能理解短意图是优势而非问题。但 2.6% 的超长消息（>500字）值得关注——可能包含大段需求描述或粘贴的错误日志，Agent 需要能正确处理。

---

## 三、会话效率分析

### 消息数分布（生存曲线）

| 消息数 | 会话数 | 占比 | 解读 |
|--------|-------|------|------|
| 1-2 | 47 | 8.9% | 快速问答或测试 |
| 3-5 | 52 | 9.8% | 简单任务 |
| 6-10 | 67 | 12.7% | |
| 11-20 | 25 | 4.7% | |
| 21-50 | 51 | 9.7% | 中等任务 |
| 51-100 | 60 | 11.4% | |
| 101-200 | 69 | 13.1% | 复杂任务 |
| 201-500 | 80 | 15.2% | 长会话 |
| 500+ | 25 | 4.7% | 超长会话 |

超长会话（>200条）占 19.9%，超短会话（≤2条）占 8.9%。分布呈现双峰：短会话用于快速问答，长会话用于复杂开发任务。

### 超长会话特征（Top 5）

| Session | 消息数 | 时长 | SubAgent | 标题 |
|---------|-------|------|---------|------|
| 019e76e6 | 1,563 | 442min | 18 | grill-me: git 管理工具设计 |
| 019e53a3 | 1,170 | 1164min | 0 | Langfuse 分析 |
| 019e6d73 | 1,128 | 492min | 32 | 实现计划执行 |
| 019e62fc | 945 | 455min | 19 | brainstorming |
| 019e5821 | 933 | 1421min | 0 | grill-me: agent 存储机制重设计 |

观察：
- 超长会话大量使用 SubAgent（平均 3.2 个，最多 32 个）
- 时长可达 23+ 小时，但可能包含大量空闲时间
- 超长会话的工具错误率极低（平均 0.4 次），说明 Agent 在长期运行中是稳定的

### 工具调用密度

| 密度 | 会话数 | 解读 |
|------|-------|------|
| 低 (<2/轮) | 195 | 简单对话为主 |
| 中 (2-4/轮) | 60 | 正常开发任务 |
| 高 (4-8/轮) | 55 | 密集代码搜索和编辑 |
| 极高 (>8/轮) | 218 | 大量并行工具调用 |

41.3% 的会话工具密度"极高"，这与框架的并发工具调用能力有关——Agent 经常在一个 turn 中并行调用 Read + Grep + Glob 等多个工具。

---

## 四、策略质量分析

### 工具使用频率排行

| 工具 | 调用次数 | 占比 |
|------|---------|------|
| Read | 8,656 | 30.4% |
| Bash | 6,328 | 22.2% |
| Edit | 4,507 | 15.8% |
| Grep | 4,410 | 15.5% |
| TodoWrite | 1,624 | 5.7% |
| Agent | 1,127 | 4.0% |
| Write | 810 | 2.8% |
| Glob | 531 | 1.9% |
| AskUserQuestion | 409 | 1.4% |
| 其他 | 612 | 2.1% |

Read + Bash + Edit + Grep 占了 83.9%，这是代码编辑 Agent 的典型工具分布。

### 工具共现对（并行调用模式）

| 组合 | 共现次数 | 解读 |
|------|---------|------|
| Grep + Read | 301 | 搜索后立即读取——最经典的代码理解模式 |
| Glob + Read | 83 | 定位文件后读取 |
| Glob + Grep | 82 | 先找文件再搜内容 |
| Bash + Grep | 72 | 在终端中搜索 |
| Bash + Read | 50 | 执行命令后读取相关文件 |

Grep + Read 是最强的共现对（301次），说明 Agent 的代码理解工作流高度依赖"搜索→阅读"模式。

### 并行工具调用

- 并行调用轮次：2,756 / 24,104 (11.4%)
- 最大并行工具数：15

11.4% 的轮次包含并行工具调用，说明 Agent 在可以并行时确实会利用这一能力。

### Read → Write/Edit 合规性

| 模式 | 次数 |
|------|------|
| 先 Read 再 Write/Edit | 4,905 |
| 未 Read 直接 Write/Edit | 7 |
| **盲写率** | **0.1%** |

**这是一个非常正面的信号**：99.9% 的文件写入操作前都有过 Read。Agent 的"先理解再修改"策略执行得很好。

---

## 五、用户行为画像

### 活跃项目

| 项目 | 会话数 | 占比 |
|------|-------|------|
| ai/perihelion | 449 | 85.0% |
| pazhou/remote-control-server | 42 | 8.0% |
| 其他 | 37 | 7.0% |

85% 的会话集中在 perihelion 项目自身——这是一个"用 Agent 开发 Agent"的 eat-your-own-dogfood 场景。

### 活跃时段

高峰时段：9-11 点、14-17 点、20-23 点（UTC+8），符合开发者作息。凌晨 0-5 点也有少量会话。

### 每日会话趋势

| 日期 | 会话数 | 事件 |
|------|-------|------|
| 2026-05-19 | 21 | |
| 2026-05-20 | 24 | |
| 2026-05-23 | 60 | 峰值——大量功能开发 |
| 2026-05-25 | 39 | |
| 2026-05-26 | 47 | |
| 2026-05-29 | 42 | |
| 2026-05-31 | 32 | |
| 2026-06-01 | 16 | （当天未结束） |

5月23日出现 60 个会话的峰值，可能对应一个大功能开发周期。

---

## 六、综合修复优先级

### 立即修复

| ID | 缺陷 | 影响 | 修复成本 |
|----|------|------|---------|
| DEF-001 | 幻觉工具调用 | 23 会话 / 51 次错误 | 低——增加工具名别名映射 |
| STR-001 | 无效重试循环 | 17 会话 | 中——需修改 ReAct 循环逻辑 |

### 近期优化

| ID | 缺陷 | 影响 | 修复成本 |
|----|------|------|---------|
| DEF-002 | Missing file_path | 35 会话 / 42 次错误 | 低——参数校验 |
| UX-001 | 高交互比 | 50 会话 | 高——需策略层面改进 |
| STR-003 | 工具单一 | 7 会话 | 中——ToolSearch 暴露 |

### 长期关注

| ID | 缺陷 | 影响 | 修复成本 |
|----|------|------|---------|
| UX-002 | 重复话题 | 89 会话 | 高——需跨会话记忆 |
| UX-003 | 极短消息 | 全局 | 低——UI 优化 |

---

## 七、方法论说明

本报告基于纯统计启发式分析，未使用机器学习模型。每个检测项基于明确的规则（正则匹配、计数阈值、序列模式），输出附带置信度评估。置信度反映的是"该缺陷是否为真缺陷"的确定性，而非数据量。

### 局限性

1. **无法检测语义缺陷**：Agent 回答了错误的内容、遗漏了关键信息等语义级问题无法通过结构分析发现
2. **因果关系不明确**：交互比高可能是任务复杂度导致，不一定是 Agent 低效
3. **缺少 token 数据**：SQLite 中未存储 token usage，无法做成本效率分析
4. **样本偏差**：85% 会话来自 perihelion 项目本身，结论不一定适用于其他类型的项目

### 复现

```bash
cd side-projects/agent-defect-analyzer
bun src/main.ts
```

---

## 八、新增检测：死循环与超大载荷分析（v0.2）

### LOOP-001 | Agent 完全重复调用同一工具 | CRITICAL | 置信度 92%

**现象**：9 个会话中存在完全重复的工具调用（同工具 + 同参数），总计 167 次无效调用。

| Session | 工具 | 循环长度 | 描述 |
|---------|------|---------|------|
| 019e72c6 | ExecuteExtraTool | 83 | 连续 83 次调用 AgentResult |
| 019e814c | ExecuteExtraTool | 31 | 连续 31 次调用 AgentResult |
| 019e814c | ExecuteExtraTool | 21 | 同会话另一段连续 21 次 |
| 019e6387 | Read | 8 | 连续读取同一文件 |
| 019e76df | ExecuteExtraTool | 8 | 连续 8 次调用 AgentResult |

**根因分析**：

ExecuteExtraTool 的 AgentResult 查询占据了 4/5 的严重循环。这表明后台 Agent 完成后的结果轮询机制存在缺陷——Agent 在查询未就绪的结果时不会等待或退避，而是疯狂重试。83 次连续调用意味着 Agent 的 ReAct 循环没有任何去重保护。

**修复建议**：
- `ExecuteExtraTool` 对 `AgentResult` 应实现自动退避：首次查询返回 "not ready" 时，注入系统消息"后台任务尚未完成，请执行其他操作或等待"
- 在 `tool_dispatch.rs` 增加"最近 N 次调用去重"检测：连续 3 次同工具+同参数自动中断

---

### LOOP-002 | Agent 在两个工具间振荡 | MEDIUM | 置信度 75%

**现象**：245 个会话中存在 A→B→A→B 的工具振荡模式，影响 193 个会话。

| 振荡模式 | 典型长度 | 出现次数 | 解读 |
|---------|---------|---------|------|
| Grep ↔ Read | 4-10 | 最多 | 搜索后读，读后搜索——正常的代码理解流程 |
| Read ↔ Edit | 4-10 | 较多 | 读后改，改后读——正常的编辑-验证循环 |
| Agent ↔ TodoWrite | 8 | 少量 | 派子 Agent 后更新 Todo |
| Grep ↔ Bash | 4 | 少量 | 搜索后执行命令 |

**分析**：245 个振荡会话中，大部分（Read↔Edit, Grep↔Read）是正常的开发工作流。真正的"振荡"应定义为 ≥6 次交替（3 轮以上无实质进展）。按此标准，严重振荡约 15 个会话。

**修复建议**：不需要完全消除振荡（很多是正常行为），但应在 ≥6 次交替时注入提示"你已多次在 X 和 Y 之间切换，考虑是否有更高效的策略"。

---

### LOOP-003 | Agent 重复调用但结果无变化 | MEDIUM | 置信度 65%

**现象**：179 个会话中 Agent 重复调用工具但得到相同长度的结果，影响 97 个会话。

| 工具 | 最大循环 | 典型场景 |
|------|---------|---------|
| Edit | 17 次 | 反复编辑同一文件，每次结果长度相同 |
| Write | 11 次 | 反复写入同一内容 |
| Read | 8 次 | 反复读取同一文件 |

**分析**：Edit 的 17 次无进展循环值得关注——Agent 可能在尝试不同的编辑策略但文件内容未实际改变（编辑冲突、权限问题等）。Write 的 11 次循环可能是写入失败后重试。

**修复建议**：在工具结果处理中增加"结果相似度检测"——如果连续 3 次结果长度相同且工具参数类似，提示 Agent"该操作似乎没有产生预期效果，请分析原因"。

---

### SIZE-001 | 工具返回超大结果导致上下文膨胀 | HIGH | 置信度 85%

**现象**：19 次工具调用返回超过 100KB 的结果，498 次超过 20KB。

**工具出参大小分布**：

| 大小 | 调用数 | 占比 |
|------|-------|------|
| <1KB | 18,171 | 62.8% |
| 1-5KB | 7,341 | 25.4% |
| 5-20KB | 2,926 | 10.1% |
| 20-50KB | 423 | 1.5% |
| 50-100KB | 56 | 0.2% |
| >100KB | 19 | 0.1% |

**超大出参 Top 5**：

| 工具 | 最大出参 | 入参预览 |
|------|---------|---------|
| Bash | 203.3KB | MCP servers 配置检查命令 |
| Grep | 155.6KB | files_with_matches 全仓搜索 |
| Bash | 114.4KB | git add + commit（含完整 diff） |
| Glob | 109.4KB | 扫描 .claude 目录 |
| Bash | 109.6KB | git add + commit |

**根因分析**：
- Bash 的 203KB 输出来自"检查 MCP servers"命令，可能输出了整个配置文件目录
- Grep 的 156KB 来自 `files_with_matches` 模式，匹配了大量文件路径
- Git commit 的 114KB 包含了完整的 diff 信息

**影响**：一次 100KB+ 的工具输出 ≈ 25K-30K tokens，直接消耗 10-15% 的上下文窗口。多次触发后必然导致 compact，降低会话效率。

**修复建议**：
- Bash 输出自动截断：超过 20KB 截断并追加 `... (输出已截断，共 X 字节)`
- Grep 结果限制：`files_with_matches` 模式默认限制返回 100 条
- Git commit 使用 `--stat` 而非完整 diff
- 在系统提示中强调"避免执行可能产生大量输出的命令"

---

### SIZE-002 | 工具入参异常大 | MEDIUM | 置信度 70%

**现象**：5 次工具调用的入参超过 50KB，全部是 Write 工具。

| 工具 | 最大入参 | 内容类型 |
|------|---------|---------|
| Write | 77.7KB | 文件内容 |
| Write | 76.2KB | 文件内容 |
| Write | 53.2KB | 文件内容 |

**按工具的入参统计**：

| 工具 | 平均入参 | 最大入参 | 说明 |
|------|---------|---------|------|
| Write | 7.0KB | 77.7KB | content 字段包含完整文件内容 |
| Edit | 1.2KB | 44.4KB | old_string + new_string |
| Agent | 2.3KB | 21.2KB | prompt 内容 |
| Bash | 261B | 10.8KB | 命令字符串 |
| Read | 103B | 164B | 仅 file_path |

**根因分析**：Write 的超大入参是正常的——Agent 需要把完整文件内容写入。但 77.7KB 的写入意味着一次 tool_use 消耗 ~20K tokens 的输入上下文。Edit 的 44.4KB 入参则更值得关注——old_string 和 new_string 不应该这么大，可能是 Agent 把整个文件作为 old_string。

**修复建议**：
- Edit 工具在入参超过 10KB 时发出警告"建议使用更精确的 old_string 匹配"
- Write 超大文件时考虑分块写入

---

### 上下文膨胀风险会话

**总出参 >500KB 的会话 Top 5**：

| Session | 总出参 | 大出参次数 | 涉及工具 |
|---------|-------|-----------|---------|
| 019e531e | 1.0MB | 6 | Read, Bash |
| 019e5d38 | 849.4KB | 6 | Read |
| 019e6811 | 847.2KB | 6 | Grep, Read |
| 019e52d4 | 834.2KB | 6 | Read, Bash |
| 019e5770 | 689.6KB | 4 | Read, Bash |

这些会话仅工具输出就消耗了 0.7-1.0MB，换算为 token 约占 200K-300K tokens 的上下文窗口。考虑到还有系统提示、工具定义、历史消息，这些会话必然频繁触发 compact。

---

## 九、修复方案（Systematic Debugging）

基于四阶段方法论（根因调查 → 模式分析 → 假设验证 → 实施），对每个高优缺陷给出可落地方案。

---

### FIX-001: 工具名大小写不一致导致 ToolNotFound

#### Phase 1: 根因

数据路径追踪：

```
工具注册 (core_tools.rs)
  → HashMap<"Bash", Tool>     // 大写 B，大小写敏感存储

工具过滤 (fork.rs:33-46)
  → name.to_lowercase() 比较  // 大小写不敏感过滤
  → "bash" 能通过 disallowedTools: ["Bash"] 的匹配

工具查找 (tool_dispatch.rs:217)
  → all_tools.get("bash")     // 大小写敏感查找
  → None → ToolNotFound       // 找不到！
```

**根因**：过滤层（fork.rs）大小写不敏感，查找层（tool_dispatch.rs）大小写敏感。LLM 输出 `bash`（小写）时能通过过滤，但在 HashMap 中找不到注册的 `Bash`（大写）。

**涉及文件**：
- `peri-middlewares/src/tool_search/core_tools.rs:18-29` — 注册名 `Bash`
- `peri-middlewares/src/subagent/fork.rs:33-46` — 过滤大小写不敏感
- `peri-agent/src/agent/executor/tool_dispatch.rs:217` — `all_tools.get(&call.name)` 大小写敏感
- `peri-agent/src/agent/executor/mod.rs:34` — `HashMap<String, Box<dyn BaseTool>>`

#### Phase 2: 模式对比

系统中有两层不一致：

| 层 | 大小写策略 | 代码位置 |
|----|-----------|---------|
| 注册 | 精确保存（PascalCase） | `mod.rs:77` |
| 过滤 | 不敏感（`.to_lowercase()`） | `fork.rs:33-46` |
| 查找 | 精确匹配（`HashMap.get()`） | `tool_dispatch.rs:217` |
| LLM 输出 | 不确定（模型决定） | `openai/invoke.rs:255` |

过滤和查找的不一致是 bug 的直接原因。

#### Phase 3: 修复假设

**假设**：在查找层做大小写归一化即可修复。

**验证**：搜索所有 `all_tools.get(` 调用点：

```
tool_dispatch.rs:217 — 主要查找入口
tool_dispatch.rs:274 — 错误路径查找
```

统一归一化即可覆盖。

#### Phase 4: 实施方案

**方案 A（推荐）：在查找入口归一化**

修改 `tool_dispatch.rs` 的查找逻辑：

```rust
// tool_dispatch.rs:217 附近
// 修改前
let tool = all_tools.get(&call.name).copied();

// 修改后：大小写不敏感查找
let tool = all_tools.get(&call.name).copied()
    .or_else(|| {
        // 尝试大小写不敏感匹配
        let lower_name = call.name.to_lowercase();
        all_tools.iter()
            .find(|(k, _)| k.to_lowercase() == lower_name)
            .map(|(_, v)| *v)
    });
```

**改动范围**：1 个文件，1 处核心修改。

**备选方案 B**：注册时统一归一化为 PascalCase。改动量大，需要修改 core_tools.rs 中所有常量 + 全局搜索所有 `register_tool` 调用。不推荐。

**测试**：
1. 构造 LLM 输出 `bash`（小写）的 tool_use 块
2. 验证 `tool_dispatch.rs` 能正确找到注册的 `Bash` 工具
3. 验证现有大写 `Bash` 仍然正常工作

---

### FIX-002: Missing file_path 重试循环

#### Phase 1: 根因

错误传播链完整追踪：

```
工具层 (read.rs:122)
  ↓ ok_or("Missing file_path parameter")
错误封装 (tool_dispatch.rs:274-276)
  ↓ ToolResult::error(id, name, e.to_string())
写入 State (tool_dispatch.rs:77-84)
  ↓ BaseMessage::tool_error(id, result.output.as_str())
  ↓ state.add_message(tool_msg)
序列化到 LLM
  ├─ Anthropic (anthropic.rs:236-242):
  │    {"type": "tool_result", "content": "Missing file_path parameter", "is_error": true} ✅
  └─ OpenAI (openai.rs:141-150):
       {"role": "tool", "tool_call_id": "...", "content": "Missing file_path parameter"} ❌ 无 is_error
LLM 下一轮 (llm_step.rs:46)
  ↓ agent.llm.generate_reasoning(state.messages(), tool_refs, streaming)
  ↓ LLM 生成同样的 tool_use（未传 file_path）→ 循环
```

**根因 1**（高优）：错误消息过于简短。`"Missing file_path parameter"` 没有告诉 LLM 哪个工具、为什么需要、如何修正。LLM 理解为普通错误，用相同方式重试。

**根因 2**（中优）：OpenAI 适配器缺少 `is_error` 字段（`openai.rs:141-150`），导致 OpenAI 兼容模型无法区分成功和失败的 tool_result。

**根因 3**（历史）：Write 工具在 max_tokens 截断时也会丢失 file_path（已部分修复）。

**涉及文件**：
- `peri-middlewares/src/filesystem/read.rs:122` — Read 的 file_path 校验
- `peri-middlewares/src/filesystem/write.rs:64` — Write 的 file_path 校验
- `peri-agent/src/messages/adapters/openai.rs:141-150` — OpenAI adapter 缺 is_error
- `peri-agent/src/agent/executor/tool_dispatch.rs` — 可增加连续失败检测

#### Phase 2: 模式分析

对比成功的错误恢复案例：当 LLM 收到 `Tool execution failed: Bash - interrupted by user` 这种具体消息时，LLM 通常不会重试。说明错误消息的具体性直接影响 LLM 的学习效果。

| 错误消息 | 重试概率 | 原因 |
|---------|---------|------|
| `Missing file_path parameter` | 高 | 不明确，LLM 不知道怎么改 |
| `Tool execution failed: Bash - interrupted by user` | 低 | 明确原因，LLM 知道被中断 |
| `工具 'Task' 不存在` | 高 | LLM 不知道该用什么替代 |

#### Phase 3: 修复假设

**假设 1**：改进错误消息可降低重试率（从 7 次降到 1-2 次）。

**假设 2**：在 ReAct 循环中增加连续失败检测可彻底打断循环。

#### Phase 4: 实施方案

**改动 1：改进错误消息**（`read.rs:122`，`write.rs:64`）

```rust
// read.rs:122 修改前
ok_or("Missing file_path parameter")?

// 修改后
let input = input.as_object().context("Read 工具参数必须是 JSON 对象")?;
let file_path = input.get("file_path")
    .and_then(|v| v.as_str())
    .filter(|s| !s.is_empty())
    .ok_or("Read 工具需要 'file_path' 参数。请提供要读取的文件绝对路径，例如: {\"file_path\": \"/path/to/file.rs\"}")?;
```

**改动 2：OpenAI 适配器补全 is_error**（`openai.rs:141-150`）

```rust
// 修改前
BaseMessage::Tool { tool_call_id, content, .. } => {
    result.push(json!({
        "role": "tool",
        "tool_call_id": tool_call_id,
        "content": Self::content_to_openai(content)
    }));
}

// 修改后
BaseMessage::Tool { tool_call_id, content, is_error, .. } => {
    let mut msg = json!({
        "role": "tool",
        "tool_call_id": tool_call_id,
        "content": Self::content_to_openai(content)
    });
    if *is_error {
        msg["is_error"] = json!(true);
    }
    result.push(msg);
}
```

**改动 3：连续失败检测**（`tool_dispatch.rs` 或 `mod.rs`）

在 ReAct 循环中增加状态追踪：

```rust
// 在 ExecutorState 中增加
last_tool_errors: HashMap<String, usize>,  // tool_name → 连续失败次数

// 在 tool_dispatch 后检查
if result.is_error() {
    let count = state.last_tool_errors.entry(tool_name.clone()).or_insert(0);
    *count += 1;
    if *count >= 2 && result.error_message() == prev_error {
        // 注入纠正消息
        state.add_message(BaseMessage::system(
            format!("警告: 工具 {} 连续 {} 次因相同错误失败: {}。请分析错误原因并调整调用参数。",
                tool_name, count, result.error_message())
        ));
    }
} else {
    state.last_tool_errors.remove(&tool_name);
}
```

**改动范围**：4 个文件，改动量小~中。

**测试**：
1. 构造 LLM 连续两次输出不带 file_path 的 Read 调用
2. 验证第二次失败后 Agent 收到纠正消息
3. 验证 OpenAI 格式中 is_error 字段正确传递
4. 验证 Anthropic 格式不受影响

---

### FIX-003: 幻觉工具名别名映射

#### Phase 1: 根因

`Task` (13次) 和 `Bash` (31次) 的高频幻觉来自两个不同来源：

1. **Task** — Claude Code 原生使用 `Task` 工具名（子代理），Peri 使用 `Agent`。LLM 在 Claude Code 的训练数据影响下倾向输出 `Task`。集中在 2026-05-15（项目早期），说明后续 system prompt 改进后已部分缓解。

2. **Bash** — 与 FIX-001 同根因（大小写问题），但还有一层：SubAgent（explore 等）的工具列表可能缺少 bash，导致 LLM 在无工具描述的情况下"猜"出了 `Bash` 这个名字。

**涉及文件**：
- `peri-middlewares/src/tool_search/core_tools.rs` — CORE_TOOLS 列表
- `peri-middlewares/src/subagent/tool/build_agent.rs` — SubAgent 工具构建
- `peri-middlewares/src/tool_search/middleware.rs` — ToolSearchMiddleware

#### Phase 4: 实施方案

在 `ToolSearchMiddleware` 的 `before_tool` 钩子中增加别名映射：

```rust
// tool_search/middleware.rs
const TOOL_ALIASES: &[(&str, &str)] = &[
    ("Task", "Agent"),
    ("Bash", "bash"),     // 如果注册名是小写
    ("Shell", "bash"),
];

fn resolve_alias(tool_name: &str) -> String {
    for (alias, real) in TOOL_ALIASES {
        if tool_name.eq_ignore_ascii_case(alias) {
            return real.to_string();
        }
    }
    tool_name.to_string()
}
```

在 `before_tool` 中，如果 `tool_name` 不在已知工具中，先查别名表再查找。如果别名命中，透明替换为真实工具名并记录日志。

**改动范围**：1 个文件，新增约 20 行。

**注意**：FIX-001（大小写归一化）修复后，Bash 的大小写问题自动解决，此处只需处理 Task→Agent 的语义别名。

---

### 修复优先级总览

| 顺序 | ID | 修复 | 改动量 | 预期效果 | 风险 |
|------|-----|------|--------|---------|------|
| 1 | FIX-001 | 工具名大小写归一化 | 小（1文件） | 消除 Bash 31次幻觉 | 低——查找兼容性增强 |
| 2 | FIX-002a | 改进错误消息 | 小（2文件） | 降低重试次数 50%+ | 低——仅改消息文本 |
| 3 | FIX-002b | OpenAI is_error 字段 | 小（1文件） | OpenAI 模型错误感知 | 低——增量字段 |
| 4 | FIX-003 | Task→Agent 别名 | 小（1文件） | 消除 Task 13次幻觉 | 低——白名单映射 |
| 5 | FIX-002c | 连续失败检测 | 中（1-2文件） | 彻底打断重试循环 | 中——需验证不影响正常流程 |

FIX-001 + FIX-002a + FIX-003 三项合计可消除 **44/51 幻觉调用** + **降低 42 次参数缺失的重试率**，投入产出比最高。

---

### FIX-004: AgentResult 轮询死循环

#### Phase 1: 根因

数据追踪：

```
ExecuteExtraTool({"params":{},"tool_name":"AgentResult"})
  → 返回 "后台任务尚未完成"（无 task_id）
  → Agent 不理解，再次调用 ExecuteExtraTool(AgentResult)
  → 返回 "后台任务尚未完成"
  → 循环 83 次...
```

**涉及文件**：
- `peri-middlewares/src/tool_search/execute_tool.rs` — ExecuteExtraTool 实现
- `peri-middlewares/src/subagent/agent_result.rs` — AgentResultTool 返回值
- `peri-agent/src/agent/executor/tool_dispatch.rs` — 无去重保护

#### Phase 4: 实施方案

**改动 1：AgentResult 返回明确的等待指令**

```rust
// agent_result.rs 修改后
if result.is_none() {
    return Ok("后台任务尚未完成。请不要再查询，继续执行其他操作。
    当任务完成时系统会自动通知你。如果需要等待，请执行其他不依赖该结果的工作。"
        .to_string());
}
```

**改动 2：tool_dispatch 增加调用去重**

```rust
// tool_dispatch.rs — 在 collect_tool_results 中
struct RecentCall {
    tool_name: String,
    args_hash: String, // 轻量参数指纹
}

// 如果最近 3 次调用中 ≥2 次为同工具+同参数，注入系统消息
if consecutive_duplicate_count >= 2 {
    state.add_message(BaseMessage::system(
        format!("警告: 工具 {} 已连续 {} 次以相同参数调用且未产生不同结果。\
                 请分析原因，更换策略，或向用户报告。", tool_name, consecutive_duplicate_count)
    ));
}
```

**改动范围**：2 个文件，中等工作量。

---

### FIX-005: 工具输出自动截断

#### Phase 1: 根因

Bash 输出 203KB、Grep 输出 156KB 的场景下，工具层未做任何截断，全量结果直接写入 state，消耗大量上下文。

**涉及文件**：
- `peri-middlewares/src/terminal/bash.rs` — Bash 输出
- `peri-agent/src/agent/executor/tool_dispatch.rs` — 可在 dispatch 层统一截断
- `peri-agent/src/messages/content.rs` — ContentBlock 构造

#### Phase 4: 实施方案

**在 tool_dispatch 层统一截断**（最优解——对所有工具生效）：

```rust
// tool_dispatch.rs — collect_tool_results 中，写入 state 前
const MAX_TOOL_OUTPUT_BYTES: usize = 20_000; // 20KB

let output = result.output;
let output = if output.len() > MAX_TOOL_OUTPUT_BYTES {
    format!("{}... (输出已截断，共 {} 字节。如需完整输出请用更精确的查询条件)",
        &output[..MAX_TOOL_OUTPUT_BYTES],
        output.len())
} else {
    output
};
```

**Bash 特殊处理**：对 git diff/commit 命令，在执行前追加 `--stat` 替代完整 diff。

**改动范围**：1-2 个文件，小工作量。

---

### 修复优先级总览（更新版）

| 顺序 | ID | 修复 | 改动量 | 预期效果 | 风险 |
|------|-----|------|--------|---------|------|
| 1 | FIX-001 | 工具名大小写归一化 | 小 | 消除 Bash 31次幻觉 | 低 |
| 2 | FIX-002a | 改进错误消息 | 小 | 降低重试次数 50%+ | 低 |
| 3 | FIX-003 | Task→Agent 别名 | 小 | 消除 Task 13次幻觉 | 低 |
| 4 | **FIX-004** | AgentResult 轮询去重 | 中 | **消除 83 次死循环** | 中 |
| 5 | **FIX-005** | 工具输出截断 | 小 | **降低 19 次超大出参** | 低 |
| 6 | FIX-002b | OpenAI is_error 字段 | 小 | OpenAI 模型错误感知 | 低 |
| 7 | FIX-002c | 连续失败检测 | 中 | 打断重试循环 | 中 |

FIX-004 和 FIX-005 是本次新增分析中发现的高价值修复项。FIX-004 的 83 次死循环是最严重的单个缺陷，FIX-005 直接影响上下文效率和 compact 频率。
