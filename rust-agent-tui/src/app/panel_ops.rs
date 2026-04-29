use super::*;

impl App {
    // ─── Model 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /model 面板
    pub fn open_model_panel(&mut self) {
        let cfg = self.zen_config.get_or_insert_with(ZenConfig::default);
        self.core.model_panel = Some(ModelPanel::from_config(cfg));
        // 互斥：关闭 login 面板
        self.core.login_panel = None;
    }

    /// 关闭 /model 面板（不保存）
    pub fn close_model_panel(&mut self) {
        self.core.model_panel = None;
    }

    /// 确认选择并保存（Enter 键）：写入 active_provider_id + active_alias + thinking，更新状态栏
    pub fn model_panel_confirm(&mut self) {
        let Some(panel) = self.core.model_panel.as_ref() else {
            return;
        };
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.apply_to_config(cfg);
        if let Err(e) = crate::config::save(cfg) {
            self.core.view_messages.push(MessageViewModel::system(format!("配置保存失败: {}", e)));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
        self.core.model_panel = None;
    }

    // ─── Login 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /login 面板（同时关闭 model 面板，实现互斥）
    pub fn open_login_panel(&mut self) {
        let cfg = self.zen_config.get_or_insert_with(ZenConfig::default);
        self.core.login_panel = Some(login_panel::LoginPanel::from_config(cfg));
        // 互斥：关闭 model 面板
        self.core.model_panel = None;
    }

    /// 关闭 /login 面板（不保存）
    pub fn close_login_panel(&mut self) {
        self.core.login_panel = None;
    }

    /// 选中（激活）光标处的 Provider
    pub fn login_panel_select_provider(&mut self) {
        let Some(panel) = self.core.login_panel.as_mut() else { return };
        let Some(cfg) = self.zen_config.as_mut() else { return };
        panel.select_provider(cfg);
        if let Err(e) = crate::config::save(cfg) {
            self.core.view_messages.push(MessageViewModel::system(format!("配置保存失败: {}", e)));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
    }

    /// 保存 Login 面板的编辑/新建内容到 ZenConfig
    pub fn login_panel_apply_edit(&mut self) {
        let Some(panel) = self.core.login_panel.as_mut() else {
            return;
        };
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.apply_edit(cfg);
        if let Err(e) = crate::config::save(cfg) {
            self.core.view_messages.push(MessageViewModel::system(format!("配置保存失败: {}", e)));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
    }

    /// 确认删除光标处的 Provider
    pub fn login_panel_confirm_delete(&mut self) {
        let Some(panel) = self.core.login_panel.as_mut() else {
            return;
        };
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.confirm_delete(cfg);
        if let Err(e) = crate::config::save(cfg) {
            self.core.view_messages.push(MessageViewModel::system(format!("配置保存失败: {}", e)));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
    }

    // ─── Agent 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /agents 面板（传入扫描到的 agent 列表）
    pub fn open_agent_panel(&mut self, agents: Vec<AgentItem>) {
        self.core.agent_panel = Some(AgentPanel::new(agents, self.agent.agent_id.clone()));
    }

    /// 关闭 /agents 面板（不选择任何 agent）
    pub fn close_agent_panel(&mut self) {
        self.core.agent_panel = None;
    }

    /// 在 agent 面板中上移光标
    pub fn agent_panel_move_up(&mut self) {
        if let Some(panel) = self.core.agent_panel.as_mut() {
            panel.move_cursor(-1);
            panel.scroll_offset =
                ensure_cursor_visible(panel.cursor as u16, panel.scroll_offset, 10);
        }
    }

    /// 在 agent 面板中下移光标
    pub fn agent_panel_move_down(&mut self) {
        if let Some(panel) = self.core.agent_panel.as_mut() {
            panel.move_cursor(1);
            panel.scroll_offset =
                ensure_cursor_visible(panel.cursor as u16, panel.scroll_offset, 10);
        }
    }

    /// 确认选择当前 agent，关闭面板，设置 agent_id
    pub fn agent_panel_confirm(&mut self) {
        // 先取出 selection，避免同时借用 panel 和 agent_id
        let (is_none, agent_id, agent_name) = {
            let panel = match self.core.agent_panel.as_mut() {
                Some(p) => p,
                None => return,
            };
            let (is_none, agent_id) = panel.get_selection();
            let agent_name = if is_none {
                None
            } else {
                agent_id
                    .as_ref()
                    .and_then(|_id| panel.current_agent().map(|a| a.name.clone()))
            };
            (is_none, agent_id, agent_name)
        };

        if is_none {
            self.set_agent_id(None);
            self.core.view_messages.push(MessageViewModel::system(
                "Agent 已重置（未设置 agent_id）".to_string(),
            ));
        } else if let Some(id) = agent_id {
            self.set_agent_id(Some(id.clone()));
            let name = agent_name.unwrap_or_else(|| id.clone());
            self.core.view_messages.push(MessageViewModel::system(format!(
                "Agent 已切换为: {} ({})",
                name, id
            )));
        }
        self.core.agent_panel = None;
    }

    /// 取消选择（不改变当前 agent_id），关闭面板
    #[allow(dead_code)]
    pub fn agent_panel_clear(&mut self) {
        self.core.agent_panel = None;
    }

}

// ─── 测试辅助方法（仅在 cfg(any(test, feature = "headless")) 下编译）──────────

#[cfg(any(test, feature = "headless"))]
impl App {
    /// 向事件队列注入 AgentEvent（测试用）
    pub fn push_agent_event(&mut self, event: AgentEvent) {
        self.agent.agent_event_queue.push(event);
    }

    /// 批量处理队列中所有待处理事件，复用 handle_agent_event 逻辑
    pub fn process_pending_events(&mut self) {
        let events: Vec<AgentEvent> = std::mem::take(&mut self.agent.agent_event_queue);
        for event in events {
            let (_updated, should_break, should_return) = self.handle_agent_event(event);
            if should_return || should_break {
                break;
            }
        }
    }

    /// 构造 Headless 测试用 App，使用 ratatui TestBackend 替代真实终端
    pub fn new_headless(width: u16, height: u16) -> (App, crate::ui::headless::HeadlessHandle) {
        use crate::thread::SqliteThreadStore;
        use ratatui::{backend::TestBackend, Terminal};

        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend).expect("TestBackend should never fail");

        // 启动渲染线程
        let (render_tx, render_cache, render_notify) =
            crate::ui::render_thread::spawn_render_thread(width);

        // 使用唯一临时 SQLite 存储，避免测试并发时文件锁冲突
        let db_name = format!("zen-threads-test-{}.db", uuid::Uuid::now_v7());
        let thread_store: Arc<dyn ThreadStore> = Arc::new(
            SqliteThreadStore::new(std::env::temp_dir().join(db_name))
                .expect("无法创建测试用 SQLite 数据库"),
        );

        let core = super::AppCore::new(
            "/tmp".to_string(),
            render_tx,
            render_cache,
            Arc::clone(&render_notify),
            crate::command::default_registry(),
            Vec::new(),
        );

        let app = App {
            core,
            agent: super::AgentComm::default(),
            langfuse: super::LangfuseState::default(),
            cwd: "/tmp".to_string(),
            provider_name: "test".to_string(),
            model_name: "test-model".to_string(),
            zen_config: None,
            thread_store,
            current_thread_id: None,
            todo_items: Vec::new(),
            cron: super::CronState::default(),
            setup_wizard: None,
            permission_mode: rust_agent_middlewares::prelude::SharedPermissionMode::new(
                rust_agent_middlewares::prelude::PermissionMode::Bypass,
            ),
            mode_highlight_until: None,
            spinner_state: perihelion_widgets::SpinnerState::new(perihelion_widgets::SpinnerMode::Idle),
        };

        let handle = crate::ui::headless::HeadlessHandle {
            terminal,
            render_notify,
        };

        (app, handle)
    }
}
