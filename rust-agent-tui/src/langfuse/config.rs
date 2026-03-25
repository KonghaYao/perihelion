/// Langfuse 配置（来自环境变量）
pub struct LangfuseConfig {
    pub public_key: String,
    pub secret_key: String,
    pub host: String,
}

impl LangfuseConfig {
    /// 从环境变量读取配置，任一必填字段缺失则返回 None（静默禁用）
    ///
    /// 环境变量：
    ///   LANGFUSE_PUBLIC_KEY  - 必填
    ///   LANGFUSE_SECRET_KEY  - 必填
    ///   LANGFUSE_BASE_URL    - 可选，默认 https://cloud.langfuse.com
    pub fn from_env() -> Option<Self> {
        let public_key = std::env::var("LANGFUSE_PUBLIC_KEY").ok()?;
        let secret_key = std::env::var("LANGFUSE_SECRET_KEY").ok()?;
        let host = std::env::var("LANGFUSE_BASE_URL")
            .unwrap_or_else(|_| "https://cloud.langfuse.com".to_string());
        Some(Self {
            public_key,
            secret_key,
            host,
        })
    }
}
