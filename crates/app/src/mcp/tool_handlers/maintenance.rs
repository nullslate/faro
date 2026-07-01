use crate::cli::open_store;
use crate::query::path_for_url;
use anyhow::{Context, bail};
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn get_db_stats_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let session_limit = bounded_limit(args, "session_limit", 5, 25);
    let repeated_limit = bounded_limit(args, "repeated_limit", 8, 50);
    let body_storage = store
        .body_storage_stats()
        .context("load body storage stats")?;
    let tables = store
        .table_row_counts()
        .context("load database table row counts")?
        .into_iter()
        .map(|count| {
            json!({
                "table": count.table,
                "rows": count.rows
            })
        })
        .collect::<Vec<_>>();
    let top_sessions = store
        .top_session_storage_stats(session_limit)
        .context("load heavy sessions")?
        .into_iter()
        .map(session_json)
        .collect::<Vec<_>>();
    let top_repeated_requests = store
        .top_repeated_request_groups(repeated_limit)
        .context("load repeated request groups")?
        .into_iter()
        .map(repeated_request_json)
        .collect::<Vec<_>>();

    Ok(json!({
        "db_path": db_path.display().to_string(),
        "body_storage": {
            "bodies": body_storage.bodies,
            "total_bytes": body_storage.total_bytes,
            "inline_bytes": body_storage.inline_bytes,
            "external_bytes": body_storage.external_bytes
        },
        "top_sessions": top_sessions,
        "top_repeated_requests": top_repeated_requests,
        "tables": tables
    }))
}

pub(super) fn list_heavy_sessions_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let limit = bounded_limit(args, "limit", 10, 100);
    let sessions = store
        .top_session_storage_stats(limit)
        .context("load heavy sessions")?
        .into_iter()
        .map(session_json)
        .collect::<Vec<_>>();
    Ok(json!({ "sessions": sessions }))
}

pub(super) fn list_repeated_requests_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let limit = bounded_limit(args, "limit", 20, 200);
    let groups = store
        .top_repeated_request_groups(limit)
        .context("load repeated request groups")?
        .into_iter()
        .map(repeated_request_json)
        .collect::<Vec<_>>();
    Ok(json!({ "groups": groups }))
}

pub(super) fn prune_session_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    if !args
        .get("confirm")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        bail!("prune_session requires confirm=true");
    }
    let session_id = args
        .get("session_id")
        .and_then(Value::as_str)
        .context("missing string argument `session_id`")?;
    let max_requests = optional_positive_limit(args, "max_requests")?;
    let max_repeated = optional_positive_limit(args, "max_repeated")?;
    let max_console = optional_positive_limit(args, "max_console")?;
    let max_websocket = optional_positive_limit(args, "max_ws")?
        .or(optional_positive_limit(args, "max_websocket")?);
    if max_requests.is_none()
        && max_repeated.is_none()
        && max_console.is_none()
        && max_websocket.is_none()
    {
        bail!("prune_session requires at least one max option");
    }

    let store = open_store(&db_path.to_path_buf())?;
    if !store.session_exists(&session_id.to_string())? {
        bail!("session not found: {session_id}");
    }

    let repeated_requests_deleted = if let Some(limit) = max_repeated {
        store
            .prune_repeated_session_requests(session_id, limit)
            .context("prune repeated session requests")?
    } else {
        0
    };
    let old_requests_deleted = if let Some(limit) = max_requests {
        store
            .prune_session_requests(session_id, limit)
            .context("prune old session requests")?
    } else {
        0
    };
    let console_logs_deleted = if let Some(limit) = max_console {
        store
            .prune_session_console_logs(session_id, limit)
            .context("prune old console logs")?
    } else {
        0
    };
    let websocket_frames_deleted = if let Some(limit) = max_websocket {
        store
            .prune_session_websocket_frames(session_id, limit)
            .context("prune old websocket frames")?
    } else {
        0
    };
    let vacuumed = args.get("vacuum").and_then(Value::as_bool).unwrap_or(false);
    if vacuumed {
        store.checkpoint_and_vacuum().context("vacuum database")?;
    }

    Ok(json!({
        "session_id": session_id,
        "repeated_requests_deleted": repeated_requests_deleted,
        "old_requests_deleted": old_requests_deleted,
        "console_logs_deleted": console_logs_deleted,
        "websocket_frames_deleted": websocket_frames_deleted,
        "vacuumed": vacuumed
    }))
}

fn session_json(session: faro_store::SessionStorageStats) -> Value {
    json!({
        "id": session.id,
        "created_at": session.created_at,
        "name": session.name,
        "root_url": session.root_url,
        "requests": session.requests,
        "console_errors": session.console_errors,
        "replays": session.replays,
        "websocket_frames": session.websocket_frames,
        "storage_events": session.storage_events,
        "cookie_events": session.cookie_events,
        "bodies": session.bodies,
        "body_bytes": session.body_bytes
    })
}

fn repeated_request_json(group: faro_store::RepeatedRequestGroup) -> Value {
    json!({
        "session_id": group.session_id,
        "root_url": group.root_url,
        "method": group.method,
        "resource_type": group.resource_type,
        "domain": domain_for_url(&group.url),
        "path": path_for_url(&group.url),
        "url": group.url,
        "requests": group.requests,
        "error_responses": group.error_responses,
        "body_bytes": group.body_bytes,
        "first_started_at": group.first_started_at,
        "last_started_at": group.last_started_at
    })
}

fn bounded_limit(args: &Value, key: &str, default: usize, max: usize) -> usize {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|value| {
            if value > usize::MAX as u64 {
                usize::MAX
            } else {
                value as usize
            }
        })
        .map(|value| value.clamp(1, max))
        .unwrap_or(default)
}

fn optional_positive_limit(args: &Value, key: &str) -> anyhow::Result<Option<usize>> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let Some(value) = value.as_u64() else {
        bail!("{key} must be a positive number");
    };
    if value == 0 {
        bail!("{key} must be greater than zero");
    }
    let value = if value > usize::MAX as u64 {
        usize::MAX
    } else {
        value as usize
    };
    Ok(Some(value))
}

fn domain_for_url(value: &str) -> String {
    let without_scheme = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
        .unwrap_or(value);
    without_scheme
        .split('/')
        .next()
        .filter(|domain| !domain.is_empty())
        .unwrap_or("-")
        .to_string()
}
