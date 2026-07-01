use crate::cli::{current_storage_items, latest_cookies, open_store};
use anyhow::Context;
use serde_json::{Map, Value, json};
use std::path::Path;

use super::helpers::{optional_string, required_string, resolve_session};

pub(super) fn list_websocket_frames_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let session = resolve_session(&store, args)?;
    let direction = optional_string(args, "direction").map(|value| value.to_lowercase());
    let opcode = args.get("opcode").and_then(Value::as_i64);
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value.min(1000) as usize)
        .unwrap_or(200);
    let frames = store
        .websocket_frames_for_session(&session.id)
        .with_context(|| format!("load websocket frames for session {}", session.id))?
        .into_iter()
        .filter(|frame| {
            direction
                .as_deref()
                .map(|direction| frame.direction.as_str() == direction)
                .unwrap_or(true)
        })
        .filter(|frame| opcode.map(|opcode| frame.opcode == opcode).unwrap_or(true))
        .rev()
        .take(limit)
        .collect::<Vec<_>>();
    Ok(json!({ "session_id": session.id, "frames": frames }))
}

pub(super) fn list_console_errors_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let session = resolve_session(&store, args)?;
    let errors = store
        .console_logs_for_session(&session.id)
        .with_context(|| format!("load console logs for session {}", session.id))?
        .into_iter()
        .filter(|log| {
            matches!(
                log.level,
                faro_core::ConsoleLevel::Error | faro_core::ConsoleLevel::Fatal
            )
        })
        .collect::<Vec<_>>();
    Ok(json!({ "session_id": session.id, "errors": errors }))
}

pub(super) fn get_storage_item_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let storage_type = required_string(args, "storage_type")?;
    let key = required_string(args, "key")?;
    let store = open_store(&db_path.to_path_buf())?;
    let session = resolve_session(&store, args)?;
    let items = current_storage_items(&store, &session.id)?
        .into_iter()
        .filter(|item| item.storage_type == storage_type && item.key == key)
        .collect::<Vec<_>>();
    Ok(json!({ "session_id": session.id, "items": items }))
}

pub(super) fn list_storage_items_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let session = resolve_session(&store, args)?;
    let storage_type = optional_string(args, "storage_type");
    let key_contains = optional_string(args, "key_contains").map(|value| value.to_lowercase());
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value.min(1000) as usize)
        .unwrap_or(200);
    let items = current_storage_items(&store, &session.id)?
        .into_iter()
        .filter(|item| {
            storage_type
                .as_deref()
                .map(|storage_type| item.storage_type == storage_type)
                .unwrap_or(true)
        })
        .filter(|item| {
            key_contains
                .as_deref()
                .map(|needle| item.key.to_lowercase().contains(needle))
                .unwrap_or(true)
        })
        .take(limit)
        .collect::<Vec<_>>();
    Ok(json!({ "session_id": session.id, "items": items }))
}

pub(super) fn list_cookies_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let session = resolve_session(&store, args)?;
    Ok(json!({
        "session_id": session.id,
        "cookies": latest_cookies(&store, &session.id)?
    }))
}

pub(super) fn run_readonly_sql_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let query = required_string(args, "query")?;
    let result = faro_store::Store::query_readonly(db_path, &query)
        .with_context(|| format!("run read-only SQL against {}", db_path.display()))?;
    let rows = result
        .rows
        .into_iter()
        .map(|row| {
            result
                .columns
                .iter()
                .cloned()
                .zip(row.into_iter().map(Value::String))
                .collect::<Map<_, _>>()
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "columns": result.columns,
        "row_count": rows.len(),
        "rows": rows,
        "duration_ms": result.duration_ms
    }))
}
