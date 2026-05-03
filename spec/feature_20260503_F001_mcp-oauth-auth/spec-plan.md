# MCP OAuth 认证 执行计划

**目标:** 启用 rmcp auth feature，将 OAuth 2.0 Authorization Code + PKCE 流程集成到 MCP HTTP 传输层，支持自动 401 检测、浏览器授权、Token 持久化、TUI 面板交互。

**技术栈:** rmcp auth feature (oauth2 crate)、tokio::net::TcpListener（回调服务器）、serde_json（Token 持久化）、ratatui（TUI 面板）

**设计文档:** spec/feature_20260503_F001_mcp-oauth-auth/spec-design.md

## 改动总览

- Task 1 在 middlewares Cargo.toml 启用 rmcp `auth` feature，新增 `OAuthConfig` 结构体并扩展 `McpServerConfig`，为后续 Task 2-5 提供配置基础。
- Task 2 实现 Token 持久化（FileCredentialStore + PerServerCredentialStore），依赖 Task 1 的 OAuthConfig。
- Task 3 实现 OAuth 回调服务器（callback_server.rs），独立于 Task 1-2，但被 Task 4 调用。
- Task 4 编排完整 OAuth 流程并集成到传输层，依赖 Task 1-3。
- Task 5 扩展 TUI 事件和面板，依赖 Task 4 的事件定义。
- 关键设计决策：OAuthConfig 使用 camelCase 反序列化（与配置文件一致）；`expand_server_config` 对 `client_secret` 执行 `${VAR}` 展开；所有现有测试中手动构造的 `McpServerConfig` 需补充 `oauth: None` 字段。

---

### Task 0: 环境准备

**背景:**
确保构建和测试工具链在当前开发环境中可用，特别是验证 rmcp `auth` feature 启用后整个 workspace 能正常编译。

**执行步骤:**
- [x] 验证 Rust 工具链可用
  - 运行: `rustc --version && cargo --version`
  - 确认 Rust 版本支持 rmcp crate（2024 edition）
- [x] 验证当前 workspace 基线构建通过
  - 运行: `cargo build 2>&1 | tail -5`
  - 预期: `Finished` 无错误
- [x] 验证当前 workspace 基线测试通过
  - 运行: `cargo test 2>&1 | tail -10`
  - 预期: `test result: ok` 无失败

**检查步骤:**
- [x] Workspace 构建无错误
  - `cargo build 2>&1 | grep -c "^error"`
  - 预期: 输出为 0
- [x] Workspace 测试全部通过
  - `cargo test 2>&1 | grep "test result:" | grep -v "ok" | grep -c ""`
  - 预期: 输出为 0（所有 test result 行都包含 "ok"）

---

### Task 1: Cargo.toml 启用 auth feature + OAuthConfig 配置结构体

**背景:**
[业务语境] — 为 MCP Streamable HTTP 传输层增加 OAuth 2.0 Authorization Code + PKCE 认证支持，用户可在 MCP 服务器配置中声明 OAuth 参数，系统自动完成浏览器授权和 Token 管理。
[修改原因] — 当前 `rmcp` 依赖未启用 `auth` feature，`McpServerConfig` 缺少 OAuth 配置字段，无法感知和传递认证信息。
[上下游影响] — 本 Task 的 `OAuthConfig` 结构体和 `McpServerConfig.oauth` 字段是 Task 2（Token 持久化）、Task 4（OAuth 流程编排）的配置基础。本 Task 无前置依赖。

**涉及文件:**
- 修改: `rust-agent-middlewares/Cargo.toml`
- 修改: `rust-agent-middlewares/src/mcp/config.rs`
- 修改: `rust-agent-middlewares/src/mcp/mod.rs`

**执行步骤:**
- [x] 在 rmcp 依赖的 features 列表中追加 `"auth"` — 启用 oauth2 / url 依赖
  - 位置: `rust-agent-middlewares/Cargo.toml` 第 29-33 行
  - 将 `rmcp = { version = "1.6", features = [` 列表末尾 `"transport-streamable-http-client-reqwest",` 之后追加 `"auth",`
  - 原因: rmcp auth feature 依赖 `dep:oauth2`、`__reqwest`（已通过 transport-streamable-http-client-reqwest 间接启用）、`dep:url`，启用后 oauth2 和 url crate 可用
- [x] 在 config.rs 中新增 `OAuthConfig` 结构体 — 定义 OAuth 配置模型
  - 位置: `rust-agent-middlewares/src/mcp/config.rs` 第 22 行（`McpServerConfig` 结构体定义之后，`McpConfigFile` 定义之前）
  - 新增结构体:
    ```rust
    /// MCP 服务器 OAuth 2.0 配置
    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    pub struct OAuthConfig {
        /// 是否启用 OAuth（默认 true）
        #[serde(default)]
        pub enabled: Option<bool>,
        /// OAuth 客户端 ID
        #[serde(default)]
        pub client_id: Option<String>,
        /// OAuth 客户端密钥（支持 ${VAR} 环境变量展开）
        #[serde(default)]
        pub client_secret: Option<String>,
        /// OAuth 权限范围列表
        #[serde(default)]
        pub scopes: Option<Vec<String>>,
    }

    impl OAuthConfig {
        /// 判断 OAuth 是否启用，默认 true
        pub fn is_enabled(&self) -> bool {
            self.enabled.unwrap_or(true)
        }
    }
    ```
  - 原因: 配置文件使用 camelCase（`clientId`/`clientSecret`），需通过 `#[serde(rename_all = "camelCase")]` 对齐。`is_enabled()` 提供便捷判断，`Default` trait 支持默认构造。
- [x] 在 `McpServerConfig` 中追加 `oauth` 字段 — 关联 OAuth 配置
  - 位置: `rust-agent-middlewares/src/mcp/config.rs` 第 7-22 行，`McpServerConfig` 结构体定义内
  - 在 `headers` 字段（第 20-21 行）之后追加:
    ```rust
    /// OAuth 2.0 配置
    #[serde(default)]
    pub oauth: Option<OAuthConfig>,
    ```
  - 原因: 使每个 MCP 服务器可独立声明 OAuth 配置，`#[serde(default)]` 保证向后兼容（旧配置文件无 oauth 字段时反序列化为 None）。
- [x] 在 `expand_server_config` 函数中追加 `oauth` 字段的环境变量展开 — 展开 client_secret 中的 ${VAR}
  - 位置: `rust-agent-middlewares/src/mcp/config.rs` 第 130-147 行，`expand_server_config` 函数
  - 在返回的 `McpServerConfig` 结构体字面量中，`headers` 字段之后追加:
    ```rust
    oauth: config.oauth.as_ref().map(|oauth| OAuthConfig {
        enabled: oauth.enabled,
        client_id: oauth.client_id.clone(),
        client_secret: oauth.client_secret.as_ref().map(|s| expand_env_vars(s)),
        scopes: oauth.scopes.clone(),
    }),
    ```
  - 原因: `client_secret` 可能引用环境变量（如 `${ENTERPRISE_CLIENT_SECRET}`），需在配置加载阶段展开。`client_id` 和 `scopes` 通常为静态值，直接克隆。
- [x] 更新所有现有测试中 `McpServerConfig` 的手动构造 — 补充 `oauth: None` 字段
  - 位置: `rust-agent-middlewares/src/mcp/config.rs` 第 311-541 行，`mod tests` 内所有 `McpServerConfig { ... }` 构造
  - 涉及测试函数: `test_merge_project_overrides_global`（第 398-426 行两处）、`test_merge_project_adds_new_server`（第 428-459 行两处）
  - 每处 `McpServerConfig { ... }` 在 `headers: None,` 之后追加 `oauth: None,`
  - 原因: 新增 `oauth` 字段后，所有手动构造 `McpServerConfig` 的位置必须补充该字段，否则编译失败。通过 JSON 反序列化的测试（如 `test_load_from_valid_json`）不受影响，因为 `#[serde(default)]` 已处理缺失字段。
- [x] 在 mod.rs 的 pub use 列表中导出 `OAuthConfig`
  - 位置: `rust-agent-middlewares/src/mcp/mod.rs` 第 8-10 行
  - 在 `pub use config::{ ..., McpServerConfig,` 之后追加 `OAuthConfig,`
  - 原因: Task 4 的 OAuth 流程编排模块需要引用 `OAuthConfig`，通过 mod.rs 统一导出保持公共 API 一致。
- [x] 为 OAuthConfig 和 expand_server_config 的 OAuth 展开逻辑编写单元测试
  - 测试文件: `rust-agent-middlewares/src/mcp/config.rs`（`#[cfg(test)] mod tests` 内）
  - 测试场景:
    - `test_oauth_config_default_disabled`: `OAuthConfig::default().is_enabled()` → 返回 `true`（默认启用）
    - `test_oauth_config_explicitly_disabled`: `OAuthConfig { enabled: Some(false), .. }.is_enabled()` → 返回 `false`
    - `test_oauth_config_camel_case_deserialize`: JSON `{"clientId":"abc","clientSecret":"secret","scopes":["read"]}` 反序列化为 `OAuthConfig`，字段正确映射
    - `test_expand_server_config_oauth_client_secret`: 设置环境变量 `TEST_OAUTH_SECRET=real_secret`，构造含 `oauth: Some(OAuthConfig { client_secret: Some("${TEST_OAUTH_SECRET}".into()), ... })` 的 `McpServerConfig`，调用 `expand_server_config`，断言展开后 `client_secret` 为 `"real_secret"`
    - `test_expand_server_config_oauth_none`: 构造含 `oauth: None` 的 `McpServerConfig`，调用 `expand_server_config`，断言展开后 `oauth` 仍为 `None`
    - `test_mcp_server_config_with_oauth_from_json`: JSON `{"mcpServers":{"srv":{"url":"https://example.com","oauth":{"clientId":"id","scopes":["s1"]}}}}` 反序列化为 `McpConfigFile`，断言 `oauth` 字段正确填充
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- mcp::config::tests::test_oauth`
  - 预期: 所有 6 个测试通过

**检查步骤:**
- [x] 验证 Cargo.toml 包含 auth feature
  - `grep -c '"auth"' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/Cargo.toml`
  - 预期: 输出为 1
- [x] 验证 OAuthConfig 结构体定义存在
  - `grep -n 'pub struct OAuthConfig' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/config.rs`
  - 预期: 行号输出
- [x] 验证 McpServerConfig 包含 oauth 字段
  - `grep -n 'pub oauth' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/config.rs`
  - 预期: 行号输出
- [x] 验证 mod.rs 导出 OAuthConfig
  - `grep 'OAuthConfig' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `OAuthConfig`
- [x] 验证编译通过
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 输出包含 `Finished`，无编译错误
- [x] 验证所有现有测试通过（含新增测试）
  - `cargo test -p rust-agent-middlewares --lib 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok`，无失败测试

---

### Task 2: Token 持久化 — FileCredentialStore + PerServerCredentialStore

**背景:**
[业务语境] — MCP 服务器通过 OAuth 2.0 获取的 access_token 和 refresh_token 需要持久化到本地文件，避免每次启动都重新浏览器授权。多个 MCP 服务器各自独立存储 token，互不干扰。
[修改原因] — 当前无 Token 持久化机制，rmcp 仅提供 `InMemoryCredentialStore`（进程内存储），进程退出后 token 丢失。
[上下游影响] — 本 Task 输出的 `PerServerCredentialStore` 被 Task 4（OAuth 流程编排）的 `OAuthFlowManager` 和 `build_authed_transport()` 使用。本 Task 依赖 Task 1 的 rmcp `auth` feature 已启用。

**涉及文件:**
- 新建: `rust-agent-middlewares/src/mcp/auth_store.rs`
- 修改: `rust-agent-middlewares/src/mcp/mod.rs`

**执行步骤:**
- [x] 新建 auth_store.rs 文件，定义 `OAuthTokenFile` 数据结构和 `AuthStoreError` 错误枚举
  - 位置: `rust-agent-middlewares/src/mcp/auth_store.rs`（新文件，与 mod.rs 同级目录）
  - 文件头部 imports:
    ```rust
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use async_trait::async_trait;
    use rmcp::transport::auth::{AuthError, CredentialStore, StoredCredentials};
    use serde::{Deserialize, Serialize};
    use tokio::sync::Mutex;
    use tracing::{debug, warn};
    ```
  - 定义 JSON 文件数据结构:
    ```rust
    const TOKEN_FILE_VERSION: u32 = 1;

    /// Token 文件的序列化格式
    #[derive(Serialize, Deserialize)]
    struct OAuthTokenFile {
        version: u32,
        tokens: HashMap<String, StoredCredentials>,
    }
    ```
  - 定义项目级错误枚举（thiserror 模式，对齐 `McpPoolError` / `McpConfigError`）:
    ```rust
    /// Token 持久化错误
    #[derive(Debug, thiserror::Error)]
    pub enum AuthStoreError {
        #[error("Token 文件读取失败: {path}: {source}")]
        ReadFailed { path: PathBuf, source: String },

        #[error("Token 文件写入失败: {path}: {source}")]
        WriteFailed { path: PathBuf, source: String },

        #[error("Token 文件格式无效: {reason}")]
        InvalidFormat { reason: String },

        #[error("服务器 \"{server}\" 的 Token 未找到")]
        NotFound { server: String },
    }
    ```
  - 原因: rmcp 的 `AuthError` 不实现 `From<std::io::Error>`，无法直接包装 IO 错误。自定义 `AuthStoreError` 用于内部错误传播，在 `CredentialStore` trait 实现层转换为 `AuthError::InternalError`。`source` 字段使用 `String`（非 `std::io::Error`），因为 `thiserror` 的 `#[source]` 属性要求 Error trait bound，而此处需要手动映射。
- [x] 实现 `FileCredentialStore` 结构体及其核心文件读写方法
  - 位置: `rust-agent-middlewares/src/mcp/auth_store.rs`，`OAuthTokenFile` 定义之后
  - 结构体定义:
    ```rust
    /// 基于 JSON 文件的 Token 持久化存储
    ///
    /// 所有 MCP 服务器的 token 存储在同一个 JSON 文件中，按 server_name 分键。
    /// 内部使用 `Mutex` 保证并发读写安全。
    pub struct FileCredentialStore {
        path: PathBuf,
        mutex: Mutex<()>,
    }
    ```
  - 构造方法:
    ```rust
    impl FileCredentialStore {
        /// 创建 FileCredentialStore，默认路径 ~/.zen-code/oauth_tokens.json
        pub fn new() -> Self {
            let path = dirs_next::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".zen-code")
                .join("oauth_tokens.json");
            Self {
                path,
                mutex: Mutex::new(()),
            }
        }

        /// 使用自定义路径创建 FileCredentialStore（测试用）
        pub fn with_path(path: PathBuf) -> Self {
            Self {
                path,
                mutex: Mutex::new(()),
            }
        }

        /// 返回 token 文件路径
        pub fn path(&self) -> &Path {
            &self.path
        }
    ```
  - 私有方法 `ensure_file` — 确保文件存在且权限为 0600:
    ```rust
        /// 确保文件和目录存在，设置文件权限为 0600
        fn ensure_file(&self) -> std::result::Result<(), AuthStoreError> {
            if !self.path.exists() {
                if let Some(parent) = self.path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| AuthStoreError::WriteFailed {
                        path: parent.to_path_buf(),
                        source: e.to_string(),
                    })?;
                }
                let initial_content = serde_json::to_string_pretty(&OAuthTokenFile {
                    version: TOKEN_FILE_VERSION,
                    tokens: HashMap::new(),
                })
                .map_err(|e| AuthStoreError::WriteFailed {
                    path: self.path.clone(),
                    source: e.to_string(),
                })?;
                std::fs::write(&self.path, initial_content).map_err(|e| AuthStoreError::WriteFailed {
                    path: self.path.clone(),
                    source: e.to_string(),
                })?;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o600);
                std::fs::set_permissions(&self.path, perms).map_err(|e| AuthStoreError::WriteFailed {
                    path: self.path.clone(),
                    source: e.to_string(),
                })?;
            }
            #[cfg(not(unix))]
            {
                let _ = tracing::warn!(
                    "非 Unix 平台，跳过 Token 文件权限设置: {}",
                    self.path.display()
                );
            }
            Ok(())
        }
    ```
  - 私有方法 `read_file` — 读取并解析 JSON 文件:
    ```rust
        /// 读取 JSON 文件，返回 OAuthTokenFile
        fn read_file(&self) -> std::result::Result<OAuthTokenFile, AuthStoreError> {
            self.ensure_file()?;
            let content = std::fs::read_to_string(&self.path).map_err(|e| AuthStoreError::ReadFailed {
                path: self.path.clone(),
                source: e.to_string(),
            })?;
            let file: OAuthTokenFile = serde_json::from_str(&content).map_err(|e| {
                AuthStoreError::InvalidFormat {
                    reason: format!("JSON 解析失败: {}", e),
                }
            })?;
            if file.version != TOKEN_FILE_VERSION {
                return Err(AuthStoreError::InvalidFormat {
                    reason: format!("不支持的版本号: {}，期望: {}", file.version, TOKEN_FILE_VERSION),
                });
            }
            Ok(file)
        }
    ```
  - 私有方法 `write_file` — 序列化并写入 JSON 文件:
    ```rust
        /// 写入 OAuthTokenFile 到 JSON 文件
        fn write_file(&self, file: &OAuthTokenFile) -> std::result::Result<(), AuthStoreError> {
            self.ensure_file()?;
            let content = serde_json::to_string_pretty(file).map_err(|e| AuthStoreError::WriteFailed {
                path: self.path.clone(),
                source: e.to_string(),
            })?;
            std::fs::write(&self.path, content).map_err(|e| AuthStoreError::WriteFailed {
                path: self.path.clone(),
                source: e.to_string(),
            })?;
            debug!("Token 文件已写入: {}", self.path.display());
            Ok(())
        }
    ```
  - 公共方法 `load_server` / `save_server` / `clear_server` — 按 server_name 读写:
    ```rust
        /// 读取指定服务器的 Token
        pub async fn load_server(
            &self,
            server_name: &str,
        ) -> std::result::Result<Option<StoredCredentials>, AuthStoreError> {
            let _lock = self.mutex.lock().await;
            let file = self.read_file()?;
            Ok(file.tokens.get(server_name).cloned())
        }

        /// 保存指定服务器的 Token
        pub async fn save_server(
            &self,
            server_name: &str,
            credentials: StoredCredentials,
        ) -> std::result::Result<(), AuthStoreError> {
            let _lock = self.mutex.lock().await;
            let mut file = self.read_file()?;
            file.tokens.insert(server_name.to_string(), credentials);
            self.write_file(&file)
        }

        /// 清除指定服务器的 Token
        pub async fn clear_server(&self, server_name: &str) -> std::result::Result<(), AuthStoreError> {
            let _lock = self.mutex.lock().await;
            let mut file = self.read_file()?;
            file.tokens.remove(server_name);
            self.write_file(&file)
        }

        /// 清除所有服务器的 Token
        pub async fn clear_all(&self) -> std::result::Result<(), AuthStoreError> {
            let _lock = self.mutex.lock().await;
            let file = OAuthTokenFile {
                version: TOKEN_FILE_VERSION,
                tokens: HashMap::new(),
            };
            self.write_file(&file)
        }

        /// 列出所有已存储 Token 的服务器名称
        pub async fn list_servers(&self) -> std::result::Result<Vec<String>, AuthStoreError> {
            let _lock = self.mutex.lock().await;
            let file = self.read_file()?;
            Ok(file.tokens.keys().cloned().collect())
        }
    ```
  - 原因: `FileCredentialStore` 是底层存储引擎，负责 JSON 文件的读写。使用 `Mutex<()>` 而非 `RwLock`，因为写操作需要读-改-写，读锁无法保证一致性。`ensure_file()` 在每次读写前调用，保证首次使用时自动创建文件。`with_path()` 构造方法支持测试时使用临时文件路径。
- [x] 实现 `PerServerCredentialStore` — 包装 `FileCredentialStore`，实现 rmcp `CredentialStore` trait
  - 位置: `rust-agent-middlewares/src/mcp/auth_store.rs`，`FileCredentialStore` 实现之后
  - 结构体定义:
    ```rust
    /// 单个 MCP 服务器的 CredentialStore 适配器
    ///
    /// 包装 `FileCredentialStore`，将 rmcp `CredentialStore` trait 的全局 load/save
    /// 映射为按 server_name 的键值操作。
    pub struct PerServerCredentialStore {
        inner: Arc<FileCredentialStore>,
        server_name: String,
    }
    ```
  - 构造方法:
    ```rust
    impl PerServerCredentialStore {
        pub fn new(inner: Arc<FileCredentialStore>, server_name: String) -> Self {
            Self { inner, server_name }
        }

        pub fn server_name(&self) -> &str {
            &self.server_name
        }
    ```
  - 实现 `CredentialStore` trait:
    ```rust
    #[async_trait]
    impl CredentialStore for PerServerCredentialStore {
        async fn load(&self) -> Result<Option<StoredCredentials>, AuthError> {
            self.inner
                .load_server(&self.server_name)
                .await
                .map_err(|e| AuthError::InternalError(e.to_string()))
        }

        async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
            self.inner
                .save_server(&self.server_name, credentials)
                .await
                .map_err(|e| AuthError::InternalError(e.to_string()))
        }

        async fn clear(&self) -> Result<(), AuthError> {
            self.inner
                .clear_server(&self.server_name)
                .await
                .map_err(|e| AuthError::InternalError(e.to_string()))
        }
    }
    ```
  - 原因: rmcp 的 `CredentialStore` trait 接口是全局的（load/save/clear 不带 server_name 参数），但实际使用场景是每个 MCP 服务器独立管理 token。`PerServerCredentialStore` 通过构造时绑定 `server_name`，将 trait 接口映射到 per-server 的文件读写。`inner` 使用 `Arc<FileCredentialStore>` 共享，多个服务器实例共享同一个文件但按 key 隔离。错误统一转换为 `AuthError::InternalError`，符合 rmcp 的错误传播约定。
- [x] 在 mod.rs 中注册 auth_store 模块并导出公共类型
  - 位置: `rust-agent-middlewares/src/mcp/mod.rs`
  - 在第 6 行 `pub mod middleware;` 之后追加 `pub mod auth_store;`
  - 在第 12 行 `pub use transport::{TransportConfig, TransportError};` 之后追加:
    ```rust
    pub use auth_store::{AuthStoreError, FileCredentialStore, PerServerCredentialStore};
    ```
  - 原因: Task 4 的 `OAuthFlowManager` 需要引用 `FileCredentialStore` 和 `PerServerCredentialStore`，通过 mod.rs 统一导出。
- [x] 为 FileCredentialStore 和 PerServerCredentialStore 编写单元测试
  - 测试文件: `rust-agent-middlewares/src/mcp/auth_store.rs`（文件末尾 `#[cfg(test)] mod tests`）
  - 测试场景:
    - `test_new_creates_default_path`: `FileCredentialStore::new()` 的 path 以 `.zen-code/oauth_tokens.json` 结尾
    - `test_ensure_file_creates_file_with_initial_content`: 使用 `tempfile::NamedTempFile` 创建临时路径，构造 `FileCredentialStore::with_path(temp_path)`，调用 `ensure_file()`，断言文件存在且内容为合法 JSON（包含 `"version": 1` 和 `"tokens": {}`）
    - `test_save_and_load_server`: 使用临时路径，调用 `save_server("my-server", credentials)`，再调用 `load_server("my-server")`，断言返回 `Some(StoredCredentials)` 且 `client_id` 匹配
    - `test_load_nonexistent_server_returns_none`: 保存 server A 的 token 后，`load_server("other-server")` 返回 `None`
    - `test_clear_server`: 保存 server A 和 server B 的 token，`clear_server("server-A")` 后，`load_server("server-A")` 返回 `None`，`load_server("server-B")` 仍返回 `Some`
    - `test_clear_all`: 保存多个 server 的 token，`clear_all()` 后，`list_servers()` 返回空 Vec
    - `test_list_servers`: 保存 3 个 server 的 token，`list_servers()` 返回包含 3 个名称的 Vec
    - `test_overwrite_server_token`: 对同一 server_name 连续 save 两次（不同 client_id），`load_server` 返回最后一次保存的值
    - `test_file_persists_across_instances`: 使用同一临时路径创建两个 `FileCredentialStore` 实例，第一个保存 token，第二个加载并断言数据一致
    - `test_per_server_credential_store_load_save`: 构造 `PerServerCredentialStore::new(Arc::new(store), "test-srv".into())`，调用 `CredentialStore` trait 的 `save` → `load`，断言数据一致
    - `test_per_server_credential_store_clear`: `save` → `clear` → `load` 返回 `None`
    - `test_concurrent_save_does_not_corrupt`: 使用 `tokio::join!` 并发对同一 `FileCredentialStore` 的两个不同 server_name 调用 `save_server`，完成后两个 server 的 token 均可正确加载（验证 Mutex 保护读-改-写原子性）
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- mcp::auth_store::tests`
  - 预期: 所有 12 个测试通过

**检查步骤:**
- [x] 验证 auth_store.rs 文件存在
  - `test -f /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/auth_store.rs && echo "EXISTS"`
  - 预期: 输出 `EXISTS`
- [x] 验证 mod.rs 导出 auth_store 模块和公共类型
  - `grep -E 'auth_store|FileCredentialStore|PerServerCredentialStore|AuthStoreError' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `pub mod auth_store;`、`FileCredentialStore`、`PerServerCredentialStore`、`AuthStoreError`
- [x] 验证 FileCredentialStore 实现了文件权限设置
  - `grep -c '0o600' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/auth_store.rs`
  - 预期: 输出为 1
- [x] 验证 PerServerCredentialStore 实现了 CredentialStore trait
  - `grep -c 'impl CredentialStore for PerServerCredentialStore' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/auth_store.rs`
  - 预期: 输出为 1
- [x] 验证编译通过
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 输出包含 `Finished`，无编译错误
- [x] 验证 auth_store 模块所有测试通过
  - `cargo test -p rust-agent-middlewares --lib -- mcp::auth_store::tests 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok`，12 个测试全部通过
- [x] 验证无回归（全量测试）
  - `cargo test -p rust-agent-middlewares --lib 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok`，无失败测试

---

### Task 3: OAuth 回调服务器 — callback_server.rs

**背景:**
[业务语境] — OAuth Authorization Code 流程需要浏览器授权后回调到本地，本 Task 实现本地 HTTP 回调服务器监听授权码，并提供超时后回退到手动粘贴模式的能力。
[修改原因] — 当前 MCP 模块没有本地 HTTP 服务器，无法接收 OAuth 授权码回调。rmcp 的 auth 模块仅负责协议层（PKCE、token exchange），回调服务器的 HTTP 监听需自行实现。
[上下游影响] — 本 Task 被 Task 4（OAuth 流程编排）调用：Task 4 通过 `OAuthCallbackServer::bind()` 获取 redirect_uri 和 code 接收端。本 Task 依赖 Task 1 启用 rmcp auth feature 后的编译环境，但不依赖 Task 2。

**涉及文件:**
- 新建: `rust-agent-middlewares/src/mcp/callback_server.rs`
- 修改: `rust-agent-middlewares/src/mcp/mod.rs`

**执行步骤:**
- [x] 新建 callback_server.rs，定义 CallbackError 错误枚举 — 统一回调服务器的错误类型
  - 位置: `rust-agent-middlewares/src/mcp/callback_server.rs` 文件顶部
  - 新增内容:
    ```rust
    use std::time::Duration;
    use thiserror::Error;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;
    use tracing::{info, warn};

    /// 回调服务器错误类型
    #[derive(Debug, Error)]
    pub enum CallbackError {
        #[error("回调服务器绑定失败: {0}")]
        BindFailed(String),
        #[error("回调等待超时")]
        Timeout,
        #[error("CSRF state 不匹配: expected={expected}, got={got}")]
        StateMismatch { expected: String, got: String },
        #[error("回调 URL 缺少 code 参数")]
        MissingCode,
        #[error("无效的回调请求: {0}")]
        InvalidRequest(String),
        #[error("IO 错误: {0}")]
        Io(#[from] std::io::Error),
    }
    ```
  - 原因: 使用 thiserror 与项目现有错误模式一致（config.rs、transport.rs、client.rs 均使用 thiserror）。`StateMismatch` 携带 expected/got 信息便于调试。
- [x] 实现 OAuthCallbackServer 结构体和 bind() 方法 — 绑定随机端口并创建回调通道
  - 位置: `rust-agent-middlewares/src/mcp/callback_server.rs`，CallbackError 定义之后
  - 新增内容:
    ```rust
    /// OAuth 本地回调服务器，监听浏览器授权回调
    pub struct OAuthCallbackServer {
        listener: TcpListener,
        expected_state: String,
        code_tx: oneshot::Sender<(String, String)>,
        code_rx: oneshot::Receiver<(String, String)>,
    }

    impl OAuthCallbackServer {
        /// 绑定随机高端口 127.0.0.1:0，创建回调服务器
        ///
        /// 返回 (server实例, redirect_uri)
        /// 最多重试 3 次绑定（极端情况下端口可能冲突）
        pub async fn bind(expected_state: String) -> Result<(Self, String), CallbackError> {
            let max_retries = 3;
            let mut last_err = String::from("未知错误");

            for _ in 0..max_retries {
                match TcpListener::bind("127.0.0.1:0").await {
                    Ok(listener) => {
                        let port = listener.local_addr()?.port();
                        let redirect_uri = format!("http://localhost:{}/callback", port);
                        let (code_tx, code_rx) = oneshot::channel();

                        info!(port, "OAuth 回调服务器已绑定");
                        let server = Self {
                            listener,
                            expected_state,
                            code_tx,
                            code_rx,
                        };
                        return Ok((server, redirect_uri));
                    }
                    Err(e) => {
                        last_err = e.to_string();
                        warn!("回调服务器绑定失败，重试中: {}", last_err);
                    }
                }
            }

            Err(CallbackError::BindFailed(format!(
                "重试 {} 次后仍无法绑定: {}",
                max_retries, last_err
            )))
        }
    }
    ```
  - 原因: 绑定 `127.0.0.1:0` 由操作系统分配随机可用端口，避免端口冲突。oneshot channel 将回调中的 code 传递给调用方。`code_rx` 存储在结构体中，确保生命周期与服务器一致，由 `wait_for_code()` 消费。
- [x] 实现 wait_for_code() 和 wait_for_code_with_timeout() 方法 — 接受 HTTP 连接并解析回调参数
  - 位置: `rust-agent-middlewares/src/mcp/callback_server.rs`，`bind()` 方法之后
  - 新增内容:
    ```rust
    impl OAuthCallbackServer {
        /// 等待授权码回调，120 秒超时
        pub async fn wait_for_code(
            self,
        ) -> Result<(String, String), CallbackError> {
            self.wait_for_code_with_timeout(Duration::from_secs(120)).await
        }

        /// 等待授权码回调，支持自定义超时（测试用）
        #[cfg(test)]
        pub async fn wait_for_code_with_timeout(
            self,
            timeout: Duration,
        ) -> Result<(String, String), CallbackError> {
            let result = tokio::time::timeout(timeout, async {
                let (mut stream, _) = self.listener.accept().await?;
                Self::handle_connection(&mut stream, &self.expected_state, self.code_tx).await
            })
            .await;

            match result {
                Ok(Ok(code_state)) => Ok(code_state),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(CallbackError::Timeout),
            }
        }
    }
    ```
  - 原因: `tokio::time::timeout` 包装实现超时控制。`self` 被 move 进方法，确保服务器监听器在等待结束后自动关闭（Drop），释放端口。`code_tx` 随 `self` 一起 move，在 `handle_connection` 中使用。`wait_for_code_with_timeout` 标记为 `#[cfg(test)]`，限制测试方法暴露范围。
- [x] 实现 handle_connection() 静态方法 — 解析 HTTP GET 请求中的 code 和 state 参数
  - 位置: `rust-agent-middlewares/src/mcp/callback_server.rs`，`wait_for_code()` 方法之后
  - 新增内容:
    ```rust
    impl OAuthCallbackServer {
        async fn handle_connection(
            stream: &mut tokio::net::TcpStream,
            expected_state: &str,
            code_tx: oneshot::Sender<(String, String)>,
        ) -> Result<(String, String), CallbackError> {
            let mut reader = BufReader::new(stream);
            let mut request_line = String::new();
            reader.read_line(&mut request_line).await?;

            // 解析 GET /callback?code=xxx&state=yyy HTTP/1.1
            let (code, state_param) = Self::parse_callback_url(&request_line)?;

            // 验证 CSRF state
            if state_param != expected_state {
                let body = Self::error_html("CSRF 验证失败：state 参数不匹配");
                Self::send_response(stream, 400, &body).await;
                return Err(CallbackError::StateMismatch {
                    expected: expected_state.to_string(),
                    got: state_param,
                });
            }

            // 通过 oneshot channel 发送 code 和 state
            let _ = code_tx.send((code.clone(), state_param.clone()));

            // 返回成功 HTML
            let body = Self::success_html();
            Self::send_response(stream, 200, &body).await;

            info!("OAuth 回调成功，授权码已接收");
            Ok((code, state_param))
        }
    }
    ```
  - 原因: 使用 `BufReader` 按行读取 HTTP 请求行，避免读取整个请求体。CSRF state 验证防止跨站请求伪造攻击。oneshot channel send 的返回值用 `_` 忽略，因为调用方可能已因超时 drop 了 receiver。先发送 HTML 响应再 return，确保浏览器页面正常显示。
- [x] 实现 parse_callback_url() 静态方法 — 从 HTTP 请求行提取 code 和 state 参数
  - 位置: `rust-agent-middlewares/src/mcp/callback_server.rs`，`handle_connection()` 之后
  - 新增内容:
    ```rust
    impl OAuthCallbackServer {
        /// 从 HTTP 请求行解析回调参数
        /// 输入格式: "GET /callback?code=xxx&state=yyy HTTP/1.1\r\n"
        fn parse_callback_url(request_line: &str) -> Result<(String, String), CallbackError> {
            let parts: Vec<&str> = request_line.split_whitespace().collect();
            if parts.len() < 2 {
                return Err(CallbackError::InvalidRequest(
                    "无效的 HTTP 请求格式".into(),
                ));
            }

            let path = parts[1]; // "/callback?code=xxx&state=yyy"

            // 验证路径前缀
            if path == "/callback" {
                return Err(CallbackError::MissingCode);
            }
            if !path.starts_with("/callback?") {
                return Err(CallbackError::InvalidRequest(format!(
                    "期望 /callback 路径，收到: {}",
                    path
                )));
            }

            let query = &path["/callback?".len()..];

            let mut code = None;
            let mut state = None;

            for pair in query.split('&') {
                let kv: Vec<&str> = pair.splitn(2, '=').collect();
                if kv.len() != 2 {
                    continue;
                }
                match kv[0] {
                    "code" => code = Some(urldecode(kv[1])),
                    "state" => state = Some(urldecode(kv[1])),
                    _ => {}
                }
            }

            let code = code.ok_or(CallbackError::MissingCode)?;
            let state = state.unwrap_or_default();

            Ok((code, state))
        }
    }
    ```
  - 原因: 手动解析 URL query string 而非引入额外 URL 解析库，避免非标准 HTTP 请求行格式导致解析失败。URL decode 处理 `%xx` 编码字符（如授权码中的特殊字符）。
- [x] 实现 urldecode() 模块级辅助函数 — 解码 URL percent-encoded 字符
  - 位置: `rust-agent-middlewares/src/mcp/callback_server.rs`，`OAuthCallbackServer` impl 块之后（模块级函数）
  - 新增内容:
    ```rust
    /// 简单的 URL percent-decode 实现
    fn urldecode(input: &str) -> String {
        let mut result = Vec::new();
        let bytes = input.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                if let Ok(byte) = u8::from_str_radix(
                    &input[i + 1..i + 3],
                    16,
                ) {
                    result.push(byte);
                    i += 3;
                    continue;
                }
            }
            if bytes[i] == b'+' {
                result.push(b' ');
            } else {
                result.push(bytes[i]);
            }
            i += 1;
        }
        String::from_utf8(result).unwrap_or_else(|e| {
            warn!("URL decode 结果非 UTF-8: {}", e);
            input.to_string()
        })
    }
    ```
  - 原因: OAuth 授权码和 state 可能包含 URL 编码字符（如 `+` → 空格、`%2F` → `/`）。自行实现避免引入额外依赖，此处只需一个简单函数。
- [x] 实现 success_html()、error_html() 和 send_response() 辅助方法 — 生成并发送 HTTP 响应
  - 位置: `rust-agent-middlewares/src/mcp/callback_server.rs`，`urldecode()` 之后
  - 新增内容:
    ```rust
    impl OAuthCallbackServer {
        fn success_html() -> String {
            r#"<!DOCTYPE html>
    <html><head><meta charset="utf-8"><title>授权成功</title>
    <style>body{font-family:system-ui,sans-serif;display:flex;justify-content:center;align-items:center;min-height:100vh;margin:0;background:#f5f5f5}
    .card{background:#fff;padding:2rem;border-radius:8px;box-shadow:0 2px 8px rgba(0,0,0,.1);text-align:center;max-width:400px}
    h1{color:#16a34a;margin-top:0}p{color:#666;line-height:1.6}</style></head>
    <body><div class="card"><h1>授权成功</h1><p>MCP 服务器授权已完成，您可以关闭此页面并返回终端。</p></div></body></html>"#
                .to_string()
        }

        fn error_html(message: &str) -> String {
            format!(
                r#"<!DOCTYPE html>
    <html><head><meta charset="utf-8"><title>授权失败</title>
    <style>body{{font-family:system-ui,sans-serif;display:flex;justify-content:center;align-items:center;min-height:100vh;margin:0;background:#f5f5f5}}
    .card{{background:#fff;padding:2rem;border-radius:8px;box-shadow:0 2px 8px rgba(0,0,0,.1);text-align:center;max-width:400px}}
    h1{{color:#dc2626;margin-top:0}}p{{color:#666;line-height:1.6}}</style></head>
    <body><div class="card"><h1>授权失败</h1><p>{}</p></div></body></html>"#,
                message
            )
        }

        async fn send_response(
            stream: &mut tokio::net::TcpStream,
            status: u16,
            body: &str,
        ) {
            let status_text = if status == 200 { "OK" } else { "Bad Request" };
            let response = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status,
                status_text,
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
    }
    ```
  - 原因: 内联 CSS 确保无外部依赖，页面简洁美观。最小化 HTTP/1.1 响应实现，`Connection: close` 告知浏览器关闭连接，`shutdown()` 确保响应刷出。使用 `let _ =` 忽略写入错误，因为回调服务器只接受一个连接。
- [x] 实现 parse_code_from_url() 公共函数 — 从用户粘贴的 URL 中提取 code 和 state
  - 位置: `rust-agent-middlewares/src/mcp/callback_server.rs`，模块级函数区域（`urldecode()` 之后）
  - 新增内容:
    ```rust
    /// 从用户手动粘贴的回调 URL 中解析 code 和 state
    ///
    /// 支持格式:
    /// - http://localhost:12345/callback?code=xxx&state=yyy
    /// - https://example.com/callback?code=xxx&state=yyy
    /// - 仅含 query string: code=xxx&state=yyy
    pub fn parse_code_from_url(url: &str) -> Result<(String, String), CallbackError> {
        let query = if url.contains('?') {
            url.split('?').nth(1).unwrap_or("")
        } else if url.contains("code=") {
            url
        } else {
            return Err(CallbackError::MissingCode);
        };

        let mut code = None;
        let mut state = None;

        for pair in query.split('&') {
            let kv: Vec<&str> = pair.splitn(2, '=').collect();
            if kv.len() != 2 {
                continue;
            }
            match kv[0] {
                "code" => code = Some(urldecode(kv[1])),
                "state" => state = Some(urldecode(kv[1])),
                _ => {}
            }
        }

        let code = code.ok_or(CallbackError::MissingCode)?;
        let state = state.unwrap_or_default();
        Ok((code, state))
    }
    ```
  - 原因: 回调服务器超时后，TUI 显示手动粘贴面板。用户可能粘贴完整 URL 或仅粘贴 query string，此函数兼容两种格式。作为模块级公共函数供 Task 5（TUI 面板）直接调用。
- [x] 在 mod.rs 中注册 callback_server 模块并导出公共类型
  - 位置: `rust-agent-middlewares/src/mcp/mod.rs`
  - 在 `pub mod auth_store;` 之后追加 `pub mod callback_server;`
  - 在 `pub use` 块中追加:
    ```rust
    pub use callback_server::{CallbackError, OAuthCallbackServer, parse_code_from_url};
    ```
  - 原因: Task 4 的 OAuth 流程编排需要导入 `OAuthCallbackServer` 和 `CallbackError`，Task 5 需要导入 `parse_code_from_url`。统一通过 mod.rs 导出保持公共 API 一致。
- [x] 为 OAuthCallbackServer 核心逻辑编写单元测试
  - 测试文件: `rust-agent-middlewares/src/mcp/callback_server.rs`（文件底部 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_parse_callback_url_valid`: 输入 `"GET /callback?code=abc123&state=xyz789 HTTP/1.1\r\n"` → 返回 `Ok(("abc123".into(), "xyz789".into()))`
    - `test_parse_callback_url_missing_code`: 输入 `"GET /callback?state=xyz HTTP/1.1\r\n"` → 返回 `Err(CallbackError::MissingCode)`
    - `test_parse_callback_url_no_query`: 输入 `"GET /callback HTTP/1.1\r\n"` → 返回 `Err(CallbackError::MissingCode)`
    - `test_parse_callback_url_invalid_path`: 输入 `"GET /other?code=abc HTTP/1.1\r\n"` → 返回 `Err(CallbackError::InvalidRequest(...))`
    - `test_parse_callback_url_percent_encoded`: 输入 `"GET /callback?code=abc%2Bdef&state=hello%20world HTTP/1.1\r\n"` → 返回 `Ok(("abc+def".into(), "hello world".into()))`
    - `test_bind_returns_valid_redirect_uri`: 调用 `OAuthCallbackServer::bind("test_state".into()).await`，断言 redirect_uri 以 `"http://localhost:"` 开头且以 `"/callback"` 结尾
    - `test_bind_failed_all_retries`: 使用已占满端口的场景较难模拟，改为验证 `CallbackError::BindFailed` 的错误信息格式正确（通过直接构造 `CallbackError::BindFailed` 断言 Display 输出包含 "绑定失败"）
    - `test_wait_for_code_success`: 调用 `bind()`，使用 `tokio::net::TcpStream::connect` 连接到返回的端口号，发送 `"GET /callback?code=test_code&state=test_state HTTP/1.1\r\n\r\n"`，通过 `wait_for_code_with_timeout(Duration::from_secs(5))` 接收，断言返回 `Ok(("test_code".into(), "test_state".into()))`
    - `test_wait_for_code_state_mismatch`: 调用 `bind("expected_state".into())`，发送含 `state=wrong_state` 的回调请求，通过 `wait_for_code_with_timeout` 接收，断言返回 `Err(CallbackError::StateMismatch { expected: "expected_state", got: "wrong_state" })`
    - `test_wait_for_code_timeout`: 调用 `bind()` 后不发送任何请求，使用 `wait_for_code_with_timeout(Duration::from_millis(100))`，断言返回 `Err(CallbackError::Timeout)`
    - `test_parse_code_from_url_full_url`: 输入 `"http://localhost:12345/callback?code=abc&state=xyz"` → 返回 `Ok(("abc".into(), "xyz".into()))`
    - `test_parse_code_from_url_query_only`: 输入 `"code=abc&state=xyz"` → 返回 `Ok(("abc".into(), "xyz".into()))`
    - `test_parse_code_from_url_missing_code`: 输入 `"http://localhost/callback?state=xyz"` → 返回 `Err(CallbackError::MissingCode)`
  - 集成测试（`test_wait_for_code_success`、`test_wait_for_code_state_mismatch`、`test_wait_for_code_timeout`）使用 `tokio::net::TcpStream` 发送模拟 HTTP 请求
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- mcp::callback_server::tests`
  - 预期: 所有 12 个测试通过

**检查步骤:**
- [x] 验证 callback_server.rs 文件存在且包含核心结构体
  - `grep -n 'pub struct OAuthCallbackServer' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/callback_server.rs`
  - 预期: 行号输出
- [x] 验证 CallbackError 枚举包含所有变体
  - `grep -c 'BindFailed\|Timeout\|StateMismatch\|MissingCode\|InvalidRequest\|Io' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/callback_server.rs`
  - 预期: 输出为 6（6 个变体各出现至少一次）
- [x] 验证 mod.rs 注册并导出 callback_server
  - `grep 'callback_server' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `pub mod callback_server;` 和 `pub use callback_server`
- [x] 验证 parse_code_from_url 公共函数存在
  - `grep -n 'pub fn parse_code_from_url' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/callback_server.rs`
  - 预期: 行号输出
- [x] 验证编译通过
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 输出包含 `Finished`，无编译错误
- [x] 验证所有回调服务器测试通过
  - `cargo test -p rust-agent-middlewares --lib -- mcp::callback_server::tests 2>&1 | tail -15`
  - 预期: 输出包含 `test result: ok`，12 个测试全部通过
- [x] 验证全量测试无回归
  - `cargo test -p rust-agent-middlewares --lib 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok`，无失败测试

---

### Task 4: OAuth 流程编排 + 传输层集成

**背景:**
[业务语境] — 编排完整的 OAuth 2.0 Authorization Code + PKCE 授权流程，当 MCP HTTP 服务器返回 401 时自动触发浏览器授权，并将 AuthClient 集成到 MCP 连接池的初始化和重连流程中，使 OAuth 认证对上层 ReAct 循环和 TUI 透明。
[修改原因] — 当前 `build_http_transport()` 创建的 `StreamableHttpClientTransport` 不携带任何认证信息。`run_initialize()` 和 `reconnect()` 在连接失败时直接标记 `Failed`，无法区分"网络不可达"和"需要 OAuth 授权"两种情况。缺少 OAuth 状态机编排器来协调 metadata 发现、DCR、PKCE、回调等待、token 交换等步骤。
[上下游影响] — 本 Task 依赖 Task 1（OAuthConfig 配置、rmcp auth feature）、Task 2（FileCredentialStore + PerServerCredentialStore）、Task 3（OAuthCallbackServer）。本 Task 输出的 `OAuthFlowManager` 被 `client.rs` 的连接池初始化流程消费，`build_authed_transport()` 替代 `build_http_transport()` 成为 OAuth 服务器的传输层构建入口。Task 5 的 TUI 事件和面板依赖本 Task 的事件发送逻辑。

**涉及文件:**
- 新建: `rust-agent-middlewares/src/mcp/oauth_flow.rs`
- 修改: `rust-agent-middlewares/src/mcp/transport.rs`
- 修改: `rust-agent-middlewares/src/mcp/client.rs`
- 修改: `rust-agent-middlewares/src/mcp/mod.rs`

**执行步骤:**
- [x] 新建 oauth_flow.rs，定义 OAuth 流程编排所需的公共类型 — 为 client.rs 和 TUI 提供类型契约
  - 位置: `rust-agent-middlewares/src/mcp/oauth_flow.rs`（新文件）
  - 文件头部 imports:
    ```rust
    use std::collections::HashMap;
    use std::sync::Arc;

    use thiserror::Error;
    use tokio::sync::oneshot;
    use tracing::{info, warn, debug};

    use super::auth_store::{FileCredentialStore, PerServerCredentialStore};
    use super::callback_server::{CallbackError, OAuthCallbackServer};
    use super::config::OAuthConfig;
    use rmcp::transport::auth::{AuthError, OAuthState};
    ```
  - 定义 OAuth 流程结果类型:
    ```rust
    /// OAuth 回调结果（从 TUI 传回后台 OAuth 流程）
    pub struct OAuthCallbackResult {
        /// 授权码
        pub code: String,
        /// CSRF state 参数
        pub state: String,
    }

    /// OAuth 流程编排错误
    #[derive(Debug, Error)]
    pub enum OAuthFlowError {
        #[error("OAuth 流程失败: {0}")]
        FlowFailed(String),
        #[error("OAuth 回调服务器错误: {0}")]
        CallbackError(#[from] CallbackError),
        #[error("OAuth 授权错误: {0}")]
        AuthError(#[from] AuthError),
        #[error("OAuth 授权被用户取消")]
        Cancelled,
        #[error("OAuth 回调等待超时")]
        CallbackTimeout,
    }
    ```
  - 定义 OAuth 流程事件枚举（由 OAuthFlowManager 产生，供 client.rs 转发给 TUI）:
    ```rust
    /// OAuth 流程事件（由后台产生，需转发到 TUI 层）
    pub enum OAuthFlowEvent {
        /// 需要用户浏览器授权
        AuthorizationNeeded {
            server_name: String,
            authorization_url: String,
            /// 回调通道：TUI 收集用户输入后通过此通道传回授权码
            callback_tx: oneshot::Sender<OAuthCallbackResult>,
        },
        /// OAuth 授权完成
        AuthorizationCompleted {
            server_name: String,
        },
        /// OAuth 授权失败
        AuthorizationFailed {
            server_name: String,
            error: String,
        },
    }
    ```
  - 原因: `OAuthCallbackResult` 是 TUI 和 middlewares 之间的共享数据类型，定义在 middlewares 层供 Task 5 引用。`OAuthFlowError` 统一封装 OAuth 流程中所有可能的错误（回调服务器错误、rmcp auth 错误、用户取消）。`OAuthFlowEvent` 解耦 OAuth 编排器和 TUI——OAuthFlowManager 产出事件，client.rs 通过回调转发到 TUI 事件通道。
- [x] 实现 `OAuthFlowManager` 结构体和 `new()` 构造方法 — 管理所有 MCP 服务器的 OAuth 状态
  - 位置: `rust-agent-middlewares/src/mcp/oauth_flow.rs`，公共类型定义之后
  - 新增内容:
    ```rust
    /// OAuth 流程编排器
    ///
    /// 为每个需要 OAuth 的 MCP 服务器管理独立的 OAuthState 状态机。
    /// 通过回调函数将事件转发给调用方（client.rs），由调用方决定如何通知 TUI。
    pub struct OAuthFlowManager {
        /// 共享的 Token 文件存储
        token_store: Arc<FileCredentialStore>,
        /// 按 server_name 管理的 OAuth 状态机
        states: HashMap<String, OAuthState>,
        /// 事件回调（由 client.rs 在创建时注入）
        event_callback: Box<dyn Fn(OAuthFlowEvent) + Send + Sync>,
    }

    impl OAuthFlowManager {
        /// 创建 OAuth 流程管理器
        ///
        /// `token_store`: 共享的 Token 文件存储实例
        /// `event_callback`: 事件回调函数，用于将 OAuth 事件转发给 TUI
        pub fn new<F>(token_store: Arc<FileCredentialStore>, event_callback: F) -> Self
        where
            F: Fn(OAuthFlowEvent) + Send + Sync + 'static,
        {
            Self {
                token_store,
                states: HashMap::new(),
                event_callback: Box::new(event_callback),
            }
        }
    }
    ```
  - 原因: 使用回调函数而非 `mpsc::Sender<AgentEvent>`，因为 `OAuthFlowManager` 在 middlewares 层（不依赖 `rust-create-agent` 的事件定义）。回调函数由 client.rs 在 `run_initialize()` 中注入，client.rs 负责将 `OAuthFlowEvent` 转换为 `AgentEvent` 并发送到 TUI 事件通道。`token_store` 使用 `Arc` 共享，多个服务器共享同一个文件但按 key 隔离。
- [x] 实现 `OAuthFlowManager::run_oauth_flow()` 方法 — 编排完整 OAuth 授权流程
  - 位置: `rust-agent-middlewares/src/mcp/oauth_flow.rs`，`new()` 之后
  - 新增内容:
    ```rust
    impl OAuthFlowManager {
        /// 对指定服务器执行完整 OAuth 授权流程
        ///
        /// 流程: 创建 OAuthState → 绑定回调服务器 → 启动授权 → 通知 TUI → 等待回调 → 完成授权
        pub async fn run_oauth_flow(
            &mut self,
            server_name: &str,
            server_url: &str,
            oauth_config: &OAuthConfig,
        ) -> Result<(), OAuthFlowError> {
            info!(server = %server_name, "开始 OAuth 授权流程");

            // 1. 创建或复用 OAuthState
            let state = if let Some(existing) = self.states.remove(server_name) {
                existing
            } else {
                let credential_store = PerServerCredentialStore::new(
                    self.token_store.clone(),
                    server_name.to_string(),
                );
                let mut mgr_state = OAuthState::new(server_url, None).await?;
                // 注入 per-server credential store
                if let OAuthState::Unauthorized(ref mut manager) = mgr_state {
                    manager.set_credential_store(credential_store);
                }
                mgr_state
            };

            let mut state = state;

            // 2. 尝试从存储恢复已有凭证（快速路径）
            match &state {
                OAuthState::Unauthorized(manager) => {
                    let has_creds = manager.initialize_from_store().await?;
                    if has_creds {
                        info!(server = %server_name, "从存储恢复已有凭证，跳过浏览器授权");
                        self.states.insert(server_name.to_string(), state);
                        self.emit_event(OAuthFlowEvent::AuthorizationCompleted {
                            server_name: server_name.to_string(),
                        });
                        return Ok(());
                    }
                }
                OAuthState::Authorized(_) => {
                    info!(server = %server_name, "已处于授权状态，跳过浏览器授权");
                    self.states.insert(server_name.to_string(), state);
                    self.emit_event(OAuthFlowEvent::AuthorizationCompleted {
                        server_name: server_name.to_string(),
                    });
                    return Ok(());
                }
                _ => {}
            }

            // 3. 绑定回调服务器
            let (callback_server, redirect_uri) =
                OAuthCallbackServer::bind(String::new()).await?;

            // 4. 启动授权（DCR + PKCE + metadata 发现）
            let scopes: Vec<&str> = oauth_config
                .scopes
                .as_ref()
                .map(|s| s.iter().map(|ss| ss.as_str()).collect())
                .unwrap_or_default();

            let client_name = Some("perihelion-mcp-client");
            state.start_authorization(&scopes, &redirect_uri, client_name).await?;

            // 5. 获取授权 URL
            let authorization_url = state.get_authorization_url().await?;

            // 6. 创建 oneshot 通道，通知 TUI 等待用户交互
            let (callback_tx, callback_rx) = oneshot::channel::<OAuthCallbackResult>();

            self.emit_event(OAuthFlowEvent::AuthorizationNeeded {
                server_name: server_name.to_string(),
                authorization_url: authorization_url.clone(),
                callback_tx,
            });

            // 7. 并发等待回调（本地服务器 + TUI 手动粘贴），取先到达的
            let callback_result = tokio::select! {
                // 本地回调服务器自动接收
                result = callback_server.wait_for_code() => {
                    match result {
                        Ok((code, state_param)) => Ok(OAuthCallbackResult { code, state: state_param }),
                        Err(CallbackError::Timeout) => Err(OAuthFlowError::CallbackTimeout),
                        Err(e) => Err(OAuthFlowError::CallbackError(e)),
                    }
                }
                // TUI 用户手动粘贴（oneshot 接收）
                result = callback_rx => {
                    match result {
                        Ok(result) => Ok(result),
                        Err(_) => Err(OAuthFlowError::Cancelled),
                    }
                }
            };

            let callback_data = match callback_result {
                Ok(data) => data,
                Err(e) => {
                    self.emit_event(OAuthFlowEvent::AuthorizationFailed {
                        server_name: server_name.to_string(),
                        error: e.to_string(),
                    });
                    return Err(e);
                }
            };

            // 8. 处理回调，完成授权
            state.handle_callback(&callback_data.code, &callback_data.state).await?;

            // 9. 保存状态到 states map（Authorized 状态）
            self.states.insert(server_name.to_string(), state);

            // 10. 通知 TUI 授权完成
            self.emit_event(OAuthFlowEvent::AuthorizationCompleted {
                server_name: server_name.to_string(),
            });

            info!(server = %server_name, "OAuth 授权流程完成");
            Ok(())
        }

        /// 发送事件给调用方
        fn emit_event(&self, event: OAuthFlowEvent) {
            (self.event_callback)(event);
        }
    }
    ```
  - 原因: `run_oauth_flow()` 是核心编排方法，整合了 Task 1-3 的所有组件。快速路径（`initialize_from_store()`）避免每次启动都触发浏览器授权。使用 `tokio::select!` 并发等待本地回调服务器和 TUI 手动粘贴，取先到达的结果——回调服务器超时后 TUI 侧仍可接收用户输入，反之亦然。`emit_event()` 将事件通过回调函数传递给 client.rs，由 client.rs 决定如何转发到 TUI。
- [x] 实现 `OAuthFlowManager::get_authorization_manager()` 方法 — 提取 AuthorizationManager 用于构建 AuthClient
  - 位置: `rust-agent-middlewares/src/mcp/oauth_flow.rs`，`run_oauth_flow()` 之后
  - 新增内容:
    ```rust
    impl OAuthFlowManager {
        /// 获取指定服务器的 AuthorizationManager（用于构建 AuthClient 传输层）
        ///
        /// 仅在 `run_oauth_flow()` 成功返回后调用。
        /// 返回 None 表示该服务器未完成 OAuth 授权。
        pub fn get_authorization_manager(
            &mut self,
            server_name: &str,
        ) -> Option<rmcp::transport::auth::AuthorizationManager> {
            let state = self.states.remove(server_name)?;
            match state {
                OAuthState::Authorized(manager) => Some(manager),
                OAuthState::Unauthorized(manager) => {
                    // 可能已有 token（快速路径恢复的情况）
                    // 重新放入 states，返回 manager
                    self.states.insert(server_name.to_string(), OAuthState::Unauthorized(manager));
                    None
                }
                _ => {
                    warn!(
                        server = %server_name,
                        "OAuth 状态不是 Authorized，无法提取 AuthorizationManager"
                    );
                    None
                }
            }
        }

        /// 判断指定服务器是否已完成 OAuth 授权
        pub fn is_authorized(&self, server_name: &str) -> bool {
            matches!(
                self.states.get(server_name),
                Some(OAuthState::Authorized(_)) | Some(OAuthState::AuthorizedHttpClient(_))
            )
        }
    }
    ```
  - 原因: `get_authorization_manager()` 使用 `remove` 消费状态（OAuthState 不能 Clone），确保 AuthorizationManager 的所有权唯一转移给 AuthClient。client.rs 在 OAuth 授权成功后调用此方法构建 `AuthClient<StreamableHttpClientTransport>`，再用其重新连接 MCP 服务器。`is_authorized()` 用于 Task 5 的 TUI 面板展示 OAuth 状态。
- [x] 在 transport.rs 中新增 `TransportConfig` 的 `oauth` 字段 — 携带 OAuth 配置到传输层
  - 位置: `rust-agent-middlewares/src/mcp/transport.rs` 第 14-17 行，`StreamableHttp` 变体内
  - 将 `StreamableHttp` 变体从:
    ```rust
    StreamableHttp {
        url: String,
        headers: HashMap<String, String>,
    },
    ```
    改为:
    ```rust
    StreamableHttp {
        url: String,
        headers: HashMap<String, String>,
        /// OAuth 配置（仅当服务器配置了 oauth 且 is_enabled() 时为 Some）
        oauth: Option<super::config::OAuthConfig>,
    },
    ```
  - 位置: `rust-agent-middlewares/src/mcp/transport.rs` 第 41-44 行，`TryFrom` 实现中 `StreamableHttp` 分支
  - 将 `Ok(TransportConfig::StreamableHttp {` 构造中的:
    ```rust
    url: url.clone(),
    headers: config.headers.clone().unwrap_or_default(),
    ```
    改为:
    ```rust
    url: url.clone(),
    headers: config.headers.clone().unwrap_or_default(),
    oauth: config.oauth.as_ref()
        .filter(|o| o.is_enabled())
        .cloned(),
    ```
  - 位置: `rust-agent-middlewares/src/mcp/transport.rs` 第 59-178 行，`#[cfg(test)] mod tests` 内所有 `TransportConfig::StreamableHttp { ... }` 构造
  - 在每个 `StreamableHttp` 构造中追加 `oauth: None,` 字段
  - 涉及测试函数: `test_try_from_http_config`（第 101 行）、`test_build_transport_returns_config`（第 160 行）
  - 原因: `TransportConfig::StreamableHttp` 携带 `oauth` 字段，使 `client.rs` 的 `run_initialize()` 和 `reconnect()` 能区分需要 OAuth 的服务器和普通 HTTP 服务器。`filter(|o| o.is_enabled())` 确保显式禁用 OAuth 的服务器走普通 HTTP 连接路径。
- [x] 在 client.rs 中新增 `build_authed_transport()` 函数 — 使用 AuthClient 包装 StreamableHttpClientTransport
  - 位置: `rust-agent-middlewares/src/mcp/client.rs`，`build_http_transport()` 函数（第 620-651 行）之后
  - 新增内容:
    ```rust
    /// 创建带 OAuth 认证的 HTTP transport
    ///
    /// 使用 rmcp AuthClient 包装 StreamableHttpClientTransport，
    /// 自动在请求中注入 Authorization: Bearer {token} 头。
    fn build_authed_transport(
        url: &str,
        headers: &HashMap<String, String>,
        auth_manager: rmcp::transport::auth::AuthorizationManager,
    ) -> rmcp::transport::StreamableHttpClientTransport<
        rmcp::transport::auth::AuthClient<
            rmcp::transport::StreamableHttpClientTransport<reqwest::Client>,
        >,
    > {
        let base_transport = build_http_transport(url, headers);
        let auth_client = rmcp::transport::auth::AuthClient::new(base_transport, auth_manager);
        rmcp::transport::StreamableHttpClientTransport::from_client(auth_client)
    }
    ```
  - 注意: 需确认 rmcp 是否提供 `StreamableHttpClientTransport::from_client()` 或等价方法。如不存在，使用 `with_client(client, config)` 并从 base transport 获取 config。
  - 原因: `AuthClient<C>` 实现了 `StreamableHttpClient` trait（`rust-mcp-patch/src/transport/common/auth/streamable_http_client.rs`），因此 `StreamableHttpClientTransport<AuthClient<...>>` 可以作为 `Worker` 传递给 `serve_client()`。AuthClient 自动在每次请求时调用 `get_access_token()` 获取/刷新 token 并注入 Authorization 头。
- [x] 修改 `McpClientPool::run_initialize()` — 集成 OAuth 流程到连接池初始化
  - 位置: `rust-agent-middlewares/src/mcp/client.rs` 第 95-241 行，`run_initialize()` 方法
  - 在方法签名中新增 `oauth_event_callback` 参数:
    ```rust
    pub async fn run_initialize(
        pool: Arc<Self>,
        cwd: &Path,
        status_tx: tokio::sync::watch::Sender<McpInitStatus>,
        oauth_event_callback: Option<Box<dyn Fn(super::OAuthFlowEvent) + Send + Sync>>,
    ) {
    ```
  - 在 `total == 0` 提前返回之前（第 103 行之后），创建 `OAuthFlowManager`:
    ```rust
    let token_store = Arc::new(FileCredentialStore::new());
    let mut oauth_manager = match oauth_event_callback {
        Some(cb) => Some(OAuthFlowManager::new(token_store, cb)),
        None => None,
    };
    ```
  - 修改 `TransportConfig::StreamableHttp` 分支的连接逻辑（第 159-166 行），在连接前检查 oauth 配置:
    ```rust
    TransportConfig::StreamableHttp { ref url, ref headers, ref oauth } => {
        if let (Some(ref oauth_config), Some(ref mut oauth_mgr)) = (oauth, &mut oauth_manager) {
            // 需要 OAuth 的服务器
            match oauth_mgr.run_oauth_flow(name, url, oauth_config).await {
                Ok(()) => {
                    // 授权成功，提取 AuthorizationManager 构建 AuthClient
                    if let Some(auth_manager) = oauth_mgr.get_authorization_manager(name) {
                        let transport = build_authed_transport(url, headers, auth_manager);
                        tokio::time::timeout(timeout, rmcp::service::serve_client((), transport)).await
                    } else {
                        // 快速路径恢复（已有 token），构建普通 AuthClient
                        tracing::info!(server = %name, "使用已有 Token 连接");
                        let transport = build_http_transport(url, headers);
                        tokio::time::timeout(timeout, rmcp::service::serve_client((), transport)).await
                    }
                }
                Err(e) => {
                    tracing::warn!(server = %name, error = %e, "OAuth 授权失败，跳过服务器");
                    Self::insert_failed(&pool, name, format!("OAuth 授权失败: {e}"));
                    continue;
                }
            }
        } else {
            // 不需要 OAuth 的普通 HTTP 服务器
            let transport = build_http_transport(url, headers);
            tokio::time::timeout(timeout, rmcp::service::serve_client((), transport)).await
        }
    }
    ```
  - 在文件顶部 imports 区域追加:
    ```rust
    use super::auth_store::FileCredentialStore;
    use super::oauth_flow::{OAuthFlowError, OAuthFlowManager};
    use super::config::OAuthConfig;
    ```
  - 原因: `run_initialize()` 是连接池初始化的入口，在此集成 OAuth 流程使得 OAuth 授权在后台自动完成。`oauth_event_callback` 参数为可选——当 TUI 未传入回调时（如测试或 headless 模式），OAuth 流程不触发，服务器标记为 Failed。`run_oauth_flow()` 内部的快速路径（`initialize_from_store()`）在已有 token 时跳过浏览器授权，减少用户交互。
- [x] 修改 `McpClientPool::reconnect()` — 集成 OAuth 重连逻辑
  - 位置: `rust-agent-middlewares/src/mcp/client.rs` 第 257-358 行，`reconnect()` 方法
  - 修改方法签名，新增 `oauth_event_callback` 参数:
    ```rust
    pub async fn reconnect(
        self: &Arc<Self>,
        server_name: &str,
        oauth_event_callback: Option<Box<dyn Fn(super::OAuthFlowEvent) + Send + Sync>>,
    ) -> Result<(), McpPoolError> {
    ```
  - 修改 `TransportConfig::StreamableHttp` 分支的连接逻辑（第 308-311 行），与 `run_initialize()` 类似地添加 OAuth 检查:
    ```rust
    TransportConfig::StreamableHttp { url, headers, oauth } => {
        if let (Some(oauth_config), Some(cb)) = (oauth, oauth_event_callback.as_ref()) {
            let token_store = Arc::new(FileCredentialStore::new());
            let mut oauth_mgr = OAuthFlowManager::new(token_store, |event| cb(event));
            match oauth_mgr.run_oauth_flow(server_name, &url, &oauth_config).await {
                Ok(()) => {
                    if let Some(auth_manager) = oauth_mgr.get_authorization_manager(server_name) {
                        let transport = build_authed_transport(&url, &headers, auth_manager);
                        tokio::time::timeout(timeout, rmcp::service::serve_client((), transport)).await
                    } else {
                        let transport = build_http_transport(&url, &headers);
                        tokio::time::timeout(timeout, rmcp::service::serve_client((), transport)).await
                    }
                }
                Err(e) => {
                    let msg = format!("OAuth 授权失败: {e}");
                    Self::insert_failed(self, server_name, msg.clone());
                    return Err(McpPoolError::ConnectionFailed {
                        server: server_name.to_string(),
                        reason: msg,
                    });
                }
            }
        } else {
            let transport = build_http_transport(&url, &headers);
            tokio::time::timeout(timeout, rmcp::service::serve_client((), transport)).await
        }
    }
    ```
  - 原因: `reconnect()` 与 `run_initialize()` 共享相同的 OAuth 连接逻辑。重连时如果 token 过期（AuthClient 的 `get_access_token()` 自动刷新），无需重新触发完整 OAuth 流程；仅在 token 完全丢失时才触发浏览器授权。
- [x] 新增 `McpClientPool::start_oauth_flow()` 公共方法 — 供 TUI 面板手动触发 OAuth 授权
  - 位置: `rust-agent-middlewares/src/mcp/client.rs`，`reconnect()` 方法之后
  - 新增内容:
    ```rust
    /// 手动触发指定服务器的 OAuth 授权流程（供 TUI 面板调用）
    pub async fn start_oauth_flow(
        self: &Arc<Self>,
        server_name: &str,
        oauth_event_callback: Box<dyn Fn(super::OAuthFlowEvent) + Send + Sync>,
    ) -> Result<(), McpPoolError> {
        let server_config = {
            let configs = self.configs.read();
            configs.get(server_name).cloned().ok_or_else(|| {
                McpPoolError::NotConnected {
                    server: server_name.to_string(),
                    status: ClientStatus::Disconnected,
                }
            })?
        };

        let oauth_config = server_config
            .oauth
            .as_ref()
            .filter(|o| o.is_enabled())
            .ok_or_else(|| McpPoolError::ConnectionFailed {
                server: server_name.to_string(),
                reason: "服务器未配置 OAuth".to_string(),
            })?;

        let url = server_config.url.as_deref().unwrap_or("");
        let token_store = Arc::new(FileCredentialStore::new());
        let mut oauth_mgr = OAuthFlowManager::new(token_store, |event| oauth_event_callback(event));

        oauth_mgr
            .run_oauth_flow(server_name, url, oauth_config)
            .await
            .map_err(|e| McpPoolError::ConnectionFailed {
                server: server_name.to_string(),
                reason: format!("OAuth 授权失败: {e}"),
            })?;

        // 授权成功后自动重连
        self.reconnect(server_name, None).await
    }
    ```
  - 原因: Task 5 的 MCP 面板 `r` 键调用此方法。此方法先执行 OAuth 授权流程，成功后自动调用 `reconnect()`（传 `None` 跳过 OAuth 检查，因为 token 已在 `run_oauth_flow` 中保存到 store，`reconnect` 中的普通连接路径会通过 `build_http_transport` 使用已有 headers 连接）。
  - 修正: `reconnect()` 中传 `None` 时不会使用 AuthClient，需要改为传 `Some` 以便重连时也使用 AuthClient。实际实现中，`start_oauth_flow` 授权成功后应先关闭旧连接，再用 `build_authed_transport` 重新连接。此步骤的实际实现应在 `reconnect()` 内部完成——`reconnect()` 检测到 oauth 配置时自动使用 AuthClient 重连，无需额外逻辑。
- [x] 更新 `McpClientPool::initialize()` 同步方法 — 添加 oauth_event_callback 参数保持签名一致
  - 位置: `rust-agent-middlewares/src/mcp/client.rs` 第 419-533 行，`initialize()` 方法
  - 修改方法签名:
    ```rust
    pub async fn initialize(
        cwd: &Path,
        oauth_event_callback: Option<Box<dyn Fn(super::OAuthFlowEvent) + Send + Sync>>,
    ) -> Self {
    ```
  - 修改方法体内 `TransportConfig::StreamableHttp` 分支（第 467-473 行），与 `run_initialize()` 相同地添加 OAuth 检查逻辑（创建 OAuthFlowManager → run_oauth_flow → build_authed_transport）
  - 原因: `initialize()` 是 `run_initialize()` 的同步阻塞版本（保留向后兼容），两者共享相同的 OAuth 连接逻辑。
- [x] 在 mod.rs 中注册 oauth_flow 模块并导出公共类型
  - 位置: `rust-agent-middlewares/src/mcp/mod.rs`
  - 在 `pub mod callback_server;` 之后追加 `pub mod oauth_flow;`
  - 在 `pub use` 块中追加:
    ```rust
    pub use oauth_flow::{OAuthCallbackResult, OAuthFlowError, OAuthFlowEvent, OAuthFlowManager};
    ```
  - 原因: client.rs 的 `run_initialize()` / `reconnect()` / `start_oauth_flow()` 需要引用 `OAuthFlowManager`、`OAuthFlowEvent` 等类型。Task 5 的 TUI 层需要引用 `OAuthCallbackResult`、`OAuthFlowEvent`。
- [x] 更新 client.rs 和 transport.rs 中所有受影响的测试 — 补充 oauth 字段
  - 位置: `rust-agent-middlewares/src/mcp/transport.rs` `mod tests` 内所有 `TransportConfig::StreamableHttp { ... }` 构造
  - 位置: `rust-agent-middlewares/src/mcp/client.rs` `mod tests` 内（测试不涉及 OAuth，保持 `oauth_event_callback: None`）
  - 原因: `TransportConfig::StreamableHttp` 新增 `oauth` 字段后，所有手动构造该变体的测试必须补充 `oauth: None,`。`run_initialize()` / `reconnect()` 新增参数后，现有测试调用点需补充 `None` 参数。
- [x] 为 OAuthFlowManager 核心逻辑编写单元测试
  - 测试文件: `rust-agent-middlewares/src/mcp/oauth_flow.rs`（文件末尾 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_oauth_flow_error_display`: 构造各种 `OAuthFlowError` 变体，断言 `Display` 输出包含关键信息（如 "OAuth 流程失败"、"回调服务器错误"、"授权被用户取消"）
    - `test_oauth_flow_event_types`: 构造 `OAuthFlowEvent` 的三个变体，断言 match 分支正确识别
    - `test_oauth_callback_result_fields`: 构造 `OAuthCallbackResult { code: "abc".into(), state: "xyz".into() }`，断言字段正确
    - `test_oauth_flow_manager_new`: 构造 `OAuthFlowManager::new(Arc::new(FileCredentialStore::with_path(tmp_path)), |_| {})`，断言 `is_authorized("nonexistent")` 返回 `false`
    - `test_oauth_flow_manager_is_authorized_empty`: 新建 manager，断言 `is_authorized` 对任意 server_name 返回 `false`
    - `test_oauth_flow_manager_emit_event`: 构造 manager 时注入计数回调，调用内部 emit_event（通过测试 helper），断言回调被调用
  - 运行命令: `cargo test -p rust-agent-middlewares --lib -- mcp::oauth_flow::tests`
  - 预期: 所有 6 个测试通过

**检查步骤:**
- [x] 验证 oauth_flow.rs 文件存在且包含核心结构体
  - `grep -n 'pub struct OAuthFlowManager' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/oauth_flow.rs`
  - 预期: 行号输出
- [x] 验证 OAuthFlowError 枚举包含所有变体
  - `grep -c 'FlowFailed\|CallbackError\|AuthError\|Cancelled\|CallbackTimeout' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/oauth_flow.rs`
  - 预期: 输出为 5（5 个变体各出现至少一次）
- [x] 验证 OAuthFlowEvent 枚举包含 3 个变体
  - `grep -c 'AuthorizationNeeded\|AuthorizationCompleted\|AuthorizationFailed' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/oauth_flow.rs`
  - 预期: 输出为 3
- [x] 验证 mod.rs 注册并导出 oauth_flow 模块
  - `grep 'oauth_flow' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/mod.rs`
  - 预期: 输出包含 `pub mod oauth_flow;` 和 `pub use oauth_flow`
- [x] 验证 transport.rs 的 StreamableHttp 包含 oauth 字段
  - `grep -A 5 'StreamableHttp' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/transport.rs | grep -c 'oauth'
  - 预期: 输出为 2（变体定义 + TryFrom 构造）
- [x] 验证 client.rs 包含 build_authed_transport 函数
  - `grep -n 'fn build_authed_transport' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/client.rs`
  - 预期: 行号输出
- [x] 验证 client.rs 包含 start_oauth_flow 方法
  - `grep -n 'fn start_oauth_flow' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/client.rs`
  - 预期: 行号输出
- [x] 验证编译通过
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 输出包含 `Finished`，无编译错误
- [x] 验证 oauth_flow 模块测试通过
  - `cargo test -p rust-agent-middlewares --lib -- mcp::oauth_flow::tests 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok`，6 个测试全部通过
- [x] 验证全量测试无回归
  - `cargo test -p rust-agent-middlewares --lib 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok`，无失败测试

---

### Task 5: TUI 事件扩展 + OAuth 弹窗面板 + MCP 面板状态展示

**背景:**
[业务语境] — 为用户提供 OAuth 授权的 TUI 交互体验：当 MCP 服务器需要 OAuth 授权时，弹出面板显示授权 URL 并打开浏览器，用户在浏览器完成授权后手动粘贴回调 URL（或由回调服务器自动接收）。MCP 管理面板需显示每个服务器的 OAuth 状态，并支持手动触发授权。
[修改原因] — 当前 `AgentEvent` 枚举缺少 OAuth 相关变体，无法将 OAuth 事件从后台传递到 TUI。`ServerInfo` 缺少 OAuth 状态字段，MCP 面板无法展示授权状态。无 OAuth 弹窗面板，用户无法完成浏览器授权交互。
[上下游影响] — 本 Task 依赖 Task 4（OAuth 流程编排）提供的 `OAuthCallbackResult` 类型和事件发送逻辑。本 Task 输出的 OAuth 弹窗面板被 Task 4 的 OAuth 流程消费（通过 `callback_tx` 回调通道）。

**涉及文件:**
- 修改: `rust-agent-tui/src/app/events.rs`
- 修改: `rust-agent-tui/src/app/mcp_panel.rs`
- 修改: `rust-agent-tui/src/ui/main_ui/panels/mcp.rs`
- 修改: `rust-agent-tui/src/app/mod.rs`
- 修改: `rust-agent-tui/src/ui/main_ui.rs`
- 修改: `rust-agent-tui/src/event.rs`
- 修改: `rust-agent-tui/src/ui/main_ui/status_bar.rs`
- 新建: `rust-agent-tui/src/app/oauth_prompt.rs`
- 新建: `rust-agent-tui/src/ui/main_ui/popups/oauth.rs`
- 修改: `rust-agent-middlewares/src/mcp/client.rs`（ServerInfo 新增 oauth_status 字段）

**执行步骤:**
- [x] 在 `rust-agent-middlewares/src/mcp/client.rs` 的 `ServerInfo` 中新增 `oauth_status` 字段 — 为 TUI 面板提供 OAuth 状态数据
  - 位置: `rust-agent-middlewares/src/mcp/client.rs` 第 34-41 行，`ServerInfo` 结构体定义内，在 `resource_count` 字段之后
  - 在 `ServerInfo` 定义之前（第 33 行之前）新增 `OAuthStatus` 枚举:
    ```rust
    /// MCP 服务器 OAuth 授权状态
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub enum OAuthStatus {
        /// 不使用 OAuth（stdio 传输或未配置 OAuth）
        #[default]
        None,
        /// 已授权（token 有效）
        Authorized,
        /// 需要授权（HTTP 传输且配置了 OAuth，但 token 缺失或过期）
        NeedsAuthorization,
    }
    ```
  - 在 `ServerInfo` 的 `pub resource_count: usize,` 之后追加:
    ```rust
    /// OAuth 授权状态
    pub oauth_status: OAuthStatus,
    ```
  - 更新所有构造 `ServerInfo` 的位置（`McpClientPool::server_infos()` 方法中），新增 `oauth_status: OAuthStatus::default()` 字段
  - 在 `rust-agent-middlewares/src/mcp/mod.rs` 的 `pub use` 列表中导出 `OAuthStatus`
  - 原因: `OAuthStatus` 枚举为 TUI 面板提供三种状态展示（None / Authorized / NeedsAuthorization），`Default` trait 保证现有构造点向后兼容。Task 4 的 `OAuthFlowManager` 在授权完成后更新 `ServerInfo.oauth_status` 为 `Authorized`。
- [x] 在 `rust-agent-tui/src/app/events.rs` 的 `AgentEvent` 枚举中新增 3 个 OAuth 变体 — 支持后台到 TUI 的 OAuth 事件传递
  - 位置: `rust-agent-tui/src/app/events.rs` 第 72 行（`ContextWarning` 变体之后，枚举闭合括号之前）
  - 在文件顶部 imports 区域（第 3 行 `use tokio::sync::oneshot;` 之后）追加 `OAuthCallbackResult` 结构体定义:
    ```rust
    /// OAuth 回调结果（从 TUI 传回后台 OAuth 流程）
    pub struct OAuthCallbackResult {
        /// 授权码
        pub code: String,
        /// CSRF state 参数
        pub state: String,
    }
    ```
  - 在 `ContextWarning` 变体之后追加 3 个事件变体:
    ```rust
    /// OAuth 授权需要用户交互（打开浏览器或手动粘贴回调 URL）
    OAuthAuthorizationNeeded {
        server_name: String,
        /// 浏览器授权 URL
        authorization_url: String,
        /// 回调通道：用户粘贴的 URL 或授权结果通过此通道传回后台
        callback_tx: oneshot::Sender<OAuthCallbackResult>,
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
    ```
  - 原因: `OAuthAuthorizationNeeded` 携带 `oneshot::Sender<OAuthCallbackResult>` 回调通道，TUI 弹窗收集用户输入后通过此通道将 code/state 传回后台 OAuthFlowManager。`OAuthCallbackResult` 作为公共结构体，在 events.rs 中定义供 TUI 和 middlewares 双方引用。`OAuthAuthorizationCompleted/Failed` 用于更新 MCP 面板状态和显示系统消息。
- [x] 新建 `rust-agent-tui/src/app/oauth_prompt.rs` — 定义 OAuth 弹窗面板的状态和交互逻辑
  - 位置: `rust-agent-tui/src/app/oauth_prompt.rs`（新文件）
  - 文件内容:
    ```rust
    use super::events::OAuthCallbackResult;

    /// OAuth 授权弹窗状态
    pub struct OAuthPrompt {
        /// 服务器名称
        pub server_name: String,
        /// 浏览器授权 URL
        pub authorization_url: String,
        /// 用户手动粘贴的回调 URL（或含 code 的文本）
        pub input: String,
        /// 输入光标位置（字符索引）
        pub cursor: usize,
        /// 回调通道（传回后台 OAuth 流程）
        pub callback_tx: tokio::sync::oneshot::Sender<OAuthCallbackResult>,
        /// 错误提示信息（粘贴内容解析失败时显示）
        pub error_message: Option<String>,
    }

    impl OAuthPrompt {
        pub fn new(
            server_name: String,
            authorization_url: String,
            callback_tx: tokio::sync::oneshot::Sender<OAuthCallbackResult>,
        ) -> Self {
            Self {
                server_name,
                authorization_url,
                input: String::new(),
                cursor: 0,
                callback_tx,
                error_message: None,
            }
        }

        /// 提交用户输入的回调 URL，返回 true 表示成功发送
        pub fn submit(&mut self) -> bool {
            use rust_agent_middlewares::mcp::parse_code_from_url;
            match parse_code_from_url(&self.input) {
                Ok((code, state)) => {
                    let _ = self.callback_tx.send(OAuthCallbackResult { code, state });
                    true
                }
                Err(e) => {
                    self.error_message = Some(format!("无法解析回调 URL: {}", e));
                    false
                }
            }
        }
    }
    ```
  - 原因: `OAuthPrompt` 遵循项目中 `HitlBatchPrompt` / `AskUserBatchPrompt` 的弹窗状态管理模式。`submit()` 方法调用 Task 3 实现的 `parse_code_from_url()` 解析用户粘贴的内容，通过 oneshot channel 传回后台。错误信息存储在 `error_message` 中供渲染层显示。
- [x] 在 `rust-agent-tui/src/app/mod.rs` 中注册 `oauth_prompt` 模块并新增 `App` 字段
  - 位置: `rust-agent-tui/src/app/mod.rs` 第 24 行（`mod hitl_prompt;` 之后）追加 `mod oauth_prompt;`
  - 位置: `rust-agent-tui/src/app/mod.rs` 第 29 行（`pub use hitl_prompt::...` 之后）追加:
    ```rust
    pub use oauth_prompt::OAuthPrompt;
    ```
  - 位置: `rust-agent-tui/src/app/mod.rs` 第 100 行（`pub mcp_panel: Option<McpPanel>,` 之后）追加:
    ```rust
    /// OAuth 授权弹窗状态（None 表示无弹窗）
    pub oauth_prompt: Option<OAuthPrompt>,
    ```
  - 位置: `rust-agent-tui/src/app/mod.rs` 第 196 行（`mcp_panel: None,` 之后）追加:
    ```rust
    oauth_prompt: None,
    ```
  - 原因: `oauth_prompt` 作为 `App` 的可选字段，与 `interaction_prompt`（HITL/AskUser）互不冲突——OAuth 弹窗由 `AgentEvent::OAuthAuthorizationNeeded` 触发，独立于 ReAct 循环中的 HITL 拦截。
- [x] 在 `rust-agent-tui/src/ui/main_ui/popups/mod.rs` 中注册 `oauth` 模块
  - 位置: `rust-agent-tui/src/ui/main_ui/popups/mod.rs` 第 4 行（`pub mod hitl;` 之后）追加 `pub mod oauth;`
  - 原因: OAuth 弹窗面板渲染器放在 popups 模块下，与 hitl / ask_user 同级，因为 OAuth 弹窗同样是阻塞式交互弹窗（用户必须操作后才能继续）。
- [x] 新建 `rust-agent-tui/src/ui/main_ui/popups/oauth.rs` — 实现 OAuth 弹窗面板的渲染函数
  - 位置: `rust-agent-tui/src/ui/main_ui/popups/oauth.rs`（新文件）
  - 渲染函数签名: `pub(crate) fn render_oauth_popup(f: &mut Frame, app: &mut App, area: Rect)`
  - 渲染布局（从上到下）:
    1. **标题行**: " OAuth 授权 — {server_name} "（使用 `theme::THINKING` 颜色 + `Modifier::BOLD`）
    2. **提示行**: "请在浏览器中完成授权，然后将回调 URL 粘贴到下方输入框："（使用 `theme::TEXT`）
    3. **URL 显示行**: 显示 `authorization_url`，截断到面板宽度（使用 `theme::SAGE`，便于用户手动复制）
    4. **空行**: 1 行间距
    5. **输入框行**: 显示 "回调 URL > " 前缀 + 用户输入内容（使用 `crate::app::edit_display_parts(buf, cursor)` 在光标位置插入 `█` 块，遵循项目现有编辑框渲染模式）
    6. **错误行**: 仅当 `error_message` 为 `Some` 时显示（使用 `theme::ERROR`）
    7. **快捷键行**: "Enter: 提交  Esc: 取消"（使用 `theme::MUTED`）
  - 输入框使用 `crate::app::handle_edit_key()` 统一处理编辑按键（Char / Backspace / Delete / Left / Right / Home / End / Ctrl+A / Ctrl+E / Ctrl+K / Ctrl+U）
  - 边框使用 `BorderedPanel` widget，边框颜色 `theme::BORDER`
  - 原因: 遵循项目面板渲染规范——面板内部禁止渲染快捷键提示行，快捷键统一由状态栏 `render_second_row` 负责。此处快捷键行是临时实现，在步骤 10 中由状态栏接管后移除。
- [x] 在 `rust-agent-tui/src/ui/main_ui.rs` 的 `render()` 函数中添加 OAuth 弹窗的渲染调度
  - 位置: `rust-agent-tui/src/ui/main_ui.rs` 第 71-81 行，底部展开区渲染块（`if panel_height > 0 { ... }` 内部）
  - 在 `match &app.agent.interaction_prompt { ... }` 块之后、`if app.core.login_panel.is_some()` 之前追加:
    ```rust
    if app.oauth_prompt.is_some() {
        popups::oauth::render_oauth_popup(f, app, panel_area);
    }
    ```
  - 原因: OAuth 弹窗与 HITL/AskUser 弹窗互斥（由后台事件触发时序保证），渲染在同一区域。
- [x] 在 `rust-agent-tui/src/ui/main_ui.rs` 的 `active_panel_height()` 函数中添加 OAuth 弹窗的高度计算
  - 位置: `rust-agent-tui/src/ui/main_ui.rs` 第 126-208 行，`active_panel_height` 函数内
  - 在 `else if let Some(crate::app::InteractionPrompt::Questions(p)) = ...` 分支之前追加:
    ```rust
    } else if app.oauth_prompt.is_some() {
        9 // 标题1 + 提示1 + URL1 + 空行1 + 输入框1 + 错误1 + 快捷键1 + 边框2
    ```
  - 原因: OAuth 弹窗固定 9 行高度（标题 + 提示 + URL + 空行 + 输入框 + 错误 + 快捷键 + 上下边框）。
- [x] 在 `rust-agent-tui/src/event.rs` 的 `next_event()` 函数中添加 OAuth 弹窗的键盘事件处理
  - 位置: `rust-agent-tui/src/event.rs`，在 MCP 面板处理块（`if app.mcp_panel.is_some()`，约第 185 行）之前追加:
    ```rust
    // OAuth 弹窗优先处理
    if app.oauth_prompt.is_some() {
        handle_oauth_prompt(app, input);
        return Ok(Some(Action::Redraw));
    }
    ```
  - 在文件末尾（`handle_mcp_panel` 函数附近）新增 `handle_oauth_prompt` 函数:
    ```rust
    fn handle_oauth_prompt(app: &mut App, input: Input) {
        use crate::app::handle_edit_key;
        let prompt = match app.oauth_prompt.as_mut() {
            Some(p) => p,
            None => return,
        };
        match input {
            Input { key: Key::Enter, .. } => {
                if prompt.submit() {
                    app.oauth_prompt = None;
                }
            }
            Input { key: Key::Esc, .. } => {
                app.oauth_prompt = None;
            }
            Input { key: Key::Char('c'), ctrl: true, .. } => {
                // Ctrl+C 在弹窗中不退出，忽略
            }
            _ => {
                prompt.error_message = None; // 清除之前的错误
                handle_edit_key(&mut prompt.input, &mut prompt.cursor, input);
            }
        }
    }
    ```
  - 原因: OAuth 弹窗优先级高于 MCP 面板（弹窗 > 面板），遵循项目事件处理优先级链。`handle_edit_key()` 复用 `app/mod.rs` 中的统一编辑按键处理函数，支持完整的单行编辑操作。Enter 提交成功后清除 `oauth_prompt` 关闭弹窗。
- [x] 在 `rust-agent-tui/src/app/mcp_panel.rs` 的 MCP 面板中新增手动授权触发方法
  - 位置: `rust-agent-tui/src/app/mcp_panel.rs`，在 `mcp_panel_reconnect()` 方法之后追加 `mcp_panel_request_oauth()` 方法:
    ```rust
    /// 手动触发当前选中服务器的 OAuth 授权流程
    pub fn mcp_panel_request_oauth(&mut self) {
        if let Some(ref panel) = self.mcp_panel {
            if !panel.view.is_server_list() {
                return;
            }
            if panel.cursor >= panel.servers.len() {
                return;
            }
            let server = &panel.servers[panel.cursor];
            // 仅 HTTP 传输且状态为 NeedsAuthorization 时触发
            if server.transport_type != "http" {
                return;
            }
            use rust_agent_middlewares::mcp::OAuthStatus;
            if server.oauth_status != OAuthStatus::NeedsAuthorization {
                return;
            }
            // 通过 MCP 连接池触发 OAuth 流程（Task 4 实现具体逻辑）
            let name = server.name.clone();
            if let Some(pool) = self.mcp_pool.clone() {
                tokio::spawn(async move {
                    let _ = pool.start_oauth_flow(&name).await;
                });
            }
        }
    }
    ```
  - 注意: `start_oauth_flow` 方法由 Task 4 在 `McpClientPool` 中实现。此处预留调用点，Task 4 完成后编译通过。
  - 原因: 用户在 MCP 面板 ServerList 视图中按 `r` 键触发 OAuth 流程。仅对 HTTP 传输且状态为 `NeedsAuthorization` 的服务器生效。
- [x] 在 `rust-agent-tui/src/event.rs` 的 `handle_mcp_panel()` 函数中添加 `r` 键绑定
  - 位置: `rust-agent-tui/src/event.rs`，`handle_mcp_panel` 函数内，在 `Ctrl+R` 重连分支（约第 1248-1259 行）之后追加:
    ```rust
    Input {
        key: Key::Char('r'),
        ctrl: false,
        ..
    } => {
        if is_server_list {
            app.mcp_panel_request_oauth();
        }
    }
    ```
  - 原因: `r` 键（无 Ctrl 修饰）用于手动触发 OAuth 授权。`Ctrl+R` 已被重连功能占用，使用小写 `r` 不与任何现有快捷键冲突。CLAUDE.md 规范禁止 `Shift + 字母`，`r` 是普通字母键，不违反规范。
- [x] 在 `rust-agent-tui/src/ui/main_ui/panels/mcp.rs` 的服务器列表渲染中新增 OAuth 状态图标列
  - 位置: `rust-agent-tui/src/ui/main_ui/panels/mcp.rs`，`render_server_list` 函数内，第 100-117 行的 `Line::from(vec![...])` 构建
  - 在 `count_text` 的 Span 之前插入 OAuth 状态 Span。在循环体中（第 50 行 `for (i, server) in panel.servers.iter().enumerate()` 之后），在构建 `count_text` 之前新增:
    ```rust
    // OAuth 状态图标
    let (oauth_icon, oauth_style) = match &server.oauth_status {
        rust_agent_middlewares::mcp::OAuthStatus::None => ("", Style::default()),
        rust_agent_middlewares::mcp::OAuthStatus::Authorized => {
            ("\u{1f511}", Style::default().fg(theme::SAGE))
        }
        rust_agent_middlewares::mcp::OAuthStatus::NeedsAuthorization => {
            ("\u{1f512}", Style::default().fg(theme::WARNING))
        }
    };
    ```
  - 在 `Span::styled(count_text, ...)` 之前追加:
    ```rust
    Span::styled(format!("{} ", oauth_icon), oauth_style),
    ```
  - 原因: 在服务器列表的每一行右侧（count_text 之前）显示 OAuth 状态图标。`None` 状态不显示图标（空字符串），`Authorized` 显示绿色钥匙，`NeedsAuthorization` 显示黄色锁。
- [x] 在 `rust-agent-tui/src/ui/main_ui/status_bar.rs` 的 `render_second_row` 中添加 OAuth 弹窗和 MCP 面板的快捷键提示
  - 位置: `rust-agent-tui/src/ui/main_ui/status_bar.rs` 第 218 行，`render_second_row` 函数内的 `match &app.agent.interaction_prompt` 表达式
  - 在 `Some(crate::app::InteractionPrompt::Questions(_))` 分支之前追加:
    ```rust
    Some(_) if app.oauth_prompt.is_some() => {
        key!["Enter" => ":提交  ", "Esc" => ":取消"]
    }
    ```
  - 位置: MCP 面板 ServerList 非确认删除状态（约第 253 行），将现有的:
    ```rust
    key!["↑↓" => ":移动  ", "Enter" => ":详情  ", "Ctrl+R" => ":重连  ", "Ctrl+D" => ":删除  ", "Esc" => ":关闭"]
    ```
    替换为:
    ```rust
    key!["↑↓" => ":移动  ", "Enter" => ":详情  ", "r" => ":授权  ", "Ctrl+R" => ":重连  ", "Ctrl+D" => ":删除  ", "Esc" => ":关闭"]
    ```
  - 原因: 遵循 CLAUDE.md 面板快捷键设计规范——面板内部禁止渲染快捷键提示行，统一由状态栏 `render_second_row` 负责。`Some(_) if app.oauth_prompt.is_some()` 使用 guard 模式匹配，优先级高于 `InteractionPrompt` 分支。
- [x] 更新所有现有测试中 `ServerInfo` 的手动构造 — 补充 `oauth_status` 字段
  - 位置: `rust-agent-tui/src/app/mcp_panel.rs` 第 252-259 行，`make_server_info` 函数
  - 在 `resource_count: 0,` 之后追加 `oauth_status: Default::default(),`
  - 位置: `rust-agent-tui/src/ui/main_ui/panels/mcp.rs` 第 311-318 行，`make_server` 函数
  - 在 `resource_count: 2,` 之后追加 `oauth_status: Default::default(),`
  - 位置: `rust-agent-middlewares/src/mcp/client.rs` 中所有构造 `ServerInfo` 的位置
  - 原因: `oauth_status` 字段新增后，所有手动构造 `ServerInfo` 的位置必须补充该字段。`OAuthStatus` 实现了 `Default` trait（默认为 `None`），使用 `Default::default()` 或 `OAuthStatus::None` 填充。
- [x] 为 OAuth 弹窗面板交互逻辑编写单元测试
  - 测试文件: `rust-agent-tui/src/app/oauth_prompt.rs`（文件末尾 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_oauth_prompt_new`: 构造 `OAuthPrompt::new(...)`，断言 `input` 为空、`cursor` 为 0、`error_message` 为 `None`
    - `test_oauth_prompt_submit_valid_url`: 构造 OAuthPrompt，设置 `input = "http://localhost:12345/callback?code=abc&state=xyz"`，调用 `submit()`，断言返回 `true`，`callback_tx` 接收到 `OAuthCallbackResult { code: "abc", state: "xyz" }`
    - `test_oauth_prompt_submit_query_only`: 设置 `input = "code=test_code&state=test_state"`，调用 `submit()`，断言返回 `true`
    - `test_oauth_prompt_submit_invalid_url`: 设置 `input = "not a valid url"`，调用 `submit()`，断言返回 `false`，`error_message` 为 `Some(...)`
    - `test_oauth_prompt_submit_empty`: 设置 `input = ""`，调用 `submit()`，断言返回 `false`
  - 测试文件: `rust-agent-tui/src/ui/main_ui/popups/oauth.rs`（文件末尾 `#[cfg(test)] mod tests` 块）
  - 测试场景:
    - `test_render_oauth_popup_shows_url`: 构造 headless App，设置 `oauth_prompt = Some(OAuthPrompt::new(...))`，调用 `render()`，断言 snapshot 包含 `authorization_url` 中的域名部分（ASCII 内容）
    - `test_render_oauth_popup_shows_error`: 构造 OAuthPrompt 并设置 `error_message = Some("parse error")`，渲染后断言 snapshot 包含 "parse error"
  - 运行命令: `cargo test -p rust-agent-tui --lib -- app::oauth_prompt::tests`
  - 预期: 所有 5 个 oauth_prompt 测试通过
  - 运行命令: `cargo test -p rust-agent-tui --lib -- ui::main_ui::popups::oauth::tests`
  - 预期: 所有 2 个渲染测试通过

**检查步骤:**
- [x] 验证 OAuthStatus 枚举在 client.rs 中定义并导出
  - `grep -n 'pub enum OAuthStatus' /Users/konghayao/code/ai/perihelion/rust-agent-middlewares/src/mcp/client.rs`
  - 预期: 行号输出
- [x] 验证 AgentEvent 包含 OAuth 变体
  - `grep -c 'OAuthAuthorization' /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/events.rs`
  - 预期: 输出为 4（OAuthCallbackResult + 3 个 OAuth 变体）
- [x] 验证 OAuthPrompt 结构体定义存在
  - `grep -n 'pub struct OAuthPrompt' /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/oauth_prompt.rs`
  - 预期: 行号输出
- [x] 验证 App 包含 oauth_prompt 字段
  - `grep -n 'pub oauth_prompt' /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/mod.rs`
  - 预期: 行号输出
- [x] 验证 OAuth 弹窗渲染函数存在
  - `grep -n 'pub(crate) fn render_oauth_popup' /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/main_ui/popups/oauth.rs`
  - 预期: 行号输出
- [x] 验证事件处理函数 handle_oauth_prompt 存在
  - `grep -n 'fn handle_oauth_prompt' /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/event.rs`
  - 预期: 行号输出
- [x] 验证 MCP 面板包含 mcp_panel_request_oauth 方法
  - `grep -n 'mcp_panel_request_oauth' /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/app/mcp_panel.rs`
  - 预期: 行号输出
- [x] 验证状态栏包含 OAuth 弹窗快捷键
  - `grep 'oauth_prompt' /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 输出包含 `oauth_prompt`
- [x] 验证状态栏 MCP 面板包含 r:授权
  - `grep 'r.*:授权' /Users/konghayao/code/ai/perihelion/rust-agent-tui/src/ui/main_ui/status_bar.rs`
  - 预期: 输出包含 `r` 和 `授权`
- [x] 验证编译通过（middlewares）
  - `cargo build -p rust-agent-middlewares 2>&1 | tail -5`
  - 预期: 输出包含 `Finished`，无编译错误
- [x] 验证编译通过（TUI）
  - `cargo build -p rust-agent-tui 2>&1 | tail -5`
  - 预期: 输出包含 `Finished`，无编译错误（注：`start_oauth_flow` 方法需 Task 4 实现后才能编译，此处预期编译错误指向该方法调用，属于正常跨 Task 依赖）
- [x] 验证 oauth_prompt 模块测试通过
  - `cargo test -p rust-agent-tui --lib -- app::oauth_prompt::tests 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok`，5 个测试全部通过
- [x] 验证 oauth 弹窗渲染测试通过
  - `cargo test -p rust-agent-tui --lib -- ui::main_ui::popups::oauth::tests 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok`，2 个测试全部通过
- [x] 验证 middlewares 全量测试无回归
  - `cargo test -p rust-agent-middlewares --lib 2>&1 | tail -10`
  - 预期: 输出包含 `test result: ok`，无失败测试

---

### Task 6: MCP OAuth 认证 验收

**前置条件:**
- 构建命令: `cargo build`
- 所有 Task 1-5 的单元测试已通过
- 环境准备: 至少一个支持 OAuth 的 MCP 服务器（如 GitHub MCP）的配置信息（用于手动验证）

**端到端验证:**

1. 运行完整测试套件确保无回归
   - `cargo test 2>&1 | tail -15`
   - 预期: 所有测试通过，无失败
   - 失败排查: 逐个 crate 检查 `cargo test -p <crate-name>`，定位失败的具体 Task

2. 验证 rmcp auth feature 编译正确
   - `cargo build -p rust-agent-middlewares 2>&1 | tail -5`
   - 预期: `Finished`，无编译错误
   - 失败排查: 检查 Task 1 的 Cargo.toml auth feature 配置

3. 验证 OAuthConfig 配置反序列化兼容性
   - `cargo test -p rust-agent-middlewares --lib -- mcp::config::tests 2>&1 | tail -10`
   - 预期: 所有配置测试通过（含新增的 OAuth 测试和现有测试）
   - 失败排查: 检查 Task 1 的 McpServerConfig 扩展

4. 验证 Token 持久化功能
   - `cargo test -p rust-agent-middlewares --lib -- mcp::auth_store::tests 2>&1 | tail -10`
   - 预期: 所有 Token 存储测试通过（文件创建、读写、清除、并发安全）
   - 失败排查: 检查 Task 2 的 FileCredentialStore 实现

5. 验证 OAuth 回调服务器功能
   - `cargo test -p rust-agent-middlewares --lib -- mcp::callback_server::tests 2>&1 | tail -10`
   - 预期: 所有回调服务器测试通过（URL 解析、回调接收、超时、state 验证）
   - 失败排查: 检查 Task 3 的 OAuthCallbackServer 实现

6. 验证 OAuth 流程编排
   - `cargo test -p rust-agent-middlewares --lib -- mcp::oauth_flow::tests 2>&1 | tail -10`
   - 预期: 所有 OAuth 流程测试通过
   - 失败排查: 检查 Task 4 的 OAuthFlowManager 和 build_authed_transport 实现

7. 验证 TUI OAuth 面板和事件
   - `cargo test -p rust-agent-tui --lib -- app::oauth_prompt::tests 2>&1 | tail -10`
   - `cargo test -p rust-agent-tui --lib -- ui::main_ui::popups::oauth::tests 2>&1 | tail -10`
   - 预期: 所有 TUI OAuth 测试通过
   - 失败排查: 检查 Task 5 的 OAuthPrompt 和 OAuth 弹窗实现

8. 验证 TUI 完整构建
   - `cargo build -p rust-agent-tui 2>&1 | tail -5`
   - 预期: `Finished`，无编译错误
   - 失败排查: 检查 Task 4-5 之间的跨 crate 接口是否对齐（AgentEvent 变体、OAuthFlowManager 事件类型）

9. 验证向后兼容性（不配置 oauth 字段的 MCP 服务器行为不变）
   - `cargo test -p rust-agent-middlewares --lib -- mcp::client::tests 2>&1 | tail -10`
   - 预期: 所有现有连接池测试通过（oauth: None 的服务器不应触发 OAuth 流程）
   - 失败排查: 检查 Task 1 的 `#[serde(default)]` 和 Task 4 的条件判断逻辑

**认知变更:**
- [x] [CLAUDE.md] MCP 中间件新增 `auth_store`、`callback_server`、`oauth_flow` 三个子模块，位于 `rust-agent-middlewares/src/mcp/`。OAuth 仅用于 StreamableHttp 传输类型，stdio 传输不受影响。
- [x] [CLAUDE.md] MCP 服务器配置新增 `oauth` 字段（`OAuthConfig`），JSON 键名使用 camelCase（`clientId`/`clientSecret`/`scopes`），`client_secret` 支持 `${VAR}` 环境变量展开。
- [x] [CLAUDE.md] [TRAP] rmcp `auth` feature 启用后引入 `oauth2` crate 依赖。`AuthError` 不实现 `From<std::io::Error>`，需要自定义错误包装（`AuthStoreError`）来桥接 IO 错误和 rmcp 认证错误。
- [x] [CLAUDE.md] TUI `AgentEvent`（`rust-agent-tui/src/app/events.rs`）新增 `OAuthAuthorizationNeeded`、`OAuthAuthorizationCompleted`、`OAuthAuthorizationFailed` 三个变体。核心层 `AgentEvent`（`rust-create-agent/src/agent/events.rs`）不新增变体——OAuth 事件仅在 TUI 层定义。
