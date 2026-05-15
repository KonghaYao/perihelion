#![allow(dead_code)]

use std::any::Any;

use tui_textarea::Input;

use ratatui::layout::Rect;
use ratatui::Frame;

use super::agent_panel::AgentPanel;
use super::config_panel::ConfigPanel;
use super::cron_state::CronPanel;
use super::hooks_panel::HooksPanel;
use super::login_panel::LoginPanel;
use super::mcp_panel::McpPanel;
use super::memory_panel::MemoryPanel;
use super::model_panel::ModelPanel;
use super::plugin_panel::PluginPanel;
use super::service_registry::ServiceRegistry;
use super::session_manager::SessionManager;
use super::status_panel::StatusPanel;
use crate::thread::ThreadBrowser;

// ─── PanelScope ─────────────────────────────────────────────────────────────

/// 面板作用域：Session 面板随 session 切换，Global 面板跨 session 保持
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelScope {
    Session,
    Global,
}

// ─── MutexGroup ─────────────────────────────────────────────────────────────

/// 互斥组：同组面板同时只能打开一个
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutexGroup {
    /// 模型/配置/登录面板互斥
    Settings,
    /// Agent/Hooks 面板互斥
    Agent,
    /// MCP/Cron/Plugin 面板互斥
    Tools,
    /// Status/Memory 面板互斥
    Info,
    /// ThreadBrowser 独占
    Thread,
}

// ─── PanelKind ──────────────────────────────────────────────────────────────

/// 穷举所有面板类型（编译时完整性保证）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelKind {
    Model,
    Login,
    Agent,
    Hooks,
    Config,
    ThreadBrowser,
    Mcp,
    Plugin,
    Cron,
    Status,
    Memory,
}

impl PanelKind {
    /// 面板优先级（数值越小优先级越高，用于互斥决策）
    pub fn priority(&self) -> u8 {
        match self {
            PanelKind::Agent => 0,
            PanelKind::Hooks => 1,
            PanelKind::Model => 2,
            PanelKind::Login => 3,
            PanelKind::Config => 4,
            PanelKind::ThreadBrowser => 5,
            PanelKind::Mcp => 6,
            PanelKind::Plugin => 7,
            PanelKind::Cron => 8,
            PanelKind::Status => 9,
            PanelKind::Memory => 10,
        }
    }

    /// 互斥组
    pub fn mutex_group(&self) -> MutexGroup {
        match self {
            PanelKind::Model | PanelKind::Login | PanelKind::Config => MutexGroup::Settings,
            PanelKind::Agent | PanelKind::Hooks => MutexGroup::Agent,
            PanelKind::Mcp | PanelKind::Plugin | PanelKind::Cron => MutexGroup::Tools,
            PanelKind::Status | PanelKind::Memory => MutexGroup::Info,
            PanelKind::ThreadBrowser => MutexGroup::Thread,
        }
    }

    /// 面板作用域
    pub fn scope(&self) -> PanelScope {
        match self {
            PanelKind::Model
            | PanelKind::Login
            | PanelKind::Agent
            | PanelKind::Hooks
            | PanelKind::Config
            | PanelKind::ThreadBrowser => PanelScope::Session,
            PanelKind::Mcp
            | PanelKind::Plugin
            | PanelKind::Cron
            | PanelKind::Status
            | PanelKind::Memory => PanelScope::Global,
        }
    }
}

// ─── EventResult ────────────────────────────────────────────────────────────

/// 面板事件处理返回值
#[derive(Debug, PartialEq)]
pub enum EventResult {
    /// 事件已被消费，无需进一步处理
    Consumed,
    /// 事件未被消费，继续传递给后续处理器
    NotConsumed,
    /// 请求关闭当前面板
    ClosePanel,
    /// 请求打开另一个面板（用于面板间导航）
    OpenPanel(PanelKind),
    /// 请求打开指定 Thread（ThreadBrowser 专用）
    OpenThread(String),
}

// ─── PanelState ─────────────────────────────────────────────────────────────

/// 穷举存储面板实例（编译时完整性保证）
pub enum PanelState {
    Model(ModelPanel),
    Login(LoginPanel),
    Agent(AgentPanel),
    Hooks(HooksPanel),
    Config(ConfigPanel),
    ThreadBrowser(ThreadBrowser),
    Mcp(McpPanel),
    Plugin(Box<PluginPanel>),
    Cron(CronPanel),
    Status(StatusPanel),
    Memory(MemoryPanel),
}

impl PanelState {
    /// 获取面板类型
    pub fn kind(&self) -> PanelKind {
        match self {
            PanelState::Model(_) => PanelKind::Model,
            PanelState::Login(_) => PanelKind::Login,
            PanelState::Agent(_) => PanelKind::Agent,
            PanelState::Hooks(_) => PanelKind::Hooks,
            PanelState::Config(_) => PanelKind::Config,
            PanelState::ThreadBrowser(_) => PanelKind::ThreadBrowser,
            PanelState::Mcp(_) => PanelKind::Mcp,
            PanelState::Plugin(_) => PanelKind::Plugin,
            PanelState::Cron(_) => PanelKind::Cron,
            PanelState::Status(_) => PanelKind::Status,
            PanelState::Memory(_) => PanelKind::Memory,
        }
    }

    /// Any downcast（不可变引用）
    pub fn as_any_ref(&self) -> &dyn Any {
        match self {
            PanelState::Model(p) => p as &dyn Any,
            PanelState::Login(p) => p as &dyn Any,
            PanelState::Agent(p) => p as &dyn Any,
            PanelState::Hooks(p) => p as &dyn Any,
            PanelState::Config(p) => p as &dyn Any,
            PanelState::ThreadBrowser(p) => p as &dyn Any,
            PanelState::Mcp(p) => p as &dyn Any,
            PanelState::Plugin(p) => p.as_ref() as &dyn Any,
            PanelState::Cron(p) => p as &dyn Any,
            PanelState::Status(p) => p as &dyn Any,
            PanelState::Memory(p) => p as &dyn Any,
        }
    }

    /// Any downcast（可变引用）
    pub fn as_any_mut(&mut self) -> &mut dyn Any {
        match self {
            PanelState::Model(p) => p as &mut dyn Any,
            PanelState::Login(p) => p as &mut dyn Any,
            PanelState::Agent(p) => p as &mut dyn Any,
            PanelState::Hooks(p) => p as &mut dyn Any,
            PanelState::Config(p) => p as &mut dyn Any,
            PanelState::ThreadBrowser(p) => p as &mut dyn Any,
            PanelState::Mcp(p) => p as &mut dyn Any,
            PanelState::Plugin(p) => p.as_mut() as &mut dyn Any,
            PanelState::Cron(p) => p as &mut dyn Any,
            PanelState::Status(p) => p as &mut dyn Any,
            PanelState::Memory(p) => p as &mut dyn Any,
        }
    }

    /// 委托渲染到对应面板组件
    pub fn render(&mut self, f: &mut Frame, app: &mut super::App, area: Rect) {
        use super::panel_component::PanelComponent;
        match self {
            PanelState::Model(p) => p.render(f, app, area),
            PanelState::Login(p) => p.render(f, app, area),
            PanelState::Agent(p) => p.render(f, app, area),
            PanelState::Hooks(p) => p.render(f, app, area),
            PanelState::Config(p) => p.render(f, app, area),
            PanelState::ThreadBrowser(p) => p.render(f, app, area),
            PanelState::Mcp(p) => p.render(f, app, area),
            PanelState::Plugin(p) => p.render(f, app, area),
            PanelState::Cron(p) => p.render(f, app, area),
            PanelState::Status(p) => p.render(f, app, area),
            PanelState::Memory(p) => p.render(f, app, area),
        }
    }

    /// 委托获取期望面板高度
    pub fn desired_height(&self, screen_height: u16, screen_width: u16) -> u16 {
        use super::panel_component::PanelComponent;
        match self {
            PanelState::Model(p) => p.desired_height(screen_height, screen_width),
            PanelState::Login(p) => p.desired_height(screen_height, screen_width),
            PanelState::Agent(p) => p.desired_height(screen_height, screen_width),
            PanelState::Hooks(p) => p.desired_height(screen_height, screen_width),
            PanelState::Config(p) => p.desired_height(screen_height, screen_width),
            PanelState::ThreadBrowser(p) => p.desired_height(screen_height, screen_width),
            PanelState::Mcp(p) => p.desired_height(screen_height, screen_width),
            PanelState::Plugin(p) => p.desired_height(screen_height, screen_width),
            PanelState::Cron(p) => p.desired_height(screen_height, screen_width),
            PanelState::Status(p) => p.desired_height(screen_height, screen_width),
            PanelState::Memory(p) => p.desired_height(screen_height, screen_width),
        }
    }

    /// 委托获取快捷键提示
    pub fn status_bar_hints(&self, lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        use super::panel_component::PanelComponent;
        match self {
            PanelState::Model(p) => p.status_bar_hints(lc),
            PanelState::Login(p) => p.status_bar_hints(lc),
            PanelState::Agent(p) => p.status_bar_hints(lc),
            PanelState::Hooks(p) => p.status_bar_hints(lc),
            PanelState::Config(p) => p.status_bar_hints(lc),
            PanelState::ThreadBrowser(p) => p.status_bar_hints(lc),
            PanelState::Mcp(p) => p.status_bar_hints(lc),
            PanelState::Plugin(p) => p.status_bar_hints(lc),
            PanelState::Cron(p) => p.status_bar_hints(lc),
            PanelState::Status(p) => p.status_bar_hints(lc),
            PanelState::Memory(p) => p.status_bar_hints(lc),
        }
    }
}

// ─── PanelContext ───────────────────────────────────────────────────────────

/// 面板处理器上下文：解耦面板与 App 的借用冲突
pub struct PanelContext<'a> {
    pub services: &'a mut ServiceRegistry,
    pub session_mgr: &'a mut SessionManager,
}

// ─── PanelManager ───────────────────────────────────────────────────────────

/// 面板管理器：集中管理面板的打开/关闭/查询和事件分发
pub struct PanelManager {
    active: Option<PanelState>,
}

impl PanelManager {
    pub fn new() -> Self {
        Self { active: None }
    }

    /// 获取当前激活面板的类型
    pub fn active_kind(&self) -> Option<PanelKind> {
        self.active.as_ref().map(|s| s.kind())
    }

    /// 获取当前激活面板的不可变引用
    pub fn active_state(&self) -> Option<&PanelState> {
        self.active.as_ref()
    }

    /// 获取当前激活面板的可变引用
    pub fn active_state_mut(&mut self) -> Option<&mut PanelState> {
        self.active.as_mut()
    }

    /// 取出当前激活面板（用于需要 &mut App 的渲染场景，避免双重可变借用）
    pub fn take_active(&mut self) -> Option<PanelState> {
        self.active.take()
    }

    /// 放回面板（配合 take_active 使用）
    pub fn put_active(&mut self, state: PanelState) {
        self.active = Some(state);
    }

    /// 检查指定类型的面板是否激活
    pub fn is_active(&self, kind: PanelKind) -> bool {
        self.active_kind() == Some(kind)
    }

    /// 检查是否有任何面板打开
    pub fn is_any_open(&self) -> bool {
        self.active.is_some()
    }

    /// 打开面板：自动关闭同作用域的前一面板，返回被关闭的面板
    pub fn open(&mut self, state: PanelState) -> Option<PanelState> {
        self.active.replace(state)
    }

    /// 关闭当前面板，返回被关闭的面板
    pub fn close(&mut self) -> Option<PanelState> {
        self.active.take()
    }

    /// 仅当指定类型的面板激活时才关闭
    pub fn close_if(&mut self, kind: PanelKind) -> Option<PanelState> {
        if self.is_active(kind) {
            self.close()
        } else {
            None
        }
    }

    /// 类型安全地获取面板的不可变引用
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.active.as_ref()?.as_any_ref().downcast_ref::<T>()
    }

    /// 类型安全地获取面板的可变引用
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.active.as_mut()?.as_any_mut().downcast_mut::<T>()
    }

    /// 分发按键事件到当前激活面板
    pub fn dispatch_key(&mut self, input: Input, ctx: &mut PanelContext<'_>) -> EventResult {
        use super::panel_component::PanelComponent;
        let Some(state) = self.active.as_mut() else {
            return EventResult::NotConsumed;
        };
        match state {
            PanelState::Model(p) => p.handle_key(input, ctx),
            PanelState::Agent(p) => p.handle_key(input, ctx),
            PanelState::Hooks(p) => p.handle_key(input, ctx),
            PanelState::Status(p) => p.handle_key(input, ctx),
            PanelState::Memory(p) => p.handle_key(input, ctx),
            PanelState::Login(p) => p.handle_key(input, ctx),
            PanelState::Config(p) => p.handle_key(input, ctx),
            PanelState::ThreadBrowser(p) => p.handle_key(input, ctx),
            PanelState::Mcp(p) => p.handle_key(input, ctx),
            PanelState::Cron(p) => p.handle_key(input, ctx),
            PanelState::Plugin(p) => p.handle_key(input, ctx),
        }
    }

    /// 分发粘贴事件到当前激活面板
    pub fn dispatch_paste(&mut self, text: &str, ctx: &mut PanelContext<'_>) -> EventResult {
        use super::panel_component::PanelComponent;
        let Some(state) = self.active.as_mut() else {
            return EventResult::NotConsumed;
        };
        match state {
            PanelState::Model(p) => p.handle_paste(text, ctx),
            PanelState::Agent(p) => p.handle_paste(text, ctx),
            PanelState::Hooks(p) => p.handle_paste(text, ctx),
            PanelState::Status(p) => p.handle_paste(text, ctx),
            PanelState::Memory(p) => p.handle_paste(text, ctx),
            PanelState::Login(p) => p.handle_paste(text, ctx),
            PanelState::Config(p) => p.handle_paste(text, ctx),
            PanelState::ThreadBrowser(p) => p.handle_paste(text, ctx),
            PanelState::Mcp(p) => p.handle_paste(text, ctx),
            PanelState::Cron(p) => p.handle_paste(text, ctx),
            PanelState::Plugin(p) => p.handle_paste(text, ctx),
        }
    }

    /// 分发滚动事件到当前激活面板
    pub fn dispatch_scroll(&mut self, lines: i16, ctx: &mut PanelContext<'_>) -> EventResult {
        use super::panel_component::PanelComponent;
        let Some(state) = self.active.as_mut() else {
            return EventResult::NotConsumed;
        };
        match state {
            PanelState::Model(p) => p.handle_scroll(lines, ctx),
            PanelState::Agent(p) => p.handle_scroll(lines, ctx),
            PanelState::Hooks(p) => p.handle_scroll(lines, ctx),
            PanelState::Status(p) => p.handle_scroll(lines, ctx),
            PanelState::Memory(p) => p.handle_scroll(lines, ctx),
            PanelState::Login(p) => p.handle_scroll(lines, ctx),
            PanelState::Config(p) => p.handle_scroll(lines, ctx),
            PanelState::ThreadBrowser(p) => p.handle_scroll(lines, ctx),
            PanelState::Mcp(p) => p.handle_scroll(lines, ctx),
            PanelState::Cron(p) => p.handle_scroll(lines, ctx),
            PanelState::Plugin(p) => p.handle_scroll(lines, ctx),
        }
    }

    /// 分发鼠标事件到当前激活面板
    pub fn dispatch_mouse(
        &mut self,
        mouse: ratatui::crossterm::event::MouseEvent,
        area: ratatui::layout::Rect,
        ctx: &mut PanelContext<'_>,
    ) -> EventResult {
        use super::panel_component::PanelComponent;
        let Some(state) = self.active.as_mut() else {
            return EventResult::NotConsumed;
        };
        match state {
            PanelState::Model(p) => p.handle_mouse(mouse, area, ctx),
            PanelState::Agent(p) => p.handle_mouse(mouse, area, ctx),
            PanelState::Hooks(p) => p.handle_mouse(mouse, area, ctx),
            PanelState::Status(p) => p.handle_mouse(mouse, area, ctx),
            PanelState::Memory(p) => p.handle_mouse(mouse, area, ctx),
            PanelState::Login(p) => p.handle_mouse(mouse, area, ctx),
            PanelState::Config(p) => p.handle_mouse(mouse, area, ctx),
            PanelState::ThreadBrowser(p) => p.handle_mouse(mouse, area, ctx),
            PanelState::Mcp(p) => p.handle_mouse(mouse, area, ctx),
            PanelState::Cron(p) => p.handle_mouse(mouse, area, ctx),
            PanelState::Plugin(p) => p.handle_mouse(mouse, area, ctx),
        }
    }

    /// 获取当前激活面板的快捷键提示
    pub fn status_bar_hints(&self, lc: &crate::i18n::LcRegistry) -> Vec<(String, String)> {
        use super::panel_component::PanelComponent;
        let Some(state) = self.active.as_ref() else {
            return Vec::new();
        };
        match state {
            PanelState::Model(p) => p.status_bar_hints(lc),
            PanelState::Agent(p) => p.status_bar_hints(lc),
            PanelState::Hooks(p) => p.status_bar_hints(lc),
            PanelState::Status(p) => p.status_bar_hints(lc),
            PanelState::Memory(p) => p.status_bar_hints(lc),
            PanelState::Login(p) => p.status_bar_hints(lc),
            PanelState::Config(p) => p.status_bar_hints(lc),
            PanelState::ThreadBrowser(p) => p.status_bar_hints(lc),
            PanelState::Mcp(p) => p.status_bar_hints(lc),
            PanelState::Cron(p) => p.status_bar_hints(lc),
            PanelState::Plugin(p) => p.status_bar_hints(lc),
        }
    }

    /// 查询当前激活面板的期望高度
    pub fn dispatch_desired_height(&self, screen_height: u16, screen_width: u16) -> Option<u16> {
        use super::panel_component::PanelComponent;
        let state = self.active.as_ref()?;
        Some(match state {
            PanelState::Model(p) => p.desired_height(screen_height, screen_width),
            PanelState::Agent(p) => p.desired_height(screen_height, screen_width),
            PanelState::Hooks(p) => p.desired_height(screen_height, screen_width),
            PanelState::Status(p) => p.desired_height(screen_height, screen_width),
            PanelState::Memory(p) => p.desired_height(screen_height, screen_width),
            PanelState::Login(p) => p.desired_height(screen_height, screen_width),
            PanelState::Config(p) => p.desired_height(screen_height, screen_width),
            PanelState::ThreadBrowser(p) => p.desired_height(screen_height, screen_width),
            PanelState::Mcp(p) => p.desired_height(screen_height, screen_width),
            PanelState::Cron(p) => p.desired_height(screen_height, screen_width),
            PanelState::Plugin(p) => p.desired_height(screen_height, screen_width),
        })
    }
}

impl Default for PanelManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "panel_manager_test.rs"]
mod tests;
