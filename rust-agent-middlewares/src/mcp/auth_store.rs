use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rmcp::transport::auth::{AuthError, CredentialStore, StoredCredentials};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::debug;

const TOKEN_FILE_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct OAuthTokenFile {
    version: u32,
    tokens: HashMap<String, StoredCredentials>,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthStoreError {
    #[error("Token 文件读取失败: {path}: {detail}")]
    ReadFailed { path: PathBuf, detail: String },
    #[error("Token 文件写入失败: {path}: {detail}")]
    WriteFailed { path: PathBuf, detail: String },
    #[error("Token 文件格式无效: {reason}")]
    InvalidFormat { reason: String },
    #[error("服务器 \"{server}\" 的 Token 未找到")]
    NotFound { server: String },
}

pub struct FileCredentialStore {
    path: PathBuf,
    mutex: Mutex<()>,
}

impl FileCredentialStore {
    pub fn new() -> Self {
        let path = dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".zen-code")
            .join("oauth_tokens.json");
        Self { path, mutex: Mutex::new(()) }
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self { path, mutex: Mutex::new(()) }
    }

    pub fn path(&self) -> &Path { &self.path }

    fn ensure_file(&self) -> Result<(), AuthStoreError> {
        if !self.path.exists() {
            if let Some(parent) = self.path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| AuthStoreError::WriteFailed { path: parent.to_path_buf(), detail: e.to_string() })?;
            }
            let initial_content = serde_json::to_string_pretty(&OAuthTokenFile { version: TOKEN_FILE_VERSION, tokens: HashMap::new() })
                .map_err(|e| AuthStoreError::WriteFailed { path: self.path.clone(), detail: e.to_string() })?;
            std::fs::write(&self.path, initial_content).map_err(|e| AuthStoreError::WriteFailed { path: self.path.clone(), detail: e.to_string() })?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| AuthStoreError::WriteFailed { path: self.path.clone(), detail: e.to_string() })?;
        }
        Ok(())
    }

    fn read_file(&self) -> Result<OAuthTokenFile, AuthStoreError> {
        self.ensure_file()?;
        let content = std::fs::read_to_string(&self.path).map_err(|e| AuthStoreError::ReadFailed { path: self.path.clone(), detail: e.to_string() })?;
        let file: OAuthTokenFile = serde_json::from_str(&content).map_err(|e| AuthStoreError::InvalidFormat { reason: format!("JSON 解析失败: {}", e) })?;
        if file.version != TOKEN_FILE_VERSION {
            return Err(AuthStoreError::InvalidFormat { reason: format!("不支持的版本号: {}，期望: {}", file.version, TOKEN_FILE_VERSION) });
        }
        Ok(file)
    }

    fn write_file(&self, file: &OAuthTokenFile) -> Result<(), AuthStoreError> {
        self.ensure_file()?;
        let content = serde_json::to_string_pretty(file).map_err(|e| AuthStoreError::WriteFailed { path: self.path.clone(), detail: e.to_string() })?;
        std::fs::write(&self.path, content).map_err(|e| AuthStoreError::WriteFailed { path: self.path.clone(), detail: e.to_string() })?;
        debug!("Token 文件已写入: {}", self.path.display());
        Ok(())
    }

    pub async fn load_server(&self, server_name: &str) -> Result<Option<StoredCredentials>, AuthStoreError> {
        let _lock = self.mutex.lock().await;
        let file = self.read_file()?;
        Ok(file.tokens.get(server_name).cloned())
    }

    pub async fn save_server(&self, server_name: &str, credentials: StoredCredentials) -> Result<(), AuthStoreError> {
        let _lock = self.mutex.lock().await;
        let mut file = self.read_file()?;
        file.tokens.insert(server_name.to_string(), credentials);
        self.write_file(&file)
    }

    pub async fn clear_server(&self, server_name: &str) -> Result<(), AuthStoreError> {
        let _lock = self.mutex.lock().await;
        let mut file = self.read_file()?;
        file.tokens.remove(server_name);
        self.write_file(&file)
    }

    pub async fn clear_all(&self) -> Result<(), AuthStoreError> {
        let _lock = self.mutex.lock().await;
        self.write_file(&OAuthTokenFile { version: TOKEN_FILE_VERSION, tokens: HashMap::new() })
    }

    pub async fn list_servers(&self) -> Result<Vec<String>, AuthStoreError> {
        let _lock = self.mutex.lock().await;
        Ok(self.read_file()?.tokens.keys().cloned().collect())
    }
}

pub struct PerServerCredentialStore {
    inner: Arc<FileCredentialStore>,
    server_name: String,
}

impl PerServerCredentialStore {
    pub fn new(inner: Arc<FileCredentialStore>, server_name: String) -> Self {
        Self { inner, server_name }
    }
}

impl CredentialStore for PerServerCredentialStore {
    async fn load(&self) -> Result<StoredCredentials, AuthError> {
        self.inner.load_server(&self.server_name).await
            .map_err(|e| AuthError::InternalError(e.to_string()))?
            .ok_or_else(|| AuthError::InternalError(format!("服务器 \"{}\" 的 Token 未找到", self.server_name)))
    }

    async fn save(&self, credentials: StoredCredentials) -> Result<(), AuthError> {
        self.inner.save_server(&self.server_name, credentials).await
            .map_err(|e| AuthError::InternalError(e.to_string()))
    }

    async fn clear(&self) -> Result<(), AuthError> {
        self.inner.clear_server(&self.server_name).await
            .map_err(|e| AuthError::InternalError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (Arc<FileCredentialStore>, tempfile::NamedTempFile) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        (Arc::new(FileCredentialStore::with_path(path)), tmp)
    }

    #[test]
    fn test_new_creates_default_path() {
        let store = FileCredentialStore::new();
        assert!(store.path().to_string_lossy().contains(".zen-code"));
    }

    #[tokio::test]
    async fn test_ensure_file_creates_file_with_initial_content() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        drop(tmp);
        let store = FileCredentialStore::with_path(path.clone());
        store.ensure_file().unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let file: OAuthTokenFile = serde_json::from_str(&content).unwrap();
        assert_eq!(file.version, TOKEN_FILE_VERSION);
        assert!(file.tokens.is_empty());
    }

    #[tokio::test]
    async fn test_load_nonexistent_server_returns_none() {
        let (store, _tmp) = temp_store();
        assert!(store.load_server("nonexistent").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_file_persists_across_instances() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        drop(tmp);
        let store1 = Arc::new(FileCredentialStore::with_path(path.clone()));
        store1.save_server("srv1", StoredCredentials::new("client1".into(), None, vec![], None)).await.unwrap();
        let store2 = Arc::new(FileCredentialStore::with_path(path));
        assert!(store2.load_server("srv1").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_clear_server() {
        let (store, _tmp) = temp_store();
        store.save_server("srv", StoredCredentials::new("c".into(), None, vec![], None)).await.unwrap();
        store.clear_server("srv").await.unwrap();
        assert!(store.load_server("srv").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_overwrite_server_token() {
        let (store, _tmp) = temp_store();
        store.save_server("srv", StoredCredentials::new("c1".into(), None, vec![], None)).await.unwrap();
        store.save_server("srv", StoredCredentials::new("c2".into(), None, vec![], None)).await.unwrap();
        assert_eq!(store.load_server("srv").await.unwrap().unwrap().client_id, "c2");
    }

    #[tokio::test]
    async fn test_clear_all() {
        let (store, _tmp) = temp_store();
        store.save_server("s1", StoredCredentials::new("c".into(), None, vec![], None)).await.unwrap();
        store.save_server("s2", StoredCredentials::new("c".into(), None, vec![], None)).await.unwrap();
        store.clear_all().await.unwrap();
        assert!(store.load_server("s1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_list_servers() {
        let (store, _tmp) = temp_store();
        store.save_server("s1", StoredCredentials::new("c".into(), None, vec![], None)).await.unwrap();
        store.save_server("s2", StoredCredentials::new("c".into(), None, vec![], None)).await.unwrap();
        let servers = store.list_servers().await.unwrap();
        assert_eq!(servers.len(), 2);
    }

    #[tokio::test]
    async fn test_concurrent_save_does_not_corrupt() {
        let (store, _tmp) = temp_store();
        let mut handles = vec![];
        for i in 0..10 {
            let s = store.clone();
            handles.push(tokio::spawn(async move {
                s.save_server(&format!("srv{}", i), StoredCredentials::new(format!("c{}", i), None, vec![], None)).await.unwrap();
            }));
        }
        for h in handles { h.await.unwrap(); }
        assert_eq!(store.list_servers().await.unwrap().len(), 10);
    }
}
