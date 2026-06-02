//! Rewind 弹窗数据结构。
//!
//! 用户选择回退节点的弹窗状态，包括消息列表、光标位置和操作模式。

/// Rewind 弹窗中的用户消息条目。
#[derive(Debug, Clone)]
pub struct RewindItem {
    /// 消息 ID（BaseMessage.id()，用于传给 /rewind 命令）
    pub message_id: String,
    /// 截断摘要文本（用于显示）
    pub summary: String,
    /// 该消息之后（含自身）的消息数量（用于显示影响范围）
    pub message_count_after: usize,
    /// 该消息之后的文件变更列表（路径 + 操作类型）
    pub file_changes: Vec<FileChangeInfo>,
}

/// 文件变更信息（用于二次确认弹窗显示）。
#[derive(Debug, Clone)]
pub struct FileChangeInfo {
    pub path: String,
    /// "Write" 或 "Edit"
    pub operation: String,
}

/// Rewind 操作模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RewindMode {
    /// 仅回退消息
    MessagesOnly,
    /// 回退消息 + 文件
    MessagesAndFiles,
    /// 二次确认阶段（显示文件列表，等 Enter 确认）
    ConfirmRevert,
}

/// Rewind 弹窗状态。
pub struct RewindPrompt {
    /// 可回退的用户消息列表
    pub items: Vec<RewindItem>,
    /// 当前光标位置
    pub cursor: usize,
    /// 当前操作模式
    pub mode: RewindMode,
}
