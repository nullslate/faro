use crate::rows::sqlite_count_to_usize;
use crate::{Result, SessionSummaryCounts, Store};
use faro_core::{Id, Session};
use rusqlite::{OptionalExtension, params};

impl Store {
    pub fn insert_session(&self, session: &Session) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions (id, created_at, name, root_url) VALUES (?1, ?2, ?3, ?4)",
            params![
                session.id,
                session.created_at,
                session.name,
                session.root_url
            ],
        )?;
        Ok(())
    }

    pub fn sessions(&self) -> Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, name, root_url
             FROM sessions
             ORDER BY created_at ASC, id ASC",
        )?;

        let sessions = stmt
            .query_map([], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    created_at: row.get(1)?,
                    name: row.get(2)?,
                    root_url: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(sessions)
    }

    pub fn delete_session(&self, id: &str) -> Result<usize> {
        let deleted = self
            .conn
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        self.delete_orphan_bodies()?;
        Ok(deleted)
    }

    pub fn delete_all_sessions(&self) -> Result<usize> {
        let deleted = self.conn.execute("DELETE FROM sessions", [])?;
        self.delete_orphan_bodies()?;
        Ok(deleted)
    }

    pub fn delete_session_requests_before(&self, session_id: &str, before: i64) -> Result<usize> {
        let deleted = self.conn.execute(
            "DELETE FROM requests
             WHERE session_id = ?1
               AND started_at <= ?2",
            params![session_id, before],
        )?;
        self.delete_orphan_bodies()?;
        Ok(deleted)
    }

    pub fn delete_orphan_bodies(&self) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM bodies
             WHERE id NOT IN (
                 SELECT body_ref FROM responses WHERE body_ref IS NOT NULL
                 UNION
                 SELECT request_body_ref FROM requests WHERE request_body_ref IS NOT NULL
                 UNION
                 SELECT response_body_ref FROM replays WHERE response_body_ref IS NOT NULL
             )",
            [],
        )?)
    }

    pub fn checkpoint_and_vacuum(&self) -> Result<()> {
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")?;
        Ok(())
    }

    pub fn session_summary_counts(&self, session_id: &str) -> Result<SessionSummaryCounts> {
        let mut stmt = self.conn.prepare(
            "SELECT
                (SELECT count(*) FROM requests WHERE session_id = ?1),
                (SELECT count(*) FROM console_logs WHERE session_id = ?1 AND level IN ('error', 'fatal')),
                (SELECT count(*) FROM replays WHERE session_id = ?1),
                (SELECT count(*) FROM websocket_frames WHERE session_id = ?1),
                (SELECT count(*) FROM storage_events WHERE session_id = ?1),
                (SELECT count(*) FROM cookie_events WHERE session_id = ?1)",
        )?;
        let counts = stmt.query_row(params![session_id], |row| {
            Ok(SessionSummaryCounts {
                requests: sqlite_count_to_usize(row.get::<_, i64>(0)?),
                console_errors: sqlite_count_to_usize(row.get::<_, i64>(1)?),
                replays: sqlite_count_to_usize(row.get::<_, i64>(2)?),
                websocket_frames: sqlite_count_to_usize(row.get::<_, i64>(3)?),
                storage_events: sqlite_count_to_usize(row.get::<_, i64>(4)?),
                cookie_events: sqlite_count_to_usize(row.get::<_, i64>(5)?),
            })
        })?;
        Ok(counts)
    }

    pub fn session_exists(&self, id: &Id) -> Result<bool> {
        let found = self
            .conn
            .query_row("SELECT 1 FROM sessions WHERE id = ?1", params![id], |row| {
                row.get::<_, i64>(0)
            })
            .optional()?;
        Ok(found.is_some())
    }
}
