use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::sync::Arc;

use crate::messages::BaseMessage;
use crate::thread::{ThreadId, ThreadMeta, ThreadStore};

/// 基于 SQLite 的 ThreadStore 实现
///
/// 使用 WAL 模式提升并发读性能，parking_lot::Mutex 串行化写操作。
pub struct SqliteThreadStore {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteThreadStore {
    /// 使用指定路径打开（或创建）数据库，并初始化 Schema
    pub fn new(db_path: impl Into<PathBuf>) -> Result<Self> {
        let db_path = db_path.into();
        // 确保父目录存在
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建目录失败: {}", parent.display()))?;
        }
        let conn = Connection::open(&db_path)
            .with_context(|| format!("打开 SQLite 失败: {}", db_path.display()))?;
        // 性能优化
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;")?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.init_schema()?;
        Ok(store)
    }

    /// 使用默认路径 `~/.zen-core/threads/threads.db` 创建
    pub fn default_path() -> Result<Self> {
        let db_path = dirs_next::home_dir()
            .context("无法获取 home 目录")?
            .join(".zen-core")
            .join("threads")
            .join("threads.db");
        Self::new(db_path)
    }

    /// 初始化 Schema（幂等，可重复调用）
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS threads (
                id          TEXT PRIMARY KEY,
                title       TEXT,
                cwd         TEXT NOT NULL DEFAULT '',
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL,
                message_count INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS messages (
                message_id  TEXT PRIMARY KEY,
                thread_id   TEXT NOT NULL,
                role        TEXT NOT NULL,
                content     TEXT NOT NULL,
                FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_messages_thread_id
                ON messages (thread_id ASC);
            ",
        )?;
        Ok(())
    }
}

// ── 辅助函数 ──────────────────────────────────────────────────────────────────

fn role_of(msg: &BaseMessage) -> &'static str {
    match msg {
        BaseMessage::Human { .. } => "user",
        BaseMessage::Ai { .. } => "assistant",
        BaseMessage::System { .. } => "system",
        BaseMessage::Tool { .. } => "tool",
    }
}

fn meta_from_row(
    id: String,
    title: Option<String>,
    cwd: String,
    created_at: String,
    updated_at: String,
    message_count: i64,
) -> Result<ThreadMeta> {
    Ok(ThreadMeta {
        id,
        title,
        cwd,
        created_at: created_at.parse::<DateTime<Utc>>()?,
        updated_at: updated_at.parse::<DateTime<Utc>>()?,
        message_count: message_count as usize,
    })
}

/// 从消息列表中提取标题（取第一条 Human 消息的前 50 字符）
fn extract_title(msgs: &[BaseMessage]) -> Option<String> {
    use crate::messages::{ContentBlock, MessageContent};
    for msg in msgs {
        if let BaseMessage::Human { content, .. } = msg {
            let text = match content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::Blocks(blocks) => blocks
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::Text { text } = b {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
                MessageContent::Raw(_) => continue,
            };
            let title: String = text.chars().take(50).collect();
            if !title.is_empty() {
                return Some(title);
            }
        }
    }
    None
}

// ── ThreadStore impl ───────────────────────────────────────────────────────────

#[async_trait]
impl ThreadStore for SqliteThreadStore {
    async fn create_thread(&self, meta: ThreadMeta) -> Result<ThreadId> {
        let id = meta.id.clone();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = conn.lock();
            conn.execute(
                "INSERT INTO threads (id, title, cwd, created_at, updated_at, message_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    meta.id,
                    meta.title,
                    meta.cwd,
                    meta.created_at.to_rfc3339(),
                    meta.updated_at.to_rfc3339(),
                    meta.message_count as i64,
                ],
            )?;
            Ok(())
        })
        .await??;
        Ok(id)
    }

    async fn append_messages(&self, id: &ThreadId, msgs: &[BaseMessage]) -> Result<()> {
        if msgs.is_empty() {
            return Ok(());
        }
        let id = id.clone();
        let msgs = msgs.to_vec();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut conn = conn.lock();
            let tx = conn.transaction()?;
            for msg in &msgs {
                let message_id = msg.id().as_uuid().to_string();
                let role = role_of(msg);
                let content = serde_json::to_string(msg)?;
                tx.execute(
                    "INSERT OR IGNORE INTO messages (message_id, thread_id, role, content)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![message_id, id, role, content],
                )?;
            }
            // 更新 threads 表的 updated_at 和 message_count
            let now = Utc::now().to_rfc3339();
            tx.execute(
                "UPDATE threads SET updated_at = ?1,
                    message_count = (SELECT COUNT(*) FROM messages WHERE thread_id = ?2)
                 WHERE id = ?2",
                params![now, id],
            )?;
            // 尝试更新标题（如果还没有标题）
            if let Some(title) = extract_title(&msgs) {
                tx.execute(
                    "UPDATE threads SET title = ?1 WHERE id = ?2 AND title IS NULL",
                    params![title, id],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    async fn load_messages(&self, id: &ThreadId) -> Result<Vec<BaseMessage>> {
        let id = id.clone();
        let conn = self.conn.clone();
        let msgs = tokio::task::spawn_blocking(move || -> Result<Vec<BaseMessage>> {
            let conn = conn.lock();
            let mut stmt = conn.prepare(
                "SELECT content FROM messages WHERE thread_id = ?1 ORDER BY rowid",
            )?;
            let msgs: Result<Vec<BaseMessage>> = stmt
                .query_map(params![id], |row| row.get::<_, String>(0))?
                .map(|r| {
                    let content = r?;
                    let msg: BaseMessage =
                        serde_json::from_str(&content).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    Ok(msg)
                })
                .collect::<std::result::Result<Vec<_>, rusqlite::Error>>()
                .map_err(|e| anyhow::anyhow!(e));
            msgs
        })
        .await??;
        Ok(msgs)
    }

    async fn load_meta(&self, id: &ThreadId) -> Result<ThreadMeta> {
        let id = id.clone();
        let conn = self.conn.clone();
        let meta = tokio::task::spawn_blocking(move || -> Result<ThreadMeta> {
            let conn = conn.lock();
            conn.query_row(
                "SELECT id, title, cwd, created_at, updated_at, message_count FROM threads WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .map_err(|e| anyhow::anyhow!(e))
            .and_then(|(id, title, cwd, created_at, updated_at, mc)| {
                meta_from_row(id, title, cwd, created_at, updated_at, mc)
            })
        })
        .await??;
        Ok(meta)
    }

    async fn update_meta(&self, id: &ThreadId, meta: ThreadMeta) -> Result<()> {
        let id = id.clone();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let conn = conn.lock();
            conn.execute(
                "UPDATE threads SET title = ?1, cwd = ?2, updated_at = ?3, message_count = ?4 WHERE id = ?5",
                params![
                    meta.title,
                    meta.cwd,
                    meta.updated_at.to_rfc3339(),
                    meta.message_count as i64,
                    id,
                ],
            )?;
            Ok(())
        })
        .await??;
        Ok(())
    }

    async fn list_threads(&self) -> Result<Vec<ThreadMeta>> {
        let conn = self.conn.clone();
        let metas = tokio::task::spawn_blocking(move || -> Result<Vec<ThreadMeta>> {
            let conn = conn.lock();
            let mut stmt = conn.prepare(
                "SELECT id, title, cwd, created_at, updated_at, message_count
                 FROM threads ORDER BY updated_at DESC",
            )?;
            let metas: Result<Vec<ThreadMeta>> = stmt
                .query_map(params![], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                })?
                .map(|r| {
                    let (id, title, cwd, created_at, updated_at, mc) =
                        r.map_err(|e| anyhow::anyhow!(e))?;
                    meta_from_row(id, title, cwd, created_at, updated_at, mc)
                })
                .collect();
            metas
        })
        .await??;
        Ok(metas)
    }

    async fn delete_thread(&self, id: &ThreadId) -> Result<()> {
        let id = id.clone();
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut conn = conn.lock();
            let tx = conn.transaction()?;
            // messages 由 FOREIGN KEY CASCADE 自动删除
            tx.execute("DELETE FROM threads WHERE id = ?1", params![id])?;
            tx.commit()?;
            Ok(())
        })
        .await??;
        Ok(())
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_store() -> SqliteThreadStore {
        let dir = tempdir().unwrap();
        SqliteThreadStore::new(dir.path().join("test.db")).unwrap()
    }

    #[tokio::test]
    async fn test_create_append_load() {
        let store = make_store();
        let meta = ThreadMeta::new("/tmp");
        let id = store.create_thread(meta).await.unwrap();

        let msgs = vec![
            BaseMessage::human("Hello"),
            BaseMessage::ai("Hi there"),
        ];
        store.append_messages(&id, &msgs).await.unwrap();

        let loaded = store.load_messages(&id).await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].content(), "Hello");
        assert_eq!(loaded[1].content(), "Hi there");
    }

    #[tokio::test]
    async fn test_list_threads_order() {
        let store = make_store();

        let m1 = ThreadMeta::new("/a");
        let id1 = store.create_thread(m1).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let m2 = ThreadMeta::new("/b");
        let id2 = store.create_thread(m2).await.unwrap();

        // 给 id2 追加消息，更新 updated_at
        store
            .append_messages(&id2, &[BaseMessage::human("msg")])
            .await
            .unwrap();

        let list = store.list_threads().await.unwrap();
        assert_eq!(list.len(), 2);
        // id2 updated_at 更新，应排在第一位
        assert_eq!(list[0].id, id2);
        assert_eq!(list[1].id, id1);
    }

    #[tokio::test]
    async fn test_delete_thread_cascade() {
        let store = make_store();
        let meta = ThreadMeta::new("/tmp");
        let id = store.create_thread(meta).await.unwrap();
        store
            .append_messages(&id, &[BaseMessage::human("msg")])
            .await
            .unwrap();

        store.delete_thread(&id).await.unwrap();

        // 消息应该被级联删除
        let msgs = store.load_messages(&id).await;
        // 线程不存在时 load_messages 应返回空（因为 SELECT 无结果）
        assert!(msgs.unwrap().is_empty());

        // 元数据应不存在
        let meta_result = store.load_meta(&id).await;
        assert!(meta_result.is_err());
    }

    #[tokio::test]
    async fn test_message_order_after_multiple_appends() {
        let store = make_store();
        let meta = ThreadMeta::new("/tmp");
        let id = store.create_thread(meta).await.unwrap();

        store
            .append_messages(&id, &[BaseMessage::human("msg1")])
            .await
            .unwrap();
        store
            .append_messages(&id, &[BaseMessage::ai("reply1")])
            .await
            .unwrap();
        store
            .append_messages(&id, &[BaseMessage::human("msg2")])
            .await
            .unwrap();

        let loaded = store.load_messages(&id).await.unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].content(), "msg1");
        assert_eq!(loaded[1].content(), "reply1");
        assert_eq!(loaded[2].content(), "msg2");
    }

    #[tokio::test]
    async fn test_title_auto_set() {
        let store = make_store();
        let meta = ThreadMeta::new("/tmp");
        let id = store.create_thread(meta).await.unwrap();

        store
            .append_messages(&id, &[BaseMessage::human("这是一条测试消息")])
            .await
            .unwrap();

        let loaded_meta = store.load_meta(&id).await.unwrap();
        assert!(loaded_meta.title.is_some());
        assert!(loaded_meta.title.unwrap().contains("这是一条测试消息"));
    }
}
