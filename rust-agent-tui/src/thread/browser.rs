use super::{ThreadId, ThreadMeta, ThreadStore};
use std::sync::Arc;

/// TUI 内 Thread 历史浏览面板
pub struct ThreadBrowser {
    pub threads: Vec<ThreadMeta>,
    /// 当前光标位置（0 = 新建对话，1+ = 历史 thread）
    pub cursor: usize,
    pub store: Arc<dyn ThreadStore>,
    /// 内容滚动偏移
    pub scroll_offset: u16,
}

impl ThreadBrowser {
    pub fn new(threads: Vec<ThreadMeta>, store: Arc<dyn ThreadStore>) -> Self {
        Self { threads, cursor: 0, store, scroll_offset: 0 }
    }

    /// 条目总数 = 1（新建）+ 历史数量
    pub fn total(&self) -> usize {
        1 + self.threads.len()
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let total = self.total();
        if total == 0 { return; }
        self.cursor = ((self.cursor as isize + delta).rem_euclid(total as isize)) as usize;
    }

    /// 当前光标是否指向「新建对话」
    pub fn is_new(&self) -> bool {
        self.cursor == 0
    }

    /// 获取光标指向的历史 thread（cursor == 0 时返回 None）
    pub fn selected_thread(&self) -> Option<&ThreadMeta> {
        if self.cursor == 0 {
            None
        } else {
            self.threads.get(self.cursor - 1)
        }
    }

    /// 获取光标指向的 ThreadId（新建时返回 None）
    pub fn selected_id(&self) -> Option<&ThreadId> {
        self.selected_thread().map(|t| &t.id)
    }

    /// 删除光标所在的历史 thread（同步，block_in_place），返回被删除的对话标题
    pub fn delete_selected(&mut self) -> Option<String> {
        if self.cursor == 0 {
            return None;
        }
        let idx = self.cursor - 1;
        let Some(meta) = self.threads.get(idx) else { return None };
        let id = meta.id.clone();
        let title = meta.title.clone().unwrap_or_else(|| "(无标题)".to_string());
        let store = self.store.clone();
        let ok = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(store.delete_thread(&id))
                .is_ok()
        });
        if ok {
            self.threads.remove(idx);
            // 光标修正：删完后保持在有效范围内
            let total = self.total();
            if self.cursor >= total {
                self.cursor = total.saturating_sub(1);
            }
            Some(title)
        } else {
            None
        }
    }
}
