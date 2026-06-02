use super::ChatSession;

/// 会话管理器：管理单个 ChatSession 实例。
pub struct SessionManager {
    session: ChatSession,
}

impl SessionManager {
    pub fn new(initial_session: ChatSession) -> Self {
        Self {
            session: initial_session,
        }
    }

    pub fn current(&self) -> &ChatSession {
        &self.session
    }

    pub fn current_mut(&mut self) -> &mut ChatSession {
        &mut self.session
    }

    /// 替换当前 session（用于 /clear 新建对话）
    pub fn replace(&mut self, new_session: ChatSession) {
        self.session = new_session;
    }
}
