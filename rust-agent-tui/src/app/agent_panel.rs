use crate::command::agents::AgentItem;

// ─── AgentPanel ────────────────────────────────────────────────────────────────

pub struct AgentPanel {
    /// 可用 agent 列表
    pub agents: Vec<AgentItem>,
    /// 当前选中的 agent_id
    pub selected_id: Option<String>,
    /// 光标位置（0 = "无 Agent" 选项，1+ = agents 列表索引-1）
    pub cursor: usize,
}

impl AgentPanel {
    pub fn new(agents: Vec<AgentItem>, current_id: Option<String>) -> Self {
        // 如果已有选中，定位光标到对应的 agents 索引+1（+1 因为第0项是"无 Agent"）
        let cursor = current_id
            .as_ref()
            .and_then(|id| agents.iter().position(|a| &a.id == id))
            .map(|i| i + 1)
            .unwrap_or(0);

        Self {
            agents,
            selected_id: current_id,
            cursor,
        }
    }

    /// 总项数 = "无 Agent" 选项 + agents 列表
    pub fn total(&self) -> usize {
        1 + self.agents.len()
    }

    /// 上下移动光标
    pub fn move_cursor(&mut self, delta: isize) {
        let total = self.total();
        if total == 0 {
            return;
        }
        self.cursor = ((self.cursor as isize + delta).rem_euclid(total as isize)) as usize;
    }

/// 选择当前光标处的 agent（Enter 确认选择）
    /// 返回 (is_none: bool, agent_id: Option<String>)
    pub fn get_selection(&self) -> (bool, Option<String>) {
        if self.cursor == 0 {
            (true, None)
        } else if let Some(agent) = self.agents.get(self.cursor - 1) {
            (false, Some(agent.id.clone()))
        } else {
            (true, None)
        }
    }

    /// 获取当前光标处的 agent（不包含"无 Agent"选项）
    pub fn current_agent(&self) -> Option<&AgentItem> {
        if self.cursor == 0 {
            None
        } else {
            self.agents.get(self.cursor - 1)
        }
    }
}
