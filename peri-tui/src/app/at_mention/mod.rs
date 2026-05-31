pub mod file_search;
pub mod popup;

use std::{collections::HashMap, time::Instant};

use file_search::FileCandidate;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// 搜索节流间隔（毫秒）
const SEARCH_DEBOUNCE_MS: u64 = 300;
/// 缓存上限：超过后清空重建
const CACHE_MAX_ENTRIES: usize = 64;

/// @ 提及状态：管理文件搜索候选、选择和弹窗
pub struct AtMentionState {
    pub active: bool,
    pub query: String,
    /// @ 符号在文本中的字符位置
    pub query_start: usize,
    pub candidates: Vec<FileCandidate>,
    pub selected: usize,
    pub scroll_offset: usize,
    /// 异步搜索取消令牌
    pub cancel_token: Option<CancellationToken>,
    /// 异步搜索结果接收端
    search_rx: Option<mpsc::UnboundedReceiver<(String, Vec<FileCandidate>)>>,
    /// 缓存：query → 搜索结果（避免重复 glob）
    search_cache: HashMap<String, Vec<FileCandidate>>,
    /// 上次 glob 搜索的时间戳：节流用
    last_search_time: Option<Instant>,
    /// 上次触发 glob 的 query 前缀（query 变长时从缓存过滤，无需重新 IO）
    last_glob_query: String,
}

impl Default for AtMentionState {
    fn default() -> Self {
        Self::new()
    }
}

impl AtMentionState {
    pub fn new() -> Self {
        Self {
            active: false,
            query: String::new(),
            query_start: 0,
            candidates: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            cancel_token: None,
            search_rx: None,
            search_cache: HashMap::new(),
            last_search_time: None,
            last_glob_query: String::new(),
        }
    }

    /// 检测光标位置前是否有 @ 触发模式
    /// 返回 (查询字符串不含@, @的位置)
    pub fn detect(text: &str, cursor_pos: usize) -> Option<(String, usize)> {
        if cursor_pos == 0 || cursor_pos > text.len() {
            return None;
        }

        let before_cursor = &text[..cursor_pos];

        // 查找最后一个 @
        let at_pos = before_cursor.rfind('@')?;
        let query = &before_cursor[at_pos + '@'.len_utf8()..];

        // @ 后面至少要有 1 个字符
        if query.is_empty() {
            return None;
        }

        // 检查 @ 前面的字符：必须是行首或空白
        if at_pos > 0 {
            let char_before = before_cursor[..at_pos].chars().next_back().unwrap();
            if !char_before.is_whitespace() && char_before != '\n' {
                return None;
            }
        }

        Some((query.to_string(), at_pos))
    }

    /// 激活 @ 提及模式
    pub fn activate(&mut self, query: String, query_start: usize) {
        self.active = true;
        self.query = query;
        self.query_start = query_start;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// 关闭 @ 提及模式
    pub fn close(&mut self) {
        self.active = false;
        self.query.clear();
        self.candidates.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }
    }

    /// 更新候选列表
    pub fn update_candidates(&mut self, candidates: Vec<FileCandidate>) {
        let len = candidates.len();
        self.candidates = candidates;
        if self.selected >= len && len > 0 {
            self.selected = len - 1;
        }
    }

    /// 判断当前 query 是否需要执行 glob 搜索。
    /// 返回 Some(candidates) 表示可以从缓存/过滤得到结果，None 表示需要 glob。
    pub fn try_filter_from_cache(&self, query: &str) -> Option<Vec<FileCandidate>> {
        // 精确缓存命中
        if let Some(cached) = self.search_cache.get(query) {
            return Some(cached.clone());
        }

        // query 是上次 glob query 的延续（变长）：从上次结果中过滤
        if query.starts_with(&self.last_glob_query) && !self.last_glob_query.is_empty() {
            if let Some(base_results) = self.search_cache.get(&self.last_glob_query) {
                let filtered = file_search::filter_candidates(base_results, query);
                return Some(filtered);
            }
        }

        None
    }

    /// 缓存搜索结果并记录时间戳
    pub fn cache_result(&mut self, query: &str, candidates: Vec<FileCandidate>) {
        if self.search_cache.len() >= CACHE_MAX_ENTRIES {
            self.search_cache.clear();
        }
        self.search_cache.insert(query.to_string(), candidates);
        self.last_search_time = Some(Instant::now());
    }

    /// 记录本次 glob 对应的 query 前缀
    pub fn set_last_glob_query(&mut self, query: &str) {
        self.last_glob_query = query.to_string();
    }

    /// 判断是否应该执行 glob（节流）
    pub fn should_search_now(&self) -> bool {
        match self.last_search_time {
            Some(t) => t.elapsed().as_millis() as u64 >= SEARCH_DEBOUNCE_MS,
            None => true,
        }
    }

    /// 启动异步搜索：在后台线程执行 glob，结果通过 channel 回传
    /// 返回 true 表示成功启动，false 表示已有进行中的搜索或被节流跳过
    pub fn start_async_search(&mut self, cwd: String, query: String) -> bool {
        // 取消旧的搜索任务
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());
        self.search_rx = Some(rx);

        let token = cancel_token;
        tokio::spawn(async move {
            // 检查取消
            if token.is_cancelled() {
                return;
            }
            // search_files 是同步阻塞的 IO，用 spawn_blocking 或直接在 spawn 中执行
            // 由于 glob 是 CPU+IO 密集，使用 blocking 比较合适
            let query_clone = query.clone();
            let candidates =
                tokio::task::spawn_blocking(move || file_search::search_files(&cwd, &query_clone))
                    .await;
            // 再次检查取消（搜索期间可能已被取消）
            if token.is_cancelled() {
                return;
            }
            if let Ok(candidates) = candidates {
                let _ = tx.send((query, candidates));
            }
        });

        true
    }

    /// 检查异步搜索结果，返回 true 表示有新结果需要更新 UI
    pub fn poll_search_result(&mut self) -> bool {
        // 先 take rx 避免借用冲突
        let mut rx = match self.search_rx.take() {
            Some(rx) => rx,
            None => return false,
        };

        let mut updated = false;
        let mut disconnected = false;
        loop {
            match rx.try_recv() {
                Ok((query, candidates)) => {
                    // 确认结果与当前 query 匹配（可能已被取消但消息已发出）
                    if self.active && self.query == query {
                        self.cache_result(&query, candidates.clone());
                        self.set_last_glob_query(&query);
                        self.update_candidates(candidates);
                        updated = true;
                    } else if !self.active || !query.starts_with(&self.query) {
                        // query 不匹配且不是前缀，缓存结果供后续使用
                        self.cache_result(&query, candidates);
                        self.set_last_glob_query(&query);
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(_) => {
                    disconnected = true;
                    break;
                }
            }
        }

        // 非 disconnected 时放回 rx
        if !disconnected {
            self.search_rx = Some(rx);
        }
        updated
    }

    /// 上移选择
    pub fn move_up(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        if self.selected > 0 {
            self.selected -= 1;
        } else {
            self.selected = self.candidates.len() - 1;
        }
        self.adjust_scroll();
    }

    /// 下移选择
    pub fn move_down(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        if self.selected < self.candidates.len() - 1 {
            self.selected += 1;
        } else {
            self.selected = 0;
        }
        self.adjust_scroll();
    }

    /// 调整滚动偏移，确保选中项在视口内
    pub fn adjust_scroll(&mut self) {
        let viewport = popup::MAX_VIEWPORT.min(self.candidates.len());
        if viewport == 0 {
            self.scroll_offset = 0;
            return;
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + viewport {
            self.scroll_offset = self.selected - viewport + 1;
        }
    }

    /// 获取当前选中的候选
    pub fn selected_candidate(&self) -> Option<&FileCandidate> {
        self.candidates.get(self.selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_at_sign_with_text() {
        let text = "请看 @main";
        let result = AtMentionState::detect(text, text.len());
        assert!(result.is_some(), "应检测到 @ 提及");
        let (query, pos) = result.unwrap();
        assert_eq!(query, "main");
        assert_eq!(pos, "请看 ".len());
    }

    #[test]
    fn test_detect_no_at_sign() {
        let result = AtMentionState::detect("hello world", "hello world".len());
        assert!(result.is_none(), "无 @ 应返回 None");
    }

    #[test]
    fn test_detect_at_sign_only() {
        let result = AtMentionState::detect("看 @", "看 @".len());
        assert!(result.is_none(), "@ 后无内容应返回 None");
    }

    #[test]
    fn test_detect_path_with_slash() {
        let text = "看 @src/main";
        let result = AtMentionState::detect(text, text.len());
        assert!(result.is_some());
        let (query, _) = result.unwrap();
        assert_eq!(query, "src/main");
    }

    #[test]
    fn test_detect_not_at_line_start() {
        let result = AtMentionState::detect("user@example", "user@example".len());
        assert!(result.is_none(), "非空白前导的 @ 不应触发");
    }

    #[test]
    fn test_move_up_down() {
        let mut state = AtMentionState::new();
        state.active = true;
        state.candidates = vec![
            FileCandidate {
                path: "a.rs".into(),
                display: "a.rs".into(),
                is_dir: false,
                score: 10,
            },
            FileCandidate {
                path: "b.rs".into(),
                display: "b.rs".into(),
                is_dir: false,
                score: 5,
            },
            FileCandidate {
                path: "c.rs".into(),
                display: "c.rs".into(),
                is_dir: false,
                score: 1,
            },
        ];
        assert_eq!(state.selected, 0);
        state.move_down();
        assert_eq!(state.selected, 1);
        state.move_down();
        assert_eq!(state.selected, 2);
        state.move_down(); // 循环回 0
        assert_eq!(state.selected, 0);
        state.move_up(); // 循环到末尾
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn test_cache_hit() {
        let mut state = AtMentionState::new();
        let candidates = vec![FileCandidate {
            path: "main.rs".into(),
            display: "main.rs".into(),
            is_dir: false,
            score: 10,
        }];
        state.cache_result("main", candidates.clone());
        let cached = state.try_filter_from_cache("main");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().len(), 1);
    }

    #[test]
    fn test_should_search_now_first_time() {
        let state = AtMentionState::new();
        assert!(state.should_search_now(), "首次应允许搜索");
    }
}
