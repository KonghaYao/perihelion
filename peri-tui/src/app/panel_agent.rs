use super::*;

impl App {
    /// 打开 /agents 面板（传入扫描到的 agent 列表）
    pub fn open_agent_panel(&mut self, agents: Vec<AgentItem>) {
        let panel = AgentPanel::new(
            agents,
            self.session_mgr.current_mut().agent.agent_id.clone(),
        );
        self.open_panel(PanelState::Agent(panel));
    }

    /// 关闭 /agents 面板（不选择任何 agent）
    pub fn close_agent_panel(&mut self) {
        self.session_mgr
            .current_mut()
            .session_panels
            .close_if(PanelKind::Agent);
    }

    /// 确认选择当前 agent，关闭面板，设置 agent_id
    pub fn agent_panel_confirm(&mut self) {
        // 先取出 selection，避免同时借用 panel 和 agent_id
        let (is_none, agent_id, agent_name) = {
            let panel = match self
                .session_mgr
                .current_mut()
                .session_panels
                .get_mut::<AgentPanel>()
            {
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
            self.session_mgr
                .current_mut()
                .messages
                .view_messages
                .push(MessageViewModel::system(
                    self.services.lc.tr("app-agent-reset"),
                ));
        } else if let Some(id) = agent_id {
            self.set_agent_id(Some(id.clone()));
            let name = agent_name.unwrap_or_else(|| id.clone());
            self.session_mgr
                .current_mut()
                .messages
                .view_messages
                .push(MessageViewModel::system(self.services.lc.tr_args(
                    "app-agent-switched",
                    &[("name".into(), name.into()), ("id".into(), id.into())],
                )));
        }
        self.session_mgr
            .current_mut()
            .session_panels
            .close_if(PanelKind::Agent);
    }
}
