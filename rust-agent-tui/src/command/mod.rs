pub mod agents;
pub mod clear;
pub mod compact;
pub mod help;
pub mod history;
pub mod model;

/// 注册所有内置命令，返回配置好的 CommandRegistry
pub fn default_registry() -> CommandRegistry {
    let mut r = CommandRegistry::new();
    r.register(Box::new(agents::AgentsCommand));
    r.register(Box::new(model::ModelCommand));
    r.register(Box::new(clear::ClearCommand));
    r.register(Box::new(compact::CompactCommand));
    r.register(Box::new(help::HelpCommand));
    r.register(Box::new(history::HistoryCommand));
    r
}

use crate::app::App;

// ─── Command trait ────────────────────────────────────────────────────────────

pub trait Command: Send + Sync {
    /// 命令名，不含 /（如 "model"、"help"、"clear"）
    fn name(&self) -> &str;
    /// 单行描述，用于 /help 展示
    fn description(&self) -> &str;
    /// 执行命令，args 是命令名之后的参数字符串（已 trim）
    fn execute(&self, app: &mut App, args: &str);
}

// ─── CommandRegistry ──────────────────────────────────────────────────────────

#[derive(Default)]
pub struct CommandRegistry {
    commands: Vec<Box<dyn Command>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, cmd: Box<dyn Command>) {
        self.commands.push(cmd);
    }

    /// 解析并执行命令。
    /// 输入格式："/name args..."
    /// 匹配优先级：精确匹配 > 前缀唯一匹配（支持 /m → /model）
    /// 返回 true 表示找到命令并执行，false 表示未知命令或有歧义。
    pub fn dispatch(&self, app: &mut App, input: &str) -> bool {
        let input = input.trim_start_matches('/');
        let (name, args) = match input.split_once(' ') {
            Some((n, a)) => (n.trim(), a.trim()),
            None => (input.trim(), ""),
        };

        // 1. 精确匹配
        if let Some(cmd) = self.commands.iter().find(|c| c.name() == name) {
            cmd.execute(app, args);
            return true;
        }

        // 2. 前缀唯一匹配（快捷命令）
        let matches: Vec<_> = self.commands.iter().filter(|c| c.name().starts_with(name)).collect();
        if matches.len() == 1 {
            matches[0].execute(app, args);
            return true;
        }

        false
    }

    /// 返回所有已注册命令的 (name, description) 列表
    pub fn list(&self) -> Vec<(&str, &str)> {
        self.commands.iter().map(|c| (c.name(), c.description())).collect()
    }

    /// 按前缀匹配命令，返回匹配的 (name, description) 列表
    /// prefix 不含 /，如 "mo" 匹配 "model"
    pub fn match_prefix(&self, prefix: &str) -> Vec<(&str, &str)> {
        self.commands
            .iter()
            .filter(|c| c.name().starts_with(prefix))
            .map(|c| (c.name(), c.description()))
            .collect()
    }
}
