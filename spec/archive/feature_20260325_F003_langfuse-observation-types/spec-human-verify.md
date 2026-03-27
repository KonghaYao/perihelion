# langfuse-observation-types 人工验收清单

**生成时间:** 2026-03-25 16:00
**关联计划:** [spec-plan.md](./spec-plan.md)
**关联设计:** [spec-design.md](./spec-design.md)

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 确认 Rust 工具链可用: `cargo --version`
- [ ] [AUTO] 编译 rust-agent-tui: `cargo build -p rust-agent-tui 2>&1 | tail -3`
- [ ] [MANUAL] （可选）如需验证 Langfuse UI 展示效果（场景 2.2、3.3、4.2），需要：准备 Langfuse 服务访问地址及 Public Key / Secret Key，在 `rust-agent-tui/.env` 中配置 `LANGFUSE_PUBLIC_KEY`、`LANGFUSE_SECRET_KEY`、`LANGFUSE_HOST`

### 测试数据准备
- 无需额外测试数据，代码级验证使用静态分析命令

---

## 验收项目

### 场景 1：构建与单元测试

#### - [x] 1.1 全量编译通过

- **来源:** Task 5 检查步骤 / Task 6 验收
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-tui 2>&1 | grep "^error" | wc -l` → 期望: 输出 `0`
- **异常排查:**
  - 如果有编译错误: 运行 `cargo build -p rust-agent-tui 2>&1 | head -30` 查看详细错误，重点检查 `rust-agent-tui/src/langfuse/mod.rs` 的 import 声明和字段定义

#### - [x] 1.2 单元测试全部通过

- **来源:** Task 5 检查步骤 / Task 6 验收
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-tui 2>&1 | grep -E "^test result"` → 期望: 输出包含 `test result: ok`
- **异常排查:**
  - 如果测试失败: 运行 `cargo test -p rust-agent-tui 2>&1 | grep "FAILED"` 定位失败测试，检查 `on_llm_end` 签名变更是否与调用方一致

---

### 场景 2：Generation 观测命名修正

#### - [x] 2.1 Generation name 字段代码验证

- **来源:** Task 3 检查步骤 / Task 6 验收
- **操作步骤:**
  1. [A] `grep -n 'format!("Chat{}"' rust-agent-tui/src/langfuse/mod.rs` → 期望: 至少 1 行，出现在 `on_llm_end` 函数体内
  2. [A] `grep -n 'provider: &str' rust-agent-tui/src/langfuse/mod.rs` → 期望: 至少 1 行，出现在 `on_llm_end` 函数签名中
- **异常排查:**
  - 如果未找到: 检查 `rust-agent-tui/src/langfuse/mod.rs` 第 107 行附近的 `on_llm_end` 实现

#### - [x] 2.2 Langfuse UI 中 Generation 名称正确显示（可选，需 Langfuse 服务）

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 启动 TUI（`cargo run -p rust-agent-tui`），发送一条消息触发 LLM 调用，等待完成后打开 Langfuse UI → 在 Traces 列表中点开当前 Trace，查看 Generations 观测的 name 字段，应显示 `ChatAnthropic` 或 `ChatOpenAI`（不再是 `llm-call-step-0`）→ 是/否
- **异常排查:**
  - 如果仍显示旧名称: 确认 `rust-agent-tui/.env` 中 Langfuse 配置正确，并检查 `app/agent.rs` 中 `provider_name_for_handler` 是否已传入 `on_llm_end`

---

### 场景 3：Agent 层级观测

#### - [x] 3.1 Agent Observation 代码级验证

- **来源:** Task 2 检查步骤 / Task 6 验收
- **操作步骤:**
  1. [A] `grep -n 'ObservationType::Agent' rust-agent-tui/src/langfuse/mod.rs` → 期望: 至少 1 行，出现在 `on_trace_start` 函数体内
  2. [A] `grep -n 'agent_span_id' rust-agent-tui/src/langfuse/mod.rs | wc -l` → 期望: 至少 5 行（字段定义 + new() 初始化 + on_trace_start + on_trace_end + on_llm_end + on_tool_start 各用到）
- **异常排查:**
  - 如果 ObservationType::Agent 未找到: 检查 `on_trace_start` 内 `IngestionEventOneOf8` 的创建逻辑

#### - [x] 3.2 parent_observation_id 传递完整性

- **来源:** Task 6 验收
- **操作步骤:**
  1. [A] `grep -n 'parent_observation_id' rust-agent-tui/src/langfuse/mod.rs | wc -l` → 期望: 至少 3（on_trace_end UpdateSpanBody + on_llm_end CreateGenerationBody + on_tool_start ObservationBody 各 1 处）
- **异常排查:**
  - 如果数量不足: 运行 `grep -n 'parent_observation_id' rust-agent-tui/src/langfuse/mod.rs` 确认缺少哪个函数中的设置

#### - [x] 3.3 Langfuse UI 中 Agent Observation 可见（可选，需 Langfuse 服务）

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在 Langfuse UI 中打开本次 agent 执行的 Trace，查看 Trace 的 Observations 列表，确认存在类型为 `AGENT`、名称为 `Agent` 的 Observation，且 Generation 和 Tool 观测均嵌套在其下（显示为子节点） → 是/否
- **异常排查:**
  - 如果 Agent 观测未出现: 检查 `on_trace_start` 中 `batcher.add()` 调用是否正确，以及 `ObservationType::Agent` 是否已设置

---

### 场景 4：Tool 类型观测

#### - [x] 4.1 Tool Observation 代码级验证

- **来源:** Task 4 检查步骤 / Task 6 验收
- **操作步骤:**
  1. [A] `grep -n 'ObservationType::Tool' rust-agent-tui/src/langfuse/mod.rs` → 期望: 至少 1 行，出现在 `on_tool_start` 函数体内
  2. [A] `grep -n 'IngestionEventOneOf8' rust-agent-tui/src/langfuse/mod.rs | wc -l` → 期望: 至少 3 行（on_trace_start Agent 创建 + on_tool_start 工具创建，每处有 struct 实例化和引用）
- **异常排查:**
  - 如果 Tool 类型未找到: 检查 `on_tool_start` 是否仍使用旧的 `client.span()` 方法（应已改为 Batcher + IngestionEventOneOf8）

#### - [x] 4.2 Langfuse UI 中 Tool 类型图标正确（可选，需 Langfuse 服务）

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在触发了工具调用的 agent 执行 Trace 中，查看 Tool 观测的类型标签，确认显示为 `TOOL` 类型图标（而非 `SPAN`），且工具名称（如 `bash`、`read_file`）正确显示，并作为 Agent Observation 的子节点出现 → 是/否
- **异常排查:**
  - 如果仍显示 SPAN 类型: 确认 `on_tool_start` 已使用 `IngestionEventOneOf8` + `ObservationType::Tool`，而非 `client.span()`

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 构建与测试 | 1.1 | 全量编译通过 | 1 | 0 | ✅ | |
| 构建与测试 | 1.2 | 单元测试全部通过 | 1 | 0 | ✅ | |
| Generation 命名 | 2.1 | name 字段代码验证 | 2 | 0 | ✅ | |
| Generation 命名 | 2.2 | Langfuse UI 名称正确 | 0 | 1 | ✅ | 可选 |
| Agent 层级观测 | 3.1 | Agent Observation 代码验证 | 2 | 0 | ✅ | |
| Agent 层级观测 | 3.2 | parent_observation_id 完整 | 1 | 0 | ✅ | |
| Agent 层级观测 | 3.3 | Langfuse UI Agent span 可见 | 0 | 1 | ✅ | 修复后通过 |
| Tool 类型观测 | 4.1 | Tool Observation 代码验证 | 2 | 0 | ✅ | 实现演进为 span-create |
| Tool 类型观测 | 4.2 | Langfuse UI Tool 图标正确 | 0 | 1 | ✅ | 可选 |

**验收结论:** ✅ 全部通过
