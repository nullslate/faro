use crate::rows::{request_from_row, response_from_row};
use crate::{Result, Store, StoreError};
use faro_core::{BodyRecord, ReplayRecord, RequestRecord, ResponseRecord};
use rusqlite::{OptionalExtension, params};

impl Store {
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

    pub fn requests_for_session(&self, session_id: &str) -> Result<Vec<RequestRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, browser_request_id, started_at, completed_at,
                    method, url, resource_type, initiator, request_headers_json, request_body_ref, status
             FROM requests
             WHERE session_id = ?1
             ORDER BY started_at ASC, id ASC",
        )?;

        let requests = stmt
            .query_map(params![session_id], request_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(requests)
    }

    pub fn request_by_id(&self, request_id: &str) -> Result<Option<RequestRecord>> {
        self.conn
            .query_row(
                "SELECT id, session_id, tab_id, run_id, browser_request_id, started_at, completed_at,
                        method, url, resource_type, initiator, request_headers_json, request_body_ref, status
                 FROM requests
                 WHERE id = ?1",
                params![request_id],
                request_from_row,
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn requests_for_session_changed_after(
        &self,
        session_id: &str,
        started_after: i64,
        completed_after: i64,
    ) -> Result<Vec<RequestRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, tab_id, run_id, browser_request_id, started_at, completed_at,
                    method, url, resource_type, initiator, request_headers_json, request_body_ref, status
             FROM requests
             WHERE session_id = ?1
               AND (started_at > ?2 OR completed_at > ?3)
             ORDER BY started_at ASC, id ASC",
        )?;

        let requests = stmt
            .query_map(params![session_id, started_after, completed_after], |row| {
                request_from_row(row)
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
            .query_map(params![request_id], response_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(responses)
    }

    pub fn latest_response_for_request(&self, request_id: &str) -> Result<Option<ResponseRecord>> {
        self.conn
            .query_row(
                "SELECT id, request_id, received_at, status_code, status_text, mime_type,
                        response_headers_json, body_ref, body_size, body_truncated
                 FROM responses
                 WHERE request_id = ?1
                 ORDER BY received_at DESC, id DESC
                 LIMIT 1",
                params![request_id],
                response_from_row,
            )
            .optional()
            .map_err(StoreError::from)
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
            .query_map(params![session_id], response_from_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(responses)
    }

    pub fn responses_for_session_after(
        &self,
        session_id: &str,
        received_after: i64,
    ) -> Result<Vec<ResponseRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT responses.id, responses.request_id, responses.received_at,
                    responses.status_code, responses.status_text, responses.mime_type,
                    responses.response_headers_json, responses.body_ref,
                    responses.body_size, responses.body_truncated
             FROM responses
             JOIN requests ON requests.id = responses.request_id
             WHERE requests.session_id = ?1
               AND responses.received_at > ?2
             ORDER BY responses.received_at ASC, responses.id ASC",
        )?;

        let responses = stmt
            .query_map(params![session_id, received_after], |row| {
                response_from_row(row)
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
}
