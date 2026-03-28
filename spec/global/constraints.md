# 项目架构约束

![技术栈概览](./images/06-tech-stack.png)

## 技术栈

- **语言:** Rust 2021 edition
- **异步运行时:** tokio 1.x（`features = ["full"]`）
- **HTTP 客户端:** reqwest 0.12（json + stream features）
- **序列化:** serde 1.x + serde_json 1.x
- **数据库:** rusqlite 0.31（bundled SQLite，WAL 模式）
- **TUI 框架:** ratatui ≥0.30 + ratatui-textarea 0.8 + pulldown-cmark 0.12 + arboard 3（剪贴板）+ png 0.17（RGBA→PNG）+ base64 0.22 + langfuse-ergonomic 0.6.3（可观测性，仅 rust-agent-tui）
- **Web 框架（Relay Server）:** axum 0.8（WebSocket feature）
- **错误处理:** thiserror 2.0（库 crate）/ anyhow 1.0（应用层）
- **日志/追踪:** tracing 0.1 + tracing-subscriber 0.3 + opentelemetry 0.31 + tracing-opentelemetry 0.32
- **OTLP 导出:** opentelemetry-otlp 0.31（http-proto + reqwest-rustls）
- **UUID:** uuid 1.x（features: v7 + serde，rust-create-agent 层消息 ID）
- **同步原语:** parking_lot 0.12
- **构建工具:** Cargo（Workspace resolver = "2"）
- **Web 前端 CDN（relay-server，ES Module，来自 esm.sh）:** preact + preact/hooks + htm + @preact/signals（声明式 UI + 响应式状态）；marked.js 15 + highlight.js 11.9（GitHub Dark 主题）+ DOMPurify（XSS 净化，动态 UMD script 注入）
- **Web 前端 Signal 订阅规则:** esm.sh 多版本场景下 @preact/signals auto-tracking 不可靠，组件必须通过 `useSignalValue(signal)` 显式订阅，禁止在 render 函数中直接读取 `signal.value`

## 架构决策

- **Workspace 多 crate 分层:** `rust-create-agent`（核心 lib）→ `rust-agent-middlewares`（中间件 lib）→ `rust-agent-tui` / `rust-relay-server`（应用层），禁止下层依赖上层
- **异步优先:** 所有 IO 密集操作使用 async/await，trait 方法通过 `async-trait` 标注
- **Middleware Chain 模式:** 横切关注点（HITL、日志、工具提供、prompt 注入）通过 `Middleware<S>` trait 解耦，不侵入核心 ReAct 执行器
- **工具系统:** `BaseTool` trait 统一工具接口，`ToolProvider` trait 支持批量动态提供，`register_tool` 手动注册优先级最高
- **消息不可变历史:** `AgentState` 消息列表只追加，不修改历史，保证 LLM 上下文一致性
- **事件驱动 TUI 通信:** Agent task 与 TUI 渲染线程通过有界 mpsc channel + oneshot 通信，禁止共享可变状态
- **线程持久化事件驱动:** 持久化由 `StateSnapshot` 事件触发增量写入，不做全量序列化

## API 风格

- **LLM 接口:** OpenAI `POST /v1/chat/completions` 格式（SSE streaming）；Anthropic `POST /v1/messages` 格式（SSE streaming）
- **Relay Server:** WebSocket 协议，JSON 消息帧，客户端通过 `tokio-tungstenite` 连接
- **工具调用格式:** OpenAI `type:"function"` + `function.arguments` JSON 字符串；Anthropic `type:"tool_use"` + `input_schema` JSON Schema
- **错误处理:** LLM 层返回 `anyhow::Result`，工具层返回结构化错误信息（`is_error: true` 的 ToolResult）

## 编码规范

- **命名约定:** Rust 标准（struct/enum PascalCase，fn/var snake_case，const SCREAMING_SNAKE_CASE）
- **异步 trait:** 使用 `async-trait` crate，不使用 `impl Trait` 返回 opaque future（兼容 dyn dispatch）
- **错误类型:** 库 crate 用 `thiserror` 定义具名错误，应用层用 `anyhow::Result` 传播
- **日志规范:** 使用 `tracing` 宏（`trace!/debug!/info!/warn!/error!`），不直接使用 `println!` / `eprintln!`
- **测试:** 单元测试在 `src/` 内 `#[cfg(test)] mod tests`，`MockLLM` 模拟 LLM 响应；bin crate 集成测试在 `src/` 内（不支持 `tests/` 目录）
- **文件组织:** 每个模块一个目录，`mod.rs` 作为入口，子文件按职责划分

## 部署方式

- **开发/本地运行:** `cargo run -p rust-agent-tui`，配置通过 `.env` 文件（`rust-agent-tui/.env`）和环境变量
- **生产构建:** `cargo build --release`，输出单一二进制，无外部动态依赖（SQLite bundled）
- **可观测性（可选）:** `docker compose -f docker-compose.otel.yml up -d` 启动 Jaeger，设置 `OTEL_EXPORTER_OTLP_ENDPOINT` 环境变量即开启 OTLP 导出
- **CI/CD:** （未检测到）

## 安全约束

- **HITL 默认拦截清单:** `bash`、`write_*`、`edit_*`、`delete_*`、`rm_*`、`folder_operations`，需明确审批才执行
- **API Key 安全:** `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` 只通过环境变量传递，`.env` 文件已 gitignore
- **SubAgent 防递归:** `launch_agent` 工具始终从子 Agent 工具集中排除自身，防止无限递归

---
*最后更新: 2026-03-28 — 由批量归档（9 个 feature）更新：Web 前端迁移到 Preact+Signals+htm（esm.sh CDN），新增 Signal 订阅规则*
