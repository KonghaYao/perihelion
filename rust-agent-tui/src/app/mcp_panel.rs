use rust_agent_middlewares::mcp::{Resource, ServerInfo, Tool};

use super::AgentEvent;

/// 详情视图中的操作菜单项
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetailAction {
    /// 查看工具列表
    ViewTools,
    /// 重新进行 OAuth 授权
    ReAuthenticate,
    /// 清除 OAuth 凭证
    ClearAuth,
    /// 重新连接
    Reconnect,
    /// 禁用/删除
    Disable,
}

/// MCP 管理面板
pub struct McpPanel {
    /// 服务器列表信息
    pub servers: Vec<ServerInfo>,
    /// 当前选中索引
    pub cursor: usize,
    /// 当前视图层级
    pub view: McpPanelView,
    /// 确认删除弹窗（server name），None 表示非确认状态
    pub confirm_delete: Option<String>,
    /// 详情页滚动偏移
    pub scroll_offset: u16,
}

/// 面板视图层级
pub enum McpPanelView {
    /// 服务器列表
    ServerList,
    /// 服务器详情（元信息 + 操作菜单）
    ServerDetail {
        server_name: String,
        tools: Vec<Tool>,
        resources: Vec<Resource>,
        /// 可用的操作菜单
        actions: Vec<DetailAction>,
        /// 是否展开显示工具列表
        show_tools: bool,
    },
}

impl McpPanelView {
    pub fn is_server_list(&self) -> bool {
        matches!(self, McpPanelView::ServerList)
    }

    /// 获取详情视图操作列表长度
    fn action_count(&self) -> usize {
        match self {
            McpPanelView::ServerList => 0,
            McpPanelView::ServerDetail { actions, .. } => actions.len(),
        }
    }
}

impl McpPanel {
    pub fn new(servers: Vec<ServerInfo>) -> Self {
        Self {
            servers,
            cursor: 0,
            view: McpPanelView::ServerList,
            confirm_delete: None,
            scroll_offset: 0,
        }
    }
}

impl crate::app::App {
    pub fn mcp_panel_move_up(&mut self) {
        if let Some(ref mut panel) = self.mcp_panel {
            match &panel.view {
                McpPanelView::ServerList => {
                    panel.cursor = panel.cursor.saturating_sub(1);
                }
                McpPanelView::ServerDetail { .. } => {
                    let max = panel.view.action_count().saturating_sub(1);
                    panel.cursor = panel.cursor.saturating_sub(1).min(max);
                }
            }
        }
    }

    pub fn mcp_panel_move_down(&mut self) {
        if let Some(ref mut panel) = self.mcp_panel {
            match &panel.view {
                McpPanelView::ServerList => {
                    let max = panel.servers.len().saturating_sub(1);
                    if panel.cursor < max {
                        panel.cursor += 1;
                    }
                }
                McpPanelView::ServerDetail { .. } => {
                    let max = panel.view.action_count().saturating_sub(1);
                    if panel.cursor < max {
                        panel.cursor += 1;
                    }
                }
            }
        }
    }

    pub fn mcp_panel_enter(&mut self) {
        if let Some(ref mut panel) = self.mcp_panel {
            match &panel.view {
                McpPanelView::ServerList => {
                    if panel.cursor >= panel.servers.len() {
                        return;
                    }
                    let name = panel.servers[panel.cursor].name.clone();
                    let server = &panel.servers[panel.cursor];
                    let tools = self
                        .mcp_pool
                        .as_ref()
                        .map(|p| p.get_tools(&name))
                        .unwrap_or_default();
                    let resources = self
                        .mcp_pool
                        .as_ref()
                        .map(|p| p.get_resources(&name))
                        .unwrap_or_default();

                    // 构建操作菜单
                    let mut actions = vec![DetailAction::ViewTools];
                    if server.transport_type == "http" {
                        actions.push(DetailAction::ReAuthenticate);
                        actions.push(DetailAction::ClearAuth);
                    }
                    actions.push(DetailAction::Reconnect);
                    actions.push(DetailAction::Disable);

                    panel.view = McpPanelView::ServerDetail {
                        server_name: name,
                        tools,
                        resources,
                        actions,
                        show_tools: false,
                    };
                    panel.cursor = 0;
                    panel.scroll_offset = 0;
                }
                McpPanelView::ServerDetail { ref actions, .. } => {
                    if panel.cursor >= actions.len() {
                        return;
                    }
                    let action = actions[panel.cursor].clone();
                    self.mcp_panel_execute_action(action);
                }
            }
        }
    }

    /// 执行详情视图选中的操作
    fn mcp_panel_execute_action(&mut self, action: DetailAction) {
        let server_name = match &self.mcp_panel.as_ref().unwrap().view {
            McpPanelView::ServerDetail { server_name, .. } => server_name.clone(),
            _ => return,
        };
        match action {
            DetailAction::ViewTools => {
                if let Some(ref mut panel) = self.mcp_panel {
                    if let McpPanelView::ServerDetail {
                        ref mut show_tools, ..
                    } = panel.view
                    {
                        *show_tools = !*show_tools;
                    }
                }
            }
            DetailAction::ReAuthenticate => {
                self.mcp_panel_back();
                self.set_mcp_cursor_to_server(&server_name);
                self.mcp_panel_request_oauth();
            }
            DetailAction::ClearAuth => {
                self.mcp_panel_back();
                self.set_mcp_cursor_to_server(&server_name);
                let pool = self.mcp_pool.clone();
                let tx = self.bg_event_tx.clone();
                let name_clone = server_name.clone();
                if let Some(pool) = pool {
                    tokio::spawn(async move {
                        let result = pool.clear_oauth(&name_clone).await;
                        let _ = tx.try_send(super::events::AgentEvent::McpActionCompleted {
                            server_name: name_clone,
                            action: "clear_auth".to_string(),
                            success: result.is_ok(),
                        });
                    });
                }
            }
            DetailAction::Reconnect => {
                self.mcp_panel_back();
                self.set_mcp_cursor_to_server(&server_name);
                self.mcp_panel_reconnect();
            }
            DetailAction::Disable => {
                self.mcp_panel_back();
                self.set_mcp_cursor_to_server(&server_name);
                self.mcp_panel_request_delete();
            }
        }
    }

    /// 将 MCP 面板 cursor 设置到指定服务器
    fn set_mcp_cursor_to_server(&mut self, server_name: &str) {
        if let Some(ref mut panel) = self.mcp_panel {
            panel.cursor = panel
                .servers
                .iter()
                .position(|s| s.name == server_name)
                .unwrap_or(0);
        }
    }

    pub fn mcp_panel_back(&mut self) {
        if let Some(ref mut panel) = self.mcp_panel {
            if panel.view.is_server_list() {
                return;
            }
            // 记住之前选中的服务器名称，返回列表时恢复 cursor
            let name = match &panel.view {
                McpPanelView::ServerDetail { server_name, .. } => server_name.clone(),
                _ => String::new(),
            };
            panel.view = McpPanelView::ServerList;
            panel.cursor = panel
                .servers
                .iter()
                .position(|s| s.name == name)
                .unwrap_or(0);
            panel.scroll_offset = 0;
        }
    }

    pub fn mcp_panel_scroll_up(&mut self, delta: u16) {
        if let Some(ref mut panel) = self.mcp_panel {
            panel.scroll_offset = panel.scroll_offset.saturating_sub(delta);
        }
    }

    pub fn mcp_panel_scroll_down(&mut self, delta: u16) {
        if let Some(ref mut panel) = self.mcp_panel {
            panel.scroll_offset = panel.scroll_offset.saturating_add(delta);
        }
    }

    pub fn mcp_panel_request_delete(&mut self) {
        if let Some(ref mut panel) = self.mcp_panel {
            if !panel.view.is_server_list() {
                return;
            }
            if panel.cursor >= panel.servers.len() {
                return;
            }
            panel.confirm_delete = Some(panel.servers[panel.cursor].name.clone());
        }
    }

    pub fn mcp_panel_confirm_delete(&mut self) {
        if let Some(ref mut panel) = self.mcp_panel {
            let name = match panel.confirm_delete.take() {
                Some(n) => n,
                None => return,
            };
            // 异步断开连接
            if let Some(pool) = self.mcp_pool.clone() {
                let name_clone = name.clone();
                tokio::spawn(async move {
                    pool.remove_server(&name_clone).await;
                });
            }
            // 持久化删除配置
            let _ = rust_agent_middlewares::mcp::remove_server_from_config(
                std::path::Path::new(&self.cwd),
                &name,
            );
            // 刷新列表
            panel.servers = self
                .mcp_pool
                .as_ref()
                .map(|p| p.server_infos())
                .unwrap_or_default();
            if panel.cursor >= panel.servers.len() && !panel.servers.is_empty() {
                panel.cursor = panel.servers.len() - 1;
            }
            if panel.servers.is_empty() {
                self.mcp_panel = None;
            }
        }
    }

    pub fn mcp_panel_cancel_delete(&mut self) {
        if let Some(ref mut panel) = self.mcp_panel {
            panel.confirm_delete = None;
        }
    }

    pub fn mcp_panel_reconnect(&mut self) {
        if let Some(ref mut panel) = self.mcp_panel {
            if !panel.view.is_server_list() {
                return;
            }
            if panel.cursor >= panel.servers.len() {
                return;
            }
            let name = panel.servers[panel.cursor].name.clone();
            if let Some(pool) = self.mcp_pool.clone() {
                let tx = self.bg_event_tx.clone();
                let pool2 = pool.clone();
                let name2 = name.clone();
                let tx2 = tx.clone();
                let oauth_cb: Box<
                    dyn Fn(rust_agent_middlewares::mcp::OAuthFlowEvent) + Send + Sync,
                > = Box::new(move |ev| {
                    use rust_agent_middlewares::mcp::OAuthFlowEvent;
                    if let OAuthFlowEvent::AuthorizationNeeded {
                        server_name,
                        authorization_url,
                        callback_tx,
                    } = ev
                    {
                        let _ = tx2.try_send(AgentEvent::OAuthAuthorizationNeeded {
                            server_name,
                            authorization_url,
                            callback_tx,
                        });
                    }
                });
                tokio::spawn(async move {
                    let result = pool2.reconnect(&name2, Some(oauth_cb)).await;
                    let _ = tx
                        .send(AgentEvent::McpActionCompleted {
                            server_name: name2,
                            action: "reconnect".to_string(),
                            success: result.is_ok(),
                        })
                        .await;
                });
            }
        }
    }

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
            if server.transport_type != "http" {
                return;
            }
            let name = server.name.clone();
            if let Some(pool) = self.mcp_pool.clone() {
                let tx = self.bg_event_tx.clone();
                let ok_tx = self.bg_event_tx.clone();
                let err_tx = self.bg_event_tx.clone();
                tokio::spawn(async move {
                    let result = pool
                        .start_oauth_flow(
                            &name,
                            Box::new(move |ev| {
                                // 只传播 AuthorizationNeeded（弹窗需要显示 URL），
                                // 完成/失败事件由 spawned task 在 pool 更新后统一发送，
                                // 避免 pool 未更新时面板就刷新导致显示 0 servers
                                use rust_agent_middlewares::mcp::OAuthFlowEvent;
                                if let OAuthFlowEvent::AuthorizationNeeded {
                                    server_name,
                                    authorization_url,
                                    callback_tx,
                                } = ev
                                {
                                    let _ = tx.try_send(
                                        super::events::AgentEvent::OAuthAuthorizationNeeded {
                                            server_name,
                                            authorization_url,
                                            callback_tx,
                                        },
                                    );
                                }
                            }),
                        )
                        .await;
                    if let Err(e) = result {
                        let _ =
                            err_tx.try_send(super::events::AgentEvent::OAuthAuthorizationFailed {
                                server_name: name,
                                error: e.to_string(),
                            });
                    } else {
                        // pool 已更新（start_oauth_flow 内部插入了新的 connected handle），安全刷新
                        let _ = ok_tx.try_send(
                            super::events::AgentEvent::OAuthAuthorizationCompleted {
                                server_name: name,
                            },
                        );
                    }
                });
            }
        }
    }

    pub fn mcp_panel_close(&mut self) {
        self.mcp_panel = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_agent_middlewares::mcp::ClientStatus;

    fn make_server_info(name: &str, status: ClientStatus) -> ServerInfo {
        ServerInfo {
            name: name.to_string(),
            transport_type: "stdio".to_string(),
            status,
            tool_count: 0,
            resource_count: 0,
            oauth_status: Default::default(),
            source: None,
            url: None,
        }
    }

    #[tokio::test]
    async fn test_mcp_panel_new() {
        let panel = McpPanel::new(vec![]);
        assert_eq!(panel.cursor, 0);
        assert!(matches!(panel.view, McpPanelView::ServerList));
        assert!(panel.confirm_delete.is_none());

        let servers = vec![
            make_server_info("a", ClientStatus::Connected),
            make_server_info("b", ClientStatus::Failed("err".into())),
            make_server_info("c", ClientStatus::Connected),
        ];
        let panel = McpPanel::new(servers);
        assert_eq!(panel.servers.len(), 3);
    }

    #[tokio::test]
    async fn test_mcp_panel_move_cursor() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24);
        let servers = vec![
            make_server_info("a", ClientStatus::Connected),
            make_server_info("b", ClientStatus::Connected),
            make_server_info("c", ClientStatus::Connected),
        ];
        app.mcp_panel = Some(McpPanel::new(servers));

        for _ in 0..5 {
            app.mcp_panel_move_up();
        }
        assert_eq!(app.mcp_panel.as_ref().unwrap().cursor, 0);

        for _ in 0..5 {
            app.mcp_panel_move_down();
        }
        assert_eq!(app.mcp_panel.as_ref().unwrap().cursor, 2);
    }

    #[tokio::test]
    async fn test_mcp_panel_close() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24);
        app.mcp_panel = Some(McpPanel::new(vec![]));
        assert!(app.mcp_panel.is_some());
        app.mcp_panel_close();
        assert!(app.mcp_panel.is_none());
    }

    #[tokio::test]
    async fn test_mcp_panel_request_cancel_delete() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24);
        app.mcp_panel = Some(McpPanel::new(vec![make_server_info(
            "test-srv",
            ClientStatus::Connected,
        )]));

        app.mcp_panel_request_delete();
        assert_eq!(
            app.mcp_panel.as_ref().unwrap().confirm_delete,
            Some("test-srv".to_string())
        );

        app.mcp_panel_cancel_delete();
        assert!(app.mcp_panel.as_ref().unwrap().confirm_delete.is_none());
    }

    #[tokio::test]
    async fn test_mcp_panel_enter_builds_actions() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24);
        let mut srv = make_server_info("http-srv", ClientStatus::Connected);
        srv.transport_type = "http".to_string();
        app.mcp_panel = Some(McpPanel::new(vec![srv]));

        app.mcp_panel_enter();
        match &app.mcp_panel.as_ref().unwrap().view {
            McpPanelView::ServerDetail { actions, .. } => {
                assert!(actions.contains(&DetailAction::ReAuthenticate));
                assert!(actions.contains(&DetailAction::ClearAuth));
                assert!(actions.contains(&DetailAction::Reconnect));
                assert!(actions.contains(&DetailAction::Disable));
            }
            _ => panic!("应进入 ServerDetail 视图"),
        }
    }

    #[tokio::test]
    async fn test_mcp_panel_back_restores_cursor() {
        let (mut app, _handle) = crate::app::App::new_headless(80, 24);
        app.mcp_panel = Some(McpPanel::new(vec![
            make_server_info("a", ClientStatus::Connected),
            make_server_info("b", ClientStatus::Connected),
        ]));
        app.mcp_panel.as_mut().unwrap().cursor = 1;
        app.mcp_panel_enter();
        app.mcp_panel_back();
        assert_eq!(app.mcp_panel.as_ref().unwrap().cursor, 1);
    }
}
