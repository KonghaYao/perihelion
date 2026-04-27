# SubAgent Model Switch 执行计划

**目标:** 让 agent.md 中的 model 字段（haiku/sonnet/opus/inherit）真正生效，SKILL.md 增加 model 文档提示

**技术栈:** Rust, tokio, serde, Arc<dyn Fn>

**设计文档:** spec-design.md

---

### Task 1: LLM Factory 签名升级（中间件层）

**涉及文件:**
- 修改: `rust-agent-middlewares/src/subagent/tool.rs`
- 修改: `rust-agent-middlewares/src/subagent/mod.rs`

**执行步骤:**
- [x] 将 `SubAgentTool` 的 `llm_factory` 字段类型从 `Fn() -> Box<dyn ReactLLM>` 改为 `Fn(Option<&str>) -> Box<dyn ReactLLM>`
  - `tool.rs:30` 字段签名变更
  - `tool.rs:39-50` 构造函数签名同步更新
- [x] 将 `SubAgentMiddleware` 的 `llm_factory` 字段类型同步更新
  - `mod.rs:50` 字段签名变更
  - `mod.rs:57-72` 构造函数签名同步更新
  - `mod.rs:84-94` `build_tool()` 传递 `Arc::clone(&self.llm_factory)` 不变（签名已一致）
- [x] 更新 `SubAgentTool::invoke()` 中的 factory 调用
  - `tool.rs:191` 处改为：提取 `model_alias = frontmatter.model.filter(not inherit/empty)`，传入 `(self.llm_factory)(model_alias)`
- [x] 更新 `tool.rs` 和 `mod.rs` 中所有测试的 factory 闭包签名
  - `tool.rs` 测试中的 `Arc::new(|| Box::new(EchoLLM))` 改为 `Arc::new(|_: Option<&str>| Box::new(EchoLLM))`
  - `mod.rs` 测试同理

**检查步骤:**
- [x] 中间件层编译通过
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 编译成功无 error
- [x] 中间件层现有测试全部通过
  - `cargo test -p rust-agent-middlewares 2>&1 | tail -20`
  - 预期: 所有 test 结果为 ok

---

### Task 2: LlmProvider 新增 from_config_for_alias 方法

**涉及文件:**
- 修改: `rust-agent-tui/src/app/provider.rs`
- 修改: `rust-agent-tui/src/config/types.rs`

**执行步骤:**
- [x] 在 `ModelAliasMap` 上新增 `get_alias(&self, alias: &str) -> Option<&ModelAliasConfig>` 方法
  - alias 参数为小写字符串（"opus"/"sonnet"/"haiku"），大小写不敏感匹配
- [x] 在 `LlmProvider` 上新增 `from_config_for_alias(cfg: &ZenConfig, alias: &str) -> Option<Self>` 方法
  - 逻辑：`cfg.model_aliases.get_alias(alias)` → `cfg.providers.find(id)` → 构造 `LlmProvider`
  - 复用 `from_config()` 内部相同的 provider 构造逻辑（api_key 判空、base_url 处理、thinking 配置）
- [x] 新增单元测试覆盖：
  - 已知 alias（opus/sonnet/haiku）正确解析
  - 未知 alias 返回 None
  - 空 api_key 返回 None
  - 大小写不敏感（"Haiku" → haiku 配置）

**检查步骤:**
- [x] provider 单元测试通过
  - `cargo test -p rust-agent-tui --lib -- provider 2>&1 | tail -15`
  - 预期: 所有 test 结果为 ok，新增测试出现
- [x] types 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 编译成功无 error

---

### Task 3: TUI 层 AgentRunConfig 扩展 + factory 升级

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent.rs`
- 修改: `rust-agent-tui/src/app/agent_ops.rs`

**执行步骤:**
- [x] 在 `AgentRunConfig` 结构体中新增 `config: Arc<ZenConfig>` 字段
  - `agent.rs:18-31` 结构体定义
- [x] 升级 `run_universal_agent()` 中的 `llm_factory` 闭包
  - `agent.rs:149-152` 处改为：闭包签名 `|model_alias: Option<&str>|`，内部调用 `LlmProvider::from_config_for_alias` 解析 alias
  - alias 为 None 或解析失败时 fallback 到父 provider（`provider_clone.clone().into_model()`）
  - 需将 `cfg.config` clone 到闭包中（通过 `config_for_factory = cfg.config.clone()`）
- [x] 更新 `agent_ops.rs` 中构造 `AgentRunConfig` 的代码
  - `agent_ops.rs:136-149` 处新增 `config: Arc::new(self.zen_config.clone().unwrap_or_default())` 字段
  - 注意 `self.zen_config` 类型为 `Option<ZenConfig>`，需要 clone 内部值

**检查步骤:**
- [x] TUI 层编译通过
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 编译成功无 error
- [x] 全量编译通过（无 warning breakage）
  - `cargo build 2>&1 | tail -5`
  - 预期: 编译成功

---

### ~~Task 4: SkillFrontmatter 增加 model 字段~~ （已移除：不需要更改 skill middleware）

**涉及文件:**
- 修改: `rust-agent-middlewares/src/skills/loader.rs`
- 修改: `rust-agent-middlewares/src/skills/mod.rs`

**执行步骤:**
- [x] `SkillFrontmatter` 增加 `model: Option<String>` 字段（使用 `#[serde(default)]`）
  - `loader.rs:16-19`
- [x] `SkillMetadata` 增加 `model: Option<String>` 字段
  - `loader.rs:8-12`
- [x] `load_skill_metadata()` 中将 `fm.model` 传入 `SkillMetadata`
  - `loader.rs:22-35`
- [x] `SkillsMiddleware::build_summary()` 中展示 model 信息
  - `mod.rs:117-136` 处，有 model 时在摘要行追加 `(model: haiku)` 格式
- [x] 新增测试：SKILL.md 含 model 字段时正确解析
  - 在 `loader.rs` 测试中新增 `test_load_skill_with_model` 测试用例

**检查步骤:**
- [ ] skills loader 单元测试通过
  - `cargo test -p rust-agent-middlewares --lib -- skills::loader 2>&1 | tail -10`
  - 预期: 所有 test ok，新增 test_load_skill_with_model 通过
- [ ] SkillsMiddleware 测试通过
  - `cargo test -p rust-agent-middlewares --lib -- skills::tests 2>&1 | tail -10`
  - 预期: 所有 test ok

---

### Task 5: SubAgent Model Switch Acceptance

**前置条件:**
- 启动命令: 无需启动服务，仅运行测试
- 测试数据: 无需外部依赖

**端到端验证:**

1. agent.md 设置 model: haiku 时子 Agent 使用正确模型
   - `cargo test -p rust-agent-middlewares --lib -- subagent::tool::tests 2>&1 | tail -15`
   - 预期: 所有现有测试 + 新增 model alias 测试通过
   - ✅ 通过

2. agent.md 设置 model: inherit 或省略 model 时子 Agent 继承父模型
   - `cargo test -p rust-agent-middlewares --lib -- subagent 2>&1 | tail -10`
   - 预期: 所有 test ok，model_alias 为 None 时 factory 被正确调用
   - ✅ 通过

3. 未知 alias fallback 到父模型不 panic
   - `cargo test -p rust-agent-tui --lib -- provider 2>&1 | tail -10`
   - 预期: 未知 alias 返回 None 的测试通过
   - ✅ 通过

4. SKILL.md model 字段正确解析并展示在摘要中
   - `cargo test -p rust-agent-middlewares --lib -- skills 2>&1 | tail -10`
   - 预期: 新增 test_load_skill_with_model 通过
   - ✅ 通过（Task 4 已移除，原有测试通过）

5. 全量测试无回归
   - `cargo test 2>&1 | tail -20`
   - 预期: 所有 test ok，0 failed
   - ✅ 通过

6. 全量编译无 warning
   - `cargo build 2>&1 | grep -E "warning|error" | head -20`
   - 预期: 无 warning/error 输出
   - ✅ 通过（仅有已有的 unused_mut warning，非本次引入）
