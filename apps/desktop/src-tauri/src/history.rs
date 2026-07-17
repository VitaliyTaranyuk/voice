use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("db error: {0}")]
    Db(String),
    #[error("path error: {0}")]
    Path(String),
    #[error("empty text")]
    EmptyText,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: String,
    pub session_id: String,
    pub text: String,
    pub raw_text: Option<String>,
    pub app_id: Option<String>,
    pub created_at: String,
    pub favorite: bool,
}

pub struct HistoryStore {
    conn: Mutex<Connection>,
}

impl HistoryStore {
    pub fn open(path: PathBuf) -> Result<Self, HistoryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| HistoryError::Path(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(|e| HistoryError::Db(e.to_string()))?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS history_items (
              id TEXT PRIMARY KEY,
              session_id TEXT NOT NULL,
              text TEXT NOT NULL,
              raw_text TEXT,
              app_id TEXT,
              created_at TEXT NOT NULL,
              favorite INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_history_created ON history_items(created_at DESC);
            "#,
        )
        .map_err(|e| HistoryError::Db(e.to_string()))?;

        // Drop blank rows left by earlier builds (empty cards in History UI).
        conn.execute(
            "DELETE FROM history_items WHERE length(trim(text)) = 0",
            [],
        )
        .map_err(|e| HistoryError::Db(e.to_string()))?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn open_default() -> Result<Self, HistoryError> {
        let dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Voice");
        Self::open(dir.join("history.db"))
    }

    pub fn insert(
        &self,
        session_id: &str,
        text: &str,
        raw_text: Option<&str>,
        app_id: Option<&str>,
    ) -> Result<HistoryItem, HistoryError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(HistoryError::EmptyText);
        }

        let id = uuid::Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();
        let conn = self.conn.lock().expect("history mutex");
        conn.execute(
            "INSERT INTO history_items (id, session_id, text, raw_text, app_id, created_at, favorite)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![id, session_id, text, raw_text, app_id, created_at],
        )
        .map_err(|e| HistoryError::Db(e.to_string()))?;

        Ok(HistoryItem {
            id,
            session_id: session_id.to_string(),
            text: text.to_string(),
            raw_text: raw_text.map(str::to_string),
            app_id: app_id.map(str::to_string),
            created_at,
            favorite: false,
        })
    }

    pub fn list_recent(&self, limit: i64) -> Result<Vec<HistoryItem>, HistoryError> {
        let conn = self.conn.lock().expect("history mutex");
        let mut stmt = conn
            .prepare(
                "SELECT id, session_id, text, raw_text, app_id, created_at, favorite
                 FROM history_items
                 WHERE length(trim(text)) > 0
                 ORDER BY created_at DESC
                 LIMIT ?1",
            )
            .map_err(|e| HistoryError::Db(e.to_string()))?;

        let rows = stmt
            .query_map(params![limit], |row| {
                Ok(HistoryItem {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    text: row.get(2)?,
                    raw_text: row.get(3)?,
                    app_id: row.get(4)?,
                    created_at: row.get(5)?,
                    favorite: row.get::<_, i64>(6)? != 0,
                })
            })
            .map_err(|e| HistoryError::Db(e.to_string()))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| HistoryError::Db(e.to_string()))?);
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_db() -> HistoryStore {
        let path = std::env::temp_dir().join(format!(
            "voice-history-test-{}-{}.db",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        HistoryStore::open(path).expect("open temp history")
    }

    #[test]
    fn insert_and_list_recent_newest_first() {
        let store = temp_db();
        store
            .insert("s1", "first", None, Some("notepad.exe"))
            .expect("insert first");
        std::thread::sleep(Duration::from_millis(5));
        store
            .insert("s2", "second", Some("raw"), Some("cursor.exe"))
            .expect("insert second");

        let items = store.list_recent(10).expect("list");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "second");
        assert_eq!(items[1].text, "first");
        assert_eq!(items[0].raw_text.as_deref(), Some("raw"));
    }

    #[test]
    fn list_recent_empty() {
        let store = temp_db();
        let items = store.list_recent(1).expect("list");
        assert!(items.is_empty());
    }

    #[test]
    fn list_recent_respects_limit() {
        let store = temp_db();
        store.insert("s1", "a", None, None).unwrap();
        store.insert("s2", "b", None, None).unwrap();
        store.insert("s3", "c", None, None).unwrap();
        let items = store.list_recent(1).expect("list");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn insert_rejects_blank_text() {
        let store = temp_db();
        assert!(matches!(
            store.insert("s1", "   ", None, None),
            Err(HistoryError::EmptyText)
        ));
        assert!(store.list_recent(10).unwrap().is_empty());
    }

    #[test]
    fn open_purges_blank_rows() {
        let path = std::env::temp_dir().join(format!(
            "voice-history-purge-{}-{}.db",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        {
            let store = HistoryStore::open(path.clone()).expect("open");
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO history_items (id, session_id, text, raw_text, app_id, created_at, favorite)
                 VALUES ('1', 's', '', NULL, NULL, '2020-01-01T00:00:00Z', 0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO history_items (id, session_id, text, raw_text, app_id, created_at, favorite)
                 VALUES ('2', 's', 'ok', NULL, NULL, '2020-01-02T00:00:00Z', 0)",
                [],
            )
            .unwrap();
        }
        let store = HistoryStore::open(path).expect("reopen");
        let items = store.list_recent(10).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "ok");
    }
}
