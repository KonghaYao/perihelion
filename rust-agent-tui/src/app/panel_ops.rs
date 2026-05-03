use super::*;

impl App {
    // ─── Model 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /model 面板
    pub fn open_model_panel(&mut self) {
        let cfg = self.zen_config.get_or_insert_with(ZenConfig::default);
        self.core.model_panel = Some(ModelPanel::from_config(cfg));
        // 互斥：关闭其他面板
        self.core.login_panel = None;
        self.core.config_panel = None;
        self.status_panel = None;
        self.memory_panel = None;
    }

    /// 关闭 /model 面板（不保存）
    pub fn close_model_panel(&mut self) {
        self.core.model_panel = None;
    }

    /// 确认选择并保存（Enter 键）：写入 active_alias + effort，更新状态栏
    pub fn model_panel_confirm(&mut self) {
        let Some(panel) = self.core.model_panel.as_ref() else {
            return;
        };
        let alias_label = panel.active_tab.label().to_string();
        let effort = panel.buf_thinking_effort.clone();
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.apply_to_config(cfg);
        let effort_display = match effort.as_str() {
            "low" => "Low",
            "high" => "High",
            _ => "Medium",
        };
        self.core
            .view_messages
            .push(MessageViewModel::system(format!(
                "模型已切换为: {} ({} effort)",
                alias_label, effort_display
            )));
        if let Err(e) = Self::save_config(cfg, self.config_path_override.as_deref()) {
            self.core
                .view_messages
                .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
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
        // 互斥：关闭其他面板
        self.core.model_panel = None;
        self.core.config_panel = None;
        self.status_panel = None;
        self.memory_panel = None;
    }

    /// 关闭 /login 面板（不保存）
    pub fn close_login_panel(&mut self) {
        self.core.login_panel = None;
    }

    /// 选中（激活）光标处的 Provider
    pub fn login_panel_select_provider(&mut self) {
        let Some(panel) = self.core.login_panel.as_mut() else {
            return;
        };
        let selected_name = panel
            .providers
            .get(panel.cursor)
            .map(|p| p.display_name().to_string())
            .unwrap_or_default();
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.select_provider(cfg);
        if !selected_name.is_empty() {
            self.core
                .view_messages
                .push(MessageViewModel::system(format!(
                    "已激活 Provider: {}",
                    selected_name
                )));
        }
        if let Err(e) = Self::save_config(cfg, self.config_path_override.as_deref()) {
            self.core
                .view_messages
                .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
        self.close_login_panel();
    }

    /// 保存 Login 面板的编辑/新建内容到 ZenConfig
    pub fn login_panel_apply_edit(&mut self) {
        let Some(panel) = self.core.login_panel.as_mut() else {
            return;
        };
        let edit_name = panel.buf_name.clone();
        let is_new = matches!(panel.mode, login_panel::LoginPanelMode::New);
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        if !panel.apply_edit(cfg) {
            self.core.view_messages.push(MessageViewModel::system(
                "保存失败：Provider 名称不能为空".to_string(),
            ));
            return;
        }
        let action = if is_new { "新建" } else { "保存" };
        let display = if edit_name.is_empty() {
            "Provider".to_string()
        } else {
            edit_name
        };
        self.core
            .view_messages
            .push(MessageViewModel::system(format!(
                "已{} Provider: {}",
                action, display
            )));
        if let Err(e) = Self::save_config(cfg, self.config_path_override.as_deref()) {
            self.core
                .view_messages
                .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
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
        let deleted_name = panel
            .providers
            .get(panel.cursor)
            .map(|p| p.display_name().to_string())
            .unwrap_or_default();
        panel.confirm_delete(cfg);
        if !deleted_name.is_empty() {
            self.core
                .view_messages
                .push(MessageViewModel::system(format!(
                    "已删除 Provider: {}",
                    deleted_name
                )));
        }
        if let Err(e) = Self::save_config(cfg, self.config_path_override.as_deref()) {
            self.core
                .view_messages
                .push(MessageViewModel::system(format!("配置保存失败: {}", e)));
        }
        if let Some(p) = agent::LlmProvider::from_config(cfg) {
            self.provider_name = p.display_name().to_string();
            self.model_name = p.model_name().to_string();
        }
    }

    // ─── Config 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /config 面板
    pub fn open_config_panel(&mut self) {
        let cfg = self.zen_config.get_or_insert_with(ZenConfig::default);
        self.core.config_panel = Some(config_panel::ConfigPanel::from_config(cfg));
        // 互斥：关闭其他面板
        self.core.login_panel = None;
        self.core.model_panel = None;
    }

    /// 关闭 /config 面板
    pub fn close_config_panel(&mut self) {
        self.core.config_panel = None;
    }

    /// 保存 Config 面板编辑并关闭
    pub fn config_panel_apply(&mut self) {
        let Some(panel) = self.core.config_panel.as_mut() else {
            return;
        };
        let Some(cfg) = self.zen_config.as_mut() else {
            return;
        };
        panel.apply_edit(cfg);
        if let Err(e) = Self::save_config(cfg, self.config_path_override.as_deref()) {
            self.core.view_messages.push(MessageViewModel::system(
                format!("配置保存失败: {}", e),
            ));
        } else {
            self.core.view_messages.push(MessageViewModel::system(
                "配置已保存".to_string(),
            ));
        }
        self.core.config_panel = None;
    }

    // ─── Status 面板操作 ───────────────────────────────────────────────────────

    /// 打开状态面板并激活指定 Tab
    pub fn open_status_panel(&mut self, tab: usize) {
        self.status_panel = Some(status_panel::StatusPanel::new(tab));
        // 互斥
        self.core.config_panel = None;
        self.core.login_panel = None;
        self.core.model_panel = None;
    }

    /// 关闭状态面板
    pub fn close_status_panel(&mut self) {
        self.status_panel = None;
    }

    // ─── Memory 面板操作 ───────────────────────────────────────────────────────

    /// 打开 /memory 面板
    pub fn open_memory_panel(&mut self) {
        let home_dir = dirs_next::home_dir();
        let mut panel = crate::app::memory_panel::MemoryPanel::new(&self.cwd, home_dir);
        panel.refresh_exists();
        self.memory_panel = Some(panel);
        // 互斥
        self.core.config_panel = None;
        self.core.login_panel = None;
        self.core.model_panel = None;
        self.status_panel = None;
    }

    /// 关闭 /memory 面板
    pub fn close_memory_panel(&mut self) {
        self.memory_panel = None;
    }

    /// 打开外部编辑器编辑选中的 memory 文件
    pub fn memory_panel_open_editor(&mut self) -> anyhow::Result<()> {
        let entry = self
            .memory_panel
            .as_ref()
            .and_then(|p| p.entries.get(p.cursor))
            .cloned();
        let Some(entry) = entry else {
            return Ok(());
        };

        // 文件不存在时创建空文件
        if !entry.path.exists() {
            if let Some(parent) = entry.path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::File::create(&entry.path)?;
            // 刷新面板中的 exists 状态
            if let Some(ref mut panel) = self.memory_panel {
                panel.refresh_exists();
            }
        }

        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
        tracing::info!("Opening memory file with {}: {:?}", editor, entry.path);

        // 挂起 TUI: 离开 alternate screen + 恢复 raw mode
        ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::terminal::LeaveAlternateScreen
        )?;
        ratatui::crossterm::terminal::disable_raw_mode()?;

        // 启动编辑器
        let status = std::process::Command::new(&editor)
            .arg(&entry.path)
            .status();

        // 恢复 TUI: 重新进入 alternate screen + raw mode
        ratatui::crossterm::terminal::enable_raw_mode()?;
        ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::terminal::EnterAlternateScreen
        )?;

        match status {
            Ok(s) if s.success() => {
                tracing::info!("Editor exited successfully");
            }
            Ok(s) => {
                tracing::warn!("Editor exited with status: {}", s);
            }
            Err(e) => {
                tracing::error!("Failed to launch editor: {}", e);
            }
        }

        Ok(())
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
            self.core
                .view_messages
                .push(MessageViewModel::system(format!(
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

        // 将配置路径重定向到临时目录，防止测试污染全局 ~/.zen-code/settings.json
        let test_config_path =
            std::env::temp_dir().join(format!("zen-config-test-{}/settings.json", uuid::Uuid::now_v7()));

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
            spinner_state: perihelion_widgets::SpinnerState::new(
                perihelion_widgets::SpinnerMode::Idle,
            ),
            config_path_override: Some(test_config_path),
            mcp_pool: None,
            mcp_init_rx: None,
            mcp_panel: None,
            mcp_ready_shown_until: std::cell::Cell::new(None),
            status_panel: None,
            memory_panel: None,
        };

        let handle = crate::ui::headless::HeadlessHandle {
            terminal,
            render_notify,
        };

        (app, handle)
    }
}
