use crate::cli::{CliOptions, open_store};
use crate::query::{RequestListQuery, list_request_rows};
use crate::services::{
    execute_replay, limited_body, request_with_latest_response, response_body_for_request,
    shareable_curl_command,
};
use anyhow::{Context, bail};
use serde_json::{Value, json};
use std::path::Path;

use super::helpers::{find_replay, optional_string, required_string, resolve_session};

pub(super) fn list_requests_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let session = resolve_session(&store, args)?;
    let filter = optional_string(args, "filter");
    let route = optional_string(args, "route");
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value.min(500) as usize)
        .unwrap_or(100);
    let rows = list_request_rows(
        &store,
        &session.id,
        &RequestListQuery {
            filter,
            route,
            limit: Some(limit),
        },
    )?;
    Ok(json!({ "session_id": session.id, "requests": rows }))
}

pub(super) fn get_request_tool(options: &CliOptions, args: &Value) -> anyhow::Result<Value> {
    let request_id = required_string(args, "request_id")?;
    let include_body = args
        .get("include_body")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let store = open_store(&options.db_path)?;
    let (request, response) = request_with_latest_response(&store, &request_id)?;
    Ok(json!({
        "request": request,
        "response": response,
        "request_body": if include_body {
            limited_body(&store, request.request_body_ref.as_deref(), options.redaction.mcp_body_limit_bytes)?
        } else {
            None
        },
        "response_body": if include_body {
            limited_body(
                &store,
                response
                    .as_ref()
                    .and_then(|response| response.body_ref.as_deref()),
                options.redaction.mcp_body_limit_bytes,
            )?
        } else {
            None
        }
    }))
}

pub(super) fn get_response_body_tool(options: &CliOptions, args: &Value) -> anyhow::Result<Value> {
    let request_id = required_string(args, "request_id")?;
    let store = open_store(&options.db_path)?;
    let body =
        response_body_for_request(&store, &request_id, options.redaction.mcp_body_limit_bytes)?;
    Ok(json!({ "request_id": request_id, "body": body }))
}

pub(super) fn list_replays_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let store = open_store(&db_path.to_path_buf())?;
    let request_id = optional_string(args, "request_id");
    let session = if request_id.is_none() {
        Some(resolve_session(&store, args)?)
    } else {
        None
    };
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value.min(500) as usize)
        .unwrap_or(100);
    let replays = if let Some(request_id) = request_id {
        store
            .replays_for_request(&request_id)
            .with_context(|| format!("load replays for request {request_id}"))?
    } else {
        let Some(session) = session.as_ref() else {
            bail!("missing resolved session");
        };
        store
            .replays_for_session(&session.id)
            .with_context(|| format!("load replays for session {}", session.id))?
    }
    .into_iter()
    .rev()
    .take(limit)
    .collect::<Vec<_>>();
    Ok(json!({
        "session_id": session.as_ref().map(|session| session.id.clone()),
        "replays": replays
    }))
}

pub(super) fn get_replay_tool(options: &CliOptions, args: &Value) -> anyhow::Result<Value> {
    let replay_id = required_string(args, "replay_id")?;
    let include_body = args
        .get("include_body")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let store = open_store(&options.db_path)?;
    let replay = find_replay(&store, &replay_id)?;
    let body = if include_body {
        limited_body(
            &store,
            replay.response_body_ref.as_deref(),
            options.redaction.mcp_body_limit_bytes,
        )?
    } else {
        None
    };
    Ok(json!({ "replay": replay, "body": body }))
}

pub(super) fn copy_request_as_curl_tool(
    options: &CliOptions,
    args: &Value,
) -> anyhow::Result<Value> {
    let request_id = required_string(args, "request_id")?;
    let include_sensitive = args
        .get("include_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let store = open_store(&options.db_path)?;
    Ok(json!(shareable_curl_command(
        &store,
        &request_id,
        include_sensitive,
        &options.redaction,
    )?))
}

pub(super) fn replay_request_tool(db_path: &Path, args: &Value) -> anyhow::Result<Value> {
    let request_id = required_string(args, "request_id")?;
    let store = open_store(&db_path.to_path_buf())?;
    Ok(json!(execute_replay(&store, &request_id)?))
}
