//! TUI interface for Rust Agent - interactive terminal playground

pub mod app;
pub mod command;
pub mod config;
pub mod event;
pub mod langfuse;
pub mod prompt;
pub mod relay_adapter;
pub mod thread;
pub mod ui;

/// CLI 参数解析结果：--remote-control [url] [--relay-token <token>] [--relay-name <name>]
/// url 为空字符串表示 `--remote-control` 无参数模式（从配置读取）
pub struct RelayCli {
    pub url: String,
    pub token: Option<String>,
    pub name: Option<String>,
}

pub fn parse_relay_args(args: &[String]) -> Option<RelayCli> {
    // 查找 --remote-control 参数位置
    let remote_idx = args.iter().position(|a| a == "--remote-control")?;

    // 检查是否有值（即 --remote-control <url> 格式）
    // 有值条件：下一个参数存在且不以 -- 开头
    let url = if remote_idx + 1 < args.len() && !args[remote_idx + 1].starts_with("--") {
        args[remote_idx + 1].clone()
    } else {
        // --remote-control 无参数，返回空字符串标记"从配置读取"
        String::new()
    };

    let token = args.windows(2)
        .find(|w| w[0] == "--relay-token")
        .map(|w| w[1].clone());
    let name = args.windows(2)
        .find(|w| w[0] == "--relay-name")
        .map(|w| w[1].clone());

    Some(RelayCli { url, token, name })
}
