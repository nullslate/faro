use crate::cli::open_store;
use crate::services::{session_summaries, session_summary};
use anyhow::{Context, bail};
use serde_json::{Value, json};
use std::path::Path;

use super::helpers::resolve_session;

pub(super) fn list_sessions_tool(db_path: &Path) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let sessions = session_summaries(&store)?
        .into_iter()
        .map(|summary| {
            json!({
                "id": summary.session.id,
                "created_at": summary.session.created_at,
                "name": summary.session.name,
                "root_url": summary.session.root_url,
                "counts": {
                    "requests": summary.request_count,
                    "errors": summary.console_error_count,
                    "replays": summary.replay_count,
                    "websocket_frames": summary.websocket_count,
                    "storage_events": summary.storage_count,
                    "cookie_events": summary.cookie_count
                }
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({ "sessions": sessions }))
}

pub(super) fn get_session_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let session = resolve_session(&store, args)?;
    let summary = session_summary(&store, session)?;
    Ok(json!({
        "session": summary.session,
        "counts": {
            "requests": summary.request_count,
            "errors": summary.console_error_count,
            "replays": summary.replay_count,
            "websocket_frames": summary.websocket_count,
            "storage_events": summary.storage_count,
            "cookie_events": summary.cookie_count
        }
    }))
}

pub(super) fn delete_all_sessions_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let confirm = args
        .get("confirm")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !confirm {
        bail!("delete_all_sessions requires confirm=true");
    }
    let store = open_store(&db_path.to_path_buf())?;
    let deleted = store.delete_all_sessions().context("delete all sessions")?;
    Ok(json!({ "deleted": deleted }))
}
