pub mod agents;
pub mod clear;
pub mod compact;
pub mod config;
pub mod context_cmd;
pub mod cost;
pub mod cron;
pub mod help;
pub mod history;
pub mod login;
pub mod loop_cmd;
pub mod mcp;
pub mod memory;
pub mod model;
pub mod split;

/// 注册所有内置命令，返回配置好的 CommandRegistry
pub fn default_registry() -> CommandRegistry {
    let mut r = CommandRegistry::new();
    r.register(Box::new(agents::AgentsCommand));
    r.register(Box::new(config::ConfigCommand));
    r.register(Box::new(login::LoginCommand));
    r.register(Box::new(model::ModelCommand));
    r.register(Box::new(clear::ClearCommand));
    r.register(Box::new(compact::CompactCommand));
    r.register(Box::new(help::HelpCommand));
    r.register(Box::new(history::HistoryCommand));
    r.register(Box::new(loop_cmd::LoopCommand));
    r.register(Box::new(cron::CronCommand));
    r.register(Box::new(mcp::McpCommand));
    r.register(Box::new(memory::MemoryCommand));
    r.register(Box::new(cost::CostCommand));
    r.register(Box::new(context_cmd::ContextCommand));
    r.register(Box::new(split::SplitCommand));
    r
}

use crate::app::App;

// ─── Command trait ────────────────────────────────────────────────────────────

pub trait Command: Send + Sync {
    /// 命令名，不含 /（如 "model"、"help"、"clear"）
    fn name(&self) -> &str;
    /// 单行描述，用于 /help 展示
    fn description(&self) -> &str;
    /// 命令别名列表（不含 /），默认为空
    fn aliases(&self) -> Vec<&str> {
        vec![]
    }
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
    /// 匹配优先级：精确匹配 > 别名精确匹配 > 前缀唯一匹配（支持 /m → /model）
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

        // 2. 别名精确匹配
        if let Some(cmd) = self.commands.iter().find(|c| c.aliases().contains(&name)) {
            cmd.execute(app, args);
            return true;
        }

        // 3. 前缀唯一匹配（同时对 name 和 aliases）
        let matches: Vec<_> = self
            .commands
            .iter()
            .filter(|c| {
                c.name().starts_with(name) || c.aliases().iter().any(|a| a.starts_with(name))
            })
            .collect();
        if matches.len() == 1 {
            matches[0].execute(app, args);
            return true;
        }

        false
    }

    /// 返回所有已注册命令的 (name, description, aliases) 列表
    pub fn list(&self) -> Vec<(&str, &str, Vec<&str>)> {
        self.commands
            .iter()
            .map(|c| (c.name(), c.description(), c.aliases()))
            .collect()
    }

    /// 按前缀匹配命令，返回匹配的 (name, description) 列表
    /// prefix 不含 /，如 "mo" 匹配 "model"
    /// 同时匹配 name 和 aliases
    pub fn match_prefix(&self, prefix: &str) -> Vec<(&str, &str)> {
        self.commands
            .iter()
            .filter(|c| {
                c.name().starts_with(prefix) || c.aliases().iter().any(|a| a.starts_with(prefix))
            })
            .map(|c| (c.name(), c.description()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    use super::*;
    use crate::app::App;

    // ── StubCommand ──

    struct StubCommand {
        n: &'static str,
        called: Arc<AtomicBool>,
        last_args: Arc<parking_lot::Mutex<String>>,
        aliases_vec: Vec<&'static str>,
    }

    impl Command for StubCommand {
        fn name(&self) -> &str {
            self.n
        }
        fn description(&self) -> &str {
            "stub"
        }
        fn aliases(&self) -> Vec<&str> {
            self.aliases_vec.clone()
        }
        fn execute(&self, _app: &mut App, args: &str) {
            self.called.store(true, Ordering::Relaxed);
            *self.last_args.lock() = args.to_string();
        }
    }

    fn make_stub(
        name: &'static str,
    ) -> (
        StubCommand,
        Arc<AtomicBool>,
        Arc<parking_lot::Mutex<String>>,
    ) {
        make_stub_with_aliases(name, vec![])
    }

    fn make_stub_with_aliases(
        name: &'static str,
        aliases: Vec<&'static str>,
    ) -> (
        StubCommand,
        Arc<AtomicBool>,
        Arc<parking_lot::Mutex<String>>,
    ) {
        let called = Arc::new(AtomicBool::new(false));
        let last_args = Arc::new(parking_lot::Mutex::new(String::new()));
        (
            StubCommand {
                n: name,
                called: called.clone(),
                last_args: last_args.clone(),
                aliases_vec: aliases,
            },
            called,
            last_args,
        )
    }

    fn headless_app() -> App {
        App::new_headless(80, 24).0
    }

    // ── dispatch 精确匹配 ──

    #[tokio::test]
    async fn test_dispatch_exact_match() {
        let mut r = CommandRegistry::new();
        let (stub, called, _) = make_stub("model");
        r.register(Box::new(stub));
        let mut app = headless_app();
        assert!(
            r.dispatch(&mut app, "/model"),
            "exact match should return true"
        );
        assert!(called.load(Ordering::Relaxed), "command should be called");
    }

    #[tokio::test]
    async fn test_dispatch_no_match() {
        let mut r = CommandRegistry::new();
        let (stub, _, _) = make_stub("model");
        r.register(Box::new(stub));
        let mut app = headless_app();
        assert!(
            !r.dispatch(&mut app, "/unknown"),
            "unknown command should return false"
        );
    }

    // ── 前缀唯一匹配 ──

    #[tokio::test]
    async fn test_dispatch_prefix_unique() {
        let mut r = CommandRegistry::new();
        let (stub, called, _) = make_stub("model");
        r.register(Box::new(stub));
        let mut app = headless_app();
        assert!(
            r.dispatch(&mut app, "/mo"),
            "unique prefix should return true"
        );
        assert!(
            called.load(Ordering::Relaxed),
            "command should be called via prefix"
        );
    }

    #[tokio::test]
    async fn test_dispatch_prefix_ambiguous() {
        let mut r = CommandRegistry::new();
        let (stub1, called1, _) = make_stub("model");
        let (stub2, called2, _) = make_stub("mock");
        r.register(Box::new(stub1));
        r.register(Box::new(stub2));
        let mut app = headless_app();
        assert!(
            !r.dispatch(&mut app, "/m"),
            "ambiguous prefix should return false"
        );
        assert!(!called1.load(Ordering::Relaxed));
        assert!(!called2.load(Ordering::Relaxed));
    }

    // ── 参数传递 ──

    #[tokio::test]
    async fn test_dispatch_with_args() {
        let mut r = CommandRegistry::new();
        let (stub, _, last_args) = make_stub("model");
        r.register(Box::new(stub));
        let mut app = headless_app();
        r.dispatch(&mut app, "/model opus");
        assert_eq!(*last_args.lock(), "opus", "args should be passed correctly");
    }

    // ── 辅助方法（纯逻辑，无需 App）──

    #[test]
    fn test_match_prefix_returns_matching() {
        let mut r = CommandRegistry::new();
        let (s1, _, _) = make_stub("model");
        let (s2, _, _) = make_stub("mock");
        let (s3, _, _) = make_stub("clear");
        r.register(Box::new(s1));
        r.register(Box::new(s2));
        r.register(Box::new(s3));
        let matches = r.match_prefix("mo");
        assert_eq!(matches.len(), 2, "should match 'model' and 'mock'");
    }

    #[test]
    fn test_list_returns_all() {
        let mut r = CommandRegistry::new();
        let (s1, _, _) = make_stub("a");
        let (s2, _, _) = make_stub("b");
        let (s3, _, _) = make_stub("c");
        r.register(Box::new(s1));
        r.register(Box::new(s2));
        r.register(Box::new(s3));
        assert_eq!(r.list().len(), 3, "list should return all 3 commands");
    }

    #[tokio::test]
    async fn test_dispatch_empty_prefix() {
        let mut r = CommandRegistry::new();
        let (s1, _, _) = make_stub("model");
        let (s2, _, _) = make_stub("clear");
        r.register(Box::new(s1));
        r.register(Box::new(s2));
        let mut app = headless_app();
        // "/" → empty name, all commands match → ambiguous → false
        assert!(
            !r.dispatch(&mut app, "/"),
            "empty prefix should return false when ambiguous"
        );
    }

    // ── 别名匹配 ──

    #[tokio::test]
    async fn test_alias_exact_match() {
        let mut r = CommandRegistry::new();
        let (stub, called, _) = make_stub_with_aliases("clear", vec!["reset", "new"]);
        r.register(Box::new(stub));
        let mut app = headless_app();
        assert!(
            r.dispatch(&mut app, "/reset"),
            "alias exact match should return true"
        );
        assert!(called.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_alias_no_match() {
        let mut r = CommandRegistry::new();
        let (stub, _, _) = make_stub("model");
        r.register(Box::new(stub));
        let mut app = headless_app();
        assert!(
            !r.dispatch(&mut app, "/reset"),
            "no alias should return false"
        );
    }

    #[tokio::test]
    async fn test_name_priority_over_alias() {
        let mut r = CommandRegistry::new();
        let (s1, called1, _) = make_stub("reset");
        let (s2, called2, _) = make_stub_with_aliases("clear", vec!["reset"]);
        r.register(Box::new(s1));
        r.register(Box::new(s2));
        let mut app = headless_app();
        assert!(r.dispatch(&mut app, "/reset"));
        assert!(called1.load(Ordering::Relaxed), "name exact should win");
        assert!(!called2.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_alias_prefix_match() {
        let mut r = CommandRegistry::new();
        let (stub, called, _) = make_stub_with_aliases("clear", vec!["reset"]);
        r.register(Box::new(stub));
        let mut app = headless_app();
        assert!(
            r.dispatch(&mut app, "/res"),
            "alias prefix unique match should return true"
        );
        assert!(called.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_alias_prefix_ambiguous() {
        let mut r = CommandRegistry::new();
        let (s1, called1, _) = make_stub_with_aliases("clear", vec!["reset"]);
        let (s2, called2, _) = make_stub("real");
        r.register(Box::new(s1));
        r.register(Box::new(s2));
        let mut app = headless_app();
        assert!(
            !r.dispatch(&mut app, "/re"),
            "ambiguous alias prefix should return false"
        );
        assert!(!called1.load(Ordering::Relaxed));
        assert!(!called2.load(Ordering::Relaxed));
    }

    #[test]
    fn test_match_prefix_covers_aliases() {
        let mut r = CommandRegistry::new();
        let (s, _, _) = make_stub_with_aliases("clear", vec!["reset"]);
        r.register(Box::new(s));
        let matches = r.match_prefix("res");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, "clear");
    }

    #[test]
    fn test_list_includes_aliases() {
        let mut r = CommandRegistry::new();
        let (s, _, _) = make_stub_with_aliases("clear", vec!["reset", "new"]);
        r.register(Box::new(s));
        let list = r.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "clear");
        assert_eq!(list[0].2, vec!["reset", "new"]);
    }

    #[test]
    fn test_no_alias_backward_compat() {
        let mut r = CommandRegistry::new();
        let (s, _, _) = make_stub("model");
        r.register(Box::new(s));
        let list = r.list();
        assert_eq!(list[0].2, Vec::<&str>::new());
        let matches = r.match_prefix("mo");
        assert_eq!(matches.len(), 1);
    }
}
