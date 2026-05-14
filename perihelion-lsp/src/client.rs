use crate::diagnostics::DiagnosticsRegistry;
use crate::error::LspError;
use crate::jsonrpc::transport::MessageDispatcher;
use crate::jsonrpc::{JsonRpcNotification, JsonRpcRequest};
use crate::protocol::notifications::{
    did_change_notification, did_close_notification, did_open_notification, did_save_notification,
    parse_publish_diagnostics,
};
use crate::protocol::requests::initialize_params;
use parking_lot::RwLock;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// LSP 服务器状态
#[derive(Debug, Clone, PartialEq)]
pub enum ServerState {
    Stopped,
    Starting,
    Running,
    Error(String),
}

/// 单个 LSP 服务器客户端
pub struct LspClient {
    name: String,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    initialization_options: Option<Value>,
    state: Arc<RwLock<ServerState>>,
    /// tokio::sync::Mutex — guard 可以跨 .await 持有
    dispatcher: Arc<tokio::sync::Mutex<Option<MessageDispatcher>>>,
    next_id: Arc<parking_lot::Mutex<i64>>,
    open_files: Arc<RwLock<HashMap<String, OpenFileInfo>>>,
    restart_count: Arc<parking_lot::Mutex<u32>>,
    max_restarts: u32,
    diagnostics: Arc<DiagnosticsRegistry>,
}

#[derive(Debug, Clone)]
struct OpenFileInfo {
    version: i32,
}

enum DidChangeAction {
    Open { language_id: String, version: i32 },
    Change(i32),
}

impl LspClient {
    pub fn new(
        name: String,
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
        initialization_options: Option<Value>,
        max_restarts: u32,
        diagnostics: Arc<DiagnosticsRegistry>,
    ) -> Self {
        Self {
            name,
            command,
            args,
            env,
            initialization_options,
            state: Arc::new(RwLock::new(ServerState::Stopped)),
            dispatcher: Arc::new(tokio::sync::Mutex::new(None)),
            next_id: Arc::new(parking_lot::Mutex::new(0)),
            open_files: Arc::new(RwLock::new(HashMap::new())),
            restart_count: Arc::new(parking_lot::Mutex::new(0)),
            max_restarts,
            diagnostics,
        }
    }

    /// 启动 LSP 服务器并完成 initialize/initialized 握手
    pub async fn start(&self, root_uri: &str) -> Result<(), LspError> {
        {
            let state = self.state.read();
            if *state == ServerState::Running {
                return Ok(());
            }
            // 不设置 Starting 状态，直接在 do_start 中设置 Running
        }

        let result = self.do_start(root_uri).await;

        {
            let mut state = self.state.write();
            match &result {
                Ok(()) => *state = ServerState::Running, // 已经在 do_start 中设置，这里再次确认
                Err(e) => *state = ServerState::Error(e.to_string()),
            }
        }

        result
    }

    async fn do_start(&self, root_uri: &str) -> Result<(), LspError> {
        let transport =
            crate::jsonrpc::transport::LspTransport::spawn(&self.command, &self.args, &self.env)?;

        let diagnostics = Arc::clone(&self.diagnostics);

        let (dispatcher, rx) = MessageDispatcher::new(transport);

        {
            let diag_clone = Arc::clone(&diagnostics);
            dispatcher.on_notification(
                "textDocument/publishDiagnostics",
                Box::new(move |params: Value| {
                    if let Some(publish_params) = parse_publish_diagnostics(&params) {
                        diag_clone.handle_publish_diagnostics(&publish_params);
                    }
                }),
            );
        }

        {
            let state = Arc::clone(&self.state);
            let name = self.name.clone();
            dispatcher.set_on_error(Box::new(move |error: LspError| {
                tracing::warn!(target: "lsp", server = %name, error = %error, "LSP 服务器错误");
                *state.write() = ServerState::Error(error.to_string());
            }));
        }

        *self.dispatcher.lock().await = Some(dispatcher);

        // 提取共享分发状态（Arc clone），不持有 tokio::sync::Mutex
        let dispatch_state = {
            let guard = self.dispatcher.lock().await;
            guard.as_ref().unwrap().dispatch_state()
        };

        // 立即设置状态为 Running，这样 initialize 请求可以通过状态检查
        *self.state.write() = ServerState::Running;

        // 启动消息分发循环（后台 task，消费 stdout 消息）
        // 使用 Arc<DispatchState> 而非持有 tokio::sync::Mutex guard，避免死锁
        tokio::spawn(async move {
            crate::jsonrpc::transport::run_dispatch_loop(dispatch_state, rx).await;
        });

        // root_uri 已经是 "file:///path" 格式，直接使用
        let workspace_uri: lsp_types::Uri = root_uri
            .parse()
            .unwrap_or_else(|_| "file:///tmp".parse().unwrap());
        let workspace_folders = vec![lsp_types::WorkspaceFolder {
            uri: workspace_uri,
            name: "workspace".to_string(),
        }];

        let init_params = initialize_params(
            root_uri.to_string(),
            workspace_folders,
            self.initialization_options.clone(),
        );

        let result = self
            .request("initialize", Some(init_params), 30_000)
            .await?;

        let _server_capabilities = result.get("capabilities").cloned();
        tracing::info!(
            target: "lsp",
            server = %self.name,
            "LSP 服务器初始化成功"
        );

        self.notify("initialized", Some(Value::Object(Default::default())))
            .await?;

        Ok(())
    }

    fn next_request_id(&self) -> i64 {
        let mut id = self.next_id.lock();
        *id += 1;
        *id
    }

    /// 发送请求并等待响应（带超时）
    pub async fn request(
        &self,
        method: &str,
        params: Option<Value>,
        timeout_ms: u64,
    ) -> Result<Value, LspError> {
        let state = self.state.read().clone();
        if state != ServerState::Running {
            return Err(LspError::NotReady {
                server: self.name.clone(),
            });
        }

        let id = self.next_request_id();
        let request = JsonRpcRequest::new(id, method, params);

        let receiver = {
            let guard = self.dispatcher.lock().await;
            match guard.as_ref() {
                Some(d) => d.register_request(id),
                None => {
                    return Err(LspError::NotReady {
                        server: self.name.clone(),
                    })
                }
            }
        };

        {
            let mut guard = self.dispatcher.lock().await;
            match guard.as_mut() {
                Some(d) => {
                    if let Err(e) = d.send_request(&request).await {
                        tracing::error!(
                            target: "lsp",
                            server = %self.name,
                            method,
                            error = %e,
                            "LSP 请求发送失败（服务器可能已崩溃）"
                        );
                        return Err(e);
                    }
                }
                None => {
                    return Err(LspError::NotReady {
                        server: self.name.clone(),
                    })
                }
            }
        }

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(LspError::RequestFailed {
                method: method.to_string(),
                reason: "请求被取消".to_string(),
            }),
            Err(_) => Err(LspError::RequestTimeout {
                method: method.to_string(),
                timeout_ms,
            }),
        }
    }

    /// 发送通知
    pub async fn notify(&self, method: &str, params: Option<Value>) -> Result<(), LspError> {
        let notification = JsonRpcNotification::new(method, params);
        let mut guard = self.dispatcher.lock().await;
        match guard.as_mut() {
            Some(d) => d.send_notification(&notification).await,
            None => Err(LspError::NotReady {
                server: self.name.clone(),
            }),
        }
    }

    /// 文件同步: didOpen
    pub async fn did_open(&self, uri: &str, language_id: &str, text: &str) -> Result<(), LspError> {
        let version = {
            let mut open = self.open_files.write();
            if open.contains_key(uri) {
                return Ok(());
            }
            let v = open.len() as i32 + 1;
            open.insert(uri.to_string(), OpenFileInfo { version: v });
            v
        };

        let notif = did_open_notification(uri, language_id, version, text);
        let mut guard = self.dispatcher.lock().await;
        match guard.as_mut() {
            Some(d) => d.send_notification(&notif).await,
            None => Err(LspError::NotReady {
                server: self.name.clone(),
            }),
        }
    }

    /// 文件同步: didChange
    pub async fn did_change(&self, uri: &str, text: &str) -> Result<(), LspError> {
        // 所有版本号操作同步完成（不跨 await），避免 parking_lot guard 的 Send 问题
        let action = {
            let mut open = self.open_files.write();
            if let Some(info) = open.get_mut(uri) {
                info.version += 1;
                DidChangeAction::Change(info.version)
            } else {
                let v = open.len() as i32 + 1;
                let language_id = Self::infer_language_id(uri);
                open.insert(uri.to_string(), OpenFileInfo { version: v });
                DidChangeAction::Open {
                    language_id,
                    version: v,
                }
            }
        };

        match action {
            DidChangeAction::Open {
                language_id,
                version,
            } => {
                let notif = did_open_notification(uri, &language_id, version, text);
                let mut guard = self.dispatcher.lock().await;
                match guard.as_mut() {
                    Some(d) => d.send_notification(&notif).await,
                    None => Err(LspError::NotReady {
                        server: self.name.clone(),
                    }),
                }
            }
            DidChangeAction::Change(version) => {
                let notif = did_change_notification(uri, version, text);
                let mut guard = self.dispatcher.lock().await;
                match guard.as_mut() {
                    Some(d) => d.send_notification(&notif).await,
                    None => Err(LspError::NotReady {
                        server: self.name.clone(),
                    }),
                }
            }
        }
    }

    /// 文件同步: didSave
    pub async fn did_save(&self, uri: &str) -> Result<(), LspError> {
        let notif = did_save_notification(uri, None);
        let mut guard = self.dispatcher.lock().await;
        match guard.as_mut() {
            Some(d) => d.send_notification(&notif).await,
            None => Err(LspError::NotReady {
                server: self.name.clone(),
            }),
        }
    }

    /// 文件同步: didClose
    pub async fn did_close(&self, uri: &str) -> Result<(), LspError> {
        self.open_files.write().remove(uri);
        let notif = did_close_notification(uri);
        let mut guard = self.dispatcher.lock().await;
        match guard.as_mut() {
            Some(d) => d.send_notification(&notif).await,
            None => Err(LspError::NotReady {
                server: self.name.clone(),
            }),
        }
    }

    pub fn is_ready(&self) -> bool {
        *self.state.read() == ServerState::Running
    }

    pub fn state(&self) -> ServerState {
        self.state.read().clone()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn infer_language_id(uri: &str) -> String {
        let ext = std::path::Path::new(uri)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        match ext {
            "rs" => "rust".to_string(),
            "ts" => "typescript".to_string(),
            "tsx" => "typescriptreact".to_string(),
            "js" => "javascript".to_string(),
            "jsx" => "javascriptreact".to_string(),
            "py" => "python".to_string(),
            "go" => "go".to_string(),
            "java" => "java".to_string(),
            "c" => "c".to_string(),
            "cpp" | "cc" | "cxx" => "cpp".to_string(),
            "h" | "hpp" => "c".to_string(),
            "rb" => "ruby".to_string(),
            "swift" => "swift".to_string(),
            "kt" | "kts" => "kotlin".to_string(),
            other => other.to_string(),
        }
    }

    pub async fn shutdown(&self) {
        let _ = self.request("shutdown", Some(Value::Null), 5_000).await;
        let _ = self.notify("exit", None).await;

        let guard = self.dispatcher.lock().await;
        if let Some(d) = guard.as_ref() {
            d.close().await;
        }

        *self.state.write() = ServerState::Stopped;
    }

    /// 检查重启次数限制并递增计数（同步操作，确保 parking_lot guard 不跨 await）
    fn check_and_increment_restart(&self) -> Result<(), LspError> {
        let mut count = self.restart_count.lock();
        if *count >= self.max_restarts {
            return Err(LspError::ServerCrashed {
                server: self.name.clone(),
                restart_count: *count,
                max_restarts: self.max_restarts,
            });
        }
        *count += 1;
        Ok(())
    }

    pub async fn try_restart(&self, root_uri: &str) -> Result<(), LspError> {
        self.check_and_increment_restart()?;

        {
            let guard = self.dispatcher.lock().await;
            if let Some(d) = guard.as_ref() {
                d.close().await;
            }
        }
        *self.dispatcher.lock().await = None;
        self.open_files.write().clear();

        match self.do_start(root_uri).await {
            Ok(()) => {
                *self.state.write() = ServerState::Running;
                *self.restart_count.lock() = 0;
                Ok(())
            }
            Err(e) => {
                *self.state.write() = ServerState::Error(e.to_string());
                Err(e)
            }
        }
    }
}
