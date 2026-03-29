use std::sync::Arc;

/// Relay 连接状态：客户端、事件接收端、重连参数
pub struct RelayState {
    /// Relay 客户端（远程控制，可选）
    pub relay_client: Option<Arc<rust_relay_server::client::RelayClient>>,
    /// Relay 事件接收端（来自 Web 端的控制消息）
    pub relay_event_rx: Option<rust_relay_server::client::RelayEventRx>,
    /// Relay 连接参数缓存（url, token, name, user_id），断线后自动重连使用
    pub relay_params: Option<(String, String, Option<String>, String)>,
    /// Relay 重连计划时间（达到后尝试重连，None 表示不需要重连）
    pub relay_reconnect_at: Option<std::time::Instant>,
}

impl Default for RelayState {
    fn default() -> Self {
        Self {
            relay_client: None,
            relay_event_rx: None,
            relay_params: None,
            relay_reconnect_at: None,
        }
    }
}
