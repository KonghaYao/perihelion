use std::any::Any;

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::Frame;
use tui_textarea::Input;

use rust_agent_middlewares::mcp::{ClientStatus, ConfigSource, Resource, ServerInfo, Tool};

use super::panel_component::PanelComponent;
use super::panel_list::PanelList;
use super::panel_manager::{EventResult, PanelContext, PanelKind};
use super::AgentEvent;
use super::App;

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
    /// 禁用（已连接的服务器）
    Disable,
    /// 启用（已禁用的服务器）
    Enable,
}

/// MCP 管理面板
#[derive(Clone)]
pub struct McpPanel {
    /// 服务器列表信息
    pub servers: Vec<ServerInfo>,
    /// ServerList 视图的列表状态（cursor + scroll_offset）
    server_list: PanelList<ServerInfo>,
    /// 当前视图层级
    pub view: McpPanelView,
    /// 确认删除弹窗（server name），None 表示非确认状态
    pub confirm_delete: Option<String>,
    /// ServerDetail 视图的光标
    detail_cursor: usize,
    /// ServerDetail 视图的滚动偏移
    detail_scroll_offset: u16,
}

/// 面板视图层级
#[derive(Clone)]
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
    pub fn new(mut servers: Vec<ServerInfo>) -> Self {
        // 排序以匹配视觉分组顺序：Project 在前，user（Global/Plugin/None）在后
        // 否则 panel.servers[cursor] 与列表页渲染的 visual cursor 不一致
        servers.sort_by(|a, b| {
            let a_is_project = matches!(a.source, Some(ConfigSource::Project(_)));
            let b_is_project = matches!(b.source, Some(ConfigSource::Project(_)));
            b_is_project
                .cmp(&a_is_project)
                .then_with(|| a.name.cmp(&b.name))
        });
        let mut server_list = PanelList::new();
        server_list.set_items(servers.clone());
        Self {
            servers,
            server_list,
            view: McpPanelView::ServerList,
            confirm_delete: None,
            detail_cursor: 0,
            detail_scroll_offset: 0,
        }
    }

    /// 委托方法：根据当前视图返回光标位置
    pub fn cursor(&self) -> usize {
        match &self.view {
            McpPanelView::ServerList => self.server_list.cursor(),
            McpPanelView::ServerDetail { .. } => self.detail_cursor,
        }
    }

    /// 委托方法：根据当前视图返回滚动偏移
    pub fn scroll_offset(&self) -> u16 {
        match &self.view {
            McpPanelView::ServerList => self.server_list.scroll_offset(),
            McpPanelView::ServerDetail { .. } => self.detail_scroll_offset,
        }
    }
}

impl crate::app::App {
    pub fn mcp_panel_move_up(&mut self) {
        if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
            match &panel.view {
                McpPanelView::ServerList => {
                    panel.server_list.move_cursor(-1);
                    panel.server_list.ensure_visible(16);
                }
                McpPanelView::ServerDetail { .. } => {
                    let max = panel.view.action_count().saturating_sub(1);
                    panel.detail_cursor = panel.detail_cursor.saturating_sub(1).min(max);
                }
            }
        }
    }

    pub fn mcp_panel_move_down(&mut self) {
        if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
            match &panel.view {
                McpPanelView::ServerList => {
                    panel.server_list.move_cursor(1);
                    panel.server_list.ensure_visible(16);
                }
                McpPanelView::ServerDetail { .. } => {
                    let max = panel.view.action_count().saturating_sub(1);
                    if panel.detail_cursor < max {
                        panel.detail_cursor += 1;
                    }
                }
            }
        }
    }

    pub fn mcp_panel_enter(&mut self) {
        if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
            match &panel.view {
                McpPanelView::ServerList => {
                    if panel.server_list.cursor() >= panel.servers.len() {
                        return;
                    }
                    let name = panel.servers[panel.server_list.cursor()].name.clone();
                    let server = &panel.servers[panel.server_list.cursor()];
                    let tools = self
                        .services
                        .mcp_pool
                        .as_ref()
                        .map(|p| p.get_tools(&name))
                        .unwrap_or_default();
                    let resources = self
                        .services
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
                    // Uninitialized server: only Reconnect (can't view tools/disable etc.)
                    if server.status == ClientStatus::Uninitialized {
                        actions = vec![DetailAction::Reconnect];
                    } else {
                        actions.push(DetailAction::Reconnect);
                        // 根据当前状态显示 Enable 或 Disable
                        if matches!(server.status, ClientStatus::Disabled) {
                            actions.push(DetailAction::Enable);
                        } else {
                            actions.push(DetailAction::Disable);
                        }
                    }

                    panel.view = McpPanelView::ServerDetail {
                        server_name: name,
                        tools,
                        resources,
                        actions,
                        show_tools: false,
                    };
                    panel.detail_cursor = 0;
                    panel.detail_scroll_offset = 0;
                }
                McpPanelView::ServerDetail { ref actions, .. } => {
                    if panel.detail_cursor >= actions.len() {
                        return;
                    }
                    let action = actions[panel.detail_cursor].clone();
                    self.mcp_panel_execute_action(action);
                }
            }
        }
    }

    /// 执行详情视图选中的操作
    fn mcp_panel_execute_action(&mut self, action: DetailAction) {
        let server_name = match &self.global_panels.get::<McpPanel>().unwrap().view {
            McpPanelView::ServerDetail { server_name, .. } => server_name.clone(),
            _ => return,
        };
        match action {
            DetailAction::ViewTools => {
                if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
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
                let pool = self.services.mcp_pool.clone();
                let tx = self.services.bg_event_tx.clone();
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
                self.mcp_panel_toggle_disabled(&server_name, true);
            }
            DetailAction::Enable => {
                self.mcp_panel_back();
                self.set_mcp_cursor_to_server(&server_name);
                self.mcp_panel_toggle_disabled(&server_name, false);
            }
        }
    }

    /// 将 MCP 面板 cursor 设置到指定服务器
    fn set_mcp_cursor_to_server(&mut self, server_name: &str) {
        if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
            let pos = panel
                .servers
                .iter()
                .position(|s| s.name == server_name)
                .unwrap_or(0);
            panel.server_list.move_cursor_to(pos);
        }
    }

    pub fn mcp_panel_back(&mut self) {
        if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
            if panel.view.is_server_list() {
                return;
            }
            // 记住之前选中的服务器名称，返回列表时恢复 cursor
            let name = match &panel.view {
                McpPanelView::ServerDetail { server_name, .. } => server_name.clone(),
                _ => String::new(),
            };
            panel.view = McpPanelView::ServerList;
            let pos = panel
                .servers
                .iter()
                .position(|s| s.name == name)
                .unwrap_or(0);
            panel.server_list.move_cursor_to(pos);
            panel.server_list.ensure_visible(16);
            panel.detail_scroll_offset = 0;
        }
    }

    pub fn mcp_panel_request_delete(&mut self) {
        if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
            if !panel.view.is_server_list() {
                return;
            }
            if panel.cursor() >= panel.servers.len() {
                return;
            }
            panel.confirm_delete = Some(panel.servers[panel.cursor()].name.clone());
        }
    }

    /// 切换服务器的 disabled 状态
    fn mcp_panel_toggle_disabled(&mut self, server_name: &str, disabled: bool) {
        // 持久化 disabled 字段到配置文件
        let _ = rust_agent_middlewares::mcp::set_server_disabled(
            std::path::Path::new(&self.services.cwd),
            server_name,
            disabled,
        );

        if disabled {
            // 禁用：断开连接，将 handle 状态设为 Disabled（保留 config 和 handle）
            if let Some(pool) = self.services.mcp_pool.clone() {
                let name_clone = server_name.to_string();
                tokio::spawn(async move {
                    pool.set_disabled(&name_clone).await;
                });
            }
        } else {
            // 启用：触发重连（使用 pool 中保存的 config）
            if let Some(pool) = self.services.mcp_pool.clone() {
                let tx = self.services.bg_event_tx.clone();
                let pool2 = pool.clone();
                let name2 = server_name.to_string();
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

        // 刷新面板列表
        if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
            panel.servers = self
                .services
                .mcp_pool
                .as_ref()
                .map(|p| p.all_server_infos())
                .unwrap_or_default();
            panel.server_list.set_items(panel.servers.clone());
            panel.server_list.clamp_cursor();
        }
    }

    pub fn mcp_panel_confirm_delete(&mut self) {
        if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
            let name = match panel.confirm_delete.take() {
                Some(n) => n,
                None => return,
            };
            // 异步断开连接
            if let Some(pool) = self.services.mcp_pool.clone() {
                let name_clone = name.clone();
                tokio::spawn(async move {
                    pool.remove_server(&name_clone).await;
                });
            }
            // 持久化删除配置
            let _ = rust_agent_middlewares::mcp::remove_server_from_config(
                std::path::Path::new(&self.services.cwd),
                &name,
            );
            // 刷新列表
            panel.servers = self
                .services
                .mcp_pool
                .as_ref()
                .map(|p| p.all_server_infos())
                .unwrap_or_default();
            panel.server_list.set_items(panel.servers.clone());
            panel.server_list.clamp_cursor();
            if panel.servers.is_empty() {
                self.global_panels.close();
            }
        }
    }

    pub fn mcp_panel_cancel_delete(&mut self) {
        if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
            panel.confirm_delete = None;
        }
    }

    pub fn mcp_panel_reconnect(&mut self) {
        if let Some(ref mut panel) = self.global_panels.get_mut::<McpPanel>() {
            if !panel.view.is_server_list() {
                return;
            }
            if panel.cursor() >= panel.servers.len() {
                return;
            }
            let name = panel.servers[panel.cursor()].name.clone();
            if let Some(pool) = self.services.mcp_pool.clone() {
                let tx = self.services.bg_event_tx.clone();
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
        if let Some(panel) = self.global_panels.get::<McpPanel>() {
            if !panel.view.is_server_list() {
                return;
            }
            if panel.cursor() >= panel.servers.len() {
                return;
            }
            let server = &panel.servers[panel.cursor()];
            if server.transport_type != "http" {
                return;
            }
            let name = server.name.clone();
            if let Some(pool) = self.services.mcp_pool.clone() {
                let tx = self.services.bg_event_tx.clone();
                let ok_tx = self.services.bg_event_tx.clone();
                let err_tx = self.services.bg_event_tx.clone();
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
        self.global_panels.close();
    }
}

impl PanelComponent for McpPanel {
    fn kind(&self) -> PanelKind {
        PanelKind::Mcp
    }

    fn handle_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use tui_textarea::Key;

        // confirm_delete mode
        if self.confirm_delete.is_some() {
            match input {
                Input {
                    key: Key::Enter, ..
                } => {
                    self.do_confirm_delete(ctx);
                    // if servers empty after delete, close
                    if self.servers.is_empty() {
                        EventResult::ClosePanel
                    } else {
                        EventResult::Consumed
                    }
                }
                _ => {
                    self.confirm_delete = None;
                    EventResult::Consumed
                }
            }
        } else {
            let is_server_list = self.view.is_server_list();
            match input {
                Input { key: Key::Up, .. } => {
                    self.do_move_up();
                    EventResult::Consumed
                }
                Input { key: Key::Down, .. } => {
                    self.do_move_down();
                    EventResult::Consumed
                }
                Input {
                    key: Key::Enter, ..
                } => {
                    self.do_enter(ctx);
                    EventResult::Consumed
                }
                Input { key: Key::Esc, .. } => {
                    if is_server_list {
                        EventResult::ClosePanel
                    } else {
                        self.do_back();
                        EventResult::Consumed
                    }
                }
                Input {
                    key: Key::Char('r'),
                    ctrl: true,
                    ..
                } if is_server_list => {
                    self.do_reconnect(ctx);
                    EventResult::Consumed
                }
                Input {
                    key: Key::Char('d'),
                    ctrl: true,
                    ..
                } if is_server_list => {
                    self.do_request_delete();
                    EventResult::Consumed
                }
                _ => EventResult::Consumed,
            }
        }
    }

    fn handle_scroll(&mut self, lines: i16, _ctx: &mut PanelContext<'_>) -> EventResult {
        match &self.view {
            McpPanelView::ServerList => {
                self.server_list.handle_scroll(lines, 16);
            }
            McpPanelView::ServerDetail { .. } => {
                if lines > 0 {
                    self.detail_scroll_offset =
                        self.detail_scroll_offset.saturating_add(lines as u16);
                } else {
                    self.detail_scroll_offset =
                        self.detail_scroll_offset.saturating_sub((-lines) as u16);
                }
            }
        }
        EventResult::Consumed
    }

    fn handle_mouse(
        &mut self,
        mouse: MouseEvent,
        area: Rect,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            match &self.view {
                McpPanelView::ServerList => {
                    if self
                        .server_list
                        .handle_mouse_click(mouse.row, mouse.column, area, 2)
                    {
                        return self.handle_key(
                            Input::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                            ctx,
                        );
                    }
                }
                McpPanelView::ServerDetail { actions, .. } => {
                    let inner_y = area.y + 4;
                    if mouse.row >= inner_y {
                        let clicked = (mouse.row - inner_y) as usize;
                        if clicked < actions.len() {
                            self.detail_cursor = clicked;
                            let action = actions[clicked].clone();
                            self.do_execute_action(ctx, action);
                            return EventResult::Consumed;
                        }
                    }
                }
            }
        }
        EventResult::NotConsumed
    }

    fn desired_height(&self, _screen_height: u16, _screen_width: u16) -> u16 {
        match &self.view {
            McpPanelView::ServerList => 16,
            McpPanelView::ServerDetail { .. } => 20,
        }
    }

    fn render(&mut self, f: &mut Frame, app: &mut App, area: Rect) {
        crate::ui::main_ui::panels::mcp::render_mcp_panel(f, self, app, area);
    }

    fn as_any_ref(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn status_bar_hints(&self, _lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        if self.confirm_delete.is_some() {
            return vec![
                ("Enter".to_string(), _lc.tr("key-delete")),
                ("Esc".to_string(), _lc.tr("key-cancel")),
            ];
        }
        if self.view.is_server_list() {
            vec![
                ("\u{2191}\u{2193}".to_string(), _lc.tr("key-move")),
                ("Enter".to_string(), _lc.tr("key-detail")),
                ("Ctrl+R".to_string(), _lc.tr("key-reconnect")),
                ("Ctrl+D".to_string(), _lc.tr("key-delete")),
                ("Esc".to_string(), _lc.tr("key-close")),
            ]
        } else {
            vec![
                ("\u{2191}\u{2193}".to_string(), _lc.tr("key-move")),
                ("Enter".to_string(), _lc.tr("key-execute")),
                ("Esc".to_string(), _lc.tr("key-back")),
            ]
        }
    }
}

impl McpPanel {
    fn do_move_up(&mut self) {
        match &self.view {
            McpPanelView::ServerList => {
                self.server_list.move_cursor(-1);
                self.server_list.ensure_visible(16);
            }
            McpPanelView::ServerDetail { .. } => {
                let max = self.view.action_count().saturating_sub(1);
                self.detail_cursor = self.detail_cursor.saturating_sub(1).min(max);
            }
        }
    }

    fn do_move_down(&mut self) {
        match &self.view {
            McpPanelView::ServerList => {
                self.server_list.move_cursor(1);
                self.server_list.ensure_visible(16);
            }
            McpPanelView::ServerDetail { .. } => {
                let max = self.view.action_count().saturating_sub(1);
                if self.detail_cursor < max {
                    self.detail_cursor += 1;
                }
            }
        }
    }

    fn do_enter(&mut self, ctx: &mut PanelContext<'_>) {
        match &self.view {
            McpPanelView::ServerList => {
                if self.cursor() >= self.servers.len() {
                    return;
                }
                let idx = self.cursor();
                let name = self.servers[idx].name.clone();
                let server = &self.servers[idx];
                let tools = ctx
                    .services
                    .mcp_pool
                    .as_ref()
                    .map(|p| p.get_tools(&name))
                    .unwrap_or_default();
                let resources = ctx
                    .services
                    .mcp_pool
                    .as_ref()
                    .map(|p| p.get_resources(&name))
                    .unwrap_or_default();

                // Build actions menu
                let mut actions = vec![DetailAction::ViewTools];
                if server.transport_type == "http" {
                    actions.push(DetailAction::ReAuthenticate);
                    actions.push(DetailAction::ClearAuth);
                }
                if server.status == ClientStatus::Uninitialized {
                    actions = vec![DetailAction::Reconnect];
                } else {
                    actions.push(DetailAction::Reconnect);
                    if matches!(server.status, ClientStatus::Disabled) {
                        actions.push(DetailAction::Enable);
                    } else {
                        actions.push(DetailAction::Disable);
                    }
                }

                self.view = McpPanelView::ServerDetail {
                    server_name: name,
                    tools,
                    resources,
                    actions,
                    show_tools: false,
                };
                self.detail_cursor = 0;
                self.detail_scroll_offset = 0;
            }
            McpPanelView::ServerDetail { actions, .. } => {
                if self.detail_cursor >= actions.len() {
                    return;
                }
                let action = actions[self.detail_cursor].clone();
                self.do_execute_action(ctx, action);
            }
        }
    }

    fn do_back(&mut self) {
        if self.view.is_server_list() {
            return;
        }
        let name = match &self.view {
            McpPanelView::ServerDetail { server_name, .. } => server_name.clone(),
            _ => String::new(),
        };
        self.view = McpPanelView::ServerList;
        let pos = self
            .servers
            .iter()
            .position(|s| s.name == name)
            .unwrap_or(0);
        self.server_list.move_cursor_to(pos);
        self.server_list.ensure_visible(16);
        self.detail_scroll_offset = 0;
    }

    fn do_request_delete(&mut self) {
        if !self.view.is_server_list() {
            return;
        }
        if self.cursor() >= self.servers.len() {
            return;
        }
        self.confirm_delete = Some(self.servers[self.cursor()].name.clone());
    }

    fn do_confirm_delete(&mut self, ctx: &mut PanelContext<'_>) {
        let name = match self.confirm_delete.take() {
            Some(n) => n,
            None => return,
        };
        // Async disconnect
        if let Some(pool) = ctx.services.mcp_pool.clone() {
            let name_clone = name.clone();
            tokio::spawn(async move {
                pool.remove_server(&name_clone).await;
            });
        }
        // Persist delete
        let _ = rust_agent_middlewares::mcp::remove_server_from_config(
            std::path::Path::new(&ctx.services.cwd),
            &name,
        );
        // Refresh list
        self.servers = ctx
            .services
            .mcp_pool
            .as_ref()
            .map(|p| p.all_server_infos())
            .unwrap_or_default();
        self.server_list.set_items(self.servers.clone());
        self.server_list.clamp_cursor();
    }

    fn do_reconnect(&mut self, ctx: &mut PanelContext<'_>) {
        if !self.view.is_server_list() {
            return;
        }
        if self.cursor() >= self.servers.len() {
            return;
        }
        let name = self.servers[self.cursor()].name.clone();
        if let Some(pool) = ctx.services.mcp_pool.clone() {
            let tx = ctx.services.bg_event_tx.clone();
            let pool2 = pool.clone();
            let name2 = name.clone();
            let tx2 = tx.clone();
            let oauth_cb: Box<dyn Fn(rust_agent_middlewares::mcp::OAuthFlowEvent) + Send + Sync> =
                Box::new(move |ev| {
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

    fn do_execute_action(&mut self, ctx: &mut PanelContext<'_>, action: DetailAction) {
        let server_name = match &self.view {
            McpPanelView::ServerDetail { server_name, .. } => server_name.clone(),
            _ => return,
        };
        match action {
            DetailAction::ViewTools => {
                if let McpPanelView::ServerDetail {
                    ref mut show_tools, ..
                } = self.view
                {
                    *show_tools = !*show_tools;
                }
            }
            DetailAction::ReAuthenticate => {
                self.do_back();
                self.set_cursor_to_server(&server_name);
                self.do_request_oauth(ctx);
            }
            DetailAction::ClearAuth => {
                self.do_back();
                self.set_cursor_to_server(&server_name);
                let pool = ctx.services.mcp_pool.clone();
                let tx = ctx.services.bg_event_tx.clone();
                let name_clone = server_name.clone();
                if let Some(pool) = pool {
                    tokio::spawn(async move {
                        let result = pool.clear_oauth(&name_clone).await;
                        let _ = tx.try_send(AgentEvent::McpActionCompleted {
                            server_name: name_clone,
                            action: "clear_auth".to_string(),
                            success: result.is_ok(),
                        });
                    });
                }
            }
            DetailAction::Reconnect => {
                self.do_back();
                self.set_cursor_to_server(&server_name);
                self.do_reconnect(ctx);
            }
            DetailAction::Disable => {
                self.do_back();
                self.set_cursor_to_server(&server_name);
                Self::toggle_disabled(ctx, &server_name, true);
                // Refresh
                self.servers = ctx
                    .services
                    .mcp_pool
                    .as_ref()
                    .map(|p| p.all_server_infos())
                    .unwrap_or_default();
                self.server_list.set_items(self.servers.clone());
                self.server_list.clamp_cursor();
            }
            DetailAction::Enable => {
                self.do_back();
                self.set_cursor_to_server(&server_name);
                Self::toggle_disabled(ctx, &server_name, false);
                // Refresh
                self.servers = ctx
                    .services
                    .mcp_pool
                    .as_ref()
                    .map(|p| p.all_server_infos())
                    .unwrap_or_default();
                self.server_list.set_items(self.servers.clone());
                self.server_list.clamp_cursor();
            }
        }
    }

    fn set_cursor_to_server(&mut self, server_name: &str) {
        let pos = self
            .servers
            .iter()
            .position(|s| s.name == server_name)
            .unwrap_or(0);
        self.server_list.move_cursor_to(pos);
    }

    fn do_request_oauth(&mut self, ctx: &mut PanelContext<'_>) {
        if !self.view.is_server_list() {
            return;
        }
        if self.cursor() >= self.servers.len() {
            return;
        }
        let server = &self.servers[self.cursor()];
        if server.transport_type != "http" {
            return;
        }
        let name = server.name.clone();
        if let Some(pool) = ctx.services.mcp_pool.clone() {
            let tx = ctx.services.bg_event_tx.clone();
            let ok_tx = ctx.services.bg_event_tx.clone();
            let err_tx = ctx.services.bg_event_tx.clone();
            tokio::spawn(async move {
                let result = pool
                    .start_oauth_flow(
                        &name,
                        Box::new(move |ev| {
                            use rust_agent_middlewares::mcp::OAuthFlowEvent;
                            if let OAuthFlowEvent::AuthorizationNeeded {
                                server_name,
                                authorization_url,
                                callback_tx,
                            } = ev
                            {
                                let _ = tx.try_send(AgentEvent::OAuthAuthorizationNeeded {
                                    server_name,
                                    authorization_url,
                                    callback_tx,
                                });
                            }
                        }),
                    )
                    .await;
                if let Err(e) = result {
                    let _ = err_tx.try_send(AgentEvent::OAuthAuthorizationFailed {
                        server_name: name,
                        error: e.to_string(),
                    });
                } else {
                    let _ = ok_tx
                        .try_send(AgentEvent::OAuthAuthorizationCompleted { server_name: name });
                }
            });
        }
    }

    fn toggle_disabled(ctx: &mut PanelContext<'_>, server_name: &str, disabled: bool) {
        let _ = rust_agent_middlewares::mcp::set_server_disabled(
            std::path::Path::new(&ctx.services.cwd),
            server_name,
            disabled,
        );

        if disabled {
            if let Some(pool) = ctx.services.mcp_pool.clone() {
                let name_clone = server_name.to_string();
                tokio::spawn(async move {
                    pool.set_disabled(&name_clone).await;
                });
            }
        } else {
            if let Some(pool) = ctx.services.mcp_pool.clone() {
                let tx = ctx.services.bg_event_tx.clone();
                let pool2 = pool.clone();
                let name2 = server_name.to_string();
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_agent_middlewares::mcp::ClientStatus;
    include!("mcp_panel_test.rs");
}
