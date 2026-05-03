# MCP OAuth Auth 人工验收清单

**生成时间:** 2026-05-03
**关联计划:** spec/feature_20260503_F001_mcp-oauth-auth/spec-plan.md
**关联设计:** spec/feature_20260503_F001_mcp-oauth-auth/spec-design.md

---

## 验收前准备

### 环境要求

- [ ] [AUTO] 检查 Rust 工具链: `rustc --version`
- [ ] [AUTO] 编译项目（含 auth feature）: `cargo build -p rust-agent-middlewares`
- [ ] [AUTO] 编译 TUI: `cargo build -p rust-agent-tui`
- [ ] [AUTO] 运行全量测试: `cargo test`
- [ ] [MANUAL] 准备一个支持 OAuth 的 MCP 服务器（如 GitHub MCP）的测试账号

### 测试数据准备

- [ ] 准备含 OAuth 配置的 `.mcp.json` 测试文件
- [ ] 准备不含 `oauth` 字段的 `.mcp.json`（向后兼容测试）
- [ ] 准备 stdio 类型 MCP 服务器配置（stdio 不受影响测试）

---

## 验收项目

### 场景 1：编译与配置基础

#### - [x] 1.1 rmcp auth feature 编译验证
- **来源:** spec-plan.md 验收标准 #1
- **目的:** 确认 auth feature 启用后编译通过
- **操作步骤:**
  1. [A] `cargo build -p rust-agent-middlewares 2>&1` → 期望包含: `Finished` 且不包含 `error`
  2. [A] `cargo build -p rust-agent-tui 2>&1` → 期望包含: `Finished` 且不包含 `error`

#### - [x] 1.2 McpServerConfig 支持 oauth 字段
- **来源:** spec-plan.md 验收标准 #2 / spec-design.md 配置扩展
- **目的:** 确认 OAuth 配置反序列化正确
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib -- mcp::config 2>&1` → 期望包含: `test result: ok`
  2. [A] 检查 `OAuthConfig` 结构体存在: `grep -r "struct OAuthConfig" rust-agent-middlewares/src/mcp/config.rs` → 期望包含: `pub struct OAuthConfig`

#### - [x] 1.3 全量测试通过
- **来源:** spec-plan.md 约束一致性（错误处理、日志）
- **目的:** 确认新增代码不破坏现有功能
- **操作步骤:**
  1. [A] `cargo test 2>&1` → 期望包含: `test result: ok`

---

### 场景 2：Token 持久化

#### - [x] 2.1 Token 文件创建与权限
- **来源:** spec-plan.md 验收标准 #6 / spec-design.md Token 持久化
- **目的:** 确认 token 文件路径正确且权限为 0600
- **操作步骤:**
  1. [A] 检查 `auth_store.rs` 存在: `ls rust-agent-middlewares/src/mcp/auth_store.rs` → 期望包含: `auth_store.rs`
  2. [A] 检查文件路径定义: `grep -r "oauth_tokens.json" rust-agent-middlewares/src/mcp/auth_store.rs` → 期望包含: `oauth_tokens.json`
  3. [A] 检查 0600 权限设置: `grep -r "0o600" rust-agent-middlewares/src/mcp/auth_store.rs` → 期望包含: `0o600`

#### - [x] 2.2 PerServerCredentialStore 包装器
- **来源:** spec-plan.md 配置扩展 / spec-design.md 按服务器分键存储
- **目的:** 确认每服务器独立凭证存储实现
- **操作步骤:**
  1. [A] `grep -r "PerServerCredentialStore" rust-agent-middlewares/src/mcp/auth_store.rs` → 期望包含: `struct PerServerCredentialStore`
  2. [A] `grep -r "impl CredentialStore for" rust-agent-middlewares/src/mcp/auth_store.rs` → 期望包含: `impl CredentialStore for PerServerCredentialStore`

---

### 场景 3：OAuth 流程编排

#### - [x] 3.1 OAuthFlowManager 实现
- **来源:** spec-plan.md OAuth 流程编排 / spec-design.md OAuth 流程编排
- **目的:** 确认 OAuth 流程管理器核心逻辑存在
- **操作步骤:**
  1. [A] 检查 `oauth_flow.rs` 存在: `ls rust-agent-middlewares/src/mcp/oauth_flow.rs` → 期望包含: `oauth_flow.rs`
  2. [A] `grep -r "struct OAuthFlowManager" rust-agent-middlewares/src/mcp/oauth_flow.rs` → 期望包含: `pub struct OAuthFlowManager`
  3. [A] `grep -r "handle_401" rust-agent-middlewares/src/mcp/oauth_flow.rs` → 期望包含: `pub async fn handle_401`

#### - [x] 3.2 回调服务器实现
- **来源:** spec-plan.md 回调服务器 / spec-design.md 回调服务器
- **目的:** 确认本地回调服务器绑定与超时逻辑
- **操作步骤:**
  1. [A] `grep -r "OAuthCallbackServer" rust-agent-middlewares/src/mcp/` → 期望包含: `struct OAuthCallbackServer`
  2. [A] `grep -r "127.0.0.1:0" rust-agent-middlewares/src/mcp/` → 期望包含: `127.0.0.1:0`
  3. [A] `grep -r "120" rust-agent-middlewares/src/mcp/ | grep -i "timeout\|Duration"` → 期望包含: `from_secs(120)`

#### - [x] 3.3 401 自动检测与 OAuth 触发
- **来源:** spec-plan.md 验收标准 #3 / spec-design.md 传输层集成
- **目的:** 确认 401 + WWW-Authenticate 自动触发 OAuth
- **操作步骤:**
  1. [A] `grep -r "WWW.Authenticate\|www_authenticate\|WWWAuthenticate" rust-agent-middlewares/src/mcp/` → 期望包含匹配结果
  2. [A] `grep -r "build_authed_transport\|AuthClient" rust-agent-middlewares/src/mcp/` → 期望包含: `AuthClient`

---

### 场景 4：TUI 集成

#### - [x] 4.1 OAuth 弹窗面板渲染
- **来源:** spec-plan.md 验收标准 #9 / spec-design.md 手动粘贴回退
- **目的:** 确认 OAuth 授权面板 UI 组件存在
- **操作步骤:**
  1. [A] `ls rust-agent-tui/src/ui/main_ui/popups/oauth.rs` → 期望包含: `oauth.rs`
  2. [A] `grep -r "OAuthAuthorizationNeeded\|OAuthAuthorizationCompleted\|OAuthAuthorizationFailed" rust-agent-tui/src/` → 期望包含匹配结果
  3. [A] `grep -r "oauth_prompt\|OAuthPrompt" rust-agent-tui/src/app/` → 期望包含匹配结果

#### - [x] 4.2 MCP 面板 OAuth 状态列
- **来源:** spec-plan.md 验收标准 #10 / spec-design.md MCP 面板状态展示
- **目的:** 确认 MCP 面板展示 OAuth 状态
- **操作步骤:**
  1. [A] `grep -r "OAuth" rust-agent-tui/src/ui/main_ui/panels/mcp.rs` → 期望包含匹配结果
  2. [A] `grep -r "oauth_status\|OAuth.*status\|授权" rust-agent-tui/src/app/mcp_panel.rs` → 期望包含匹配结果

#### - [x] 4.3 手动触发授权快捷键
- **来源:** spec-design.md MCP 面板状态展示（`r` 键）
- **目的:** 确认 MCP 面板支持 `r` 键手动触发 OAuth
- **操作步骤:**
  1. [A] `grep -r "'r'" rust-agent-tui/src/app/mcp_panel.rs` → 期望包含: 手动触发授权相关逻辑
  2. [H] 启动 TUI，打开 MCP 面板，观察 OAuth 状态列是否正常渲染 → 是/否

---

### 场景 5：边界与回归

#### - [x] 5.1 向后兼容——无 oauth 字段
- **来源:** spec-plan.md 验收标准 #13 / spec-design.md 约束一致性
- **目的:** 确认不配置 OAuth 时行为完全不变
- **操作步骤:**
  1. [A] `cargo test -p rust-agent-middlewares --lib 2>&1` → 期望包含: `test result: ok`
  2. [A] 检查 `OAuthConfig` 均为 Option 字段: `grep -A10 "pub struct OAuthConfig" rust-agent-middlewares/src/mcp/config.rs` → 期望包含: `Option<`

#### - [x] 5.2 stdio 传输不受影响
- **来源:** spec-plan.md 验收标准 #14 / spec-design.md 边界情况
- **目的:** 确认 stdio 类型 MCP 服务器不触发 OAuth
- **操作步骤:**
  1. [A] `grep -r "StreamableHttp\|stdio" rust-agent-middlewares/src/mcp/oauth_flow.rs` → 期望包含: 仅 StreamableHttp 条件判断
  2. [A] `grep -r "command" rust-agent-middlewares/src/mcp/client.rs | head -5` → 期望包含: stdio 连接逻辑未被修改

#### - [x] 5.3 Refresh Token 失效回退
- **来源:** spec-plan.md 验收标准 #11 / spec-design.md 边界情况
- **目的:** 确认 Refresh Token 失效时重新触发完整 OAuth
- **操作步骤:**
  1. [A] `grep -r "refresh\|clear\|重新" rust-agent-middlewares/src/mcp/oauth_flow.rs` → 期望包含: 清除存储并重新授权逻辑

#### - [!] 5.4 Scope 升级
- **来源:** spec-plan.md 验收标准 #12 / spec-design.md 边界情况
- **目的:** 确认 scope 不足时触发升级流程
- **操作步骤:**
  1. [A] `grep -r "insufficient_scope\|scope_upgrade\|request_scope" rust-agent-middlewares/src/mcp/` → 期望包含: scope 升级逻辑

#### - [x] 5.5 多服务器并发隔离
- **来源:** spec-design.md 边界情况
- **目的:** 确认每个服务器 OAuth 状态独立
- **操作步骤:**
  1. [A] `grep -r "HashMap.*OAuthState\|states:" rust-agent-middlewares/src/mcp/oauth_flow.rs` → 期望包含: 按 server_name 隔离的状态管理

#### - [x] 5.6 敏感信息不泄露
- **来源:** spec-design.md 约束一致性
- **目的:** 确认 Debug 实现不暴露 token
- **操作步骤:**
  1. [A] `grep -r "REDACTED\|redacted" rust-agent-middlewares/src/mcp/auth_store.rs` → 期望包含: `[REDACTED]` 或 redacted 实现

#### - [!] 5.7 文件权限启动检查
- **来源:** spec-design.md 约束一致性（文件权限）
- **目的:** 确认启动时检查并警告 token 文件权限
- **操作步骤:**
  1. [A] `grep -r "warn\|permissions\|0o600" rust-agent-middlewares/src/mcp/auth_store.rs` → 期望包含: 权限检查警告逻辑

---

## 验收后清理

无需清理后台服务（本功能为库/TUI 集成，无独立服务进程）。

---

## 验收结果汇总

| 场景 | 序号 | 验收项 | [A] | [H] | 结果 |
|------|------|--------|-----|-----|------|
| 场景 1 | 1.1 | rmcp auth feature 编译验证 | 2 | 0 | ⬜ |
| 场景 1 | 1.2 | McpServerConfig 支持 oauth 字段 | 2 | 0 | ⬜ |
| 场景 1 | 1.3 | 全量测试通过 | 1 | 0 | ⬜ |
| 场景 2 | 2.1 | Token 文件创建与权限 | 3 | 0 | ⬜ |
| 场景 2 | 2.2 | PerServerCredentialStore 包装器 | 2 | 0 | ⬜ |
| 场景 3 | 3.1 | OAuthFlowManager 实现 | 3 | 0 | ⬜ |
| 场景 3 | 3.2 | 回调服务器实现 | 3 | 0 | ⬜ |
| 场景 3 | 3.3 | 401 自动检测与 OAuth 触发 | 2 | 0 | ⬜ |
| 场景 4 | 4.1 | OAuth 弹窗面板渲染 | 3 | 0 | ⬜ |
| 场景 4 | 4.2 | MCP 面板 OAuth 状态列 | 2 | 0 | ⬜ |
| 场景 4 | 4.3 | 手动触发授权快捷键 | 1 | 1 | ⬜ |
| 场景 5 | 5.1 | 向后兼容——无 oauth 字段 | 2 | 0 | ⬜ |
| 场景 5 | 5.2 | stdio 传输不受影响 | 2 | 0 | ⬜ |
| 场景 5 | 5.3 | Refresh Token 失效回退 | 1 | 0 | ⬜ |
| 场景 5 | 5.4 | Scope 升级 | 1 | 0 | ⬜ |
| 场景 5 | 5.5 | 多服务器并发隔离 | 1 | 0 | ⬜ |
| 场景 5 | 5.6 | 敏感信息不泄露 | 1 | 0 | ⬜ |
| 场景 5 | 5.7 | 文件权限启动检查 | 1 | 0 | ⬜ |

**验收结论:** ⬜ 全部通过 / ⬜ 存在问题
