use std::path::PathBuf;

/// Memory 文件条目
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub label: String,
    pub path: PathBuf,
    pub exists: bool,
}

/// /memory 面板状态
#[derive(Debug, Clone)]
pub struct MemoryPanel {
    pub entries: Vec<MemoryEntry>,
    pub cursor: usize,
    pub scroll_offset: u16,
}

impl MemoryPanel {
    /// 根据 cwd 和 home 目录创建面板，自动检测文件是否存在
    pub fn new(cwd: &str, home_dir: Option<PathBuf>) -> Self {
        let project_path = PathBuf::from(cwd).join("CLAUDE.md");
        let global_path = home_dir
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(".claude")
            .join("CLAUDE.md");

        let entries = vec![
            MemoryEntry {
                label: "项目说明".to_string(),
                path: project_path,
                exists: false, // 延迟到 refresh_exists 时检查
            },
            MemoryEntry {
                label: "用户全局".to_string(),
                path: global_path,
                exists: false,
            },
        ];

        Self {
            entries,
            cursor: 0,
            scroll_offset: 0,
        }
    }

    /// 刷新所有条目的 exists 状态
    pub fn refresh_exists(&mut self) {
        for entry in &mut self.entries {
            entry.exists = entry.path.exists();
        }
    }

    /// 光标上移
    pub fn move_cursor_up(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// 光标下移
    pub fn move_cursor_down(&mut self) {
        if self.cursor < self.entries.len() - 1 {
            self.cursor += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_panel_new_entries() {
        let panel = MemoryPanel::new("/test/project", Some(PathBuf::from("/home/user")));
        assert_eq!(panel.entries.len(), 2);
        assert_eq!(panel.entries[0].label, "项目说明");
        assert_eq!(panel.entries[1].label, "用户全局");
        assert_eq!(
            panel.entries[0].path,
            PathBuf::from("/test/project/CLAUDE.md")
        );
        assert_eq!(
            panel.entries[1].path,
            PathBuf::from("/home/user/.claude/CLAUDE.md")
        );
    }

    #[test]
    fn test_memory_panel_cursor_navigation() {
        let mut panel = MemoryPanel::new("/test", None);
        assert_eq!(panel.cursor, 0);
        panel.move_cursor_down();
        assert_eq!(panel.cursor, 1);
        panel.move_cursor_down(); // 不再下移
        assert_eq!(panel.cursor, 1);
        panel.move_cursor_up();
        assert_eq!(panel.cursor, 0);
        panel.move_cursor_up(); // 不再上移
        assert_eq!(panel.cursor, 0);
    }

    #[test]
    fn test_memory_panel_refresh_exists() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_memory_panel_exists.md");
        std::fs::write(&temp_file, "test").ok();

        let mut panel = MemoryPanel::new("/test", None);
        // 手动设置一个条目的路径到临时文件
        panel.entries[0].path = temp_file.clone();
        panel.refresh_exists();
        assert!(panel.entries[0].exists);
        assert!(!panel.entries[1].exists);

        std::fs::remove_file(&temp_file).ok();
    }
}
