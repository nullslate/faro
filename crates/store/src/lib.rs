use faro_core::{BodyRecord, ConsoleLog, EventEnvelope, Run, Tab, WebSocketFrameRecord};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::path::Path;

mod cookies;
mod models;
mod network;
mod rows;
mod schema;
mod scripts;
mod sessions;
mod sql;
mod storage;

pub use models::{
    BodyStorageStats, RepeatedRequestGroup, ScriptRecord, SessionStorageStats,
    SessionSummaryCounts, TableRowCount,
};
use rows::{console_log_from_row, optional_json, parse_run_trigger, websocket_frame_from_row};
pub use sql::SqlQueryResult;
#[cfg(test)]
pub(crate) use sql::validate_readonly_sql;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("query rejected: {0}")]
    QueryRejected(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    fn configure(&self) -> Result<()> {
        self.conn.pragma_update(None, "journal_mode", "WAL")?;
        self.conn.pragma_update(None, "foreign_keys", "ON")?;
        self.conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(schema::SCHEMA)?;
        Ok(())
    }

    pub fn insert_tab(&self, tab: &Tab) -> Result<()> {
        self.conn.execute(
            "INSERT INTO tabs (id, session_id, created_at, current_url, title)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                tab.id,
                tab.session_id,
                tab.created_at,
                tab.current_url,
                tab.title
            ],
        )?;
        Ok(())
    }

    pub fn insert_run(&self, run: &Run) -> Result<()> {
        self.conn.execute(
            "INSERT INTO runs (id, session_id, tab_id, started_at, ended_at, url, trigger)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                run.id,
                run.session_id,
                run.tab_id,
                run.started_at,
                run.ended_at,
                run.url,
                serde_json::to_string(&run.trigger)?.trim_matches('"')
            ],
        )?;
        Ok(())
    }

    pub fn runs_for_session(&self, session_id: &str) -> Result<Vec<Run>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, started_at, ended_at, url, trigger
             FROM runs
             WHERE session_id = ?1
             ORDER BY started_at ASC, id ASC",
        )?;

        let runs = stmt
            .query_map(params![session_id], |row| {
                let trigger: String = row.get(6)?;
                Ok(Run {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    url: row.get(5)?,
                    trigger: parse_run_trigger(&trigger),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(runs)
    }

    pub fn append_event(&self, event: &EventEnvelope) -> Result<()> {
        self.conn.execute(
            "INSERT INTO events (id, session_id, tab_id, run_id, ts, kind, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.id,
                event.session_id,
                event.tab_id,
                event.run_id,
                event.ts,
                event.kind.as_str(),
                serde_json::to_string(&event.payload)?
            ],
        )?;
        Ok(())
    }

    pub fn insert_console_log(&self, log: &ConsoleLog) -> Result<()> {
        self.conn.execute(
            "INSERT INTO console_logs
             (id, session_id, tab_id, run_id, ts, level, message, source, line, stack_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                log.id,
                log.session_id,
                log.tab_id,
                log.run_id,
                log.ts,
                log.level.as_str(),
                log.message,
                log.source,
                log.line,
                optional_json(&log.stack_json)?
            ],
        )?;
        Ok(())
    }

    pub fn insert_websocket_frame(&self, frame: &WebSocketFrameRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO websocket_frames
             (id, session_id, tab_id, run_id, browser_request_id, ts, direction, opcode, mask, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                frame.id,
                frame.session_id,
                frame.tab_id,
                frame.run_id,
                frame.browser_request_id,
                frame.ts,
                frame.direction.as_str(),
                frame.opcode,
                if frame.mask { 1 } else { 0 },
                frame.payload
            ],
        )?;
        Ok(())
    }

    pub fn console_logs_for_session(&self, session_id: &str) -> Result<Vec<ConsoleLog>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, ts, level, message, source, line, stack_json
             FROM console_logs
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let logs = stmt
            .query_map(params![session_id], console_log_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(logs)
    }

    pub fn console_logs_for_session_after(
        &self,
        session_id: &str,
        after_ts: i64,
    ) -> Result<Vec<ConsoleLog>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, ts, level, message, source, line, stack_json
             FROM console_logs
             WHERE session_id = ?1
               AND ts > ?2
             ORDER BY ts ASC, id ASC",
        )?;

        let logs = stmt
            .query_map(params![session_id, after_ts], console_log_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(logs)
    }

    pub fn console_log_ids_for_session(&self, session_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT id
             FROM console_logs
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let ids = stmt
            .query_map(params![session_id], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(ids)
    }

    pub fn websocket_frames_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<WebSocketFrameRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, browser_request_id, ts, direction, opcode, mask, payload
             FROM websocket_frames
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let frames = stmt
            .query_map(params![session_id], websocket_frame_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(frames)
    }

    pub fn websocket_frame_ids_for_session(&self, session_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT id
             FROM websocket_frames
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let ids = stmt
            .query_map(params![session_id], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(ids)
    }

    pub fn websocket_frames_for_session_after(
        &self,
        session_id: &str,
        after_ts: i64,
    ) -> Result<Vec<WebSocketFrameRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, browser_request_id, ts, direction, opcode, mask, payload
             FROM websocket_frames
             WHERE session_id = ?1
               AND ts > ?2
             ORDER BY ts ASC, id ASC",
        )?;

        let frames = stmt
            .query_map(params![session_id, after_ts], websocket_frame_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(frames)
    }

    pub fn event_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?)
    }
}

pub fn inline_text_body(content_type: Option<String>, text: String) -> BodyRecord {
    let digest = Sha256::digest(text.as_bytes());
    let sha256 = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    BodyRecord::inline_text(content_type, text, sha256)
}

#[cfg(test)]
mod tests;
