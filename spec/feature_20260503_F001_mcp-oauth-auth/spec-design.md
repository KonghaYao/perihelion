# Feature: 20260503_F001 - mcp-oauth-auth

## 需求背景

当前 MCP 中间件仅支持通过 `headers` 字段手动注入静态 API Key 进行 HTTP 认证。随着 MCP 生态发展，越来越多的远程 MCP 服务器（如 GitHub、Google Workspace、企业 IdP）采用 OAuth 2.0 授权流程。当前实现无法应对需要浏览器交互授权的场景。

rmcp crate 已内置完整的 OAuth 2.0 支持（`transport::auth` 模块），包含 Authorization Code + PKCE 流程、动态客户端注册（DCR）、Token 刷新、Scope 升级等能力，但项目未启用 `auth` feature，这些能力完全不可用。

## 目标

- 启用 rmcp `auth` feature，将 `AuthClient<C>` 集成到 MCP HTTP 传输层
- 支持 Authorization Code + PKCE 完整 OAuth 流程，兼容 MCP 规范的 metadata 发现（RFC 8414）和动态客户端注册（RFC 7591）
- HTTP 401 + `WWW-Authenticate` 自动触发 OAuth 授权流程
- Token 持久化到 `~/.zen-code/oauth_tokens.json`（0600 权限），跨会话复用
- 混合回调模式：优先本地 HTTP 回调服务器，失败时回退到 TUI 手动粘贴
- TUI 面板展示 OAuth 授权状态和交互引导

## 方案设计

### 整体架构

```
MCP HTTP 请求
  → AuthClient<StreamableHttpClientTransport>（rmcp auth 模块）
    → 自动注入 Authorization: Bearer {access_token}
    → 401 + WWW-Authenticate → 触发 OAuth 流程
      → discover_metadata() → 获取授权服务器信息
      → DCR（可选）→ 动态注册客户端
      → start_authorization() → 生成 PKCE + CSRF
      → TUI 面板展示授权 URL + 打开浏览器
      → 本地回调服务器等待 / 手动粘贴
      → handle_callback() → exchange_code()
      → FileCredentialStore.save()
      → 重试原始请求
```

### 配置扩展

**McpServerConfig 新增字段**

```rust
// rust-agent-middlewares/src/mcp/config.rs
pub struct McpServerConfig {
    // 现有字段
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub url: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    // 新增
    pub oauth: Option<OAuthConfig>,
}

pub struct OAuthConfig {
    /// 启用 OAuth 自动认证（默认 true）
    pub enabled: Option<bool>,
    /// 预注册的 client_id（为空则使用 DCR）
    pub client_id: Option<String>,
    /// 预注册的 client_secret（仅 confidential client）
    pub client_secret: Option<String>,
    /// 请求的 scopes
    pub scopes: Option<Vec<String>>,
}

impl OAuthConfig {
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}
```

**配置示例（.mcp.json）**

```json
{
  "mcpServers": {
    "github": {
      "url": "https://api.github.com/mcp",
      "oauth": {
        "scopes": ["repo", "user"]
      }
    },
    "enterprise-api": {
      "url": "https://internal.company.com/mcp",
      "oauth": {
        "clientId": "pre-registered-id",
        "clientSecret": "${ENTERPRISE_CLIENT_SECRET}",
        "scopes": ["read", "write"]
      }
    }
  }
}
```

### Token 持久化

**文件存储**

路径：`~/.zen-code/oauth_tokens.json`，权限 `0600`。

```rust
// rust-agent-middlewares/src/mcp/auth_store.rs（新文件）
use rmcp::transport::auth::{CredentialStore, StoredCredentials};

#[derive(Serialize, Deserialize)]
struct OAuthTokenFile {
    version: u32,
    tokens: HashMap<String, StoredCredentials>,
}

pub struct FileCredentialStore {
    path: PathBuf,
}

impl FileCredentialStore {
    pub fn new() -> Self {
        let path = dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".zen-code")
            .join("oauth_tokens.json");
        Self { path }
    }

    /// 确保文件存在且权限为 0600
    fn ensure_file(&self) -> Result<(), AuthError> {
        if !self.path.exists() {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&self.path, r#"{"version":1,"tokens":{}}"#)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.path, perms)?;
        }
        Ok(())
    }
}

#[async_trait]
impl CredentialStore for FileCredentialStore {
    async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
        // 按 server_name 从文件读取
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        // 追加/更新到文件
    }

    async fn clear(&self) -> Result<(), AuthError> {
        // 清除文件中所有 tokens
    }
}
```

**按 server 分键存储**

`FileCredentialStore` 需要支持按 server_name 读写。由于 rmcp 的 `CredentialStore` trait 设计为全局 load/save，我们为每个 MCP 服务器创建独立的 `FileCredentialStore` 实例（传入 server_name 作为构造参数），或在内部维护 per-server 的读写逻辑。

推荐方案：每个服务器独立的 `PerServerCredentialStore` 包装器：

```rust
pub struct PerServerCredentialStore {
    inner: FileCredentialStore,
    server_name: String,
}
```

### OAuth 流程编排

**核心文件：`mcp/oauth_flow.rs`（新文件）**

```rust
pub struct OAuthFlowManager {
    /// 按 server_name 管理的 OAuth 状态
    states: HashMap<String, OAuthState>,
    /// TUI 事件发送通道
    event_tx: mpsc::Sender<AgentEvent>,
    /// 凭证存储
    credential_stores: HashMap<String, Arc<PerServerCredentialStore>>,
}

impl OAuthFlowManager {
    /// 处理 401 响应，触发 OAuth 流程
    pub async fn handle_401(
        &mut self,
        server_name: &str,
        www_authenticate: &WWWAuthenticateParams,
    ) -> Result<(), AuthError> {
        let state = self.states.entry(server_name.to_string())
            .or_insert_with(|| OAuthState::new_pending(server_name));

        // 1. 创建 OAuthState
        let url = get_server_url(server_name);
        let mut oauth = OAuthState::new(url, None).await?;

        // 2. 尝试加载已存储的 credentials
        if let Ok(Some(creds)) = self.load_credentials(server_name).await {
            oauth.set_credentials(&creds.client_id, creds.token_response.unwrap()).await?;
            // 已授权，直接返回（token 可能过期，后续自动刷新）
        }

        // 3. 启动授权
        let redirect_uri = self.start_callback_server().await?;
        oauth.start_authorization(&[], &redirect_uri, None).await?;

        // 4. 获取授权 URL 并通知 TUI
        let auth_url = oauth.get_authorization_url().await?;
        self.event_tx.send(AgentEvent::OAuthAuthorizationNeeded {
            server_name: server_name.to_string(),
            authorization_url: auth_url,
            manual_fallback: false,
        }).await?;

        // 5. 等待回调
        let (code, state_param) = self.wait_for_callback().await?;

        // 6. 完成授权
        oauth.handle_callback(&code, &state_param).await?;
        oauth.complete_authorization().await?;

        // 7. 持久化 credentials
        let manager = oauth.into_authorization_manager().unwrap();
        let creds = manager.get_credentials().await?;
        self.save_credentials(server_name, creds).await?;

        // 8. 通知 TUI 完成
        self.event_tx.send(AgentEvent::OAuthAuthorizationCompleted {
            server_name: server_name.to_string(),
        }).await?;

        Ok(())
    }
}
```

### 回调服务器

```rust
pub struct OAuthCallbackServer {
    listener: TcpListener,
    expected_state: String,
    code_tx: oneshot::Sender<(String, String)>,
}

impl OAuthCallbackServer {
    /// 绑定随机高端口（49152-65535），最多重试 3 次
    pub async fn bind(expected_state: String) -> Result<(Self, String), AuthError> {
        let max_retries = 3;
        for _ in 0..max_retries {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let port = listener.local_addr()?.port();
            let redirect_uri = format!("http://localhost:{}/callback", port);

            let (code_tx, code_rx) = oneshot::channel();
            let server = Self { listener, expected_state: expected_state.clone(), code_tx };

            return Ok((server, redirect_uri));
        }
        Err(AuthError::CallbackServerFailed)
    }

    /// 等待授权码回调，120 秒超时
    pub async fn wait_for_code(self) -> Result<(String, String), AuthError> {
        tokio::time::timeout(
            Duration::from_secs(120),
            self.accept_callback()
        ).await?
    }
}
```

**手动粘贴回退**

回调服务器绑定失败或等待超时后，切换到手动模式：

```
TUI 面板显示：
┌──────────────────────────────────────────┐
│ MCP OAuth 授权 - github                  │
│                                          │
│ 本地回调服务器不可用，请手动完成授权：     │
│                                          │
│ 1. 在浏览器中打开以下 URL：               │
│    https://auth.example.com/authorize?... │
│                                          │
│ 2. 完成授权后，粘贴回调 URL：             │
│    ┌──────────────────────────────┐      │
│    │ http://localhost:xxx/callback?code= │
│    └──────────────────────────────┘      │
│                                          │
│ [Enter 确认] [Esc 取消]                  │
└──────────────────────────────────────────┘
```

### 传输层集成

**AuthClient 包装**

```rust
// mcp/client.rs - build_authed_transport()
fn build_authed_transport(
    url: &str,
    headers: &HashMap<String, String>,
    credential_store: Arc<PerServerCredentialStore>,
) -> AuthClient<StreamableHttpClientTransport<reqwest::Client>> {
    let transport = build_http_transport(url, headers);
    let auth_manager = AuthorizationManager::from_store(url, credential_store);
    AuthClient::new(transport, auth_manager)
}
```

**连接池初始化流程变更**

```rust
// mcp/client.rs - run_initialize()
// 现有流程基础上，对 StreamableHttp 类型服务器：
// 1. 先尝试普通连接（带已有 token）
// 2. 收到 401 → 解析 WWW-Authenticate
// 3. 触发 OAuthFlowManager::handle_401()
// 4. 授权完成后用 AuthClient 重新连接
```

### TUI 事件扩展

```rust
// rust-create-agent/src/event.rs（或在 TUI 层扩展）
pub enum AgentEvent {
    // 现有事件...

    /// 需要用户浏览器授权
    OAuthAuthorizationNeeded {
        server_name: String,
        authorization_url: String,
        manual_fallback: bool,
    },

    /// OAuth 授权完成
    OAuthAuthorizationCompleted {
        server_name: String,
    },

    /// OAuth 授权失败
    OAuthAuthorizationFailed {
        server_name: String,
        error: String,
    },
}
```

TUI 层 `poll_agent()` 处理这些事件：
- `OAuthAuthorizationNeeded` → 弹出 OAuth 面板 + `open::that(&url)` 打开浏览器
- `OAuthAuthorizationCompleted` → 关闭面板 + 状态栏显示成功
- `OAuthAuthorizationFailed` → 面板显示错误信息

### MCP 面板状态展示

MCP 面板新增 OAuth 状态列：

```
┌─────────────────────────────────────────────┐
│ MCP Servers                                  │
│                                              │
│  ● github       http  12 tools  OAuth ✓     │
│  ○ enterprise   http  Failed    OAuth 需授权 │
│  ● filesystem   stdio  8 tools              │
│                                              │
│ [Enter 重连] [r 授权] [d 删除] [Esc 关闭]   │
└─────────────────────────────────────────────┘
```

- `r` 键：手动触发 OAuth 授权（即使未收到 401）
- 状态列：`OAuth ✓`（已授权）、`OAuth 需授权`（token 过期/未授权）、空（不使用 OAuth）

## 实现要点

### 关键技术决策

1. **复用 rmcp OAuthState 状态机**：不自行实现 OAuth 协议，所有 PKCE、DCR、token refresh 由 rmcp 处理
2. **PerServerCredentialStore 包装**：rmcp 的 `CredentialStore` trait 是全局接口，需要包装为 per-server 粒度
3. **混合回调模式**：优先本地 HTTP 回调（用户体验好），失败时回退手动粘贴（兼容性好）
4. **401 自动检测**：`AuthClient` 自动拦截 401，无需在每个工具调用点手动处理
5. **Token 过期前自动刷新**：rmcp `AuthorizationManager` 在 `get_access_token()` 时检查 `expires_at`，过期前 5 分钟自动使用 refresh_token

### 依赖

- rmcp `auth` feature（已存在于 `rust-mcp-patch`，只需在 middlewares 的 Cargo.toml 启用）
- `open` crate（打开浏览器，如需要新增依赖）
- `tokio::net::TcpListener`（回调服务器，tokio 已有）

### 实现顺序建议

1. **Cargo.toml 启用 auth feature** + 验证 rmcp auth 模块编译通过
2. **auth_store.rs**：`FileCredentialStore` + `PerServerCredentialStore`
3. **config.rs**：`OAuthConfig` 结构体 + serde 反序列化
4. **transport 层**：`build_authed_transport()` 包装 `AuthClient`
5. **oauth_flow.rs**：`OAuthFlowManager` + `OAuthCallbackServer`
6. **client.rs**：`run_initialize()` 集成 401 检测 + OAuth 触发
7. **TUI 事件**：`AgentEvent` 扩展 + 面板渲染 + 手动粘贴模式

### 边界情况

- **Token 过期**：`AuthorizationManager.get_access_token()` 自动检查，过期前 5 分钟 refresh
- **Refresh Token 失效**：清除存储 → 重新触发完整 OAuth 流程
- **Scope 不足**：`WWW-Authenticate` 返回 `insufficient_scope` → `request_scope_upgrade()`
- **多服务器并发**：每个服务器独立 `OAuthState`，互不影响
- **服务器不支持 OAuth**：401 无 `WWW-Authenticate` → 记录错误，不触发 OAuth
- **DCR 失败**：回退到 `OAuthConfig.client_id`（预注册）
- **网络中断**：超时后标记 Failed，面板提供手动重试入口
- **stdio 传输**：不涉及 OAuth，OAuth 仅用于 StreamableHttp 传输

## 约束一致性

- **依赖方向**：新增 `auth_store.rs`、`oauth_flow.rs` 位于 `rust-agent-middlewares`，依赖 `rust-create-agent` 的事件定义，符合 workspace 依赖方向
- **错误处理**：库 crate 使用 `thiserror`，遵循现有 `McpPoolError` / `ToolCallError` 模式
- **日志**：使用 `tracing` 宏，不使用 `println!`
- **文件权限**：Token 文件 0600，启动时检查并警告
- **敏感信息**：`StoredCredentials` 的 Debug 实现使用 `[REDACTED]`（rmcp 已实现）
- **配置兼容**：`OAuthConfig` 字段均为 `Option`，不存在该字段时行为与现有完全一致

## 验收标准

- [ ] rmcp `auth` feature 启用，项目编译通过
- [ ] `McpServerConfig` 支持 `oauth` 字段，`.mcp.json` 可配置 OAuth 参数
- [ ] HTTP MCP 服务器返回 401 + `WWW-Authenticate` 时自动触发 OAuth 流程
- [ ] OAuth metadata 发现（RFC 8414）和动态客户端注册（RFC 7591）正常工作
- [ ] Authorization Code + PKCE 流程端到端通过
- [ ] Token 持久化到 `~/.zen-code/oauth_tokens.json`，文件权限 0600
- [ ] 重启应用后已授权服务器直接加载 token，无需重新授权
- [ ] 本地回调服务器正常工作，超时后自动切换手动粘贴模式
- [ ] TUI 面板展示 OAuth 授权 URL 和手动粘贴输入框
- [ ] MCP 面板显示 OAuth 状态，支持手动触发授权
- [ ] Token 过期自动刷新，Refresh Token 失效时重新触发授权
- [ ] Scope 不足时自动触发 scope 升级流程
- [ ] 不配置 `oauth` 字段的 MCP 服务器行为完全不变（向后兼容）
- [ ] stdio 传输的 MCP 服务器不受影响
