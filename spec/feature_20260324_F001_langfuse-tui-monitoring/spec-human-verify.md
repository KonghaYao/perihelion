# Langfuse TUI 监控接入 人工验收清单

**生成时间:** 2026-03-24 22:00
**关联计划:** [spec-plan.md](./spec-plan.md)
**关联设计:** [spec-design.md](./spec-design.md)

---

## 验收前准备

### 环境要求
- [ ] [AUTO] 确认当前目录为项目根目录: `test -f Cargo.toml && echo "OK"`
- [ ] [AUTO] 编译全 workspace（首次可能需要下载依赖，耗时较长）: `cargo build 2>&1 | tail -3`
- [ ] [AUTO] 确认 langfuse-ergonomic 依赖已下载: `grep "langfuse-ergonomic" rust-agent-tui/Cargo.toml`

### 端到端测试数据准备（场景 6 需要）
- [ ] [MANUAL] 准备 Langfuse 账号：前往 https://cloud.langfuse.com 注册或登录，创建项目，生成 Public Key（pk-lf-...）和 Secret Key（sk-lf-...）
- [ ] [AUTO] 在 rust-agent-tui/.env 设置密钥（替换 YOUR_PK 和 YOUR_SK 为真实密钥）: `echo "LANGFUSE_PUBLIC_KEY=YOUR_PK\nLANGFUSE_SECRET_KEY=YOUR_SK" >> rust-agent-tui/.env`

---

## 验收项目

### 场景 1：编译与单元测试

#### - [x] 1.1 全 workspace 编译通过

- **来源:** Task 1/2/4/5 检查步骤
- **操作步骤:**
  1. [A] `cargo build 2>&1 | grep -E "^error" | head -5` → 期望: 无任何输出（零编译错误）
- **异常排查:**
  - 若有编译错误: 检查 `rust-create-agent/src/llm/types.rs` 中 `TokenUsage` derive 属性是否完整；检查 `rust-agent-tui/src/langfuse/mod.rs` 中 import 是否正确

#### - [x] 1.2 rust-create-agent 单元测试全绿

- **来源:** Task 1/2/4 检查步骤
- **操作步骤:**
  1. [A] `cargo test -p rust-create-agent --lib 2>&1 | tail -5` → 期望: 输出最后一行包含 `test result: ok`，passed 数量 ≥ 32，failed 为 0
- **异常排查:**
  - 若测试失败: 查看失败测试名称，检查对应文件是否正确初始化了新字段（`usage: None, model: String::new()`）

---

### 场景 2：核心层 Hook 扩展验证

#### - [x] 2.1 AgentEvent 新变体 LlmCallStart/LlmCallEnd 存在

- **来源:** Task 2 检查步骤
- **操作步骤:**
  1. [A] `grep -n "LlmCallStart\|LlmCallEnd" rust-create-agent/src/agent/events.rs` → 期望: 至少 2 行输出，分别包含 `LlmCallStart` 和 `LlmCallEnd`
  2. [A] `grep -c "LlmCallStart\|LlmCallEnd" rust-create-agent/src/agent/events.rs` → 期望: 输出 `2` 或更大数字
- **异常排查:**
  - 若无输出: 检查 `rust-create-agent/src/agent/events.rs` 文件是否包含两个新变体定义

#### - [x] 2.2 executor.rs 正确 emit LlmCallStart 和 LlmCallEnd 事件

- **来源:** Task 2 执行步骤
- **操作步骤:**
  1. [A] `grep -n "LlmCallStart\|LlmCallEnd" rust-create-agent/src/agent/executor.rs` → 期望: 至少 2 行输出（emit 调用处）
  2. [A] `grep -n "emit.*LlmCall" rust-create-agent/src/agent/executor.rs` → 期望: 含 `self.emit(AgentEvent::LlmCallStart` 和 `self.emit(AgentEvent::LlmCallEnd`
- **异常排查:**
  - 若无 emit 调用: 检查 executor.rs 在 `generate_reasoning` 调用前后是否有 `self.emit(AgentEvent::LlmCallStart` 和 `self.emit(AgentEvent::LlmCallEnd`

#### - [x] 2.3 ReactLLM trait 包含 model_name 默认方法

- **来源:** Task 2 执行步骤
- **操作步骤:**
  1. [A] `grep -n "model_name" rust-create-agent/src/agent/react.rs` → 期望: 至少 2 行输出（trait 定义处 + blanket impl 处）
- **异常排查:**
  - 若无输出: 检查 `ReactLLM` trait 定义是否包含 `fn model_name(&self) -> String { "unknown".to_string() }`

---

### 场景 3：Langfuse 模块结构验证

#### - [x] 3.1 langfuse 模块文件正确创建

- **来源:** Task 3 检查步骤
- **操作步骤:**
  1. [A] `ls rust-agent-tui/src/langfuse/` → 期望: 输出包含 `config.rs` 和 `mod.rs` 两个文件名
  2. [A] `wc -l rust-agent-tui/src/langfuse/mod.rs` → 期望: 行数 > 50（实现非空）
- **异常排查:**
  - 若文件不存在: 检查 `rust-agent-tui/src/langfuse/` 目录是否已创建

#### - [x] 3.2 config.rs 正确引用三个环境变量

- **来源:** Task 3 检查步骤、spec-design.md 配置要求
- **操作步骤:**
  1. [A] `grep -n "LANGFUSE_PUBLIC_KEY\|LANGFUSE_SECRET_KEY\|LANGFUSE_HOST" rust-agent-tui/src/langfuse/config.rs` → 期望: 至少 3 行输出，分别包含三个变量名
- **异常排查:**
  - 若输出不足 3 行: 检查 `LangfuseConfig::from_env()` 实现是否包含全部三个环境变量读取

#### - [x] 3.3 mod.rs 包含完整的 LangfuseTracer 方法

- **来源:** Task 3 执行步骤
- **操作步骤:**
  1. [A] `grep -n "pub fn on_" rust-agent-tui/src/langfuse/mod.rs` → 期望: 至少 5 行输出，包含 `on_trace_start`、`on_llm_start`、`on_llm_end`、`on_tool_start`、`on_trace_end`
- **异常排查:**
  - 若缺少方法: 检查 `LangfuseTracer` 结构体实现是否完整

#### - [x] 3.4 Cargo.toml 包含 langfuse-ergonomic 依赖

- **来源:** Task 4 执行步骤
- **操作步骤:**
  1. [A] `grep "langfuse-ergonomic" rust-agent-tui/Cargo.toml` → 期望: 输出包含 `langfuse-ergonomic = "0.6.3"`
- **异常排查:**
  - 若无输出: 检查 `rust-agent-tui/Cargo.toml` 的 `[dependencies]` 部分是否添加了该依赖

---

### 场景 4：TUI 集成结构验证

#### - [x] 4.1 main.rs 声明模块，App 包含 langfuse_tracer 字段

- **来源:** Task 4 执行步骤
- **操作步骤:**
  1. [A] `grep -n "mod langfuse" rust-agent-tui/src/main.rs` → 期望: 1 行输出，包含 `mod langfuse;`
  2. [A] `grep -n "langfuse_tracer" rust-agent-tui/src/app/mod.rs` → 期望: 至少 3 行输出（字段定义、App::new 初始化、new_headless 初始化）
  3. [A] `grep -c "langfuse_tracer" rust-agent-tui/src/app/mod.rs` → 期望: 数字 ≥ 3
- **异常排查:**
  - 若 mod langfuse 不存在: 检查 main.rs 中模块声明列表
  - 若 langfuse_tracer 出现次数不足: 检查 App::new() 和 new_headless() 初始化块

#### - [x] 4.2 agent.rs 包含完整 Langfuse hook 调用路径

- **来源:** Task 4 检查步骤
- **操作步骤:**
  1. [A] `grep -n "LlmCallStart\|LlmCallEnd\|on_llm_start\|on_llm_end" rust-agent-tui/src/app/agent.rs` → 期望: ≥ 4 行输出
  2. [A] `grep -n "langfuse_tracer\|langfuse_for_handler" rust-agent-tui/src/app/agent.rs` → 期望: ≥ 2 行输出（函数参数声明处 + 闭包克隆处）
- **异常排查:**
  - 若调用行数不足: 检查 `run_universal_agent` 中 `langfuse_tracer` 参数和 `FnEventHandler` 内的 match 分支

---

### 场景 5：静默降级行为验证

#### - [x] 5.1 未配置 Langfuse 环境变量时 TUI 正常启动不崩溃

- **来源:** Task 4 检查步骤、spec-design.md 目标
- **操作步骤:**
  1. [A] `env -i HOME=$HOME timeout 3 cargo run -p rust-agent-tui -- -y 2>&1 | grep -c "panic"` → 期望: 输出为 `0`（零 panic）
  2. [A] `env -i HOME=$HOME LANGFUSE_PUBLIC_KEY="" timeout 3 cargo run -p rust-agent-tui -- -y 2>&1 | grep -i "langfuse\|panic" | head -5` → 期望: 无输出或无包含 panic/error 的行
- **异常排查:**
  - 若出现 panic: 检查 `LangfuseConfig::from_env()` 返回 `None` 时的代码路径，确认 `langfuse_tracer: None` 分支被正确处理

---

### 场景 6：端到端 Langfuse 数据追踪（需 Langfuse 实例）

> ⚠️ 本场景需要有效的 Langfuse API Key 和可访问的 Langfuse 实例。若无条件，可跳过本场景。

#### - [x] 6.1 发送消息后 Langfuse 控制台可见新 Trace，并包含 Generation 和 Span

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在终端执行（替换真实 Key）：`LANGFUSE_PUBLIC_KEY=<pk> LANGFUSE_SECRET_KEY=<sk> cargo run -p rust-agent-tui`，启动 TUI 后输入任意问题并发送，等待回答完成 → 操作是否成功（TUI 显示了 AI 回答）？ 是/否
  2. [H] 打开 https://cloud.langfuse.com（或自托管地址），进入对应项目的「Traces」列表，刷新页面，查看是否出现名为 `agent-run` 的新 Trace，且时间戳与刚才发送时间接近 → 是否可见新 Trace？ 是/否
  3. [H] 点击进入该 Trace，查看 Trace 详情页，确认存在至少 1 个 Generation 类型的子节点（名称格式为 `llm-call-step-N`）→ 是否存在 Generation？ 是/否
- **异常排查:**
  - 若 Trace 不出现: 检查 `LANGFUSE_PUBLIC_KEY` 和 `LANGFUSE_SECRET_KEY` 是否正确；检查网络是否可访问 `https://cloud.langfuse.com`
  - 若无 Generation: 检查 `agent.rs` 中 `on_llm_start`/`on_llm_end` 调用是否正确连接

#### - [x] 6.2 Generation 携带 model 字段和 token usage

- **来源:** spec-design.md 验收标准
- **操作步骤:**
  1. [H] 在 Langfuse Trace 详情页，点击 Generation 子节点，查看右侧详情面板，确认「Model」字段显示了正确的模型名称（如 `claude-sonnet-4-6` 或配置的模型名）→ model 字段是否有值？ 是/否
  2. [H] 在同一 Generation 详情面板中，查看「Usage」部分，确认显示了 `prompt_tokens` 和 `completion_tokens` 数值（若使用 Anthropic API，对应 input_tokens/output_tokens）→ token 用量是否显示（非零）？ 是/否
- **异常排查:**
  - 若 model 为空: 检查 `ChatAnthropic::model_name()` 或 `ChatOpenAI::model_name()` 返回值；检查 `LlmCallEnd` 事件的 model 字段是否正确传入
  - 若 usage 为 0 或空: 确认 LLM Provider 返回了 usage 字段（某些 OpenAI 兼容代理不返回 usage）；检查 Anthropic/OpenAI `invoke` 方法中 usage 解析逻辑

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | 自动步骤 | 人工步骤 | 结果 | 备注 |
|------|------|--------|----------|----------|------|------|
| 编译与单元测试 | 1.1 | 全 workspace 编译通过 | 1 | 0 | ⬜ | |
| 编译与单元测试 | 1.2 | rust-create-agent 单元测试全绿 | 1 | 0 | ⬜ | |
| 核心层 Hook 扩展 | 2.1 | AgentEvent 新变体存在 | 2 | 0 | ⬜ | |
| 核心层 Hook 扩展 | 2.2 | executor.rs emit 调用存在 | 2 | 0 | ⬜ | |
| 核心层 Hook 扩展 | 2.3 | ReactLLM::model_name 方法存在 | 1 | 0 | ⬜ | |
| Langfuse 模块结构 | 3.1 | langfuse 模块文件存在 | 2 | 0 | ⬜ | |
| Langfuse 模块结构 | 3.2 | config.rs 环境变量引用正确 | 1 | 0 | ⬜ | |
| Langfuse 模块结构 | 3.3 | mod.rs tracer 方法完整 | 1 | 0 | ⬜ | |
| Langfuse 模块结构 | 3.4 | Cargo.toml 包含依赖 | 1 | 0 | ⬜ | |
| TUI 集成结构 | 4.1 | main.rs 和 App 结构集成 | 3 | 0 | ⬜ | |
| TUI 集成结构 | 4.2 | FnEventHandler hook 调用路径 | 2 | 0 | ⬜ | |
| 静默降级行为 | 5.1 | 无环境变量时无 panic | 2 | 0 | ⬜ | |
| 端到端追踪 | 6.1 | Trace/Generation/Span 上报 | 0 | 3 | ⬜ | 需 Langfuse 账号 |
| 端到端追踪 | 6.2 | Generation model 与 usage | 0 | 2 | ⬜ | 需 Langfuse 账号 |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
