use crate::tools::BaseTool;

/// ToolProvider trait — 工具包接口
///
/// 与 `Middleware` 正交：纯工具包只需实现此 trait，
/// 不必实现任何生命周期钩子。
///
/// 既提供工具又需要拦截逻辑的组件可同时实现 `ToolProvider` 和 `Middleware`。
pub trait ToolProvider: Send + Sync {
    /// 根据工作目录返回工具列表
    fn tools(&self, cwd: &str) -> Vec<Box<dyn BaseTool>>;
}
