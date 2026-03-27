# langfuse-observation-types 执行计划

**目标:** 修正 Langfuse 追踪记录的观测类型：Generation 命名规范化、添加 Agent 层级观测、工具调用改为 Tool 类型

**技术栈:** Rust, langfuse-client-base 0.7.1, langfuse-ergonomic 0.6.3, tokio

**设计文档:** [spec-design.md](./spec-design.md)

---

### Task 1: 扩展 imports 和 LangfuseTracer 结构体

**涉及文件:**
- 修改: `rust-agent-tui/src/langfuse/mod.rs`

**执行步骤:**
- [x] 扩展 `langfuse_client_base::models` 的 import，新增所需类型
  ```rust
  use langfuse_client_base::models::{
      ingestion_event_one_of_3,       // Type::SpanUpdate
      ingestion_event_one_of_4::Type as GenType,
      ingestion_event_one_of_8,       // Type::ObservationCreate
      CreateGenerationBody, IngestionEvent,
      IngestionEventOneOf3, IngestionEventOneOf4, IngestionEventOneOf8,
      ObservationBody, ObservationType, UpdateSpanBody,
  };
  ```
- [x] 在 `LangfuseTracer` 结构体中添加 `agent_span_id: String` 字段
  - 位置：`trace_id: String` 字段之后
- [x] 在 `LangfuseTracer::new()` 中初始化该字段
  ```rust
  agent_span_id: uuid::Uuid::now_v7().to_string(),
  ```

**检查步骤:**
- [x] 新字段和 import 不引起编译错误
  - `cargo build -p rust-agent-tui 2>&1 | grep -E "^error" | head -5`
  - 预期: 无输出（若只剩后续 task 的 unused import 警告可忽略）

---

### Task 2: Agent Observation 生命周期（on_trace_start / on_trace_end）

**涉及文件:**
- 修改: `rust-agent-tui/src/langfuse/mod.rs`

**执行步骤:**
- [x] 修改 `on_trace_start`：在 tokio::spawn 内，trace 创建之后额外创建 Agent Observation
  ```rust
  let batcher_clone = Arc::clone(&self.session.batcher);
  let agent_span_id = self.agent_span_id.clone();
  // ... 在 spawn 内，client.trace()...call().await 之后：
  let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
  let body = ObservationBody {
      id: Some(Some(agent_span_id.clone())),
      trace_id: Some(Some(trace_id.clone())),
      r#type: ObservationType::Agent,
      name: Some(Some("Agent".to_string())),
      input: Some(Some(serde_json::json!(input.clone()))),
      start_time: Some(Some(timestamp.clone())),
      ..Default::default()
  };
  let obs_event = IngestionEventOneOf8 {
      id: uuid::Uuid::now_v7().to_string(),
      timestamp,
      body: Box::new(body),
      r#type: ingestion_event_one_of_8::Type::ObservationCreate,
      metadata: None,
  };
  let _ = batcher_clone
      .add(IngestionEvent::IngestionEventOneOf8(Box::new(obs_event)))
      .await;
  ```
- [x] 修改 `on_trace_end`：在 tokio::spawn 内，trace 更新之后额外发送 Agent Observation end_time 更新
  ```rust
  let batcher_clone = Arc::clone(&self.session.batcher);
  let agent_span_id = self.agent_span_id.clone();
  // ... 在 spawn 内，client.trace()...call().await 之后：
  let end_ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
  let update_body = UpdateSpanBody {
      id: agent_span_id,
      trace_id: Some(Some(trace_id.clone())),
      end_time: Some(Some(end_ts.clone())),
      name: None, start_time: None, metadata: None,
      input: None, output: None, level: None,
      status_message: None, parent_observation_id: None,
      version: None, environment: None,
  };
  let update_event = IngestionEventOneOf3 {
      id: uuid::Uuid::now_v7().to_string(),
      timestamp: end_ts,
      body: Box::new(update_body),
      r#type: ingestion_event_one_of_3::Type::SpanUpdate,
      metadata: None,
  };
  let _ = batcher_clone
      .add(IngestionEvent::IngestionEventOneOf3(Box::new(update_event)))
      .await;
  ```

**检查步骤:**
- [x] 编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep "^error" | head -5`
  - 预期: 无输出

---

### Task 3: Generation 命名修正（on_llm_end）

**涉及文件:**
- 修改: `rust-agent-tui/src/langfuse/mod.rs`

**执行步骤:**
- [x] 修改 `on_llm_end` 函数签名，在 `model: &str` 后添加 `provider: &str` 参数
  ```rust
  pub fn on_llm_end(
      &mut self,
      step: usize,
      model: &str,
      provider: &str,   // 新增："OpenAI" 或 "Anthropic"
      output: &str,
      usage: Option<&TokenUsage>,
  )
  ```
- [x] 在函数体内修改 generation name 和添加 parent_observation_id
  - 将 `name: Some(Some(format!("llm-call-step-{}", step_for_closure)))` 改为：
    ```rust
    name: Some(Some(format!("Chat{}", provider_name))),
    ```
  - 在 `CreateGenerationBody` 中补充 `parent_observation_id` 字段：
    ```rust
    parent_observation_id: Some(Some(agent_span_id)),
    ```
  - 需要在 `tokio::spawn` 前捕获相关变量：
    ```rust
    let provider_name = provider.to_string();
    let agent_span_id = self.agent_span_id.clone();
    ```

**检查步骤:**
- [x] 编译时确认 on_llm_end 签名变更
  - `cargo build -p rust-agent-tui 2>&1 | grep "on_llm_end" | head -5`
  - 预期: 有 "expected 6 arguments" 的调用方报错（说明签名已更新，等 Task 5 修复调用方）
- [x] 若先注释掉 app/agent.rs 的调用编译应通过
  - 预期: `cargo build -p rust-agent-tui 2>&1 | grep "^error"` 无输出

---

### Task 4: Tool Observation 类型修正（on_tool_start / on_tool_end_by_name_order）

**涉及文件:**
- 修改: `rust-agent-tui/src/langfuse/mod.rs`

**执行步骤:**
- [x] 重写 `on_tool_start`：删除 `Arc::clone(&self.session.client)` 行，改为 Batcher + ObservationCreate
  ```rust
  pub fn on_tool_start(&mut self, tool_call_id: &str, name: &str, input: &serde_json::Value) {
      let span_id = uuid::Uuid::now_v7().to_string();
      self.pending_spans.push_back(span_id.clone());
      let batcher = Arc::clone(&self.session.batcher);
      let trace_id = self.trace_id.clone();
      let agent_span_id = self.agent_span_id.clone();
      let name = name.to_string();
      let input = input.clone();
      let _tool_call_id = tool_call_id.to_string();
      tokio::spawn(async move {
          let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
          let body = ObservationBody {
              id: Some(Some(span_id)),
              trace_id: Some(Some(trace_id)),
              r#type: ObservationType::Tool,
              name: Some(Some(name)),
              input: Some(Some(input)),
              parent_observation_id: Some(Some(agent_span_id)),
              start_time: Some(Some(timestamp.clone())),
              ..Default::default()
          };
          let event = IngestionEventOneOf8 {
              id: uuid::Uuid::now_v7().to_string(),
              timestamp,
              body: Box::new(body),
              r#type: ingestion_event_one_of_8::Type::ObservationCreate,
              metadata: None,
          };
          let _ = batcher.add(IngestionEvent::IngestionEventOneOf8(Box::new(event))).await;
      });
  }
  ```
- [x] 重写 `on_tool_end_by_name_order`：删除 `Arc::clone(&self.session.client)` 行，改为 Batcher + SpanUpdate
  ```rust
  pub fn on_tool_end_by_name_order(&mut self, output: &str, is_error: bool) {
      let Some(span_id) = self.pending_spans.pop_front() else { return; };
      let batcher = Arc::clone(&self.session.batcher);
      let trace_id = self.trace_id.clone();
      let output = output.to_string();
      tokio::spawn(async move {
          let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
          let update_body = UpdateSpanBody {
              id: span_id,
              trace_id: Some(Some(trace_id)),
              output: Some(Some(serde_json::json!(output))),
              end_time: Some(Some(timestamp.clone())),
              status_message: if is_error { Some(Some("error".to_string())) } else { None },
              name: None, start_time: None, metadata: None,
              input: None, level: None,
              parent_observation_id: None, version: None, environment: None,
          };
          let event = IngestionEventOneOf3 {
              id: uuid::Uuid::now_v7().to_string(),
              timestamp,
              body: Box::new(update_body),
              r#type: ingestion_event_one_of_3::Type::SpanUpdate,
              metadata: None,
          };
          let _ = batcher.add(IngestionEvent::IngestionEventOneOf3(Box::new(event))).await;
      });
  }
  ```

**检查步骤:**
- [x] 编译通过（Task 4 完成后除 on_llm_end 调用方外应无错误）
  - `cargo build -p rust-agent-tui 2>&1 | grep "^error" | grep -v "on_llm_end" | head -5`
  - 预期: 无输出

---

### Task 5: app/agent.rs 传入 provider_name

**涉及文件:**
- 修改: `rust-agent-tui/src/app/agent.rs`

**执行步骤:**
- [x] 在 `provider.into_model()` 调用前提取 provider 名称
  ```rust
  // 在 "let model = BaseModelReactLLM::new(provider.into_model())" 前添加：
  let provider_name = provider.display_name().to_string(); // "OpenAI" 或 "Anthropic"
  ```
- [x] 将 `provider_name` 移入 langfuse handler closure 的捕获变量
  - 在 `let langfuse_for_handler = langfuse_tracer.clone();` 后添加：
    ```rust
    let provider_name_for_handler = provider_name.clone();
    ```
- [x] 修改 `LlmCallEnd` 分支的 `on_llm_end` 调用，传入 provider_name
  ```rust
  ExecutorEvent::LlmCallEnd { step, model, output, usage } =>
      t.on_llm_end(*step, model, &provider_name_for_handler, output, usage.as_ref()),
  ```

**检查步骤:**
- [x] 全量编译通过
  - `cargo build -p rust-agent-tui 2>&1 | grep "^error" | head -5`
  - 预期: 无输出
- [x] 全量测试通过
  - `cargo test -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 "test result: ok"

---

### Task 6: langfuse-observation-types Acceptance

**Prerequisites:**
- 编译: `cargo build -p rust-agent-tui`
- 测试: `cargo test -p rust-agent-tui`
- （可选）Langfuse 服务运行 + TUI 已配置 `LANGFUSE_PUBLIC_KEY`/`LANGFUSE_SECRET_KEY`

**End-to-end verification:**

1. [x] 全量编译通过
   - `cargo build -p rust-agent-tui 2>&1 | grep "^error" | wc -l`
   - Expected: `0`
   - On failure: 检查 Task 1~5 的 import 和类型使用

2. [x] 单元测试全部通过
   - `cargo test -p rust-agent-tui 2>&1 | grep -E "^test result"`
   - Expected: `test result: ok`
   - On failure: 检查 Task 3（on_llm_end 签名变更是否同步更新了调用方）

3. [x] Generation name 字段验证（代码级静态检查）
   - `grep -n "Chat{}" rust-agent-tui/src/langfuse/mod.rs`
   - Expected: 至少 1 行包含 `format!("Chat{}", provider_name)` 的代码
   - On failure: 检查 Task 3 的 name 赋值逻辑

4. [x] Agent Observation 创建验证（代码级静态检查）
   - `grep -n "ObservationType::Agent" rust-agent-tui/src/langfuse/mod.rs`
   - Expected: 至少 1 行（在 on_trace_start 内）
   - On failure: 检查 Task 2 的 Agent Observation 创建逻辑

5. [x] Tool Observation 类型验证（代码级静态检查）
   - `grep -n "ObservationType::Tool" rust-agent-tui/src/langfuse/mod.rs`
   - Expected: 至少 1 行（在 on_tool_start 内）
   - On failure: 检查 Task 4 的工具观测类型

6. [x] parent_observation_id 传递验证（代码级静态检查）
   - `grep -n "parent_observation_id" rust-agent-tui/src/langfuse/mod.rs | wc -l`
   - Expected: 至少 3 行（on_trace_start Agent 创建 + on_llm_end + on_tool_start 各 1 处）
   - On failure: 检查 Task 2~4 中 parent_observation_id 字段赋值
