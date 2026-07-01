use crate::cli::parse_duration;
use crate::services::latest_session;
use anyhow::{Context, bail};
use faro_cdp::CaptureUpdate;
use faro_core::{ReplayRecord, Session};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

pub(super) fn resolve_session(store: &faro_store::Store, args: &Value) -> anyhow::Result<Session> {
    if let Some(session_id) = optional_string(args, "session_id") {
        return store
            .sessions()
            .context("load sessions")?
            .into_iter()
            .find(|session| session.id == session_id)
            .with_context(|| format!("session `{session_id}` not found"));
    }
    latest_session(store)?.context("no faro sessions found")
}

pub(super) fn find_replay(
    store: &faro_store::Store,
    replay_id: &str,
) -> anyhow::Result<ReplayRecord> {
    for session in store.sessions().context("load sessions")? {
        if let Some(replay) = store
            .replays_for_session(&session.id)
            .with_context(|| format!("load replays for session {}", session.id))?
            .into_iter()
            .find(|replay| replay.id == replay_id)
        {
            return Ok(replay);
        }
    }
    bail!("replay `{replay_id}` not found")
}

pub(super) fn parse_capture_duration(args: &Value) -> anyhow::Result<Duration> {
    Ok(optional_string(args, "duration")
        .map(|value| parse_duration(&value))
        .transpose()?
        .or_else(|| {
            args.get("duration_ms")
                .and_then(Value::as_u64)
                .map(Duration::from_millis)
        })
        .unwrap_or_else(|| Duration::from_secs(10)))
}

pub(super) fn capture_update_json(update: CaptureUpdate, db_path: &Path) -> Value {
    match update {
        CaptureUpdate::SessionStarted { session_id, url } => json!({
            "kind": "session_started",
            "db_path": db_path,
            "session_id": session_id,
            "url": url
        }),
        CaptureUpdate::Attached { url, websocket_url } => json!({
            "kind": "attached",
            "db_path": db_path,
            "url": url,
            "websocket_url": websocket_url
        }),
        CaptureUpdate::Status(message) => json!({
            "kind": "status",
            "db_path": db_path,
            "message": message
        }),
        CaptureUpdate::StoreChanged => json!({
            "kind": "store_changed",
            "db_path": db_path
        }),
        CaptureUpdate::Error(message) => json!({
            "kind": "error",
            "db_path": db_path,
            "message": message
        }),
    }
}

pub(super) fn required_string(args: &Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .with_context(|| format!("missing string argument `{key}`"))
}

pub(super) fn optional_string(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_str).map(str::to_string)
}
