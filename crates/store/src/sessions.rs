use crate::rows::sqlite_count_to_usize;
use crate::{
    BodyStorageStats, RepeatedRequestGroup, Result, SessionStorageStats, SessionSummaryCounts,
    Store, TableRowCount,
};
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

    pub fn prune_session_requests(&self, session_id: &str, max_requests: usize) -> Result<usize> {
        let max_requests = max_requests.max(1);
        let excess = self.conn.query_row(
            "SELECT max(count(*) - ?2, 0)
             FROM requests
             WHERE session_id = ?1",
            params![session_id, max_requests as i64],
            |row| row.get::<_, i64>(0),
        )?;
        if excess <= 0 {
            return Ok(0);
        }

        let deleted = self.conn.execute(
            "DELETE FROM requests
             WHERE id IN (
                 SELECT id
                 FROM requests
                 WHERE session_id = ?1
                 ORDER BY started_at ASC, id ASC
                 LIMIT ?2
             )",
            params![session_id, excess],
        )?;
        self.delete_request_events_without_request(session_id)?;
        self.delete_orphan_bodies()?;
        Ok(deleted)
    }

    pub fn prune_repeated_session_requests(
        &self,
        session_id: &str,
        max_repeated: usize,
    ) -> Result<usize> {
        if max_repeated == 0 {
            return Ok(0);
        }
        let deleted = self.conn.execute(
            "DELETE FROM requests
             WHERE id IN (
                 SELECT id
                 FROM (
                     SELECT id,
                            row_number() OVER (
                                PARTITION BY method, url, coalesce(resource_type, '')
                                ORDER BY started_at DESC, id DESC
                            ) AS duplicate_index
                     FROM requests
                     WHERE session_id = ?1
                 )
                 WHERE duplicate_index > ?2
             )",
            params![session_id, max_repeated as i64],
        )?;
        if deleted > 0 {
            self.delete_request_events_without_request(session_id)?;
            self.delete_orphan_bodies()?;
        }
        Ok(deleted)
    }

    pub fn prune_session_console_logs(&self, session_id: &str, max_logs: usize) -> Result<usize> {
        let deleted = self.prune_session_rows("console_logs", session_id, max_logs)?;
        if deleted > 0 {
            self.delete_console_events_without_log(session_id)?;
        }
        Ok(deleted)
    }

    pub fn prune_session_websocket_frames(
        &self,
        session_id: &str,
        max_frames: usize,
    ) -> Result<usize> {
        let deleted = self.prune_session_rows("websocket_frames", session_id, max_frames)?;
        if deleted > 0 {
            self.delete_websocket_events_without_frame(session_id)?;
        }
        Ok(deleted)
    }

    fn prune_session_rows(
        &self,
        table: &'static str,
        session_id: &str,
        max_rows: usize,
    ) -> Result<usize> {
        let max_rows = max_rows.max(1);
        let excess = self.conn.query_row(
            &format!(
                "SELECT max(count(*) - ?2, 0)
                 FROM {table}
                 WHERE session_id = ?1"
            ),
            params![session_id, max_rows as i64],
            |row| row.get::<_, i64>(0),
        )?;
        if excess <= 0 {
            return Ok(0);
        }

        Ok(self.conn.execute(
            &format!(
                "DELETE FROM {table}
                 WHERE id IN (
                     SELECT id
                     FROM {table}
                     WHERE session_id = ?1
                     ORDER BY ts ASC, id ASC
                     LIMIT ?2
                 )"
            ),
            params![session_id, excess],
        )?)
    }

    fn delete_request_events_without_request(&self, session_id: &str) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM events
             WHERE session_id = ?1
               AND kind IN ('request_started', 'response_received', 'request_completed')
               AND json_extract(payload_json, '$.request_id') IS NOT NULL
               AND json_extract(payload_json, '$.request_id') NOT IN (
                   SELECT id FROM requests WHERE session_id = ?1
               )",
            params![session_id],
        )?)
    }

    fn delete_console_events_without_log(&self, session_id: &str) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM events
             WHERE session_id = ?1
               AND kind = 'console_logged'
               AND json_extract(payload_json, '$.console_log_id') IS NOT NULL
               AND json_extract(payload_json, '$.console_log_id') NOT IN (
                   SELECT id FROM console_logs WHERE session_id = ?1
               )",
            params![session_id],
        )?)
    }

    fn delete_websocket_events_without_frame(&self, session_id: &str) -> Result<usize> {
        Ok(self.conn.execute(
            "DELETE FROM events
             WHERE session_id = ?1
               AND kind = 'websocket_frame'
               AND json_extract(payload_json, '$.websocket_frame_id') IS NOT NULL
               AND json_extract(payload_json, '$.websocket_frame_id') NOT IN (
                   SELECT id FROM websocket_frames WHERE session_id = ?1
               )",
            params![session_id],
        )?)
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

    pub fn table_row_counts(&self) -> Result<Vec<TableRowCount>> {
        const TABLES: &[&str] = &[
            "sessions",
            "tabs",
            "runs",
            "events",
            "requests",
            "responses",
            "replays",
            "bodies",
            "console_logs",
            "storage_snapshots",
            "storage_events",
            "cookie_snapshots",
            "cookie_events",
            "websocket_frames",
            "scripts",
        ];

        let mut counts = Vec::with_capacity(TABLES.len());
        for table in TABLES {
            let rows =
                self.conn
                    .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                        Ok(sqlite_count_to_usize(row.get::<_, i64>(0)?))
                    })?;
            counts.push(TableRowCount {
                table: (*table).to_string(),
                rows,
            });
        }
        Ok(counts)
    }

    pub fn body_storage_stats(&self) -> Result<BodyStorageStats> {
        let mut stmt = self.conn.prepare(
            "SELECT
                count(*),
                coalesce(sum(size), 0),
                coalesce(sum(CASE WHEN storage_kind = 'inline' THEN size ELSE 0 END), 0),
                coalesce(sum(CASE WHEN storage_kind != 'inline' THEN size ELSE 0 END), 0)
             FROM bodies",
        )?;
        let stats = stmt.query_row([], |row| {
            Ok(BodyStorageStats {
                bodies: sqlite_count_to_usize(row.get::<_, i64>(0)?),
                total_bytes: non_negative_i64_to_u64(row.get::<_, i64>(1)?),
                inline_bytes: non_negative_i64_to_u64(row.get::<_, i64>(2)?),
                external_bytes: non_negative_i64_to_u64(row.get::<_, i64>(3)?),
            })
        })?;
        Ok(stats)
    }

    pub fn top_session_storage_stats(&self, limit: usize) -> Result<Vec<SessionStorageStats>> {
        let limit = limit.max(1) as i64;
        let mut stmt = self.conn.prepare(
            "WITH body_refs AS (
                SELECT session_id, request_body_ref AS body_id
                FROM requests
                WHERE request_body_ref IS NOT NULL
                UNION
                SELECT requests.session_id, responses.body_ref AS body_id
                FROM responses
                JOIN requests ON requests.id = responses.request_id
                WHERE responses.body_ref IS NOT NULL
                UNION
                SELECT session_id, response_body_ref AS body_id
                FROM replays
                WHERE response_body_ref IS NOT NULL
             ),
             body_totals AS (
                SELECT body_refs.session_id,
                       count(*) AS bodies,
                       coalesce(sum(bodies.size), 0) AS body_bytes
                FROM body_refs
                JOIN bodies ON bodies.id = body_refs.body_id
                GROUP BY body_refs.session_id
             )
             SELECT sessions.id,
                    sessions.created_at,
                    sessions.name,
                    sessions.root_url,
                    (SELECT count(*) FROM requests WHERE session_id = sessions.id),
                    (SELECT count(*) FROM console_logs WHERE session_id = sessions.id AND level IN ('error', 'fatal')),
                    (SELECT count(*) FROM replays WHERE session_id = sessions.id),
                    (SELECT count(*) FROM websocket_frames WHERE session_id = sessions.id),
                    (SELECT count(*) FROM storage_events WHERE session_id = sessions.id),
                    (SELECT count(*) FROM cookie_events WHERE session_id = sessions.id),
                    coalesce(body_totals.bodies, 0),
                    coalesce(body_totals.body_bytes, 0)
             FROM sessions
             LEFT JOIN body_totals ON body_totals.session_id = sessions.id
             ORDER BY coalesce(body_totals.body_bytes, 0) DESC,
                      (SELECT count(*) FROM requests WHERE session_id = sessions.id) DESC,
                      sessions.created_at DESC,
                      sessions.id ASC
             LIMIT ?1",
        )?;
        let stats = stmt
            .query_map(params![limit], |row| {
                Ok(SessionStorageStats {
                    id: row.get(0)?,
                    created_at: row.get(1)?,
                    name: row.get(2)?,
                    root_url: row.get(3)?,
                    requests: sqlite_count_to_usize(row.get::<_, i64>(4)?),
                    console_errors: sqlite_count_to_usize(row.get::<_, i64>(5)?),
                    replays: sqlite_count_to_usize(row.get::<_, i64>(6)?),
                    websocket_frames: sqlite_count_to_usize(row.get::<_, i64>(7)?),
                    storage_events: sqlite_count_to_usize(row.get::<_, i64>(8)?),
                    cookie_events: sqlite_count_to_usize(row.get::<_, i64>(9)?),
                    bodies: sqlite_count_to_usize(row.get::<_, i64>(10)?),
                    body_bytes: non_negative_i64_to_u64(row.get::<_, i64>(11)?),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(stats)
    }

    pub fn top_repeated_request_groups(&self, limit: usize) -> Result<Vec<RepeatedRequestGroup>> {
        let limit = limit.max(1) as i64;
        let mut stmt = self.conn.prepare(
            "WITH response_stats AS (
                SELECT request_id,
                       max(coalesce(body_size, 0)) AS body_size,
                       max(coalesce(status_code, 0)) AS status_code
                FROM responses
                GROUP BY request_id
             )
             SELECT requests.session_id,
                    sessions.root_url,
                    requests.method,
                    requests.resource_type,
                    requests.url,
                    count(*) AS requests_count,
                    coalesce(sum(CASE WHEN response_stats.status_code >= 400 THEN 1 ELSE 0 END), 0),
                    coalesce(sum(response_stats.body_size), 0),
                    min(requests.started_at),
                    max(requests.started_at)
             FROM requests
             LEFT JOIN sessions ON sessions.id = requests.session_id
             LEFT JOIN response_stats ON response_stats.request_id = requests.id
             GROUP BY requests.session_id,
                      requests.method,
                      coalesce(requests.resource_type, ''),
                      requests.url
             HAVING requests_count > 1
             ORDER BY requests_count DESC,
                      coalesce(sum(response_stats.body_size), 0) DESC,
                      max(requests.started_at) DESC
             LIMIT ?1",
        )?;
        let groups = stmt
            .query_map(params![limit], |row| {
                Ok(RepeatedRequestGroup {
                    session_id: row.get(0)?,
                    root_url: row.get(1)?,
                    method: row.get(2)?,
                    resource_type: row.get(3)?,
                    url: row.get(4)?,
                    requests: sqlite_count_to_usize(row.get::<_, i64>(5)?),
                    error_responses: sqlite_count_to_usize(row.get::<_, i64>(6)?),
                    body_bytes: non_negative_i64_to_u64(row.get::<_, i64>(7)?),
                    first_started_at: row.get(8)?,
                    last_started_at: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(groups)
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

fn non_negative_i64_to_u64(value: i64) -> u64 {
    if value < 0 { 0 } else { value as u64 }
}
