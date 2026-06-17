use faro_core::{
    BodyRecord, ConsoleLevel, ConsoleLog, CookieEventRecord, CookieSnapshotRecord, EventEnvelope,
    Header, Id, ReplayRecord, RequestRecord, RequestStatus, ResponseRecord, Run, Session,
    StorageEventRecord, StorageSnapshotRecord, Tab, WebSocketFrameDirection, WebSocketFrameRecord,
    new_id, now_ms,
};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, params,
    types::{Type, ValueRef},
};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Instant;

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

#[derive(Debug, Clone)]
pub struct ScriptRecord {
    pub id: String,
    pub name: String,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_run_at: Option<i64>,
}

impl ScriptRecord {
    pub fn new(name: impl Into<String>, body: impl Into<String>) -> Self {
        let now = now_ms();
        Self {
            id: new_id(),
            name: name.into(),
            body: body.into(),
            created_at: now,
            updated_at: now,
            last_run_at: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SqlQueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub duration_ms: u128,
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

    pub fn query_readonly(path: impl AsRef<Path>, sql: &str) -> Result<SqlQueryResult> {
        validate_readonly_sql(sql)?;
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.pragma_update(None, "query_only", "ON")?;
        let mut stmt = conn.prepare(sql)?;
        if !stmt.readonly() {
            return Err(StoreError::QueryRejected(
                "statement is not read-only".to_string(),
            ));
        }
        let columns = stmt
            .column_names()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let column_count = stmt.column_count();
        let started = Instant::now();
        let mut rows = stmt.query([])?;
        let mut result_rows = Vec::new();
        while let Some(row) = rows.next()? {
            let mut values = Vec::with_capacity(column_count);
            for index in 0..column_count {
                values.push(sql_value_to_string(row.get_ref(index)?));
            }
            result_rows.push(values);
        }
        Ok(SqlQueryResult {
            columns,
            rows: result_rows,
            duration_ms: started.elapsed().as_millis(),
        })
    }

    pub fn schema_sql(path: impl AsRef<Path>) -> Result<String> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut stmt = conn.prepare(
            "SELECT sql
             FROM sqlite_schema
             WHERE sql IS NOT NULL
               AND type IN ('table', 'index', 'view', 'trigger')
               AND name NOT LIKE 'sqlite_%'
             ORDER BY type, name",
        )?;
        let statements = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(statements.join(";\n\n") + ";\n")
    }

    fn configure(&self) -> Result<()> {
        self.conn.pragma_update(None, "journal_mode", "WAL")?;
        self.conn.pragma_update(None, "foreign_keys", "ON")?;
        self.conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA)?;
        Ok(())
    }

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

    pub fn insert_request(&self, request: &RequestRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO requests
             (id, session_id, tab_id, run_id, browser_request_id, started_at, completed_at,
              method, url, resource_type, initiator, request_headers_json, request_body_ref, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                request.id,
                request.session_id,
                request.tab_id,
                request.run_id,
                request.browser_request_id,
                request.started_at,
                request.completed_at,
                request.method,
                request.url,
                request.resource_type,
                request.initiator,
                serde_json::to_string(&request.request_headers)?,
                request.request_body_ref,
                request.status.as_str()
            ],
        )?;
        Ok(())
    }

    pub fn complete_request(&self, request: &RequestRecord) -> Result<()> {
        self.conn.execute(
            "UPDATE requests
             SET completed_at = ?2, status = ?3
             WHERE id = ?1",
            params![request.id, request.completed_at, request.status.as_str()],
        )?;
        Ok(())
    }

    pub fn insert_response(&self, response: &ResponseRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO responses
             (id, request_id, received_at, status_code, status_text, mime_type,
              response_headers_json, body_ref, body_size, body_truncated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                response.id,
                response.request_id,
                response.received_at,
                response.status_code,
                response.status_text,
                response.mime_type,
                serde_json::to_string(&response.response_headers)?,
                response.body_ref,
                response.body_size,
                response.body_truncated
            ],
        )?;
        Ok(())
    }

    pub fn insert_body(&self, body: &BodyRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO bodies
             (id, content_type, encoding, size, sha256, storage_kind, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                body.id,
                body.content_type,
                body.encoding,
                body.size,
                body.sha256,
                body.storage_kind,
                &body.data
            ],
        )?;
        Ok(())
    }

    pub fn response_body(&self, body_id: &str) -> Result<Option<BodyRecord>> {
        self.conn
            .query_row(
                "SELECT id, content_type, encoding, size, sha256, storage_kind, data
                 FROM bodies
                 WHERE id = ?1",
                params![body_id],
                |row| {
                    Ok(BodyRecord {
                        id: row.get(0)?,
                        content_type: row.get(1)?,
                        encoding: row.get(2)?,
                        size: row.get(3)?,
                        sha256: row.get(4)?,
                        storage_kind: row.get(5)?,
                        data: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn attach_body_to_latest_response(
        &self,
        session_id: &str,
        method: &str,
        url: &str,
        status_code: Option<i64>,
        body: &BodyRecord,
        body_truncated: bool,
    ) -> Result<bool> {
        let response_id = self
            .conn
            .query_row(
                "SELECT responses.id
                 FROM responses
                 JOIN requests ON requests.id = responses.request_id
                 WHERE requests.session_id = ?1
                   AND requests.method = ?2
                   AND requests.url = ?3
                   AND (?4 IS NULL OR responses.status_code = ?4)
                   AND responses.body_ref IS NULL
                 ORDER BY responses.received_at DESC, responses.id DESC
                 LIMIT 1",
                params![session_id, method, url, status_code],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        let Some(response_id) = response_id else {
            return Ok(false);
        };

        self.insert_body(body)?;

        self.conn.execute(
            "UPDATE responses
             SET body_ref = ?2,
                 body_size = ?3,
                 body_truncated = ?4
             WHERE id = ?1",
            params![
                response_id,
                body.id,
                body.size,
                if body_truncated { 1 } else { 0 }
            ],
        )?;

        Ok(true)
    }

    pub fn attach_body_to_response_by_browser_request_id(
        &self,
        browser_request_id: &str,
        body: &BodyRecord,
        body_truncated: bool,
    ) -> Result<bool> {
        let response_id = self
            .conn
            .query_row(
                "SELECT responses.id
                 FROM responses
                 JOIN requests ON requests.id = responses.request_id
                 WHERE requests.browser_request_id = ?1
                   AND responses.body_ref IS NULL
                 ORDER BY responses.received_at DESC, responses.id DESC
                 LIMIT 1",
                params![browser_request_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        let Some(response_id) = response_id else {
            return Ok(false);
        };

        self.insert_body(body)?;

        self.conn.execute(
            "UPDATE responses
             SET body_ref = ?2,
                 body_size = ?3,
                 body_truncated = ?4
             WHERE id = ?1",
            params![
                response_id,
                body.id,
                body.size,
                if body_truncated { 1 } else { 0 }
            ],
        )?;

        Ok(true)
    }

    pub fn insert_storage_event(&self, event: &StorageEventRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO storage_events
             (id, session_id, tab_id, run_id, ts, origin, storage_type, operation,
              key, old_value, new_value, stack_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                event.id,
                event.session_id,
                event.tab_id,
                event.run_id,
                event.ts,
                event.origin,
                event.storage_type,
                event.operation,
                event.key,
                event.old_value,
                event.new_value,
                optional_json(&event.stack_json)?
            ],
        )?;
        Ok(())
    }

    pub fn insert_storage_snapshot(&self, snapshot: &StorageSnapshotRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO storage_snapshots
             (id, session_id, tab_id, run_id, ts, origin, storage_type, data_json, sha256)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                snapshot.id,
                snapshot.session_id,
                snapshot.tab_id,
                snapshot.run_id,
                snapshot.ts,
                snapshot.origin,
                snapshot.storage_type,
                serde_json::to_string(&snapshot.entries)?,
                snapshot.sha256
            ],
        )?;
        Ok(())
    }

    pub fn insert_cookie_snapshot(&self, snapshot: &CookieSnapshotRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cookie_snapshots
             (id, session_id, tab_id, run_id, ts, url, cookies_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                snapshot.id,
                snapshot.session_id,
                snapshot.tab_id,
                snapshot.run_id,
                snapshot.ts,
                snapshot.url,
                serde_json::to_string(&snapshot.cookies)?
            ],
        )?;
        Ok(())
    }

    pub fn insert_cookie_event(&self, event: &CookieEventRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO cookie_events
             (id, session_id, tab_id, run_id, ts, operation, name, domain, path, value, attributes_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                event.id,
                event.session_id,
                event.tab_id,
                event.run_id,
                event.ts,
                event.operation,
                event.name,
                event.domain,
                event.path,
                event.value,
                optional_json(&event.attributes_json)?
            ],
        )?;
        Ok(())
    }

    pub fn insert_replay(&self, replay: &ReplayRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO replays
             (id, session_id, tab_id, run_id, source_request_id, ts, command, exit_code,
              status_code, response_body_ref, output_path, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                replay.id,
                replay.session_id,
                replay.tab_id,
                replay.run_id,
                replay.source_request_id,
                replay.ts,
                replay.command,
                replay.exit_code,
                replay.status_code,
                replay.response_body_ref,
                replay.output_path,
                replay.error
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

    pub fn scripts(&self) -> Result<Vec<ScriptRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, body, created_at, updated_at, last_run_at
             FROM scripts
             ORDER BY updated_at DESC, name ASC",
        )?;
        let scripts = stmt
            .query_map([], |row| {
                Ok(ScriptRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    body: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    last_run_at: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(scripts)
    }

    pub fn script(&self, id: &str) -> Result<Option<ScriptRecord>> {
        self.conn
            .query_row(
                "SELECT id, name, body, created_at, updated_at, last_run_at
                 FROM scripts
                 WHERE id = ?1",
                params![id],
                |row| {
                    Ok(ScriptRecord {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        body: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                        last_run_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn save_script(&self, script: &ScriptRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO scripts (id, name, body, created_at, updated_at, last_run_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                body = excluded.body,
                updated_at = excluded.updated_at,
                last_run_at = excluded.last_run_at",
            params![
                script.id,
                script.name,
                script.body,
                script.created_at,
                script.updated_at,
                script.last_run_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_script(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM scripts WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn mark_script_run(&self, id: &str, ts: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE scripts
             SET last_run_at = ?2, updated_at = updated_at
             WHERE id = ?1",
            params![id, ts],
        )?;
        Ok(())
    }

    pub fn requests_for_session(&self, session_id: &str) -> Result<Vec<RequestRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, browser_request_id, started_at, completed_at,
                    method, url, resource_type, initiator, request_headers_json, request_body_ref, status
             FROM requests
             WHERE session_id = ?1
             ORDER BY started_at ASC, id ASC",
        )?;

        let requests = stmt
            .query_map(params![session_id], |row| {
                let headers_json: Option<String> = row.get(11)?;
                let status: String = row.get(13)?;
                Ok(RequestRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    run_id: row.get(3)?,
                    browser_request_id: row.get(4)?,
                    started_at: row.get(5)?,
                    completed_at: row.get(6)?,
                    method: row.get(7)?,
                    url: row.get(8)?,
                    resource_type: row.get(9)?,
                    initiator: row.get(10)?,
                    request_headers: parse_headers(headers_json)?,
                    request_body_ref: row.get(12)?,
                    status: parse_request_status(&status),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(requests)
    }

    pub fn responses_for_request(&self, request_id: &str) -> Result<Vec<ResponseRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, request_id, received_at, status_code, status_text, mime_type,
                    response_headers_json, body_ref, body_size, body_truncated
             FROM responses
             WHERE request_id = ?1
             ORDER BY received_at ASC, id ASC",
        )?;

        let responses = stmt
            .query_map(params![request_id], |row| {
                let headers_json: Option<String> = row.get(6)?;
                Ok(ResponseRecord {
                    id: row.get(0)?,
                    request_id: row.get(1)?,
                    received_at: row.get(2)?,
                    status_code: row.get(3)?,
                    status_text: row.get(4)?,
                    mime_type: row.get(5)?,
                    response_headers: parse_headers(headers_json)?,
                    body_ref: row.get(7)?,
                    body_size: row.get(8)?,
                    body_truncated: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(responses)
    }

    pub fn responses_for_session(&self, session_id: &str) -> Result<Vec<ResponseRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT responses.id, responses.request_id, responses.received_at,
                    responses.status_code, responses.status_text, responses.mime_type,
                    responses.response_headers_json, responses.body_ref,
                    responses.body_size, responses.body_truncated
             FROM responses
             JOIN requests ON requests.id = responses.request_id
             WHERE requests.session_id = ?1
             ORDER BY responses.received_at ASC, responses.id ASC",
        )?;

        let responses = stmt
            .query_map(params![session_id], |row| {
                let headers_json: Option<String> = row.get(6)?;
                Ok(ResponseRecord {
                    id: row.get(0)?,
                    request_id: row.get(1)?,
                    received_at: row.get(2)?,
                    status_code: row.get(3)?,
                    status_text: row.get(4)?,
                    mime_type: row.get(5)?,
                    response_headers: parse_headers(headers_json)?,
                    body_ref: row.get(7)?,
                    body_size: row.get(8)?,
                    body_truncated: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(responses)
    }

    pub fn replays_for_request(&self, request_id: &str) -> Result<Vec<ReplayRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, source_request_id, ts, command, exit_code,
                    status_code, response_body_ref, output_path, error
             FROM replays
             WHERE source_request_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let replays = stmt
            .query_map(params![request_id], |row| {
                Ok(ReplayRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    run_id: row.get(3)?,
                    source_request_id: row.get(4)?,
                    ts: row.get(5)?,
                    command: row.get(6)?,
                    exit_code: row.get(7)?,
                    status_code: row.get(8)?,
                    response_body_ref: row.get(9)?,
                    output_path: row.get(10)?,
                    error: row.get(11)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(replays)
    }

    pub fn replays_for_session(&self, session_id: &str) -> Result<Vec<ReplayRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, source_request_id, ts, command, exit_code,
                    status_code, response_body_ref, output_path, error
             FROM replays
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let replays = stmt
            .query_map(params![session_id], |row| {
                Ok(ReplayRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    run_id: row.get(3)?,
                    source_request_id: row.get(4)?,
                    ts: row.get(5)?,
                    command: row.get(6)?,
                    exit_code: row.get(7)?,
                    status_code: row.get(8)?,
                    response_body_ref: row.get(9)?,
                    output_path: row.get(10)?,
                    error: row.get(11)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(replays)
    }

    pub fn console_logs_for_session(&self, session_id: &str) -> Result<Vec<ConsoleLog>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, ts, level, message, source, line, stack_json
             FROM console_logs
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let logs = stmt
            .query_map(params![session_id], |row| {
                let level: String = row.get(5)?;
                let stack_json: Option<String> = row.get(9)?;
                Ok(ConsoleLog {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    run_id: row.get(3)?,
                    ts: row.get(4)?,
                    level: parse_console_level(&level),
                    message: row.get(6)?,
                    source: row.get(7)?,
                    line: row.get(8)?,
                    stack_json: optional_value_from_sql(stack_json, 9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(logs)
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
            .query_map(params![session_id], |row| {
                let direction: String = row.get(6)?;
                Ok(WebSocketFrameRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    run_id: row.get(3)?,
                    browser_request_id: row.get(4)?,
                    ts: row.get(5)?,
                    direction: parse_websocket_direction(&direction),
                    opcode: row.get(7)?,
                    mask: row.get(8)?,
                    payload: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(frames)
    }

    pub fn storage_events_for_session(&self, session_id: &str) -> Result<Vec<StorageEventRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, ts, origin, storage_type, operation,
                    key, old_value, new_value, stack_json
             FROM storage_events
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let events = stmt
            .query_map(params![session_id], |row| {
                let stack_json: Option<String> = row.get(11)?;
                Ok(StorageEventRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    run_id: row.get(3)?,
                    ts: row.get(4)?,
                    origin: row.get(5)?,
                    storage_type: row.get(6)?,
                    operation: row.get(7)?,
                    key: row.get(8)?,
                    old_value: row.get(9)?,
                    new_value: row.get(10)?,
                    stack_json: optional_value_from_sql(stack_json, 11)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(events)
    }

    pub fn storage_snapshots_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<StorageSnapshotRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, ts, origin, storage_type, data_json, sha256
             FROM storage_snapshots
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let snapshots = stmt
            .query_map(params![session_id], |row| {
                let data_json: String = row.get(7)?;
                Ok(StorageSnapshotRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    run_id: row.get(3)?,
                    ts: row.get(4)?,
                    origin: row.get(5)?,
                    storage_type: row.get(6)?,
                    entries: json_from_sql(&data_json, 7)?,
                    sha256: row.get(8)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(snapshots)
    }

    pub fn cookie_snapshots_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<CookieSnapshotRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, ts, url, cookies_json
             FROM cookie_snapshots
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let snapshots = stmt
            .query_map(params![session_id], |row| {
                let cookies_json: String = row.get(6)?;
                Ok(CookieSnapshotRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    run_id: row.get(3)?,
                    ts: row.get(4)?,
                    url: row.get(5)?,
                    cookies: json_from_sql(&cookies_json, 6)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(snapshots)
    }

    pub fn cookie_events_for_session(&self, session_id: &str) -> Result<Vec<CookieEventRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, ts, operation, name, domain, path, value, attributes_json
             FROM cookie_events
             WHERE session_id = ?1
             ORDER BY ts ASC, id ASC",
        )?;

        let events = stmt
            .query_map(params![session_id], |row| {
                let attributes_json: Option<String> = row.get(10)?;
                Ok(CookieEventRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    tab_id: row.get(2)?,
                    run_id: row.get(3)?,
                    ts: row.get(4)?,
                    operation: row.get(5)?,
                    name: row.get(6)?,
                    domain: row.get(7)?,
                    path: row.get(8)?,
                    value: row.get(9)?,
                    attributes_json: optional_value_from_sql(attributes_json, 10)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(events)
    }

    pub fn event_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?)
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

fn validate_readonly_sql(sql: &str) -> Result<()> {
    let stripped = strip_sql_comments(sql);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return Err(StoreError::QueryRejected("query is empty".to_string()));
    }
    if has_multiple_sql_statements(trimmed) {
        return Err(StoreError::QueryRejected(
            "only one SQL statement is allowed".to_string(),
        ));
    }
    let keyword = first_sql_keyword(trimmed);
    match keyword.as_deref() {
        Some("select" | "with" | "values" | "explain") => Ok(()),
        Some(other) => Err(StoreError::QueryRejected(format!(
            "{other} statements are not allowed"
        ))),
        None => Err(StoreError::QueryRejected(
            "query must start with a SQL keyword".to_string(),
        )),
    }
}

fn sql_value_to_string(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "NULL".to_string(),
        ValueRef::Integer(value) => value.to_string(),
        ValueRef::Real(value) => value.to_string(),
        ValueRef::Text(value) => String::from_utf8_lossy(value).to_string(),
        ValueRef::Blob(value) => format!("BLOB {} bytes", value.len()),
    }
}

fn first_sql_keyword(sql: &str) -> Option<String> {
    let keyword = sql
        .chars()
        .take_while(|character| character.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_lowercase();
    if keyword.is_empty() {
        None
    } else {
        Some(keyword)
    }
}

fn has_multiple_sql_statements(sql: &str) -> bool {
    let mut seen_statement_end = false;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut chars = sql.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\'' if !in_double_quote => {
                if in_single_quote && chars.peek() == Some(&'\'') {
                    let _escaped_quote = chars.next();
                } else {
                    in_single_quote = !in_single_quote;
                }
            }
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            ';' if !in_single_quote && !in_double_quote => seen_statement_end = true,
            _ if seen_statement_end && !character.is_whitespace() => return true,
            _ => {}
        }
    }
    false
}

fn strip_sql_comments(sql: &str) -> String {
    let mut output = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    while let Some(character) = chars.next() {
        match character {
            '\'' if !in_double_quote => {
                output.push(character);
                if in_single_quote && chars.peek() == Some(&'\'') {
                    if let Some(escaped_quote) = chars.next() {
                        output.push(escaped_quote);
                    }
                } else {
                    in_single_quote = !in_single_quote;
                }
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
                output.push(character);
            }
            '-' if !in_single_quote && !in_double_quote && chars.peek() == Some(&'-') => {
                let _second_dash = chars.next();
                for comment_character in chars.by_ref() {
                    if comment_character == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            }
            '/' if !in_single_quote && !in_double_quote && chars.peek() == Some(&'*') => {
                let _asterisk = chars.next();
                let mut previous = '\0';
                for comment_character in chars.by_ref() {
                    if previous == '*' && comment_character == '/' {
                        break;
                    }
                    previous = comment_character;
                }
                output.push(' ');
            }
            _ => output.push(character),
        }
    }
    output
}

pub fn inline_text_body(content_type: Option<String>, text: String) -> BodyRecord {
    let digest = Sha256::digest(text.as_bytes());
    let sha256 = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    BodyRecord::inline_text(content_type, text, sha256)
}

fn optional_json(value: &Option<serde_json::Value>) -> Result<Option<String>> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(StoreError::from)
}

fn parse_console_level(level: &str) -> ConsoleLevel {
    match level {
        "trace" => ConsoleLevel::Trace,
        "debug" => ConsoleLevel::Debug,
        "warning" => ConsoleLevel::Warning,
        "error" => ConsoleLevel::Error,
        "fatal" => ConsoleLevel::Fatal,
        _ => ConsoleLevel::Info,
    }
}

fn optional_value_from_sql(
    value: Option<String>,
    column: usize,
) -> rusqlite::Result<Option<serde_json::Value>> {
    value.map(|json| json_from_sql(&json, column)).transpose()
}

fn parse_headers(headers_json: Option<String>) -> rusqlite::Result<Vec<Header>> {
    let Some(headers_json) = headers_json else {
        return Ok(Vec::new());
    };
    json_from_sql(&headers_json, 0)
}

fn json_from_sql<T: serde::de::DeserializeOwned>(json: &str, column: usize) -> rusqlite::Result<T> {
    serde_json::from_str(json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
    })
}

fn parse_request_status(status: &str) -> RequestStatus {
    match status {
        "complete" => RequestStatus::Complete,
        "failed" => RequestStatus::Failed,
        "canceled" => RequestStatus::Canceled,
        _ => RequestStatus::Pending,
    }
}

fn parse_run_trigger(trigger: &str) -> faro_core::RunTrigger {
    match trigger {
        "reload" => faro_core::RunTrigger::Reload,
        "navigation" => faro_core::RunTrigger::Navigation,
        _ => faro_core::RunTrigger::InitialLoad,
    }
}

fn parse_websocket_direction(direction: &str) -> WebSocketFrameDirection {
    match direction {
        "sent" => WebSocketFrameDirection::Sent,
        _ => WebSocketFrameDirection::Received,
    }
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    name TEXT,
    root_url TEXT
);

CREATE TABLE IF NOT EXISTS tabs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    current_url TEXT,
    title TEXT
);

CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT NOT NULL REFERENCES tabs(id) ON DELETE CASCADE,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    url TEXT NOT NULL,
    trigger TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    ts INTEGER NOT NULL,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_events_session_ts ON events(session_id, ts);
CREATE INDEX IF NOT EXISTS idx_events_kind_ts ON events(kind, ts);

CREATE TABLE IF NOT EXISTS requests (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    browser_request_id TEXT,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    resource_type TEXT,
    initiator TEXT,
    request_headers_json TEXT,
    request_body_ref TEXT,
    status TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_requests_session_started ON requests(session_id, started_at);
CREATE INDEX IF NOT EXISTS idx_requests_run_started ON requests(run_id, started_at);

CREATE TABLE IF NOT EXISTS responses (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL REFERENCES requests(id) ON DELETE CASCADE,
    received_at INTEGER NOT NULL,
    status_code INTEGER,
    status_text TEXT,
    mime_type TEXT,
    response_headers_json TEXT,
    body_ref TEXT,
    body_size INTEGER,
    body_truncated INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS replays (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    source_request_id TEXT NOT NULL REFERENCES requests(id) ON DELETE CASCADE,
    ts INTEGER NOT NULL,
    command TEXT NOT NULL,
    exit_code INTEGER,
    status_code INTEGER,
    response_body_ref TEXT,
    output_path TEXT,
    error TEXT
);

CREATE INDEX IF NOT EXISTS idx_replays_request_ts ON replays(source_request_id, ts);
CREATE INDEX IF NOT EXISTS idx_replays_session_ts ON replays(session_id, ts);

CREATE TABLE IF NOT EXISTS bodies (
    id TEXT PRIMARY KEY,
    content_type TEXT,
    encoding TEXT,
    size INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    storage_kind TEXT NOT NULL,
    data BLOB
);

CREATE TABLE IF NOT EXISTS console_logs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    ts INTEGER NOT NULL,
    level TEXT NOT NULL,
    message TEXT NOT NULL,
    source TEXT,
    line INTEGER,
    stack_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_console_session_ts ON console_logs(session_id, ts);
CREATE INDEX IF NOT EXISTS idx_console_run_ts ON console_logs(run_id, ts);

CREATE TABLE IF NOT EXISTS storage_snapshots (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    ts INTEGER NOT NULL,
    origin TEXT NOT NULL,
    storage_type TEXT NOT NULL,
    data_json TEXT NOT NULL,
    sha256 TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS storage_events (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    ts INTEGER NOT NULL,
    origin TEXT NOT NULL,
    storage_type TEXT NOT NULL,
    operation TEXT NOT NULL,
    key TEXT,
    old_value TEXT,
    new_value TEXT,
    stack_json TEXT
);

CREATE TABLE IF NOT EXISTS cookie_snapshots (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    ts INTEGER NOT NULL,
    url TEXT,
    cookies_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cookie_events (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    ts INTEGER NOT NULL,
    operation TEXT NOT NULL,
    name TEXT,
    domain TEXT,
    path TEXT,
    value TEXT,
    attributes_json TEXT
);

CREATE TABLE IF NOT EXISTS websocket_frames (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    tab_id TEXT REFERENCES tabs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    browser_request_id TEXT NOT NULL,
    ts INTEGER NOT NULL,
    direction TEXT NOT NULL,
    opcode INTEGER NOT NULL,
    mask INTEGER NOT NULL,
    payload TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_websocket_frames_session_ts ON websocket_frames(session_id, ts);
CREATE INDEX IF NOT EXISTS idx_websocket_frames_request_ts ON websocket_frames(browser_request_id, ts);

CREATE TABLE IF NOT EXISTS scripts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    last_run_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_scripts_updated ON scripts(updated_at DESC, name ASC);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use faro_core::{
        CookieEventRecord, CookieRecord, CookieSnapshotRecord, ReplayRecord, RunTrigger,
        StorageEntry, StorageSnapshotRecord, WebSocketFrameDirection, WebSocketFrameRecord,
        console_event, cookie_event_observed_event, cookie_observed_event, request_completed_event,
        request_replayed_event, request_started_event, response_received_event,
        storage_snapshot_created_event, websocket_frame_event,
    };

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn persists_session_run_event_and_console_projection() -> TestResult {
        let store = Store::open_memory()?;
        let session = Session::new(
            Some("Smoke".to_string()),
            Some("http://localhost:3000".to_string()),
        );
        let tab = Tab::new(session.id.clone(), session.root_url.clone());
        let run = Run::new(
            session.id.clone(),
            tab.id.clone(),
            "http://localhost:3000".to_string(),
            RunTrigger::InitialLoad,
        );
        let log = ConsoleLog::new(
            session.id.clone(),
            Some(tab.id.clone()),
            Some(run.id.clone()),
            ConsoleLevel::Info,
            "hello from localhost".to_string(),
            Some("http://localhost:3000/main.js".to_string()),
            Some(42),
        );
        let event = console_event(&log);

        store.insert_session(&session)?;
        store.insert_tab(&tab)?;
        store.insert_run(&run)?;
        store.insert_console_log(&log)?;
        store.append_event(&event)?;

        assert!(store.session_exists(&session.id)?);
        assert_eq!(store.event_count()?, 1);

        let logs = store.console_logs_for_session(&session.id)?;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "hello from localhost");
        Ok(())
    }

    #[test]
    fn persists_network_projection_and_matching_events() -> TestResult {
        let store = Store::open_memory()?;
        let session = Session::new(None, Some("http://localhost:3000".to_string()));
        let tab = Tab::new(session.id.clone(), session.root_url.clone());
        let run = Run::new(
            session.id.clone(),
            tab.id.clone(),
            "http://localhost:3000".to_string(),
            RunTrigger::InitialLoad,
        );
        store.insert_session(&session)?;
        store.insert_tab(&tab)?;
        store.insert_run(&run)?;

        let mut request = RequestRecord::started(
            session.id.clone(),
            Some(tab.id.clone()),
            Some(run.id.clone()),
            "GET",
            "http://localhost:3000/api/todos",
        );
        request.browser_request_id = Some("cef-1".to_string());
        request.request_headers = vec![Header::new("accept", "application/json")];

        let mut response = ResponseRecord::received(request.id.clone());
        response.status_code = Some(200);
        response.mime_type = Some("application/json".to_string());
        response.body_size = Some(27);

        store.insert_request(&request)?;
        store.append_event(&request_started_event(&request))?;
        store.insert_response(&response)?;
        store.append_event(&response_received_event(
            session.id.clone(),
            Some(tab.id.clone()),
            Some(run.id.clone()),
            &response,
        ))?;

        request.completed_at = Some(faro_core::now_ms());
        request.status = RequestStatus::Complete;
        store.complete_request(&request)?;
        store.append_event(&request_completed_event(&request))?;

        assert_eq!(store.event_count()?, 3);

        let requests = store.requests_for_session(&session.id)?;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].status, RequestStatus::Complete);
        assert_eq!(requests[0].request_headers[0].name, "accept");

        let responses = store.responses_for_request(&request.id)?;
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].status_code, Some(200));

        let mut replay = ReplayRecord::new(
            session.id.clone(),
            Some(tab.id.clone()),
            Some(run.id.clone()),
            request.id.clone(),
            "curl -X GET http://localhost:3000/api/todos".to_string(),
        );
        replay.exit_code = Some(0);
        replay.status_code = Some(200);
        store.insert_replay(&replay)?;
        store.append_event(&request_replayed_event(&replay))?;

        let replays = store.replays_for_request(&request.id)?;
        assert_eq!(replays.len(), 1);
        assert_eq!(replays[0].status_code, Some(200));
        assert_eq!(store.replays_for_session(&session.id)?.len(), 1);
        Ok(())
    }

    #[test]
    fn persists_websocket_frames() -> TestResult {
        let store = Store::open_memory()?;
        let session = Session::new(None, Some("http://localhost:3000".to_string()));
        let tab = Tab::new(session.id.clone(), session.root_url.clone());
        let run = Run::new(
            session.id.clone(),
            tab.id.clone(),
            "http://localhost:3000".to_string(),
            RunTrigger::InitialLoad,
        );
        store.insert_session(&session)?;
        store.insert_tab(&tab)?;
        store.insert_run(&run)?;

        let frame = WebSocketFrameRecord::new(
            session.id.clone(),
            Some(tab.id.clone()),
            Some(run.id.clone()),
            "ws-1".to_string(),
            WebSocketFrameDirection::Received,
            1,
            false,
            "{\"type\":\"hello\"}".to_string(),
        );
        store.insert_websocket_frame(&frame)?;
        store.append_event(&websocket_frame_event(&frame))?;

        let frames = store.websocket_frames_for_session(&session.id)?;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].browser_request_id, "ws-1");
        assert_eq!(frames[0].direction, WebSocketFrameDirection::Received);
        assert_eq!(frames[0].payload, "{\"type\":\"hello\"}");
        Ok(())
    }

    #[test]
    fn persists_storage_and_cookie_snapshots() -> TestResult {
        let store = Store::open_memory()?;
        let session = Session::new(None, Some("http://localhost:3000".to_string()));
        let tab = Tab::new(session.id.clone(), session.root_url.clone());
        let run = Run::new(
            session.id.clone(),
            tab.id.clone(),
            "http://localhost:3000".to_string(),
            RunTrigger::InitialLoad,
        );
        store.insert_session(&session)?;
        store.insert_tab(&tab)?;
        store.insert_run(&run)?;

        let storage = StorageSnapshotRecord::new(
            session.id.clone(),
            Some(tab.id.clone()),
            Some(run.id.clone()),
            "http://localhost:3000".to_string(),
            "localStorage".to_string(),
            vec![StorageEntry::new("token", "abc")],
            "sha".to_string(),
        );
        store.insert_storage_snapshot(&storage)?;
        store.append_event(&storage_snapshot_created_event(&storage))?;

        let cookies = CookieSnapshotRecord::new(
            session.id.clone(),
            Some(tab.id.clone()),
            Some(run.id.clone()),
            Some("http://localhost:3000".to_string()),
            vec![CookieRecord {
                name: "sid".to_string(),
                value: "123".to_string(),
                domain: "localhost".to_string(),
                path: "/".to_string(),
                expires: None,
                http_only: true,
                secure: false,
                same_site: Some("Lax".to_string()),
            }],
        );
        store.insert_cookie_snapshot(&cookies)?;
        store.append_event(&cookie_observed_event(&cookies))?;
        let cookie_event = CookieEventRecord::new(
            session.id.clone(),
            Some(tab.id.clone()),
            Some(run.id.clone()),
            "document.cookie",
            Some("sid".to_string()),
            Some("localhost".to_string()),
            Some("/".to_string()),
            Some("123".to_string()),
            None,
        );
        store.insert_cookie_event(&cookie_event)?;
        store.append_event(&cookie_event_observed_event(&cookie_event))?;

        let storage_snapshots = store.storage_snapshots_for_session(&session.id)?;
        assert_eq!(storage_snapshots.len(), 1);
        assert_eq!(storage_snapshots[0].entries[0].key, "token");

        let cookie_snapshots = store.cookie_snapshots_for_session(&session.id)?;
        assert_eq!(cookie_snapshots.len(), 1);
        assert_eq!(cookie_snapshots[0].cookies[0].name, "sid");
        let cookie_events = store.cookie_events_for_session(&session.id)?;
        assert_eq!(cookie_events.len(), 1);
        assert_eq!(cookie_events[0].operation, "document.cookie");
        assert_eq!(store.event_count()?, 3);
        Ok(())
    }

    #[test]
    fn validates_readonly_sql_queries() -> TestResult {
        validate_readonly_sql(
            "
            -- recent requests
            SELECT method, url
            FROM requests
            WHERE url LIKE '%api%';
            ",
        )?;

        assert!(validate_readonly_sql("SELECT 1; SELECT 2").is_err());
        assert!(validate_readonly_sql("UPDATE requests SET method = 'GET'").is_err());
        assert!(validate_readonly_sql("PRAGMA writable_schema = 1").is_err());
        Ok(())
    }

    #[test]
    fn executes_readonly_sql_query() -> TestResult {
        let db_path =
            std::env::temp_dir().join(format!("faro-query-test-{}.db", uuid::Uuid::new_v4()));
        let session = Session::new(None, Some("https://example.test".to_string()));
        {
            let store = Store::open(&db_path)?;
            store.insert_session(&session)?;
        }

        let result = Store::query_readonly(
            &db_path,
            "SELECT root_url, name FROM sessions ORDER BY created_at DESC",
        )?;
        assert_eq!(result.columns, vec!["root_url", "name"]);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0], "https://example.test");
        assert_eq!(result.rows[0][1], "NULL");
        std::fs::remove_file(db_path)?;
        Ok(())
    }
}
