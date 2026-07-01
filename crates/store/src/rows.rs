use crate::{Result, StoreError};
use faro_core::{
    ConsoleLevel, ConsoleLog, Header, RequestRecord, RequestStatus, ResponseRecord,
    WebSocketFrameDirection, WebSocketFrameRecord,
};
use rusqlite::{Row, types::Type};

pub(super) fn sqlite_count_to_usize(value: i64) -> usize {
    if value <= 0 {
        return 0;
    }
    match usize::try_from(value) {
        Ok(value) => value,
        Err(_) => usize::MAX,
    }
}

pub(super) fn optional_json(value: &Option<serde_json::Value>) -> Result<Option<String>> {
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

pub(super) fn optional_value_from_sql(
    value: Option<String>,
    column: usize,
) -> rusqlite::Result<Option<serde_json::Value>> {
    value.map(|json| json_from_sql(&json, column)).transpose()
}

pub(super) fn request_from_row(row: &Row<'_>) -> rusqlite::Result<RequestRecord> {
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
}

pub(super) fn response_from_row(row: &Row<'_>) -> rusqlite::Result<ResponseRecord> {
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
}

pub(super) fn console_log_from_row(row: &Row<'_>) -> rusqlite::Result<ConsoleLog> {
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
}

pub(super) fn websocket_frame_from_row(row: &Row<'_>) -> rusqlite::Result<WebSocketFrameRecord> {
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
}

fn parse_headers(headers_json: Option<String>) -> rusqlite::Result<Vec<Header>> {
    let Some(headers_json) = headers_json else {
        return Ok(Vec::new());
    };
    json_from_sql(&headers_json, 0)
}

pub(super) fn json_from_sql<T: serde::de::DeserializeOwned>(
    json: &str,
    column: usize,
) -> rusqlite::Result<T> {
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

pub(super) fn parse_run_trigger(trigger: &str) -> faro_core::RunTrigger {
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
