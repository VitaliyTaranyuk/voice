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
    pub fn open_default() -> Result<Self, HistoryError> {
        let dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Voice");
        std::fs::create_dir_all(&dir).map_err(|e| HistoryError::Path(e.to_string()))?;
        let path = dir.join("history.db");
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
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert(
        &self,
        session_id: &str,
        text: &str,
        raw_text: Option<&str>,
        app_id: Option<&str>,
    ) -> Result<HistoryItem, HistoryError> {
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
